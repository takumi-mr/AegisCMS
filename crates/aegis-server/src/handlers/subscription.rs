//! 会員のサブスク（docs/members.md 16）。Stripe Subscription で自動更新する。
//!
//! - `GET /api/v1/members/plans` — プランの一覧（公開）
//! - `POST /api/v1/members/subscriptions` / `GET /account/subscribe?plan=..` — 契約を始める（決済ページへ）
//! - `GET /api/v1/members/subscriptions` — 自分の契約
//! - `POST /api/v1/members/billing-portal` / `GET /account/billing` — 解約・カード変更（Stripe の Customer Portal）
//!
//! 契約の状態は、Stripe の通知（Webhook）が決める。ここのテーブルはその写しで、
//! 有効なレベルは、手動で付けたレベルと契約中のプランのレベルの高いほう。

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::audit::AuditContext;
use crate::handlers::members::resolve_member_session;
use crate::handlers::purchase::{api_error, ensure_enabled, internal, stripe_client};
use crate::state::AppState;
use aegis_core::{AuditAction, AuditOutcome};
use aegis_db::{
    SubscriptionRecord, SUBSCRIPTION_ACTIVE, SUBSCRIPTION_CANCELLED, SUBSCRIPTION_PAST_DUE,
};

type ApiError = (StatusCode, Json<Value>);

/// 期間の終わりを過ぎても、更新の通知が届くまで契約中として扱う猶予（日）
const GRACE_DAYS: i64 = 3;

/// 契約が、いま閲覧の権利を与えているか。契約中（`active`）か、支払いに失敗していても期間の終わりまで
/// （`past_due`）。期間の終わりを過ぎて通知が届かないままなら、猶予のあとで外れる
pub fn grants_access(
    subscription: &SubscriptionRecord,
    now: chrono::DateTime<chrono::Utc>,
) -> bool {
    if subscription.member_id.is_none() {
        return false;
    }
    if subscription.status != SUBSCRIPTION_ACTIVE && subscription.status != SUBSCRIPTION_PAST_DUE {
        return false;
    }
    chrono::DateTime::parse_from_rfc3339(&subscription.current_period_end)
        .map(|end| end + chrono::Duration::days(GRACE_DAYS) > now)
        .unwrap_or(false)
}

/// 会員が契約中のプランのレベル。購入が無効・調べられないときは空（見せない側に倒す）
pub async fn granted_levels(state: &AppState, member_id: &str) -> Vec<String> {
    if !crate::handlers::access::purchase_enabled(state) {
        return Vec::new();
    }
    let now = chrono::Utc::now();
    state
        .db
        .list_member_subscriptions(member_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|s| grants_access(s, now))
        .map(|s| s.level)
        .collect()
}

fn view(subscription: &SubscriptionRecord, state: &AppState) -> Value {
    let plan = state.config.members.purchase.plan(&subscription.plan_id);
    json!({
        "id": subscription.id,
        "plan_id": subscription.plan_id,
        "plan_label": plan.map(|p| p.display_label()),
        "level": subscription.level,
        "status": subscription.status,
        "current_period_end": subscription.current_period_end,
        "cancel_at_period_end": subscription.cancel_at_period_end,
        "active": grants_access(subscription, chrono::Utc::now()),
        "created_at": subscription.created_at,
    })
}

/// 管理画面・マイページ向けの表示
pub fn views(state: &AppState, subscriptions: &[SubscriptionRecord]) -> Vec<Value> {
    subscriptions.iter().map(|s| view(s, state)).collect()
}

/// `GET /api/v1/members/plans` — プランの一覧
pub async fn list_plans(State(state): State<Arc<AppState>>) -> Result<Json<Value>, ApiError> {
    ensure_enabled(&state)?;
    let config = &state.config.members.purchase;
    let plans: Vec<Value> = config
        .plans
        .iter()
        .map(|p| {
            json!({
                "id": p.id,
                "label": p.display_label(),
                "level": p.level,
                "amount": p.amount,
                "currency": config.currency,
                "interval": p.interval.as_str(),
                "interval_count": p.interval_count,
            })
        })
        .collect();
    Ok(Json(json!({ "plans": plans })))
}

/// `GET /api/v1/members/subscriptions` — ログイン会員の契約（新しい順）。`level` は有効なレベル
pub async fn list_my_subscriptions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    ensure_enabled(&state)?;
    let session = resolve_member_session(&state, &headers).await?;
    let subscriptions = state
        .db
        .list_member_subscriptions(&session.member_id)
        .await
        .map_err(internal)?;
    let level = match state.members.as_ref() {
        Some(service) => {
            let profile = service
                .get_profile(&session.member_id)
                .await
                .map_err(internal)?;
            let granted = granted_levels(&state, &session.member_id).await;
            let granted: Vec<&str> = granted.iter().map(String::as_str).collect();
            aegis_members::effective_level(&state.config.members.levels, &profile.level, &granted)
                .to_string()
        }
        None => String::new(),
    };
    Ok(Json(json!({
        "subscriptions": views(&state, &subscriptions),
        "level": level,
    })))
}

#[derive(Debug, Deserialize)]
pub struct StartSubscriptionRequest {
    pub plan_id: String,
}

/// Checkout Session の作成パラメータ（`mode=subscription`）。金額・間隔はプランの設定だけから作る
fn subscription_form(
    plan: &aegis_members::PlanConfig,
    currency: &str,
    member_id: &str,
    email: &str,
    customer: Option<&str>,
    success_url: &str,
    cancel_url: &str,
) -> Vec<(String, String)> {
    let mut form: Vec<(String, String)> = vec![
        ("mode".into(), "subscription".into()),
        ("success_url".into(), success_url.into()),
        ("cancel_url".into(), cancel_url.into()),
        ("client_reference_id".into(), member_id.into()),
        ("metadata[member_id]".into(), member_id.into()),
        ("metadata[plan_id]".into(), plan.id.clone()),
        (
            "subscription_data[metadata][member_id]".into(),
            member_id.into(),
        ),
        (
            "subscription_data[metadata][plan_id]".into(),
            plan.id.clone(),
        ),
        ("line_items[0][quantity]".into(), "1".into()),
        (
            "line_items[0][price_data][currency]".into(),
            currency.into(),
        ),
        (
            "line_items[0][price_data][unit_amount]".into(),
            plan.amount.to_string(),
        ),
        (
            "line_items[0][price_data][recurring][interval]".into(),
            plan.interval.as_str().into(),
        ),
        (
            "line_items[0][price_data][recurring][interval_count]".into(),
            plan.interval_count.to_string(),
        ),
        (
            "line_items[0][price_data][product_data][name]".into(),
            plan.display_label().into(),
        ),
    ];
    match customer {
        // 以前の契約と同じ Stripe の顧客に結びつける（カードや請求の履歴がつながる）
        Some(customer) => form.push(("customer".into(), customer.into())),
        None => form.push(("customer_email".into(), email.into())),
    }
    form
}

/// 契約を始める: 会員とプランを確かめ、Checkout Session を作って、決済ページの URL を返す
async fn start_subscription(
    state: &AppState,
    headers: &HeaderMap,
    plan_id: &str,
) -> Result<String, ApiError> {
    ensure_enabled(state)?;
    let session = resolve_member_session(state, headers).await?;
    let service = state.members.as_ref().ok_or_else(|| {
        api_error(
            StatusCode::NOT_FOUND,
            "Member accounts are not enabled",
            "purchase_disabled",
        )
    })?;
    let member = service
        .get_profile(&session.member_id)
        .await
        .map_err(internal)?;
    if member.status != "active" {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            "This account is suspended",
            "member_suspended",
        ));
    }
    let config = &state.config.members.purchase;
    let plan = config
        .plan(plan_id)
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Unknown plan", "unknown_plan"))?;

    let existing = state
        .db
        .list_member_subscriptions(&member.id)
        .await
        .map_err(internal)?;
    let now = chrono::Utc::now();
    if existing.iter().any(|s| grants_access(s, now)) {
        return Err(api_error(
            StatusCode::CONFLICT,
            "You already have a subscription",
            "already_subscribed",
        ));
    }
    let customer = existing
        .iter()
        .map(|s| s.stripe_customer_id.as_str())
        .find(|c| !c.is_empty());

    let base = state.config.members.public_url.trim().trim_end_matches('/');
    let form = subscription_form(
        plan,
        &config.currency,
        &member.id,
        &member.email,
        customer,
        &format!("{base}/account?subscribed=1"),
        &format!("{base}/account"),
    );
    let checkout = stripe_client(state)?
        .create_checkout_session(&form, &format!("aegis-subscribe-{}", uuid::Uuid::new_v4()))
        .await
        .map_err(|e| {
            tracing::error!("Stripe checkout failed: {}", e);
            api_error(
                StatusCode::BAD_GATEWAY,
                "The payment service could not start the payment",
                "payment_start_failed",
            )
        })?;
    Ok(checkout.url)
}

/// `POST /api/v1/members/subscriptions` — 契約を始める。`{ "redirect_url" }` を返す
pub async fn create_subscription(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<StartSubscriptionRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let url = start_subscription(&state, &headers, &request.plan_id).await?;
    Ok((StatusCode::CREATED, Json(json!({ "redirect_url": url }))))
}

#[derive(Debug, Deserialize)]
pub struct SubscribeQuery {
    pub plan: String,
}

/// `GET /account/subscribe?plan=..` — 契約を始めて、決済ページへ移動する（購入のリンクと同じ理由で GET）。
/// ログインしていなければ、ログインのあとにここへ戻る
pub async fn subscribe_page(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<SubscribeQuery>,
) -> Response {
    match start_subscription(&state, &headers, &query.plan).await {
        Ok(url) => Redirect::to(&url).into_response(),
        Err((StatusCode::UNAUTHORIZED, _)) => login_then(&format!(
            "/account/subscribe?plan={}",
            crate::handlers::purchase::encode_component(&query.plan)
        )),
        Err(e) => e.into_response(),
    }
}

fn login_then(back: &str) -> Response {
    Redirect::to(&format!(
        "/account/login?next={}",
        crate::handlers::purchase::encode_component(back)
    ))
    .into_response()
}

/// Customer Portal の URL（解約・カード変更・請求履歴を Stripe の画面で行う）
async fn billing_portal_url(state: &AppState, headers: &HeaderMap) -> Result<String, ApiError> {
    ensure_enabled(state)?;
    let session = resolve_member_session(state, headers).await?;
    let subscriptions = state
        .db
        .list_member_subscriptions(&session.member_id)
        .await
        .map_err(internal)?;
    let customer = subscriptions
        .iter()
        .map(|s| s.stripe_customer_id.as_str())
        .find(|c| !c.is_empty())
        .ok_or_else(|| {
            api_error(
                StatusCode::NOT_FOUND,
                "You have no subscription",
                "no_subscription",
            )
        })?;
    let base = state.config.members.public_url.trim().trim_end_matches('/');
    let form = vec![
        ("customer".to_string(), customer.to_string()),
        ("return_url".to_string(), format!("{base}/account")),
    ];
    let portal = stripe_client(state)?
        .post_form(
            "/v1/billing_portal/sessions",
            &form,
            &format!("aegis-portal-{}", uuid::Uuid::new_v4()),
        )
        .await
        .map_err(|e| {
            tracing::error!("Stripe portal failed: {}", e);
            api_error(
                StatusCode::BAD_GATEWAY,
                "The payment service could not open the billing page",
                "billing_portal_failed",
            )
        })?;
    portal["url"].as_str().map(str::to_string).ok_or_else(|| {
        api_error(
            StatusCode::BAD_GATEWAY,
            "The payment service returned no billing page",
            "billing_portal_failed",
        )
    })
}

/// `POST /api/v1/members/billing-portal` — Customer Portal の URL を返す
pub async fn billing_portal(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let url = billing_portal_url(&state, &headers).await?;
    Ok(Json(json!({ "url": url })))
}

/// `GET /account/billing` — Customer Portal へ移動する
pub async fn billing_page(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    match billing_portal_url(&state, &headers).await {
        Ok(url) => Redirect::to(&url).into_response(),
        Err((StatusCode::UNAUTHORIZED, _)) => login_then("/account/billing"),
        Err(e) => e.into_response(),
    }
}

/// Stripe の契約の状態を、写しの状態にする。決められない状態（支払い待ちの `incomplete` など）は `None`
fn map_status(stripe_status: &str) -> Option<&'static str> {
    match stripe_status {
        "active" | "trialing" => Some(SUBSCRIPTION_ACTIVE),
        "past_due" | "unpaid" => Some(SUBSCRIPTION_PAST_DUE),
        "canceled" | "incomplete_expired" => Some(SUBSCRIPTION_CANCELLED),
        _ => None,
    }
}

/// 契約の期間の終わり。新しい API は契約の明細（`items`）に持つ
fn period_end(subscription: &Value) -> Option<String> {
    let ts = subscription["current_period_end"]
        .as_i64()
        .or_else(|| subscription["items"]["data"][0]["current_period_end"].as_i64())?;
    chrono::DateTime::from_timestamp(ts, 0).map(|t| t.to_rfc3339())
}

async fn record(
    state: &AppState,
    audit: &AuditContext,
    action: AuditAction,
    outcome: AuditOutcome,
    subscription: &SubscriptionRecord,
    details: Value,
) {
    state
        .audit
        .record(
            audit
                .event(action, outcome)
                .with_resource("subscription", Some(subscription.id.clone()))
                .with_details(details),
        )
        .await;
}

/// Stripe の通知がサブスクのものなら反映して `true`（購入の Webhook はその先を飛ばす）
pub async fn apply_event(
    state: &AppState,
    audit: &AuditContext,
    event: &Value,
) -> Result<bool, ApiError> {
    let event_type = event["type"].as_str().unwrap_or_default();
    let object = &event["data"]["object"];
    let event_id = event["id"].as_str().unwrap_or_default();
    match event_type {
        "checkout.session.completed" if object["mode"].as_str() == Some("subscription") => {
            start_from_checkout(state, audit, object, event_id).await?;
            Ok(true)
        }
        "customer.subscription.updated" | "customer.subscription.deleted" => {
            sync_subscription(state, audit, object, event_type, event_id).await?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// 決済ページを終えた契約を記録する。金額・通貨がプランと合うときだけ。契約の状態は Stripe から引き直す
async fn start_from_checkout(
    state: &AppState,
    audit: &AuditContext,
    session: &Value,
    event_id: &str,
) -> Result<(), ApiError> {
    let config = &state.config.members.purchase;
    let (Some(stripe_id), Some(member_id)) = (
        session["subscription"].as_str(),
        session["client_reference_id"].as_str(),
    ) else {
        return Ok(());
    };
    let plan_id = session["metadata"]["plan_id"].as_str().unwrap_or_default();
    let problem = |problem: &str| {
        json!({
            "problem": problem,
            "stripe_subscription_id": stripe_id,
            "member_id": member_id,
            "plan_id": plan_id,
            "event_id": event_id,
        })
    };
    let placeholder = SubscriptionRecord {
        id: stripe_id.to_string(),
        ..SubscriptionRecord::default()
    };

    let Some(plan) = config.plan(plan_id) else {
        record(
            state,
            audit,
            AuditAction::PurchaseUpdate,
            AuditOutcome::Failure,
            &placeholder,
            problem("unknown_plan"),
        )
        .await;
        return Ok(());
    };
    let amount = session["amount_total"].as_i64().unwrap_or(-1);
    let currency = session["currency"]
        .as_str()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if amount != plan.amount || currency != config.currency.to_ascii_lowercase() {
        record(
            state,
            audit,
            AuditAction::PurchaseUpdate,
            AuditOutcome::Failure,
            &placeholder,
            problem("amount_mismatch"),
        )
        .await;
        return Ok(());
    }
    if state
        .db
        .get_member_by_id(member_id)
        .await
        .map_err(internal)?
        .is_none()
    {
        record(
            state,
            audit,
            AuditAction::PurchaseUpdate,
            AuditOutcome::Failure,
            &placeholder,
            problem("unknown_member"),
        )
        .await;
        return Ok(());
    }

    // 期間の終わりと状態は、Stripe に聞く（この通知にも、更新の通知にも、順序の保証がない）
    let remote = stripe_client(state)?
        .get(&format!("/v1/subscriptions/{stripe_id}"))
        .await
        .map_err(internal)?;
    let (Some(status), Some(end)) = (
        remote["status"].as_str().and_then(map_status),
        period_end(&remote),
    ) else {
        return Ok(());
    };
    let now = chrono::Utc::now().to_rfc3339();
    let customer = session["customer"].as_str().unwrap_or_default().to_string();
    let cancel_at_period_end = remote["cancel_at_period_end"].as_bool().unwrap_or(false);

    if let Some(mut existing) = state
        .db
        .get_subscription_by_stripe_id(stripe_id)
        .await
        .map_err(internal)?
    {
        // 通知の再送。状態だけ合わせる
        existing.status = status.to_string();
        existing.current_period_end = end;
        existing.cancel_at_period_end = cancel_at_period_end;
        existing.updated_at = now;
        state
            .db
            .update_subscription(&existing)
            .await
            .map_err(internal)?;
        return Ok(());
    }
    let created = SubscriptionRecord {
        id: uuid::Uuid::new_v4().to_string(),
        member_id: Some(member_id.to_string()),
        plan_id: plan.id.clone(),
        level: plan.level.clone(),
        status: status.to_string(),
        stripe_customer_id: customer,
        stripe_subscription_id: stripe_id.to_string(),
        current_period_end: end,
        cancel_at_period_end,
        created_at: now.clone(),
        updated_at: now,
    };
    state
        .db
        .insert_subscription(&created)
        .await
        .map_err(internal)?;
    record(
        state,
        audit,
        AuditAction::PurchaseCreate,
        AuditOutcome::Success,
        &created,
        json!({
            "member_id": member_id,
            "plan_id": created.plan_id,
            "level": created.level,
            "status": created.status,
            "event_id": event_id,
        }),
    )
    .await;
    Ok(())
}

/// 契約の更新・解約の通知を写しに反映する。知らない契約は何もしない（開始の通知が先に来る）
async fn sync_subscription(
    state: &AppState,
    audit: &AuditContext,
    remote: &Value,
    event_type: &str,
    event_id: &str,
) -> Result<(), ApiError> {
    let Some(stripe_id) = remote["id"].as_str() else {
        return Ok(());
    };
    let Some(mut record_now) = state
        .db
        .get_subscription_by_stripe_id(stripe_id)
        .await
        .map_err(internal)?
    else {
        return Ok(());
    };
    let status = if event_type == "customer.subscription.deleted" {
        Some(SUBSCRIPTION_CANCELLED)
    } else {
        remote["status"].as_str().and_then(map_status)
    };
    let before = record_now.clone();
    if let Some(status) = status {
        record_now.status = status.to_string();
    }
    if let Some(end) = period_end(remote) {
        record_now.current_period_end = end;
    }
    if let Some(flag) = remote["cancel_at_period_end"].as_bool() {
        record_now.cancel_at_period_end = flag;
    }
    if let Some(customer) = remote["customer"].as_str() {
        record_now.stripe_customer_id = customer.to_string();
    }
    if record_now == before {
        return Ok(());
    }
    record_now.updated_at = chrono::Utc::now().to_rfc3339();
    state
        .db
        .update_subscription(&record_now)
        .await
        .map_err(internal)?;
    record(
        state,
        audit,
        AuditAction::PurchaseUpdate,
        AuditOutcome::Success,
        &record_now,
        json!({
            "from": before.status,
            "to": record_now.status,
            "cancel_at_period_end": record_now.cancel_at_period_end,
            "source": "payment_webhook",
            "event_id": event_id,
        }),
    )
    .await;
    Ok(())
}

/// 契約を Stripe で即時に解約し、写しも解約にする。解約できなかったら `Err`
pub async fn cancel_now(
    state: &AppState,
    audit: &AuditContext,
    subscription: &SubscriptionRecord,
    source: &str,
) -> Result<(), ApiError> {
    if subscription.status == SUBSCRIPTION_CANCELLED {
        return Ok(());
    }
    let result = stripe_client(state)?
        .delete(&format!(
            "/v1/subscriptions/{}",
            subscription.stripe_subscription_id
        ))
        .await;
    if let Err(e) = result {
        tracing::error!("Stripe cancel failed: {}", e);
        return Err(api_error(
            StatusCode::BAD_GATEWAY,
            "The payment service could not cancel the subscription",
            "cancel_failed",
        ));
    }
    let mut updated = subscription.clone();
    updated.status = SUBSCRIPTION_CANCELLED.to_string();
    updated.updated_at = chrono::Utc::now().to_rfc3339();
    state
        .db
        .update_subscription(&updated)
        .await
        .map_err(internal)?;
    record(
        state,
        audit,
        AuditAction::PurchaseUpdate,
        AuditOutcome::Success,
        &updated,
        json!({
            "from": subscription.status,
            "to": updated.status,
            "source": source,
        }),
    )
    .await;
    Ok(())
}

/// 退会する会員の契約を、すべて解約する（課金が続かないように。解約できなければ退会を止める）
pub async fn cancel_member_subscriptions(
    state: &AppState,
    audit: &AuditContext,
    member_id: &str,
) -> Result<(), ApiError> {
    if !crate::handlers::access::purchase_enabled(state) {
        return Ok(());
    }
    let subscriptions = state
        .db
        .list_member_subscriptions(member_id)
        .await
        .map_err(internal)?;
    for subscription in &subscriptions {
        cancel_now(state, audit, subscription, "member_deleted").await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(status: &str, end: &str) -> SubscriptionRecord {
        SubscriptionRecord {
            member_id: Some("m1".to_string()),
            status: status.to_string(),
            current_period_end: end.to_string(),
            ..SubscriptionRecord::default()
        }
    }

    #[test]
    fn access_follows_the_status_and_the_period() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-10T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let future = "2026-12-01T00:00:00Z";
        assert!(grants_access(&record("active", future), now));
        assert!(grants_access(&record("past_due", future), now));
        assert!(!grants_access(&record("cancelled", future), now));
        // 期間を過ぎても、猶予の間は契約中
        assert!(grants_access(
            &record("active", "2026-11-08T00:00:00Z"),
            now
        ));
        assert!(!grants_access(
            &record("active", "2026-11-01T00:00:00Z"),
            now
        ));
        assert!(!grants_access(&record("active", "garbage"), now));
        // 退会した会員の契約は、権利を与えない
        let mut detached = record("active", future);
        detached.member_id = None;
        assert!(!grants_access(&detached, now));
    }

    #[test]
    fn stripe_states_map_to_ours() {
        assert_eq!(map_status("trialing"), Some("active"));
        assert_eq!(map_status("unpaid"), Some("past_due"));
        assert_eq!(map_status("canceled"), Some("cancelled"));
        assert_eq!(map_status("incomplete"), None);
    }

    #[test]
    fn the_period_end_is_read_from_either_api_shape() {
        let old = json!({ "current_period_end": 1_800_000_000 });
        let new = json!({ "items": { "data": [{ "current_period_end": 1_800_000_000 }] } });
        assert_eq!(period_end(&old), period_end(&new));
        assert!(period_end(&old).is_some());
        assert!(period_end(&json!({})).is_none());
    }
}
