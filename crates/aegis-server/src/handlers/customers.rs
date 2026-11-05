//! 顧客アカウント（Customer Account）の API。
//! AegisCMS の管理ユーザー（`_aegis_users`）とは完全に別管理の、EC 顧客アカウント。

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::audit::ClientIp;
use crate::auth::authorize::RequireRole;
use crate::auth::EditorSession;
use crate::state::AppState;
use aegis_commerce::{
    CustomerLoginRequest, CustomerRegisterRequest, CustomerResponse, CustomerService,
    CustomerSessionData, CustomerUpdateProfileRequest, CustomerVerification, CUSTOMER_TOKEN_PREFIX,
    EMAIL_NOT_VERIFIED_MESSAGE, EMAIL_VERIFICATION_TTL_SECS, PASSWORD_RESET_TTL_SECS,
};
use aegis_core::{AegisError, NotificationEvent, Recipient};

type ApiError = (StatusCode, Json<serde_json::Value>);

fn api_error(status: StatusCode, message: impl Into<String>) -> ApiError {
    (status, Json(json!({ "error": message.into() })))
}

fn map_aegis_error(e: AegisError) -> ApiError {
    let status = match e {
        AegisError::Validation(_) => StatusCode::BAD_REQUEST,
        AegisError::Authentication(_) => StatusCode::UNAUTHORIZED,
        AegisError::PermissionDenied(_) => StatusCode::FORBIDDEN,
        AegisError::NotFound(_) => StatusCode::NOT_FOUND,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, Json(json!({ "error": e.to_string() })))
}

fn customers(state: &AppState) -> Result<Arc<CustomerService>, ApiError> {
    state
        .customers
        .clone()
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Customer accounts are not enabled"))
}

/// Authorization ヘッダーから Bearer トークンを取得
fn extract_bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .map(str::trim)
}

/// 顧客のセッションを持つ Cookie の名前。`[server]` の名前空間から作る（管理画面のものと混ざらない）
pub fn customer_cookie_name(state: &AppState) -> String {
    format!(
        "{}_customer",
        state.config.server.namespace().replace('-', "_")
    )
}

/// `Cookie` ヘッダーから、名前が合う Cookie の値を取り出す
fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// 顧客のセッションのトークン。API のクライアントは `Authorization: Bearer`、ブラウザは Cookie
pub fn session_token(state: &AppState, headers: &HeaderMap) -> Option<String> {
    extract_bearer_token(headers)
        .map(str::to_string)
        .or_else(|| cookie_value(headers, &customer_cookie_name(state)))
}

/// HTTPS で配信しているか（Cookie の `Secure` を付けるかの判断。リバースプロキシの背後では `X-Forwarded-Proto`）
fn is_https(state: &AppState, headers: &HeaderMap) -> bool {
    headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|proto| proto.eq_ignore_ascii_case("https"))
        || state.config.commerce.public_url.starts_with("https://")
}

/// セッションの Cookie（`HttpOnly` なので JavaScript はトークンに触れない。`SameSite=Lax` で他のサイトからの POST には付かない）
fn session_cookie(
    state: &AppState,
    headers: &HeaderMap,
    value: &str,
    max_age_secs: u64,
) -> HeaderValue {
    let secure = if is_https(state, headers) {
        "; Secure"
    } else {
        ""
    };
    let cookie = format!(
        "{}={}; Path=/; Max-Age={}; HttpOnly; SameSite=Lax{}",
        customer_cookie_name(state),
        value,
        max_age_secs,
        secure
    );
    HeaderValue::from_str(&cookie).unwrap_or_else(|_| HeaderValue::from_static(""))
}

/// 顧客セッションを解決する（未ログイン時は 401）
pub async fn resolve_customer_session(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<CustomerSessionData, ApiError> {
    let token = session_token(state, headers)
        .ok_or_else(|| api_error(StatusCode::UNAUTHORIZED, "Customer authentication required"))?;
    let token = token.as_str();

    if !token.starts_with(CUSTOMER_TOKEN_PREFIX) {
        return Err(api_error(
            StatusCode::UNAUTHORIZED,
            "Invalid customer session token",
        ));
    }

    let service = customers(state)?;
    let session = service
        .verify_session(token)
        .await
        .map_err(map_aegis_error)?
        .ok_or_else(|| {
            api_error(
                StatusCode::UNAUTHORIZED,
                "Customer session has expired or is invalid",
            )
        })?;

    Ok(session)
}

/// `POST /api/v1/customers/register` — 会員登録
pub async fn register(
    State(state): State<Arc<AppState>>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(req): Json<CustomerRegisterRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let service = customers(&state)?;
    let user_agent = headers
        .get("user-agent")
        .and_then(|h| h.to_str().ok())
        .map(str::to_string);

    if service.requires_email_verification() {
        if !verification_available(&state) {
            return Err(api_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "Email confirmation is not available",
            ));
        }
        // 確認が済むまで使えない顧客として登録し、確認メールを送る。登録済みのアドレスでも同じ応答にする
        let pending = service
            .register_pending(req, client_ip, user_agent)
            .await
            .map_err(map_aegis_error)?;
        if let Some(verification) = pending {
            send_verification_email(&state, &verification);
        }
        return Ok((
            StatusCode::ACCEPTED,
            Json(json!({
                "status": "verification_sent",
                "expires_in_secs": EMAIL_VERIFICATION_TTL_SECS,
            })),
        ));
    }

    let customer = service
        .register(req, client_ip, user_agent)
        .await
        .map_err(map_aegis_error)?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "customer": customer,
        })),
    ))
}

/// 確認メールを送れる設定か。メールを送れる（`[notifications]`）ことと、リンクの元になる公開 URL が要る
/// （再設定と同じく、メールのリンクは `Host` ヘッダーではなく `[commerce].public_url` から作る）
pub(crate) fn verification_available(state: &AppState) -> bool {
    state.notifier.is_some() && !state.config.commerce.public_url.trim().is_empty()
}

fn send_verification_email(state: &AppState, verification: &CustomerVerification) {
    let Some(notifier) = state.notifier.as_ref() else {
        return;
    };
    let verify_url = format!(
        "{}/account/verify?token={}",
        state
            .config
            .commerce
            .public_url
            .trim()
            .trim_end_matches('/'),
        verification.token
    );
    notifier.notify(NotificationEvent::new(
        "customer.verify",
        vec![Recipient::person(
            &verification.email,
            verification.name.clone(),
        )],
        json!({
            "name": verification.name,
            "verify_url": verify_url,
            "expires_hours": EMAIL_VERIFICATION_TTL_SECS.div_ceil(3600),
        }),
    ));
}

#[derive(Debug, Deserialize)]
pub struct VerifyRequest {
    pub token: String,
}

/// `POST /api/v1/customers/verify` — 確認メールのリンクのトークンで、メールアドレスを確認する。
/// 確認が済むと、ログインできる
pub async fn verify_email(
    State(state): State<Arc<AppState>>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(req): Json<VerifyRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let service = customers(&state)?;
    if !verification_available(&state) {
        return Err(api_error(
            StatusCode::NOT_FOUND,
            "Email confirmation is not available",
        ));
    }
    let user_agent = headers
        .get("user-agent")
        .and_then(|h| h.to_str().ok())
        .map(str::to_string);
    service
        .verify_email(&req.token, client_ip, user_agent)
        .await
        .map_err(map_aegis_error)?;
    Ok(Json(json!({ "status": "verified" })))
}

#[derive(Debug, Deserialize)]
pub struct ResendRequest {
    pub email: String,
}

/// `POST /api/v1/customers/verify/resend` — 確認メールを送り直す。
/// 登録の有無を教えないため、メールアドレスの形が正しければ、いつも `202` を返す
pub async fn resend_verification(
    State(state): State<Arc<AppState>>,
    ClientIp(client_ip): ClientIp,
    Json(req): Json<ResendRequest>,
) -> Result<Response, ApiError> {
    let service = customers(&state)?;
    if !verification_available(&state) {
        return Err(api_error(
            StatusCode::NOT_FOUND,
            "Email confirmation is not available",
        ));
    }
    let pending = service
        .resend_verification(&req.email, client_ip.as_deref())
        .await
        .map_err(|e| match e {
            AegisError::ResourceLimitExceeded(message) => {
                api_error(StatusCode::TOO_MANY_REQUESTS, message)
            }
            other => map_aegis_error(other),
        })?;
    if let Some(verification) = pending {
        send_verification_email(&state, &verification);
    }
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "status": "verification_sent",
            "expires_in_secs": EMAIL_VERIFICATION_TTL_SECS,
        })),
    )
        .into_response())
}

/// `POST /api/v1/customers/login` — ログイン
pub async fn login(
    State(state): State<Arc<AppState>>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(req): Json<CustomerLoginRequest>,
) -> Result<Response, ApiError> {
    let service = customers(&state)?;
    let user_agent = headers
        .get("user-agent")
        .and_then(|h| h.to_str().ok())
        .map(str::to_string);

    let (session, customer) =
        service
            .login(req, client_ip, user_agent)
            .await
            .map_err(|e| match e {
                // パスワードが合っていて、確認だけが済んでいない。ページが「確認メールを送り直す」を出せるよう、コードを付ける
                AegisError::PermissionDenied(message) if message == EMAIL_NOT_VERIFIED_MESSAGE => (
                    StatusCode::FORBIDDEN,
                    Json(json!({ "error": message, "code": "email_not_verified" })),
                ),
                other => map_aegis_error(other),
            })?;

    // ブラウザには Cookie でも渡す（JavaScript から触れない）。API のクライアントは従来どおり本文のトークンを使う
    let cookie = session_cookie(&state, &headers, &session.token, service.session_ttl_secs());
    Ok((
        [(header::SET_COOKIE, cookie)],
        Json(json!({
            "token": session.token,
            "expires_at": session.expires_at,
            "customer": customer,
        })),
    )
        .into_response())
}

/// `POST /api/v1/customers/logout` — ログアウト
pub async fn logout(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let service = customers(&state)?;
    if let Some(token) = session_token(&state, &headers) {
        let _ = service.logout(&token).await;
    }
    // Cookie も消す
    let cleared = session_cookie(&state, &headers, "", 0);
    Ok((StatusCode::NO_CONTENT, [(header::SET_COOKIE, cleared)]).into_response())
}

/// `GET /api/v1/customers/me` — ログイン顧客情報取得
pub async fn get_me(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<CustomerResponse>, ApiError> {
    let session = resolve_customer_session(&state, &headers).await?;
    let service = customers(&state)?;
    let profile = service
        .get_profile(&session.customer_id)
        .await
        .map_err(map_aegis_error)?;
    Ok(Json(profile))
}

/// `PUT /api/v1/customers/me` — ログイン顧客情報更新
pub async fn update_me(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<CustomerUpdateProfileRequest>,
) -> Result<Response, ApiError> {
    let session = resolve_customer_session(&state, &headers).await?;
    let service = customers(&state)?;
    let changed_password = req.new_password.is_some();
    let updated = service
        .update_profile(&session.customer_id, req)
        .await
        .map_err(map_aegis_error)?;
    if !changed_password {
        return Ok(Json(updated).into_response());
    }
    // パスワードを変えると、ほかの端末のセッションは終わる。変えた本人の分は作り直して、続けて使えるようにする
    let fresh = service
        .reissue_session(&session.customer_id)
        .await
        .map_err(map_aegis_error)?;
    let cookie = session_cookie(&state, &headers, &fresh.token, service.session_ttl_secs());
    let mut body = serde_json::to_value(&updated).unwrap_or_default();
    body["token"] = json!(fresh.token);
    body["expires_at"] = json!(fresh.expires_at);
    Ok(([(header::SET_COOKIE, cookie)], Json(body)).into_response())
}

// --- パスワードの再設定（ログイン不要） ---

#[derive(Debug, Deserialize)]
pub struct PasswordResetRequest {
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct PasswordResetComplete {
    pub token: String,
    pub password: String,
}

/// 再設定を使える設定か。メールを送れる（`[notifications]`）ことと、リンクの元になる公開 URL
/// （`[commerce].public_url`。`Host` ヘッダーは偽装できるので、メールのリンクには使わない）が要る
pub(crate) fn password_reset_available(state: &AppState) -> bool {
    state.config.commerce.customers.password_reset
        && state.notifier.is_some()
        && !state.config.commerce.public_url.trim().is_empty()
}

/// `POST /api/v1/customers/password-reset` — 再設定のメールを申し込む。
/// 登録の有無を教えないため、メールアドレスの形が正しければ、いつも `202` を返す
pub async fn request_password_reset(
    State(state): State<Arc<AppState>>,
    ClientIp(client_ip): ClientIp,
    Json(req): Json<PasswordResetRequest>,
) -> Result<Response, ApiError> {
    let service = customers(&state)?;
    if !password_reset_available(&state) {
        return Err(api_error(
            StatusCode::NOT_FOUND,
            "Password reset is not enabled",
        ));
    }
    let requested = service
        .request_password_reset(&req.email, client_ip.as_deref())
        .await
        .map_err(|e| match e {
            AegisError::ResourceLimitExceeded(message) => {
                api_error(StatusCode::TOO_MANY_REQUESTS, message)
            }
            other => map_aegis_error(other),
        })?;

    if let (Some(requested), Some(notifier)) = (requested, state.notifier.as_ref()) {
        let reset_url = format!(
            "{}/account/reset?token={}",
            state
                .config
                .commerce
                .public_url
                .trim()
                .trim_end_matches('/'),
            requested.token
        );
        notifier.notify(NotificationEvent::new(
            "customer.password_reset",
            vec![Recipient::person(&requested.email, requested.name.clone())],
            json!({
                "name": requested.name,
                "reset_url": reset_url,
                "expires_hours": PASSWORD_RESET_TTL_SECS.div_ceil(3600),
            }),
        ));
    }
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "status": "pending_reset",
            "expires_in_secs": PASSWORD_RESET_TTL_SECS,
        })),
    )
        .into_response())
}

/// `POST /api/v1/customers/password-reset/complete` — メールのリンクのトークンで、新しいパスワードを設定する。
/// 完了すると、その顧客のセッションはすべて終わる（ログインし直す）
pub async fn complete_password_reset(
    State(state): State<Arc<AppState>>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(req): Json<PasswordResetComplete>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let service = customers(&state)?;
    if !password_reset_available(&state) {
        return Err(api_error(
            StatusCode::NOT_FOUND,
            "Password reset is not enabled",
        ));
    }
    let user_agent = headers
        .get("user-agent")
        .and_then(|h| h.to_str().ok())
        .map(str::to_string);
    service
        .complete_password_reset(&req.token, &req.password, client_ip, user_agent)
        .await
        .map_err(map_aegis_error)?;
    Ok(Json(json!({ "status": "ok" })))
}

/// `GET /api/v1/customers/orders` — ログイン顧客の注文履歴一覧
pub async fn list_my_orders(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let session = resolve_customer_session(&state, &headers).await?;
    let service = customers(&state)?;
    let orders = service
        .list_orders(&session.customer_id)
        .await
        .map_err(map_aegis_error)?;

    let views: Vec<_> = orders
        .iter()
        .map(|o| {
            let lines: Vec<serde_json::Value> =
                serde_json::from_str(&o.lines_json).unwrap_or_default();
            json!({
                "id": o.id,
                "number": o.number,
                "status": o.status,
                "currency": o.currency,
                "total": o.total,
                "lines": lines,
                "created_at": o.created_at,
                "paid_at": o.paid_at,
            })
        })
        .collect();

    Ok(Json(json!({ "orders": views })))
}

/// `GET /api/v1/customers/orders/{id}` — ログイン顧客の注文詳細
pub async fn get_my_order(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(order_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let session = resolve_customer_session(&state, &headers).await?;
    let service = customers(&state)?;
    let order = service
        .get_order(&session.customer_id, &order_id)
        .await
        .map_err(map_aegis_error)?;

    let lines: Vec<serde_json::Value> = serde_json::from_str(&order.lines_json).unwrap_or_default();
    let adjustments: Vec<serde_json::Value> =
        serde_json::from_str(&order.adjustments_json).unwrap_or_default();
    let shipping: Option<serde_json::Value> = order
        .shipping_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok());

    Ok(Json(json!({
        "id": order.id,
        "number": order.number,
        "status": order.status,
        "currency": order.currency,
        "subtotal": order.subtotal,
        "total": order.total,
        "lines": lines,
        "adjustments": adjustments,
        "customer_name": order.customer_name,
        "customer_email": order.customer_email,
        "shipping_address": shipping,
        "created_at": order.created_at,
        "paid_at": order.paid_at,
    })))
}

/// `GET /api/v1/customers/reviews` — ログイン顧客が投稿したレビュー一覧
pub async fn list_my_reviews(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let session = resolve_customer_session(&state, &headers).await?;
    let reviews_service = state
        .reviews
        .clone()
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Product reviews are not enabled"))?;

    let reviews = reviews_service
        .list_customer_reviews(&session.customer_id)
        .await
        .map_err(map_aegis_error)?;

    Ok(Json(json!({ "reviews": reviews })))
}

// --- 管理者向けエンドポイント ---

#[derive(Debug, Deserialize)]
pub struct AdminCustomerListQuery {
    #[serde(default = "default_limit")]
    pub limit: u64,
    #[serde(default)]
    pub offset: u64,
}

fn default_limit() -> u64 {
    50
}

#[derive(Debug, Deserialize)]
pub struct PatchCustomerStatusRequest {
    pub status: String,
}

/// `GET /api/v1/admin/customers` — 管理者: 顧客一覧
pub async fn admin_list_customers(
    RequireRole(_session): EditorSession,
    State(state): State<Arc<AppState>>,
    Query(query): Query<AdminCustomerListQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let service = customers(&state)?;
    let (customers, total) = service
        .admin_list_customers(query.limit, query.offset)
        .await
        .map_err(map_aegis_error)?;

    Ok(Json(json!({
        "customers": customers,
        "total": total,
    })))
}

/// `GET /api/v1/admin/customers/{id}` — 管理者: 顧客詳細と注文履歴
pub async fn admin_get_customer(
    RequireRole(_session): EditorSession,
    State(state): State<Arc<AppState>>,
    Path(customer_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let service = customers(&state)?;
    let customer = service
        .admin_get_customer(&customer_id)
        .await
        .map_err(map_aegis_error)?;

    let orders = service
        .list_orders(&customer_id)
        .await
        .map_err(map_aegis_error)?;

    // 注文は一覧に要る項目だけを返す（注文確認ページのトークンのハッシュなどは載せない）
    let orders: Vec<serde_json::Value> = orders
        .iter()
        .map(|order| {
            json!({
                "id": order.id,
                "number": order.number,
                "status": order.status,
                "currency": order.currency,
                "total": order.total,
                "created_at": order.created_at,
            })
        })
        .collect();

    Ok(Json(json!({
        "customer": customer,
        "orders": orders,
    })))
}

/// `PATCH /api/v1/admin/customers/{id}` — 管理者: 顧客ステータス変更（active/suspended）
pub async fn admin_set_customer_status(
    RequireRole(_session): EditorSession,
    State(state): State<Arc<AppState>>,
    Path(customer_id): Path<String>,
    Json(req): Json<PatchCustomerStatusRequest>,
) -> Result<StatusCode, ApiError> {
    let service = customers(&state)?;
    service
        .admin_set_status(&customer_id, &req.status)
        .await
        .map_err(map_aegis_error)?;
    Ok(StatusCode::NO_CONTENT)
}
