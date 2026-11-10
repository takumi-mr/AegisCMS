//! 記事・ページの単独購入（docs/members.md 16）。
//!
//! - `POST /api/v1/members/purchases` — 購入を始める（API のクライアント向け。決済ページの URL を返す）
//! - `GET /account/purchase` — 購入を始めて、決済ページへ移動する（案内のリンク。ブラウザ向け）
//! - `GET /api/v1/members/purchases` — 自分の購入
//! - `POST /api/v1/members/webhooks/stripe` — Stripe からの入金・返金の通知（署名を確かめる）
//!
//! 金額と通貨は、記事の `purchase_price` と `[members.purchase].currency` だけから作る。
//! リクエストの値は使わない。権利は、入金の通知で金額・通貨が合ったときだけ付く。

use axum::{
    body::Bytes,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::audit::AuditContext;
use crate::auth::authorize::RequireRole;
use crate::auth::AdminSession;
use crate::handlers::access::purchase_enabled;
use crate::handlers::content::is_currently_published;
use crate::handlers::members::resolve_member_session;
use crate::state::AppState;
use aegis_commerce::{verify_webhook_event, StripeClient};
use aegis_core::{AuditAction, AuditOutcome};
use aegis_db::{
    PurchaseRecord, PURCHASE_CANCELLED, PURCHASE_PAID, PURCHASE_PENDING, PURCHASE_REFUNDED,
    PURCHASE_REVOKED,
};
use aegis_members::Viewer;

type ApiError = (StatusCode, Json<Value>);

pub(crate) fn api_error(status: StatusCode, message: impl Into<String>, code: &str) -> ApiError {
    (
        status,
        Json(json!({ "error": message.into(), "code": code })),
    )
}

pub(crate) fn internal(e: impl std::fmt::Display) -> ApiError {
    tracing::error!("Purchase error: {}", e);
    api_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Internal server error",
        "internal",
    )
}

/// 購入が無効なサイトでは、API も画面も 404
pub(crate) fn ensure_enabled(state: &AppState) -> Result<(), ApiError> {
    if purchase_enabled(state) {
        Ok(())
    } else {
        Err(api_error(
            StatusCode::NOT_FOUND,
            "Purchases are not enabled",
            "purchase_disabled",
        ))
    }
}

/// Stripe のクライアント（`[members.purchase.stripe]` のキー）
pub(crate) fn stripe_client(state: &AppState) -> Result<StripeClient, ApiError> {
    let config = &state.config.members.purchase.stripe;
    StripeClient::new(&config.secret_key(), &config.api_base).map_err(internal)
}

/// 購入を始めるリクエスト
#[derive(Debug, Deserialize)]
pub struct StartPurchaseRequest {
    pub content_type: String,
    pub slug: String,
}

/// 決済ページを作った購入
struct Started {
    purchase: PurchaseRecord,
    redirect_url: String,
}

/// Checkout Session の作成パラメータ（`application/x-www-form-urlencoded`）。金額・通貨は引数の値だけ
fn checkout_form(
    purchase_id: &str,
    title: &str,
    amount: i64,
    currency: &str,
    email: &str,
    success_url: &str,
    cancel_url: &str,
) -> Vec<(String, String)> {
    vec![
        ("mode".into(), "payment".into()),
        ("success_url".into(), success_url.into()),
        ("cancel_url".into(), cancel_url.into()),
        ("client_reference_id".into(), purchase_id.into()),
        ("customer_email".into(), email.into()),
        ("metadata[purchase_id]".into(), purchase_id.into()),
        (
            "payment_intent_data[metadata][purchase_id]".into(),
            purchase_id.into(),
        ),
        ("line_items[0][quantity]".into(), "1".into()),
        (
            "line_items[0][price_data][currency]".into(),
            currency.into(),
        ),
        (
            "line_items[0][price_data][unit_amount]".into(),
            amount.to_string(),
        ),
        (
            "line_items[0][price_data][product_data][name]".into(),
            title.into(),
        ),
    ]
}

/// 購入を始める: 記事・会員を確かめ、Checkout Session を作って、`pending` の購入を記録する
async fn start_purchase(
    state: &AppState,
    audit: &AuditContext,
    headers: &HeaderMap,
    request: &StartPurchaseRequest,
) -> Result<Started, ApiError> {
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

    let not_for_sale = || {
        api_error(
            StatusCode::NOT_FOUND,
            "This content is not for sale",
            "not_for_sale",
        )
    };
    let schema = state
        .schemas
        .read()
        .await
        .get(&request.content_type)
        .cloned()
        .ok_or_else(not_for_sale)?;
    let record = state
        .db
        .get_published_content(&schema, &request.slug)
        .await
        .map_err(internal)?
        .filter(|r| {
            is_currently_published(&r.status, r.unpublish_at.as_deref(), chrono::Utc::now())
        })
        .ok_or_else(not_for_sale)?;
    // 単独購入は、閲覧が制限されていて、価格のある記事だけ
    let price = aegis_db::purchase_price_of(&record.custom_fields).ok_or_else(not_for_sale)?;
    if !record.is_restricted() {
        return Err(not_for_sale());
    }

    // すでに読める（レベルが足りている・買っている）なら、買わせない
    let required = aegis_db::access_of(&record.custom_fields);
    let viewer = Viewer::Member {
        level: &member.level,
    };
    if aegis_members::may_view(&state.config.members.levels, required, viewer) {
        return Err(api_error(
            StatusCode::CONFLICT,
            "You already have access to this content",
            "already_accessible",
        ));
    }
    if state
        .db
        .has_paid_purchase(&member.id, &record.content_type, &record.id)
        .await
        .map_err(internal)?
    {
        return Err(api_error(
            StatusCode::CONFLICT,
            "You have already purchased this content",
            "already_purchased",
        ));
    }

    let config = &state.config.members.purchase;
    let path =
        crate::handlers::public::public_content_path(state, &record.content_type, &record.slug);
    let base = state.config.members.public_url.trim().trim_end_matches('/');
    let cancel_url = format!("{base}{path}");
    let success_url = format!("{cancel_url}?purchased=1");

    let purchase_id = uuid::Uuid::new_v4().to_string();
    let form = checkout_form(
        &purchase_id,
        &record.title,
        price,
        &config.currency,
        &member.email,
        &success_url,
        &cancel_url,
    );
    let checkout = stripe_client(state)?
        .create_checkout_session(&form, &format!("aegis-purchase-{purchase_id}"))
        .await
        .map_err(|e| {
            tracing::error!("Stripe checkout failed: {}", e);
            api_error(
                StatusCode::BAD_GATEWAY,
                "The payment service could not start the payment",
                "payment_start_failed",
            )
        })?;

    let now = chrono::Utc::now().to_rfc3339();
    let purchase = PurchaseRecord {
        id: purchase_id,
        member_id: Some(member.id.clone()),
        content_type: record.content_type.clone(),
        content_id: record.id.clone(),
        amount: price,
        currency: config.currency.clone(),
        status: PURCHASE_PENDING.to_string(),
        provider_ref: checkout.id,
        transaction_id: None,
        created_at: now.clone(),
        paid_at: None,
        updated_at: now,
    };
    state
        .db
        .insert_purchase(&purchase)
        .await
        .map_err(internal)?;
    state
        .audit
        .record(
            audit
                .event(AuditAction::PurchaseCreate, AuditOutcome::Success)
                .with_resource("purchase", Some(purchase.id.clone()))
                .with_details(json!({
                    "member_id": member.id,
                    "content_type": purchase.content_type,
                    "content_id": purchase.content_id,
                    "amount": purchase.amount,
                    "currency": purchase.currency,
                })),
        )
        .await;
    Ok(Started {
        purchase,
        redirect_url: checkout.url,
    })
}

/// `POST /api/v1/members/purchases` — 購入を始める。`{ "purchase_id", "redirect_url" }` を返す
pub async fn create_purchase(
    State(state): State<Arc<AppState>>,
    audit: AuditContext,
    headers: HeaderMap,
    Json(request): Json<StartPurchaseRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let started = start_purchase(&state, &audit, &headers, &request).await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "purchase_id": started.purchase.id,
            "redirect_url": started.redirect_url,
        })),
    ))
}

/// `GET /account/purchase?content_type=..&slug=..` — 購入を始めて、決済ページへ移動する。
/// 案内の「購入」のリンクから来る（`form-action 'self'` の CSP では、フォームの送信先から外部の
/// 決済ページへのリダイレクトが止まるため、リンクにしている）。セッションの Cookie は `SameSite=Lax` で、
/// 他のサイトからのリンクでも付くが、決済ページを作るだけで、入金されなければ何も起きない。
/// ログインしていなければ、ログインのあとにここへ戻す
pub async fn start_purchase_page(
    State(state): State<Arc<AppState>>,
    audit: AuditContext,
    headers: HeaderMap,
    Query(request): Query<StartPurchaseRequest>,
) -> Response {
    if let Err(e) = ensure_enabled(&state) {
        return e.into_response();
    }
    match start_purchase(&state, &audit, &headers, &request).await {
        Ok(started) => Redirect::to(&started.redirect_url).into_response(),
        Err((StatusCode::UNAUTHORIZED, _)) => {
            let back = format!(
                "/account/purchase?content_type={}&slug={}",
                encode_component(&request.content_type),
                encode_component(&request.slug)
            );
            Redirect::to(&format!("/account/login?next={}", encode_component(&back)))
                .into_response()
        }
        Err(e) => e.into_response(),
    }
}

/// クエリの値のパーセントエンコード
pub(crate) fn encode_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// 購入の一覧の表示用（記事が消えていても、購入の記録は返す。タイトルと URL だけ空になる）
pub async fn purchase_views(state: &AppState, purchases: Vec<PurchaseRecord>) -> Vec<Value> {
    let schemas = state.schemas.read().await.clone();
    let mut out = Vec::with_capacity(purchases.len());
    for purchase in purchases {
        let content = match schemas.get(&purchase.content_type) {
            Some(schema) => state
                .db
                .get_content(schema, &purchase.content_id)
                .await
                .ok()
                .flatten(),
            None => None,
        };
        let (title, url) = match &content {
            Some(c) => (
                Some(c.title.clone()),
                Some(crate::handlers::public::public_content_path(
                    state,
                    &c.content_type,
                    &c.slug,
                )),
            ),
            None => (None, None),
        };
        out.push(json!({
            "id": purchase.id,
            "content_type": purchase.content_type,
            "content_id": purchase.content_id,
            "title": title,
            "url": url,
            "amount": purchase.amount,
            "currency": purchase.currency,
            "status": purchase.status,
            "created_at": purchase.created_at,
            "paid_at": purchase.paid_at,
            "manual": purchase.provider_ref.starts_with(MANUAL_REF_PREFIX),
        }));
    }
    out
}

/// 管理者が手で付けた権利の `provider_ref` の接頭辞（Stripe の決済ではない）
pub const MANUAL_REF_PREFIX: &str = "manual:";

/// `GET /api/v1/members/purchases` — ログイン会員の購入（新しい順）
pub async fn list_my_purchases(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    ensure_enabled(&state)?;
    let session = resolve_member_session(&state, &headers).await?;
    let purchases = state
        .db
        .list_member_purchases(&session.member_id)
        .await
        .map_err(internal)?;
    Ok(Json(
        json!({ "purchases": purchase_views(&state, purchases).await }),
    ))
}

/// `POST /api/v1/members/webhooks/stripe` — Stripe からの通知（署名を確かめる）
pub async fn stripe_webhook(
    State(state): State<Arc<AppState>>,
    audit: AuditContext,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    ensure_enabled(&state)?;
    let secret = state.config.members.purchase.stripe.webhook_secret();
    let event = match verify_webhook_event(&secret, &headers, &body) {
        Ok(event) => event,
        Err(e) => {
            state
                .audit
                .record(
                    audit
                        .event(AuditAction::AccessDenied, AuditOutcome::Denied)
                        .with_resource(
                            "endpoint",
                            Some("POST /api/v1/members/webhooks/stripe".to_string()),
                        )
                        .with_details(json!({
                            "reason": "invalid_webhook_signature",
                            "error": e.to_string(),
                        })),
                )
                .await;
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                e.to_string(),
                "invalid_signature",
            ));
        }
    };
    apply_event(&state, &audit, &event).await?;
    Ok(Json(json!({ "received": true })))
}

/// 通知を購入に反映する。購入に関係しない通知・知らない購入は何もしない（`200` で受け取り、再送を止める）
async fn apply_event(
    state: &AppState,
    audit: &AuditContext,
    event: &Value,
) -> Result<(), ApiError> {
    let event_type = event["type"].as_str().unwrap_or_default();
    let object = &event["data"]["object"];
    let event_id = event["id"].as_str().unwrap_or_default();

    // サブスクの通知は、そちらで扱う
    if crate::handlers::subscription::apply_event(state, audit, event).await? {
        return Ok(());
    }

    match event_type {
        "checkout.session.completed" | "checkout.session.async_payment_succeeded" => {
            let Some(purchase) = purchase_of_session(state, object).await? else {
                return Ok(());
            };
            // コンビニ払いなど、支払いが後になるものは、成功の通知を待つ
            let paid = matches!(
                object["payment_status"].as_str(),
                Some("paid") | Some("no_payment_required")
            );
            if event_type == "checkout.session.completed" && !paid {
                return Ok(());
            }
            mark_paid(state, audit, &purchase, object, event_id).await?;
            Ok(())
        }
        "checkout.session.expired" | "checkout.session.async_payment_failed" => {
            let Some(purchase) = purchase_of_session(state, object).await? else {
                return Ok(());
            };
            transition(
                state,
                audit,
                &purchase,
                PURCHASE_CANCELLED,
                None,
                None,
                event_id,
                None,
            )
            .await?;
            Ok(())
        }
        "charge.refunded" => {
            // 全額の返金だけ権利を外す（一部返金は残す）
            if object["refunded"].as_bool() != Some(true) {
                return Ok(());
            }
            let Some(transaction_id) = object["payment_intent"].as_str() else {
                return Ok(());
            };
            let Some(purchase) = state
                .db
                .get_purchase_by_transaction(transaction_id)
                .await
                .map_err(internal)?
            else {
                return Ok(());
            };
            transition(
                state,
                audit,
                &purchase,
                PURCHASE_REFUNDED,
                None,
                None,
                event_id,
                None,
            )
            .await?;
            Ok(())
        }
        _ => Ok(()),
    }
}

/// 通知の Checkout Session に対応する購入。知らないセッション（ショップの注文など）は `None`
async fn purchase_of_session(
    state: &AppState,
    session: &Value,
) -> Result<Option<PurchaseRecord>, ApiError> {
    let Some(session_id) = session["id"].as_str() else {
        return Ok(None);
    };
    state
        .db
        .get_purchase_by_ref(session_id)
        .await
        .map_err(internal)
}

/// 入金された購入を `paid` にする。金額と通貨が購入の記録と合わなければ権利を付けない
async fn mark_paid(
    state: &AppState,
    audit: &AuditContext,
    purchase: &PurchaseRecord,
    session: &Value,
    event_id: &str,
) -> Result<bool, ApiError> {
    let amount = session["amount_total"].as_i64().unwrap_or(-1);
    let currency = session["currency"]
        .as_str()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if amount != purchase.amount || currency != purchase.currency.to_ascii_lowercase() {
        record_update(
            state,
            audit,
            purchase,
            AuditOutcome::Failure,
            json!({
                "problem": "amount_mismatch",
                "expected_amount": purchase.amount,
                "expected_currency": purchase.currency,
                "amount": amount,
                "currency": currency,
                "event_id": event_id,
            }),
        )
        .await;
        return Ok(false);
    }

    // 同じ会員が同じ記事を二重に払った（2 つの決済ページを開いた）か
    let duplicate = match &purchase.member_id {
        Some(member_id) => state
            .db
            .has_paid_purchase(member_id, &purchase.content_type, &purchase.content_id)
            .await
            .map_err(internal)?,
        None => false,
    };
    let now = chrono::Utc::now().to_rfc3339();
    transition(
        state,
        audit,
        purchase,
        PURCHASE_PAID,
        Some(&now),
        session["payment_intent"].as_str(),
        event_id,
        duplicate.then_some("duplicate_payment"),
    )
    .await
}

/// 購入の状態を変える（条件付きの更新。再送で二重に動かない）。変えたときだけ監査ログに残す
#[allow(clippy::too_many_arguments)]
pub async fn transition(
    state: &AppState,
    audit: &AuditContext,
    purchase: &PurchaseRecord,
    to: &str,
    paid_at: Option<&str>,
    transaction_id: Option<&str>,
    event_id: &str,
    problem: Option<&str>,
) -> Result<bool, ApiError> {
    let from: &[&str] = match to {
        PURCHASE_PAID | PURCHASE_CANCELLED => &[PURCHASE_PENDING],
        PURCHASE_REFUNDED | PURCHASE_REVOKED => &[PURCHASE_PAID],
        _ => &[],
    };
    // 管理者の操作は `event_id` が空で、監査ログの操作主体（`audit`）が残る
    let source = if event_id.is_empty() {
        "admin"
    } else {
        "payment_webhook"
    };
    let now = chrono::Utc::now().to_rfc3339();
    let changed = state
        .db
        .transition_purchase(&purchase.id, from, to, paid_at, transaction_id, &now)
        .await
        .map_err(internal)?;
    if changed {
        let mut details = json!({
            "from": purchase.status,
            "to": to,
            "source": source,
            "event_id": event_id,
        });
        if let Some(problem) = problem {
            details["problem"] = json!(problem);
        }
        record_update(state, audit, purchase, AuditOutcome::Success, details).await;
    }
    Ok(changed)
}

async fn record_update(
    state: &AppState,
    audit: &AuditContext,
    purchase: &PurchaseRecord,
    outcome: AuditOutcome,
    details: Value,
) {
    state
        .audit
        .record(
            audit
                .event(AuditAction::PurchaseUpdate, outcome)
                .with_resource("purchase", Some(purchase.id.clone()))
                .with_details(details),
        )
        .await;
}

// --- 管理者向け（会員の詳細から。docs/members.md 16.7） ---

#[derive(Debug, Deserialize)]
pub struct GrantPurchaseRequest {
    pub content_type: String,
    pub slug: String,
}

/// `POST /api/v1/admin/members/{id}/purchases` — 管理者: 会員に記事の閲覧の権利を無料で付ける
/// （購入と同じ扱いの記録。金額は 0、決済は無い）。有料の付与にあたるので、`admin` だけ
pub async fn admin_grant_purchase(
    RequireRole(session): AdminSession,
    State(state): State<Arc<AppState>>,
    audit: AuditContext,
    axum::extract::Path(member_id): axum::extract::Path<String>,
    Json(request): Json<GrantPurchaseRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    ensure_enabled(&state)?;
    let not_found = |what: &str| {
        api_error(
            StatusCode::NOT_FOUND,
            format!("{what} not found"),
            "not_found",
        )
    };
    state
        .db
        .get_member_by_id(&member_id)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("Member"))?;
    let schema = state
        .schemas
        .read()
        .await
        .get(&request.content_type)
        .cloned()
        .ok_or_else(|| not_found("Content"))?;
    let record = state
        .db
        .get_published_content(&schema, &request.slug)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("Content"))?;
    if !record.is_restricted() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "This content is not restricted, so there is nothing to grant",
            "not_restricted",
        ));
    }
    if state
        .db
        .has_paid_purchase(&member_id, &record.content_type, &record.id)
        .await
        .map_err(internal)?
    {
        return Err(api_error(
            StatusCode::CONFLICT,
            "The member already has access through a purchase",
            "already_purchased",
        ));
    }
    let now = chrono::Utc::now().to_rfc3339();
    let purchase = PurchaseRecord {
        id: uuid::Uuid::new_v4().to_string(),
        member_id: Some(member_id.clone()),
        content_type: record.content_type.clone(),
        content_id: record.id.clone(),
        amount: 0,
        currency: state.config.members.purchase.currency.clone(),
        status: PURCHASE_PAID.to_string(),
        provider_ref: format!("{MANUAL_REF_PREFIX}{}", uuid::Uuid::new_v4()),
        transaction_id: None,
        created_at: now.clone(),
        paid_at: Some(now.clone()),
        updated_at: now,
    };
    state
        .db
        .insert_purchase(&purchase)
        .await
        .map_err(internal)?;
    state
        .audit
        .record(
            audit
                .event(AuditAction::PurchaseCreate, AuditOutcome::Success)
                .with_resource("purchase", Some(purchase.id.clone()))
                .with_details(json!({
                    "member_id": member_id,
                    "content_type": purchase.content_type,
                    "content_id": purchase.content_id,
                    "manual": true,
                    "granted_by": session.user_id,
                })),
        )
        .await;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "purchase_id": purchase.id })),
    ))
}

async fn purchase_by_id(state: &AppState, id: &str) -> Result<PurchaseRecord, ApiError> {
    state
        .db
        .get_purchase(id)
        .await
        .map_err(internal)?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Purchase not found", "not_found"))
}

/// `POST /api/v1/admin/purchases/{id}/revoke` — 管理者: 購入の権利を取り消す（返金はしない）
pub async fn admin_revoke_purchase(
    RequireRole(_session): AdminSession,
    State(state): State<Arc<AppState>>,
    audit: AuditContext,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<StatusCode, ApiError> {
    ensure_enabled(&state)?;
    let purchase = purchase_by_id(&state, &id).await?;
    let changed = transition(
        &state,
        &audit,
        &purchase,
        PURCHASE_REVOKED,
        None,
        None,
        "",
        None,
    )
    .await?;
    if changed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(api_error(
            StatusCode::CONFLICT,
            "Only a paid purchase can be revoked",
            "not_paid",
        ))
    }
}

/// `POST /api/v1/admin/purchases/{id}/refund` — 管理者: 全額を返金して、権利を外す（Stripe の返金 API）
pub async fn admin_refund_purchase(
    RequireRole(_session): AdminSession,
    State(state): State<Arc<AppState>>,
    audit: AuditContext,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<StatusCode, ApiError> {
    ensure_enabled(&state)?;
    let purchase = purchase_by_id(&state, &id).await?;
    if purchase.status != PURCHASE_PAID {
        return Err(api_error(
            StatusCode::CONFLICT,
            "Only a paid purchase can be refunded",
            "not_paid",
        ));
    }
    let Some(payment_intent) = purchase
        .transaction_id
        .as_deref()
        .filter(|_| purchase.amount > 0)
    else {
        return Err(api_error(
            StatusCode::CONFLICT,
            "This purchase has no payment to refund (use revoke)",
            "nothing_to_refund",
        ));
    };
    let form = vec![
        ("payment_intent".to_string(), payment_intent.to_string()),
        ("amount".to_string(), purchase.amount.to_string()),
        ("metadata[purchase_id]".to_string(), purchase.id.clone()),
    ];
    stripe_client(&state)?
        .post_form(
            "/v1/refunds",
            &form,
            &format!("aegis-purchase-refund-{}", purchase.id),
        )
        .await
        .map_err(|e| {
            tracing::error!("Stripe refund failed: {}", e);
            api_error(
                StatusCode::BAD_GATEWAY,
                "The payment service could not refund",
                "refund_failed",
            )
        })?;
    // Stripe の返金の通知（`charge.refunded`）も同じ遷移をするが、条件付きなので二重には動かない
    transition(
        &state,
        &audit,
        &purchase,
        PURCHASE_REFUNDED,
        None,
        None,
        "",
        None,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/v1/admin/subscriptions/{id}/cancel` — 管理者: 契約を即時に解約する
pub async fn admin_cancel_subscription(
    RequireRole(_session): AdminSession,
    State(state): State<Arc<AppState>>,
    audit: AuditContext,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<StatusCode, ApiError> {
    ensure_enabled(&state)?;
    let subscription = state
        .db
        .get_subscription(&id)
        .await
        .map_err(internal)?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Subscription not found", "not_found"))?;
    crate::handlers::subscription::cancel_now(&state, &audit, &subscription, "admin").await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkout_form_takes_the_amount_only_from_its_arguments() {
        let form = checkout_form(
            "p1",
            "Title",
            500,
            "jpy",
            "a@example.com",
            "https://x/s",
            "https://x/c",
        );
        let get = |key: &str| {
            form.iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.as_str())
                .unwrap_or_default()
        };
        assert_eq!(get("mode"), "payment");
        assert_eq!(get("client_reference_id"), "p1");
        assert_eq!(get("line_items[0][price_data][unit_amount]"), "500");
        assert_eq!(get("line_items[0][price_data][currency]"), "jpy");
        assert_eq!(get("payment_intent_data[metadata][purchase_id]"), "p1");
    }

    #[test]
    fn encode_component_escapes_reserved_characters() {
        assert_eq!(encode_component("a b&c"), "a%20b%26c");
        assert_eq!(encode_component("abc-_.~"), "abc-_.~");
    }
}
