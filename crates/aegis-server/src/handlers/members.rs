//! 会員アカウント（Member Account）の API。
//! AegisCMS の管理ユーザー（`_aegis_users`）とは完全に別管理の、EC 会員アカウント。

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
use crate::auth::{AdminSession, EditorSession};
use crate::state::AppState;
use aegis_core::{AegisError, NotificationEvent, Recipient};
use aegis_members::{
    MemberLoginRequest, MemberRegisterRequest, MemberResponse, MemberService, MemberSessionData,
    MemberUpdateProfileRequest, MemberVerification, EMAIL_NOT_VERIFIED_MESSAGE,
    EMAIL_VERIFICATION_TTL_SECS, MEMBER_TOKEN_PREFIX, PASSWORD_RESET_TTL_SECS,
};

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

fn members(state: &AppState) -> Result<Arc<MemberService>, ApiError> {
    state
        .members
        .clone()
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Member accounts are not enabled"))
}

/// Authorization ヘッダーから Bearer トークンを取得
fn extract_bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .map(str::trim)
}

/// 会員のセッションを持つ Cookie の名前。`[server]` の名前空間から作る（管理画面のものと混ざらない）
pub fn member_cookie_name(state: &AppState) -> String {
    format!(
        "{}_member",
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

/// 会員のセッションのトークン。API のクライアントは `Authorization: Bearer`、ブラウザは Cookie
pub fn session_token(state: &AppState, headers: &HeaderMap) -> Option<String> {
    extract_bearer_token(headers)
        .map(str::to_string)
        .or_else(|| cookie_value(headers, &member_cookie_name(state)))
}

/// HTTPS で配信しているか（Cookie の `Secure` を付けるかの判断。リバースプロキシの背後では `X-Forwarded-Proto`）
fn is_https(state: &AppState, headers: &HeaderMap) -> bool {
    headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|proto| proto.eq_ignore_ascii_case("https"))
        || state.config.members.public_url.starts_with("https://")
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
        member_cookie_name(state),
        value,
        max_age_secs,
        secure
    );
    HeaderValue::from_str(&cookie).unwrap_or_else(|_| HeaderValue::from_static(""))
}

/// 会員セッションを解決する（未ログイン時は 401）
/// リクエストがこのサイトのページから送られたものか（`Origin`、無ければ `Referer` の
/// ホストが、このサイトのホストと同じか）。Cookie で認証する HTML のフォーム（掲示板の投稿など）が、
/// 他のサイトから送られるのを断るのに使う。どちらのヘッダーも無いときは、断る
pub fn request_comes_from_this_site(state: &AppState, headers: &HeaderMap) -> bool {
    let Some(origin) = headers
        .get(header::ORIGIN)
        .or_else(|| headers.get(header::REFERER))
        .and_then(|v| v.to_str().ok())
    else {
        return false;
    };
    let Some(origin_host) = host_of(origin) else {
        return false;
    };

    // このサイトのホスト: `Host`（プロキシ越しなら `X-Forwarded-Host`）と、設定した `public_url`
    let mut allowed: Vec<String> = Vec::new();
    for name in ["host", "x-forwarded-host"] {
        if let Some(value) = headers.get(name).and_then(|v| v.to_str().ok()) {
            allowed.push(value.trim().to_ascii_lowercase());
        }
    }
    if let Some(host) = host_of(&state.config.members.public_url) {
        allowed.push(host);
    }
    allowed.contains(&origin_host)
}

/// URL のホスト（ポートつき。小文字）。`null` や URL でない値は `None`
fn host_of(url: &str) -> Option<String> {
    let rest = url.trim().split_once("://")?.1;
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit('@').next()?;
    if host.is_empty() {
        None
    } else {
        Some(host.to_ascii_lowercase())
    }
}

pub async fn resolve_member_session(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<MemberSessionData, ApiError> {
    let token = session_token(state, headers)
        .ok_or_else(|| api_error(StatusCode::UNAUTHORIZED, "Member authentication required"))?;
    let token = token.as_str();

    if !token.starts_with(MEMBER_TOKEN_PREFIX) {
        return Err(api_error(
            StatusCode::UNAUTHORIZED,
            "Invalid member session token",
        ));
    }

    let service = members(state)?;
    let session = service
        .verify_session(token)
        .await
        .map_err(map_aegis_error)?
        .ok_or_else(|| {
            api_error(
                StatusCode::UNAUTHORIZED,
                "Member session has expired or is invalid",
            )
        })?;

    Ok(session)
}

/// `POST /api/v1/members/register` — 会員登録
pub async fn register(
    State(state): State<Arc<AppState>>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(req): Json<MemberRegisterRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let service = members(&state)?;
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
        // 確認が済むまで使えない会員として登録し、確認メールを送る。登録済みのアドレスでも同じ応答にする
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

    let member = service
        .register(req, client_ip, user_agent)
        .await
        .map_err(map_aegis_error)?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "member": member,
        })),
    ))
}

/// 確認メールを送れる設定か。メールを送れる（`[notifications]`）ことと、リンクの元になる公開 URL が要る
/// （再設定と同じく、メールのリンクは `Host` ヘッダーではなく `[members].public_url` から作る）
pub(crate) fn verification_available(state: &AppState) -> bool {
    state.notifier.is_some() && !state.config.members.public_url.trim().is_empty()
}

fn send_verification_email(state: &AppState, verification: &MemberVerification) {
    let Some(notifier) = state.notifier.as_ref() else {
        return;
    };
    let verify_url = format!(
        "{}/account/verify?token={}",
        state.config.members.public_url.trim().trim_end_matches('/'),
        verification.token
    );
    notifier.notify(NotificationEvent::new(
        "member.verify",
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

/// `POST /api/v1/members/verify` — 確認メールのリンクのトークンで、メールアドレスを確認する。
/// 確認が済むと、ログインできる
pub async fn verify_email(
    State(state): State<Arc<AppState>>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(req): Json<VerifyRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let service = members(&state)?;
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

/// `POST /api/v1/members/verify/resend` — 確認メールを送り直す。
/// 登録の有無を教えないため、メールアドレスの形が正しければ、いつも `202` を返す
pub async fn resend_verification(
    State(state): State<Arc<AppState>>,
    ClientIp(client_ip): ClientIp,
    Json(req): Json<ResendRequest>,
) -> Result<Response, ApiError> {
    let service = members(&state)?;
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

/// `POST /api/v1/members/login` — ログイン
pub async fn login(
    State(state): State<Arc<AppState>>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(req): Json<MemberLoginRequest>,
) -> Result<Response, ApiError> {
    let service = members(&state)?;
    let user_agent = headers
        .get("user-agent")
        .and_then(|h| h.to_str().ok())
        .map(str::to_string);

    let (session, member) =
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
            "member": member,
        })),
    )
        .into_response())
}

/// `POST /api/v1/members/logout` — ログアウト
pub async fn logout(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let service = members(&state)?;
    if let Some(token) = session_token(&state, &headers) {
        let _ = service.logout(&token).await;
    }
    // Cookie も消す
    let cleared = session_cookie(&state, &headers, "", 0);
    Ok((StatusCode::NO_CONTENT, [(header::SET_COOKIE, cleared)]).into_response())
}

/// `GET /api/v1/members/me` — ログイン会員情報取得
pub async fn get_me(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<MemberResponse>, ApiError> {
    let session = resolve_member_session(&state, &headers).await?;
    let service = members(&state)?;
    let profile = service
        .get_profile(&session.member_id)
        .await
        .map_err(map_aegis_error)?;
    Ok(Json(with_avatar_url(&state, profile)))
}

/// `PUT /api/v1/members/me` — ログイン会員情報更新
pub async fn update_me(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<MemberUpdateProfileRequest>,
) -> Result<Response, ApiError> {
    let session = resolve_member_session(&state, &headers).await?;
    let service = members(&state)?;
    let changed_password = req.new_password.is_some();
    let updated = service
        .update_profile(&session.member_id, req)
        .await
        .map_err(map_aegis_error)?;
    let updated = with_avatar_url(&state, updated);
    if !changed_password {
        return Ok(Json(updated).into_response());
    }
    // パスワードを変えると、ほかの端末のセッションは終わる。変えた本人の分は作り直して、続けて使えるようにする
    let fresh = service
        .reissue_session(&session.member_id)
        .await
        .map_err(map_aegis_error)?;
    let cookie = session_cookie(&state, &headers, &fresh.token, service.session_ttl_secs());
    let mut body = serde_json::to_value(&updated).unwrap_or_default();
    body["token"] = json!(fresh.token);
    body["expires_at"] = json!(fresh.expires_at);
    Ok(([(header::SET_COOKIE, cookie)], Json(body)).into_response())
}

/// 応答に、アバター画像の公開 URL（ストレージの設定に従う）を埋める
fn with_avatar_url(state: &AppState, mut member: MemberResponse) -> MemberResponse {
    member.avatar_url = member
        .avatar_key
        .as_deref()
        .map(|key| state.storage.public_url(key));
    member
}

/// 会員 ID から、アバター画像の公開 URL を引く（画像がある会員だけ入る）。掲示板・レビューの表示用。
/// 表示のたびに今の値を引くので、画像を変えれば過去の投稿にも反映される。重複は除いて、1 回の問い合わせでまとめて引く
pub(crate) async fn avatar_urls(
    state: &AppState,
    member_ids: Vec<String>,
) -> std::collections::HashMap<String, String> {
    let Some(service) = state.members.as_ref() else {
        return Default::default();
    };
    match service.avatar_keys(&member_ids).await {
        Ok(keys) => keys
            .into_iter()
            .map(|(id, key)| (id, state.storage.public_url(&key)))
            .collect(),
        Err(e) => {
            tracing::warn!("Failed to load member avatars: {}", e);
            Default::default()
        }
    }
}

/// 掲示板のページには投稿者のアバターが出るので、画像を変えたら掲示板のキャッシュを更新する
async fn purge_forum_pages(state: &AppState) {
    if state.forum.is_some() {
        let mut tags = aegis_cache::CacheTags::new();
        tags.add_dependency("collection", "forum");
        crate::cdn::purge_tags(state, &tags).await;
    }
}

/// Cookie だけで認証した multipart の変更は、他のサイトのフォームから送れてしまうので、
/// このサイトからのものに限る（`Authorization` ヘッダーで認証したものは対象外）
fn require_same_site_for_cookie(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    if !headers.contains_key(header::AUTHORIZATION) && !request_comes_from_this_site(state, headers)
    {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            "Cross-site requests are not allowed",
        ));
    }
    Ok(())
}

/// `PUT /api/v1/members/me/avatar` — アバター画像（multipart の `file`。PNG / JPEG / GIF / WebP、
/// 2 MB まで）を設定する
pub async fn upload_my_avatar(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    multipart: axum::extract::Multipart,
) -> Result<Json<MemberResponse>, ApiError> {
    let session = resolve_member_session(&state, &headers).await?;
    require_same_site_for_cookie(&state, &headers)?;
    let service = members(&state)?;
    let (data, content_type, extension) = super::users::read_avatar_image(multipart).await?;

    // 画像を差し替えるたびにキーを変え、ブラウザ・CDN のキャッシュに古い画像が残らないようにする
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let key = format!(
        "{}{}-{}.{}",
        super::users::AVATAR_PREFIX,
        session.member_id,
        &suffix[..8],
        extension
    );
    state
        .storage
        .put(&key, &data, content_type)
        .await
        .map_err(|e| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Storage put error: {}", e),
            )
        })?;
    let (member, previous) = match service
        .set_avatar(&session.member_id, Some(key.clone()))
        .await
    {
        Ok(result) => result,
        Err(e) => {
            super::users::remove_avatar_object(&state, &key).await;
            return Err(map_aegis_error(e));
        }
    };
    if let Some(previous) = previous.filter(|p| *p != key) {
        super::users::remove_avatar_object(&state, &previous).await;
    }
    purge_forum_pages(&state).await;
    Ok(Json(with_avatar_url(&state, member)))
}

/// `DELETE /api/v1/members/me/avatar` — アバター画像を外す
pub async fn delete_my_avatar(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<MemberResponse>, ApiError> {
    let session = resolve_member_session(&state, &headers).await?;
    let service = members(&state)?;
    let (member, previous) = service
        .set_avatar(&session.member_id, None)
        .await
        .map_err(map_aegis_error)?;
    if let Some(previous) = previous {
        super::users::remove_avatar_object(&state, &previous).await;
    }
    purge_forum_pages(&state).await;
    Ok(Json(with_avatar_url(&state, member)))
}

#[derive(Debug, Deserialize)]
pub struct DeleteAccountRequest {
    pub password: String,
}

/// `DELETE /api/v1/members/me` — 退会。パスワードで本人を確かめる。
/// 注文は会員との紐づきだけを外して残し、レビューは匿名にして残す
pub async fn delete_me(
    State(state): State<Arc<AppState>>,
    audit: crate::audit::AuditContext,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(req): Json<DeleteAccountRequest>,
) -> Result<Response, ApiError> {
    let session = resolve_member_session(&state, &headers).await?;
    let service = members(&state)?;
    let user_agent = headers
        .get("user-agent")
        .and_then(|h| h.to_str().ok())
        .map(str::to_string);
    let avatar_key = service
        .get_profile(&session.member_id)
        .await
        .ok()
        .and_then(|profile| profile.avatar_key);
    // 1. 本人の確認（違えば、何も変えない）
    service
        .confirm_password(&session.member_id, &req.password)
        .await
        .map_err(map_aegis_error)?;
    // 2. 機能ごとのデータの後始末（注文の紐づきを外す・レビューを匿名にする）。会員の行を消す前に行う
    let product_ids = aegis_commerce::member_data::release_member(&*state.db, &session.member_id)
        .await
        .map_err(map_aegis_error)?;
    // 2b. 掲示板の投稿も、残して匿名にする（スレッドが途切れないように）
    if let Some(forum) = state.forum.as_ref() {
        let anonymized = forum
            .release_member(&session.member_id)
            .await
            .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if anonymized > 0 {
            let mut tags = aegis_cache::CacheTags::new();
            tags.add_dependency("collection", "forum");
            crate::cdn::purge_tags(&state, &tags).await;
        }
    }
    // 2c-0. 契約中のサブスクは、課金が続かないように解約する（解約できなければ、退会を止める）
    crate::handlers::subscription::cancel_member_subscriptions(&state, &audit, &session.member_id)
        .await?;
    state
        .db
        .detach_member_subscriptions(&session.member_id)
        .await
        .map_err(map_aegis_error)?;
    // 2c. 購入の記録は、会員との紐づきだけを外して残す（会計の記録。権利は外れる）
    state
        .db
        .detach_member_purchases(&session.member_id)
        .await
        .map_err(map_aegis_error)?;
    // 3. 会員の行とセッションを消す
    service
        .delete_account(&session.member_id, client_ip, user_agent)
        .await
        .map_err(map_aegis_error)?;
    if let Some(key) = avatar_key {
        super::users::remove_avatar_object(&state, &key).await;
    }

    // 公開の商品ページに出ていた投稿者名が変わるので、そのキャッシュを更新する
    if !product_ids.is_empty() {
        let mut tags = aegis_cache::CacheTags::new();
        for product_id in &product_ids {
            tags.add_dependency("reviews", product_id);
            tags.add_dependency(&state.config.commerce.product_type, product_id);
        }
        crate::cdn::purge_tags(&state, &tags).await;
    }
    // Cookie も消す
    let cleared = session_cookie(&state, &headers, "", 0);
    Ok((StatusCode::NO_CONTENT, [(header::SET_COOKIE, cleared)]).into_response())
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
/// （`[members].public_url`。`Host` ヘッダーは偽装できるので、メールのリンクには使わない）が要る
pub(crate) fn password_reset_available(state: &AppState) -> bool {
    state.config.members.password_reset
        && state.notifier.is_some()
        && !state.config.members.public_url.trim().is_empty()
}

/// `POST /api/v1/members/password-reset` — 再設定のメールを申し込む。
/// 登録の有無を教えないため、メールアドレスの形が正しければ、いつも `202` を返す
pub async fn request_password_reset(
    State(state): State<Arc<AppState>>,
    ClientIp(client_ip): ClientIp,
    Json(req): Json<PasswordResetRequest>,
) -> Result<Response, ApiError> {
    let service = members(&state)?;
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
            state.config.members.public_url.trim().trim_end_matches('/'),
            requested.token
        );
        notifier.notify(NotificationEvent::new(
            "member.password_reset",
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

/// `POST /api/v1/members/password-reset/complete` — メールのリンクのトークンで、新しいパスワードを設定する。
/// 完了すると、その会員のセッションはすべて終わる（ログインし直す）
pub async fn complete_password_reset(
    State(state): State<Arc<AppState>>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(req): Json<PasswordResetComplete>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let service = members(&state)?;
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

/// `GET /api/v1/members/orders` — ログイン会員の注文履歴一覧
pub async fn list_my_orders(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let session = resolve_member_session(&state, &headers).await?;
    let orders = aegis_commerce::member_data::list_member_orders(&*state.db, &session.member_id)
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

/// `GET /api/v1/members/orders/{id}` — ログイン会員の注文詳細
pub async fn get_my_order(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(order_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let session = resolve_member_session(&state, &headers).await?;
    let order =
        aegis_commerce::member_data::get_member_order(&*state.db, &session.member_id, &order_id)
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

/// `GET /api/v1/members/reviews` — ログイン会員が投稿したレビュー一覧
pub async fn list_my_reviews(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let session = resolve_member_session(&state, &headers).await?;
    let reviews_service = state
        .reviews
        .clone()
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Product reviews are not enabled"))?;

    let reviews = reviews_service
        .list_member_reviews(&session.member_id)
        .await
        .map_err(map_aegis_error)?;

    Ok(Json(json!({ "reviews": reviews })))
}

// --- 管理者向けエンドポイント ---

#[derive(Debug, Deserialize)]
pub struct AdminMemberListQuery {
    #[serde(default = "default_limit")]
    pub limit: u64,
    #[serde(default)]
    pub offset: u64,
}

fn default_limit() -> u64 {
    50
}

#[derive(Debug, Deserialize)]
pub struct PatchMemberStatusRequest {
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct SetMemberLevelRequest {
    pub level: String,
}

/// `PUT /api/v1/admin/members/{id}/level` — 管理者: 会員の権限レベルを変える（有料メンバー限定の閲覧制御。
/// docs/members.md 15）。有料の付与にあたるので、`admin` だけ
pub async fn admin_set_member_level(
    RequireRole(session): AdminSession,
    State(state): State<Arc<AppState>>,
    Path(member_id): Path<String>,
    Json(req): Json<SetMemberLevelRequest>,
) -> Result<StatusCode, ApiError> {
    let service = members(&state)?;
    service
        .admin_set_level(&member_id, req.level.trim(), &session.user_id)
        .await
        .map_err(map_aegis_error)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/v1/admin/members` — 管理者: 会員一覧
pub async fn admin_list_members(
    RequireRole(_session): EditorSession,
    State(state): State<Arc<AppState>>,
    Query(query): Query<AdminMemberListQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let service = members(&state)?;
    let (members, total) = service
        .admin_list_members(query.limit, query.offset)
        .await
        .map_err(map_aegis_error)?;
    let members: Vec<MemberResponse> = members
        .into_iter()
        .map(|member| with_avatar_url(&state, member))
        .collect();

    Ok(Json(json!({
        "members": members,
        "total": total,
    })))
}

/// `GET /api/v1/admin/members/{id}` — 管理者: 会員詳細と注文履歴
pub async fn admin_get_member(
    RequireRole(_session): EditorSession,
    State(state): State<Arc<AppState>>,
    Path(member_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let service = members(&state)?;
    let member = service
        .admin_get_member(&member_id)
        .await
        .map_err(map_aegis_error)?;
    let member = with_avatar_url(&state, member);

    let orders = aegis_commerce::member_data::list_member_orders(&*state.db, &member_id)
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

    // 掲示板が有効なら、その会員の投稿数も返す（無効なら null）
    let forum = match state.forum.as_ref() {
        Some(forum) => {
            let (threads, posts) = forum
                .member_post_counts(&member_id)
                .await
                .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            Some(json!({ "threads": threads, "posts": posts }))
        }
        None => None,
    };

    Ok(Json(json!({
        "member": member,
        // 付与できるレベル（低 → 高）。管理画面の選択肢
        "levels": service.levels(),
        "orders": orders,
        "forum": forum,
        // 購入・サブスク（購入が無効なら空）
        "purchases": if state.config.members.purchase.enabled {
            let purchases = state
                .db
                .list_member_purchases(&member_id)
                .await
                .map_err(map_aegis_error)?;
            crate::handlers::purchase::purchase_views(&state, purchases).await
        } else {
            Vec::new()
        },
        "subscriptions": if state.config.members.purchase.enabled {
            let subscriptions = state
                .db
                .list_member_subscriptions(&member_id)
                .await
                .map_err(map_aegis_error)?;
            crate::handlers::subscription::views(&state, &subscriptions)
        } else {
            Vec::new()
        },
        "purchase_enabled": state.config.members.purchase.enabled,
    })))
}

/// `PATCH /api/v1/admin/members/{id}` — 管理者: 会員ステータス変更（active/suspended）
pub async fn admin_set_member_status(
    RequireRole(_session): EditorSession,
    State(state): State<Arc<AppState>>,
    Path(member_id): Path<String>,
    Json(req): Json<PatchMemberStatusRequest>,
) -> Result<StatusCode, ApiError> {
    let service = members(&state)?;
    service
        .admin_set_status(&member_id, &req.status)
        .await
        .map_err(map_aegis_error)?;
    Ok(StatusCode::NO_CONTENT)
}
