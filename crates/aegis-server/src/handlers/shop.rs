//! EC の API。公開（ログイン不要）: 見積もり・注文・注文の確認・決済の Webhook。
//! 管理（`editor` 以上）: 注文の一覧・詳細・状態の変更

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use crate::audit::{AuditContext, ClientIp};
use crate::auth::authorize::RequireRole;
use crate::auth::EditorSession;
use crate::commerce::payment::ReturnUrls;
use crate::commerce::{hash_token, order_context, CartRequest, CommerceError, CommerceService};
use crate::db::{OrderListQuery, OrderRecord};
use crate::state::AppState;
use aegis_core::{
    AuditAction, AuditOutcome, OrderAdjustment, OrderLine, OrderStatus, PaymentProviderKind,
    ShippingAddress,
};

type ApiError = (StatusCode, Json<serde_json::Value>);

fn api_error(status: StatusCode, message: impl Into<String>) -> ApiError {
    (status, Json(json!({ "error": message.into() })))
}

fn commerce_error(e: CommerceError) -> ApiError {
    let status = match &e {
        CommerceError::Invalid(_) => StatusCode::BAD_REQUEST,
        CommerceError::NotFound(_) => StatusCode::NOT_FOUND,
        CommerceError::OutOfStock { .. } | CommerceError::Conflict(_) => StatusCode::CONFLICT,
        CommerceError::Rejected(_) => StatusCode::UNPROCESSABLE_ENTITY,
        CommerceError::RateLimited => StatusCode::TOO_MANY_REQUESTS,
        CommerceError::Payment(_) => StatusCode::BAD_GATEWAY,
        CommerceError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    let mut body = json!({ "error": e.to_string() });
    if let CommerceError::OutOfStock { product_id, .. } = &e {
        body["product_id"] = json!(product_id);
    }
    (status, Json(body))
}

fn commerce(state: &AppState) -> Result<Arc<CommerceService>, ApiError> {
    state.commerce.clone().ok_or_else(|| {
        api_error(
            StatusCode::NOT_FOUND,
            "The shop is not enabled ([commerce].enabled)",
        )
    })
}

/// 公開サイトの URL（`[commerce].public_url`、無ければリクエストの Host と `[server].base_path`）
fn site_url(state: &AppState, headers: &HeaderMap) -> String {
    let configured = state
        .config
        .commerce
        .public_url
        .trim()
        .trim_end_matches('/');
    if !configured.is_empty() {
        return configured.to_string();
    }
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.split(',').next().unwrap_or_default().trim().to_string())
            .filter(|v| !v.is_empty())
    };
    // プロキシのヘッダーは信頼できる場合のみ（`[audit].trust_forwarded_for`）
    let trust_proxy = state.audit.trust_forwarded_for();
    let host = trust_proxy
        .then(|| header("x-forwarded-host"))
        .flatten()
        .or_else(|| header("host"))
        .unwrap_or_else(|| format!("{}:{}", state.config.server.host, state.config.server.port));
    let scheme = trust_proxy
        .then(|| header("x-forwarded-proto"))
        .flatten()
        .unwrap_or_else(|| "http".to_string());
    let base_path = state
        .config
        .server
        .normalized_base_path()
        .unwrap_or_default();
    format!("{}://{}{}", scheme, host, base_path)
}

/// 注文確認ページの URL（ベースパス込みの相対 URL）
fn order_page_path(state: &AppState, order_id: &str, token: &str) -> String {
    let base_path = state
        .config
        .server
        .normalized_base_path()
        .unwrap_or_default();
    format!("{}/orders/{}?token={}", base_path, order_id, token)
}

/// 見積もり・顧客に見せる注文
#[derive(Debug, Serialize)]
pub struct QuoteView {
    pub currency: String,
    pub lines: Vec<OrderLine>,
    pub adjustments: Vec<OrderAdjustment>,
    pub subtotal: i64,
    pub total: i64,
}

/// 顧客に見せる注文（注文確認ページ）
#[derive(Debug, Serialize)]
pub struct PublicOrderView {
    pub id: String,
    pub number: String,
    pub status: String,
    pub currency: String,
    pub lines: Vec<OrderLine>,
    pub adjustments: Vec<OrderAdjustment>,
    pub subtotal: i64,
    pub total: i64,
    pub customer_name: String,
    pub shipping_address: Option<ShippingAddress>,
    pub payment_provider: String,
    /// 支払い方法の案内（`manual` の支払い待ちのみ）
    pub payment_instructions: Option<String>,
    /// 支払い待ちの期限
    pub expires_at: Option<String>,
    pub created_at: String,
    pub paid_at: Option<String>,
}

fn public_view(state: &AppState, order: &OrderRecord) -> PublicOrderView {
    let ctx = order_context(order);
    let pending = order.status == OrderStatus::PendingPayment.as_str();
    let instructions = (pending
        && order.payment_provider == "manual"
        && state.config.commerce.provider == PaymentProviderKind::Manual)
        .then(|| state.config.commerce.manual.instructions.clone())
        .filter(|s| !s.trim().is_empty());
    PublicOrderView {
        id: order.id.clone(),
        number: order.number.clone(),
        status: order.status.clone(),
        currency: order.currency.clone(),
        lines: ctx.lines,
        adjustments: ctx.adjustments,
        subtotal: order.subtotal,
        total: order.total,
        customer_name: order.customer_name.clone(),
        shipping_address: ctx.shipping_address,
        payment_provider: order.payment_provider.clone(),
        payment_instructions: instructions,
        expires_at: if pending {
            order.expires_at.clone()
        } else {
            None
        },
        created_at: order.created_at.clone(),
        paid_at: order.paid_at.clone(),
    }
}

/// `GET /api/v1/shop/config` — ショップの公開設定（Admin Studio・テーマのスクリプト向け。秘密情報は含めない）
pub async fn shop_config(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let commerce = &state.config.commerce;
    Json(json!({
        "enabled": state.commerce.is_some(),
        "currency": commerce.currency,
        "product_type": commerce.product_type,
        "provider": state.commerce.as_ref().map(|c| c.provider().id()),
    }))
}

/// `POST /api/v1/shop/quote` — カートの見積もり（送料・税・割引を含む合計）。何も保存しない
pub async fn quote(
    State(state): State<Arc<AppState>>,
    Json(request): Json<CartRequest>,
) -> Result<Json<QuoteView>, ApiError> {
    let commerce = commerce(&state)?;
    let ctx = commerce
        .quote(&state, &request)
        .await
        .map_err(commerce_error)?;
    Ok(Json(QuoteView {
        currency: ctx.currency,
        lines: ctx.lines,
        adjustments: ctx.adjustments,
        subtotal: ctx.subtotal,
        total: ctx.total,
    }))
}

/// `POST /api/v1/shop/checkout` — 注文を作成して在庫を確保し、決済を始める。
/// `redirect_url`（決済ページ）があればそこへ、無ければ `order_url`（注文確認ページ）へ移動する
pub async fn checkout(
    State(state): State<Arc<AppState>>,
    audit: AuditContext,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(request): Json<CartRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let commerce = commerce(&state)?;
    let site = site_url(&state, &headers);
    let result = commerce
        .checkout(
            &state,
            &audit,
            &request,
            client_ip.as_deref(),
            |order, token| ReturnUrls {
                success_url: format!("{}/orders/{}?token={}", site, order.id, token),
                cancel_url: format!("{}/cart", site),
            },
        )
        .await
        .map_err(commerce_error)?;
    let order_url = order_page_path(&state, &result.order.id, &result.access_token);
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "order": public_view(&state, &result.order),
            "token": result.access_token,
            "order_url": order_url,
            "redirect_url": result.redirect_url,
        })),
    ))
}

#[derive(Debug, Deserialize)]
pub struct OrderTokenParams {
    pub token: String,
}

/// トークンが注文のものか（定数時間で比較する）
fn token_matches(order: &OrderRecord, token: &str) -> bool {
    let given = hash_token(token);
    let expected = &order.access_token_hash;
    given.len() == expected.len()
        && given
            .bytes()
            .zip(expected.bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
}

/// 注文 ID とトークンで注文を引く（どちらが違っても同じ 404）
pub async fn find_order_with_token(
    state: &AppState,
    id: &str,
    token: &str,
) -> Result<OrderRecord, ApiError> {
    let not_found = || api_error(StatusCode::NOT_FOUND, "Order not found");
    let order = state
        .db
        .get_order(id)
        .await
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(not_found)?;
    if token_matches(&order, token) {
        Ok(order)
    } else {
        Err(not_found())
    }
}

/// `GET /api/v1/shop/orders/{id}?token=...` — 顧客の注文確認（注文時に渡したトークンが必要）
pub async fn get_public_order(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(params): Query<OrderTokenParams>,
) -> Result<Json<PublicOrderView>, ApiError> {
    commerce(&state)?;
    let order = find_order_with_token(&state, &id, &params.token).await?;
    Ok(Json(public_view(&state, &order)))
}

/// `POST /api/v1/shop/webhooks/{provider}` — 決済サービスからの通知（署名を確かめる）
pub async fn payment_webhook(
    State(state): State<Arc<AppState>>,
    audit: AuditContext,
    Path(provider): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<serde_json::Value>, ApiError> {
    let commerce = commerce(&state)?;
    if provider != commerce.provider().id() {
        return Err(api_error(StatusCode::NOT_FOUND, "Unknown payment provider"));
    }
    let event = match commerce.provider().parse_webhook(&headers, &body) {
        Ok(Some(event)) => event,
        // 注文に関係しないイベント
        Ok(None) => return Ok(Json(json!({ "received": true }))),
        Err(e) => {
            state
                .audit
                .record(
                    audit
                        .event(AuditAction::AccessDenied, AuditOutcome::Denied)
                        .with_resource(
                            "endpoint",
                            Some(format!("POST /api/v1/shop/webhooks/{}", provider)),
                        )
                        .with_details(json!({
                            "reason": "invalid_webhook_signature",
                            "error": e.to_string(),
                        })),
                )
                .await;
            return Err(api_error(StatusCode::BAD_REQUEST, e.to_string()));
        }
    };
    match commerce.apply_payment_event(&state, &audit, &event).await {
        Ok(()) => Ok(Json(json!({ "received": true }))),
        // 他の環境の注文など。再送されても結果は同じなので 200 で受け取る
        Err(CommerceError::NotFound(message)) => {
            tracing::warn!("Payment webhook for an unknown order: {}", message);
            Ok(Json(json!({ "received": true, "ignored": message })))
        }
        // 500 を返すと決済サービスが再送する
        Err(e) => Err(commerce_error(e)),
    }
}

/// 管理画面の注文
#[derive(Debug, Serialize)]
pub struct AdminOrderView {
    #[serde(flatten)]
    pub order: PublicOrderView,
    pub customer_email: String,
    pub coupon_code: Option<String>,
    pub metadata: serde_json::Value,
    pub payment_ref: Option<String>,
    pub note: Option<String>,
    pub updated_at: String,
    /// 支払い待ちの注文を手動で「支払い済み」にできるか（入金を Webhook で確かめる決済では不可）
    pub can_mark_paid: bool,
}

fn admin_view(state: &AppState, commerce: &CommerceService, order: &OrderRecord) -> AdminOrderView {
    AdminOrderView {
        order: public_view(state, order),
        customer_email: order.customer_email.clone(),
        coupon_code: order.coupon_code.clone(),
        metadata: serde_json::from_str(&order.metadata_json).unwrap_or_else(|_| json!({})),
        payment_ref: order.payment_ref.clone(),
        note: order.note.clone(),
        updated_at: order.updated_at.clone(),
        can_mark_paid: order.status == OrderStatus::PendingPayment.as_str()
            && order.payment_provider == commerce.provider().id()
            && commerce.provider().allows_manual_confirmation(),
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct ListOrdersParams {
    pub status: Option<String>,
    pub limit: Option<u64>,
    #[serde(default)]
    pub offset: u64,
}

/// `GET /api/v1/orders` — 注文の一覧（新しい順、`editor` 以上）
pub async fn list_orders(
    State(state): State<Arc<AppState>>,
    _session: EditorSession,
    Query(params): Query<ListOrdersParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let commerce = commerce(&state)?;
    let status = params.status.filter(|s| !s.is_empty());
    if let Some(status) = &status {
        status
            .parse::<OrderStatus>()
            .map_err(|e| api_error(StatusCode::BAD_REQUEST, e))?;
    }
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let page = state
        .db
        .list_orders(&OrderListQuery {
            status,
            limit,
            offset: params.offset,
        })
        .await
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let orders: Vec<AdminOrderView> = page
        .orders
        .iter()
        .map(|o| admin_view(&state, &commerce, o))
        .collect();
    Ok(Json(json!({
        "orders": orders,
        "total": page.total,
        "limit": limit,
        "offset": params.offset,
    })))
}

async fn find_order(state: &AppState, id: &str) -> Result<OrderRecord, ApiError> {
    state
        .db
        .get_order(id)
        .await
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Order not found"))
}

/// `GET /api/v1/orders/{id}` — 注文の詳細（`editor` 以上）
pub async fn get_order(
    State(state): State<Arc<AppState>>,
    _session: EditorSession,
    Path(id): Path<String>,
) -> Result<Json<AdminOrderView>, ApiError> {
    let commerce = commerce(&state)?;
    let order = find_order(&state, &id).await?;
    Ok(Json(admin_view(&state, &commerce, &order)))
}

#[derive(Debug, Deserialize)]
pub struct UpdateOrderStatusRequest {
    pub status: OrderStatus,
    #[serde(default)]
    pub note: Option<String>,
}

/// `POST /api/v1/orders/{id}/status` — 注文の状態を変える（`editor` 以上）。
/// 支払い済み（手動確認できる決済のみ）・発送・完了・取り消し（発送前なら在庫を戻す）
pub async fn update_order_status(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): EditorSession,
    audit: AuditContext,
    Path(id): Path<String>,
    Json(request): Json<UpdateOrderStatusRequest>,
) -> Result<Json<AdminOrderView>, ApiError> {
    let commerce = commerce(&state)?;
    let order = find_order(&state, &id).await?;
    if request.status == OrderStatus::Paid && !admin_view(&state, &commerce, &order).can_mark_paid {
        return Err(api_error(
            StatusCode::CONFLICT,
            "This order's payment is confirmed by the payment provider, not by hand",
        ));
    }
    let note = request
        .note
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty());
    let updated = commerce
        .transition(&state, &audit, &order, request.status, note, "admin")
        .await
        .map_err(commerce_error)?;
    Ok(Json(admin_view(&state, &commerce, &updated)))
}
