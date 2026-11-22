//! 公開ページの閲覧の制限（会員レベル・パスワード保護）。docs/members.md 15。
//!
//! 記事・ページの `access`（必要なレベル、または `"password"`）と閲覧者（Cookie / Bearer の会員のセッション、
//! または記事ごとのパスワード解除 Cookie）から、本文を見せるかを決める。

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;

use crate::audit::ClientIp;
use crate::handlers::content::is_currently_published;
use crate::state::AppState;
use aegis_members::Viewer;

pub use aegis_schema::ACCESS_FIELD;

/// 1 つのブラウザの Cookie に保持する、パスワード解除済みの記事トークンの最大件数
const MAX_UNLOCKED_ITEMS: usize = 32;
/// パスワード解除 Cookie の有効期間（24 時間）
const PASSWORD_COOKIE_TTL_SECS: u64 = 24 * 3600;
/// パスワード誤り・回数制限の案内をリダイレクト先に伝える短命 Cookie の有効期間（秒）
const PASSWORD_ERROR_COOKIE_TTL_SECS: u64 = 10;

/// 閲覧者によらず出す本文（フィード・検索・一覧・API 用）。制限していれば、だれにでも見せてよい部分だけ
pub fn shared_blocks(
    fields: &HashMap<String, serde_json::Value>,
    blocks: Vec<aegis_ast::BlockNode>,
) -> Vec<aegis_ast::BlockNode> {
    if aegis_db::is_restricted(fields) {
        aegis_ast::public_blocks(&blocks)
    } else {
        blocks
    }
}

/// 一覧・フィード・関連記事の要約。制限していれば、書き手が書いた `excerpt` を優先し、無ければ
/// だれにでも見せてよい部分から作る（本文の先頭を載せない）
pub fn summary_excerpt(
    fields: &HashMap<String, serde_json::Value>,
    blocks: &[aegis_ast::BlockNode],
    max_chars: usize,
) -> String {
    use crate::handlers::public::extract_excerpt;
    if !aegis_db::is_restricted(fields) {
        return extract_excerpt(blocks, max_chars);
    }
    let written = fields
        .get("excerpt")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    match written {
        Some(text) => text.chars().take(max_chars).collect(),
        None => extract_excerpt(&aegis_ast::public_blocks(blocks), max_chars),
    }
}

/// 見せない閲覧者に出す本文: だれにでも見せてよい部分（今は無し）と、そのあとの案内
pub async fn teaser_with_notice(
    state: &AppState,
    fields: &HashMap<String, serde_json::Value>,
    blocks_json: &str,
    notice_html: &str,
) -> String {
    let blocks: Vec<aegis_ast::BlockNode> = serde_json::from_str(blocks_json).unwrap_or_default();
    let teaser = shared_blocks(fields, blocks);
    let teaser_html = if teaser.is_empty() {
        String::new()
    } else {
        crate::render::render_content_html(state, &teaser).await
    };
    format!("{teaser_html}{notice_html}")
}

/// 公開済みの記事・ページに付くコメントの API 用。制限した記事のコメントを、見られない閲覧者に出さない。
/// 見られないなら `403`（記事が無い・コメントの対象でないときは `None` を返し、元の処理に任せる）
pub async fn deny_comments(
    state: &AppState,
    headers: &HeaderMap,
    content_type: &str,
    slug: &str,
) -> Option<(axum::http::StatusCode, axum::Json<serde_json::Value>)> {
    let schema = state.schemas.read().await.get(content_type).cloned()?;
    let record = state
        .db
        .get_published_content(&schema, slug)
        .await
        .ok()
        .flatten()?;
    if !record.is_restricted() {
        return None;
    }
    let path = crate::handlers::public::public_content_path(state, content_type, slug);
    let content = ContentRef {
        content_type: &record.content_type,
        id: &record.id,
        slug: &record.slug,
    };
    match gate(state, headers, &record.custom_fields, &path, content).await {
        Gate::Denied { .. } => Some((
            axum::http::StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({
                "error": "This content is restricted",
                "code": "access_restricted"
            })),
        )),
        _ => None,
    }
}

/// 制限の判定の結果
pub enum Gate {
    /// 制限なし。これまでどおり（共有キャッシュに載せてよい）
    Open,
    /// 制限があり、この閲覧者は見られる
    Allowed,
    /// 制限があり、この閲覧者は見られない。本文の代わりに出す案内（HTML）
    Denied { notice_html: String },
}

impl Gate {
    /// 閲覧者によって応答が変わる（共有キャッシュ・CDN に載せてはならない）
    pub fn is_restricted(&self) -> bool {
        !matches!(self, Gate::Open)
    }

    pub fn is_denied(&self) -> bool {
        matches!(self, Gate::Denied { .. })
    }
}

/// 判定の対象の記事・ページ。購入の権利を調べ、購入の案内を作るのに使う
#[derive(Debug, Clone, Copy)]
pub struct ContentRef<'a> {
    pub content_type: &'a str,
    pub id: &'a str,
    pub slug: &'a str,
}

/// ログイン中の有効な会員（ID とレベル）。未ログイン・停止中・会員アカウントが無効なら `None`
async fn member_session(state: &AppState, headers: &HeaderMap) -> Option<(String, String)> {
    let service = state.members.as_ref()?;
    let token = crate::handlers::members::session_token(state, headers)?;
    let session = service.verify_session(&token).await.ok()??;
    let member = service.get_profile(&session.member_id).await.ok()?;
    if member.status != "active" {
        return None;
    }
    // 手動で付けたレベルと、契約中のプランのレベルの高いほう
    let granted = crate::handlers::subscription::granted_levels(state, &member.id).await;
    let granted: Vec<&str> = granted.iter().map(String::as_str).collect();
    let level =
        aegis_members::effective_level(&state.config.members.levels, &member.level, &granted)
            .to_string();
    Some((member.id, level))
}

/// 購入が使えるか（会員アカウントと `[members.purchase]` がどちらも有効）
pub fn purchase_enabled(state: &AppState) -> bool {
    state.members.is_some() && state.config.members.purchase.enabled
}

/// パスワード保護を解除した記事のトークンを持つ Cookie の名前
pub fn password_cookie_name(state: &AppState) -> String {
    format!("{}_pass", state.config.server.namespace().replace('-', "_"))
}

/// パスワード入力の失敗（誤り・試行回数超過）をリダイレクト先に伝える Cookie の名前
pub fn password_error_cookie_name(state: &AppState) -> String {
    format!(
        "{}_pass_err",
        state.config.server.namespace().replace('-', "_")
    )
}

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

/// 記事 ID とその記事の現在の `access_password`（Argon2id ハッシュ）から、その記事専用の解除トークンを派生させる。
/// `access_password` のハッシュ自体は外部に出さず、パスワードを変えるたびにソルトごと変わるため、
/// パスワードを変更・解除した瞬間に古い Cookie のトークンは自動的に無効になる
pub fn password_unlock_token(content_type: &str, content_id: &str, password_hash: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"aegis-content-pass:");
    hasher.update(content_type.as_bytes());
    hasher.update(b":");
    hasher.update(content_id.as_bytes());
    hasher.update(b":");
    hasher.update(password_hash.as_bytes());
    let digest = hasher.finalize();
    hex::encode(&digest[..16])
}

/// この記事のパスワード保護が Cookie で解除されているか
fn has_password_unlock(
    state: &AppState,
    headers: &HeaderMap,
    fields: &HashMap<String, serde_json::Value>,
    content: ContentRef<'_>,
) -> bool {
    if content.id.is_empty() {
        return false;
    }
    let Some(hash) = aegis_db::access_password_of(fields) else {
        return false;
    };
    let expected = password_unlock_token(content.content_type, content.id, hash);
    let Some(raw) = cookie_value(headers, &password_cookie_name(state)) else {
        return false;
    };
    raw.split('.').any(|token| token == expected)
}

/// `fields["access"]` を見て、この閲覧者に本文を見せてよいかを決める。
/// `path` は案内のログインのリンクやパスワード送信後に戻る先（サイト内のパス）。
/// レベルが足りなくても、その記事を買っている会員には見せる（docs/members.md 16.5）
pub async fn gate(
    state: &AppState,
    headers: &HeaderMap,
    fields: &HashMap<String, serde_json::Value>,
    path: &str,
    content: ContentRef<'_>,
) -> Gate {
    // 制限のないページでは、閲覧者（セッションの検索）を引かない
    if !aegis_db::is_restricted(fields) {
        return Gate::Open;
    }
    if aegis_db::is_password_protected(fields) {
        if has_password_unlock(state, headers, fields, content) {
            return Gate::Allowed;
        }
        let err = cookie_value(headers, &password_error_cookie_name(state));
        return Gate::Denied {
            notice_html: password_notice(state, path, Some(content), err.as_deref()),
        };
    }
    let member = member_session(state, headers).await;
    let viewer = match member.as_ref() {
        Some((_, level)) => Viewer::Member { level },
        None => Viewer::Guest,
    };
    let mut owned = false;
    if let Some((member_id, _)) = &member {
        // 買っているかは、レベルで読めないときだけ調べる（DB の読み取りを減らす）。
        // 調べられなかったら、持っていないものとして扱う（見せない側に倒す）
        let required = aegis_db::access_of(fields);
        let levels = &state.config.members.levels;
        if purchase_enabled(state)
            && aegis_db::purchase_price_of(fields).is_some()
            && !aegis_members::may_view(levels, required, viewer)
        {
            owned = state
                .db
                .has_paid_purchase(member_id, content.content_type, content.id)
                .await
                .unwrap_or(false);
        }
    }
    decide_with(state, fields, viewer, path, Some(content), owned)
}

/// 閲覧者が決まっているときの判定（公開ページは Cookie から引いた閲覧者、管理画面のプレビューは選んだ閲覧者）。
/// 購入の権利・パスワード解除 Cookie は調べない（プレビューは「未購入・パスワード未入力」の姿を出す）
pub fn decide(
    state: &AppState,
    fields: &HashMap<String, serde_json::Value>,
    viewer: Viewer<'_>,
    path: &str,
    content: Option<ContentRef<'_>>,
) -> Gate {
    decide_with(state, fields, viewer, path, content, false)
}

fn decide_with(
    state: &AppState,
    fields: &HashMap<String, serde_json::Value>,
    viewer: Viewer<'_>,
    path: &str,
    content: Option<ContentRef<'_>>,
    owned: bool,
) -> Gate {
    let required = fields.get(ACCESS_FIELD).and_then(|v| v.as_str());
    if aegis_members::is_public(required) {
        return Gate::Open;
    }
    if aegis_db::is_password_protected(fields) {
        if matches!(viewer, Viewer::Staff) {
            return Gate::Allowed;
        }
        return Gate::Denied {
            notice_html: password_notice(state, path, content, None),
        };
    }
    let required = required.map(str::trim).unwrap_or_default();
    let levels = &state.config.members.levels;
    if owned || aegis_members::may_view(levels, Some(required), viewer) {
        return Gate::Allowed;
    }
    let offer = content.and_then(|content| purchase_offer(state, fields, content));
    Gate::Denied {
        notice_html: notice(
            state,
            required,
            matches!(viewer, Viewer::Member { .. }),
            path,
            offer.as_ref(),
        ),
    }
}

/// 案内に出す購入の申し出（金額は設定した通貨で整形する）
struct PurchaseOffer {
    price_text: String,
    buy_path: String,
}

fn purchase_offer(
    state: &AppState,
    fields: &HashMap<String, serde_json::Value>,
    content: ContentRef<'_>,
) -> Option<PurchaseOffer> {
    if !purchase_enabled(state) {
        return None;
    }
    let price = aegis_db::purchase_price_of(fields)?;
    Some(PurchaseOffer {
        price_text: aegis_core::format_money(price, &state.config.members.purchase.currency),
        buy_path: format!(
            "/account/purchase?content_type={}&slug={}",
            urlencode(content.content_type),
            urlencode(content.slug)
        ),
    })
}

/// 本文の代わりに出す案内。ゲストにはログインと会員登録、レベルが足りない会員には必要なレベルと、
/// 設定していれば `[members].upgrade_url` のリンク、買える記事なら購入のリンクを出す
fn notice(
    state: &AppState,
    required: &str,
    logged_in: bool,
    path: &str,
    offer: Option<&PurchaseOffer>,
) -> String {
    let locale = &state.config.theme.locale;
    let t = |key: &str| aegis_theme::messages::translate(locale, key, &[]);
    let escape = aegis_ast::renderer::escape_html;
    let members = &state.config.members;

    let mut links = Vec::new();
    let mut extra = String::new();
    let message = if logged_in {
        let params = [("level".to_string(), required.to_string())];
        if let Some(offer) = offer {
            let params = [("price".to_string(), offer.price_text.clone())];
            links.push(format!(
                r#"<a class="aegis-access-buy" href="{}">{}</a>"#,
                escape(&offer.buy_path),
                aegis_theme::messages::translate(locale, "access.buy", &params)
            ));
            extra = format!(
                r#"<p class="aegis-access-note">{}</p>"#,
                t("access.buy_note")
            );
        }
        if !members.upgrade_url.trim().is_empty() {
            links.push(format!(
                r#"<a href="{}">{}</a>"#,
                escape(members.upgrade_url.trim()),
                t("access.upgrade")
            ));
        } else if purchase_enabled(state) && !members.purchase.plans.is_empty() {
            links.push(format!(
                r##"<a href="/account#aegis-membership">{}</a>"##,
                t("access.plans")
            ));
        }
        aegis_theme::messages::translate(locale, "access.needs_level", &params)
    } else {
        if state.members.is_some() {
            // ログインのあとに、この記事へ戻る（`next` はサイト内のパスだけを受け付ける）
            let next = urlencode(path);
            links.push(format!(
                r#"<a href="/account/login?next={next}">{}</a>"#,
                t("access.login")
            ));
            if members.registration == aegis_members::MemberRegistration::Open {
                links.push(format!(
                    r#"<a href="/account/register?next={next}">{}</a>"#,
                    t("access.register")
                ));
            }
        }
        match offer {
            Some(offer) => {
                let params = [("price".to_string(), offer.price_text.clone())];
                aegis_theme::messages::translate(locale, "access.purchasable", &params)
            }
            None => t("access.members_only"),
        }
    };

    let links = if links.is_empty() {
        String::new()
    } else {
        format!(r#"<p class="aegis-access-links">{}</p>"#, links.join(" · "))
    };
    format!(r#"<div class="aegis-access-notice" role="note"><p>{message}</p>{links}{extra}</div>"#)
}

/// パスワード（合言葉）で保護された記事・ページの入力フォーム
fn password_notice(
    state: &AppState,
    path: &str,
    content: Option<ContentRef<'_>>,
    error_code: Option<&str>,
) -> String {
    let locale = &state.config.theme.locale;
    let t = |key: &str| aegis_theme::messages::translate(locale, key, &[]);
    let escape = aegis_ast::renderer::escape_html;
    let base = state
        .config
        .server
        .normalized_base_path()
        .unwrap_or_default();
    let action = escape(&format!("{base}/api/v1/content-auth"));
    let (ct, slug) = match content {
        Some(c) => (escape(c.content_type), escape(c.slug)),
        None => (String::new(), String::new()),
    };
    let return_to = escape(path);
    let error_html = match error_code {
        Some("rate_limited") => format!(
            r#"<p class="aegis-access-error" role="alert">{}</p>"#,
            t("access.password_rate_limited")
        ),
        Some("invalid") => format!(
            r#"<p class="aegis-access-error" role="alert">{}</p>"#,
            t("access.password_invalid")
        ),
        _ => String::new(),
    };
    let label = escape(&t("access.password_label"));
    format!(
        r#"<div class="aegis-access-notice" role="note"><p>{}</p>{error_html}<form class="aegis-access-form" method="post" action="{action}"><input type="hidden" name="content_type" value="{ct}"><input type="hidden" name="slug" value="{slug}"><input type="hidden" name="return_to" value="{return_to}"><input type="password" name="password" placeholder="{label}" aria-label="{label}" autocomplete="current-password" required><button type="submit">{}</button></form></div>"#,
        t("access.password_protected"),
        t("access.password_submit"),
    )
}

#[derive(Debug, Default, Deserialize)]
pub struct ContentAuthRequest {
    pub content_type: String,
    pub slug: String,
    pub password: String,
    #[serde(default)]
    pub return_to: Option<String>,
}

fn parse_auth_request(headers: &HeaderMap, body: &[u8]) -> Option<(ContentAuthRequest, bool)> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let accepts_json = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|a| a.contains("application/json"));
    if content_type.starts_with("application/json") {
        let req = serde_json::from_slice::<ContentAuthRequest>(body).ok()?;
        return Some((req, true));
    }
    let mut req = ContentAuthRequest::default();
    for (k, v) in url::form_urlencoded::parse(body) {
        match k.as_ref() {
            "content_type" => req.content_type = v.into_owned(),
            "slug" => req.slug = v.into_owned(),
            "password" => req.password = v.into_owned(),
            "return_to" => req.return_to = Some(v.into_owned()),
            _ => {}
        }
    }
    if req.content_type.is_empty() && req.slug.is_empty() {
        if let Ok(json_req) = serde_json::from_slice::<ContentAuthRequest>(body) {
            return Some((json_req, true));
        }
        return None;
    }
    Some((req, accepts_json))
}

fn is_https(state: &AppState, headers: &HeaderMap) -> bool {
    headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|proto| proto.eq_ignore_ascii_case("https"))
        || state.config.members.public_url.starts_with("https://")
}

fn build_cookie(
    state: &AppState,
    headers: &HeaderMap,
    name: &str,
    value: &str,
    max_age_secs: u64,
) -> HeaderValue {
    let secure = if is_https(state, headers) {
        "; Secure"
    } else {
        ""
    };
    let cookie =
        format!("{name}={value}; Path=/; Max-Age={max_age_secs}; HttpOnly; SameSite=Lax{secure}");
    HeaderValue::from_str(&cookie).unwrap_or_else(|_| HeaderValue::from_static(""))
}

fn safe_return_location(state: &AppState, req: &ContentAuthRequest) -> String {
    let base = state
        .config
        .server
        .normalized_base_path()
        .unwrap_or_default();
    let local = req
        .return_to
        .as_deref()
        .map(str::trim)
        .filter(|p| {
            p.starts_with('/')
                && !p.starts_with("//")
                && !p.contains('\\')
                && !p.chars().any(char::is_control)
        })
        .map(str::to_string)
        .unwrap_or_else(|| {
            crate::handlers::public::public_content_path(state, &req.content_type, &req.slug)
        });
    format!("{base}{local}")
}

/// `POST /api/v1/content-auth` — パスワード（合言葉）で保護された記事・ページの閲覧を解除する。
/// 公開ページの `<form method="post">` からは `303 See Other` で元のページに戻し、JSON の要求には JSON で応答する
pub async fn unlock_content_password(
    State(state): State<Arc<AppState>>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some((req, wants_json)) = parse_auth_request(&headers, &body) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "content_type, slug and password are required",
                "code": "invalid_request"
            })),
        )
            .into_response();
    };
    let location = safe_return_location(&state, &req);
    let err_cookie_name = password_error_cookie_name(&state);

    // 試行回数の制限（同じ IP・記事への総当たりと Argon2 の負荷を防ぐ）
    let limiter_key = format!(
        "content_pass:{}:{}:{}",
        client_ip.as_deref().unwrap_or("unknown"),
        req.content_type,
        req.slug
    );
    match state
        .login_limiter
        .check(&limiter_key, client_ip.as_deref())
        .await
    {
        Ok(Some(lockout)) => {
            if wants_json {
                return (
                    StatusCode::TOO_MANY_REQUESTS,
                    [(header::RETRY_AFTER, lockout.retry_after_secs.to_string())],
                    Json(json!({
                        "error": "Too many failed password attempts. Try again later.",
                        "code": "rate_limited",
                        "retry_after_secs": lockout.retry_after_secs,
                    })),
                )
                    .into_response();
            }
            let cookie = build_cookie(
                &state,
                &headers,
                &err_cookie_name,
                "rate_limited",
                PASSWORD_ERROR_COOKIE_TTL_SECS,
            );
            return (
                StatusCode::SEE_OTHER,
                [
                    (
                        header::LOCATION,
                        HeaderValue::from_str(&location)
                            .unwrap_or_else(|_| HeaderValue::from_static("/")),
                    ),
                    (header::SET_COOKIE, cookie),
                ],
            )
                .into_response();
        }
        Ok(None) => {}
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    }

    let schema = {
        let schemas = state.schemas.read().await;
        schemas.get(&req.content_type).cloned()
    };
    let record = match schema {
        Some(ref s) => state
            .db
            .get_published_content(s, &req.slug)
            .await
            .ok()
            .flatten()
            .filter(|r| {
                is_currently_published(&r.status, r.unpublish_at.as_deref(), jiff::Timestamp::now())
                    && aegis_db::is_password_protected(&r.custom_fields)
            }),
        None => None,
    };

    let stored_hash = record
        .as_ref()
        .and_then(|r| aegis_db::access_password_of(&r.custom_fields))
        .map(str::to_string);

    let password = req.password.clone();
    let hash_for_verify = stored_hash.clone();
    let matched = tokio::task::spawn_blocking(move || match hash_for_verify {
        Some(hash) => aegis_auth::password::verify_password(&password, &hash).unwrap_or(false),
        None => {
            aegis_auth::password::verify_dummy_password(&password);
            false
        }
    })
    .await
    .unwrap_or(false);

    let (Some(record), Some(stored_hash)) = (record, stored_hash) else {
        if wants_json {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": "Password-protected content not found",
                    "code": "not_found"
                })),
            )
                .into_response();
        }
        return (
            StatusCode::SEE_OTHER,
            [(
                header::LOCATION,
                HeaderValue::from_str(&location).unwrap_or_else(|_| HeaderValue::from_static("/")),
            )],
        )
            .into_response();
    };

    if !matched {
        let _ = state
            .login_limiter
            .record_failure_with_max_ip(
                &limiter_key,
                client_ip.as_deref(),
                state.system().max_login_attempts,
            )
            .await;
        if wants_json {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "error": "Invalid password",
                    "code": "invalid_password"
                })),
            )
                .into_response();
        }
        let cookie = build_cookie(
            &state,
            &headers,
            &err_cookie_name,
            "invalid",
            PASSWORD_ERROR_COOKIE_TTL_SECS,
        );
        return (
            StatusCode::SEE_OTHER,
            [
                (
                    header::LOCATION,
                    HeaderValue::from_str(&location)
                        .unwrap_or_else(|_| HeaderValue::from_static("/")),
                ),
                (header::SET_COOKIE, cookie),
            ],
        )
            .into_response();
    }

    let _ = state.login_limiter.record_success(&limiter_key).await;
    let token = password_unlock_token(&record.content_type, &record.id, &stored_hash);
    let pass_cookie_name = password_cookie_name(&state);
    let mut tokens = vec![token.clone()];
    if let Some(existing) = cookie_value(&headers, &pass_cookie_name) {
        for item in existing.split('.') {
            if !item.is_empty() && item != token && tokens.len() < MAX_UNLOCKED_ITEMS {
                tokens.push(item.to_string());
            }
        }
    }
    let joined = tokens.join(".");
    let pass_cookie = build_cookie(
        &state,
        &headers,
        &pass_cookie_name,
        &joined,
        PASSWORD_COOKIE_TTL_SECS,
    );
    let clear_err_cookie = build_cookie(&state, &headers, &err_cookie_name, "", 0);

    let mut response = if wants_json {
        (StatusCode::OK, Json(json!({ "status": "unlocked" }))).into_response()
    } else {
        (
            StatusCode::SEE_OTHER,
            [(
                header::LOCATION,
                HeaderValue::from_str(&location).unwrap_or_else(|_| HeaderValue::from_static("/")),
            )],
        )
            .into_response()
    };
    response
        .headers_mut()
        .append(header::SET_COOKIE, pass_cookie);
    response
        .headers_mut()
        .append(header::SET_COOKIE, clear_err_cookie);
    make_private(response)
}

/// URL のクエリに入れる文字列のパーセントエンコード
fn urlencode(value: &str) -> String {
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

/// 閲覧者によって変わる応答は、共有キャッシュ・CDN に載せない（会員の Cookie があるときだけ本文が出る。
/// ゲスト向けの案内を CDN が覚えると、会員にも案内が返ってしまう）
pub fn make_private(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urlencode_keeps_unreserved_and_escapes_the_rest() {
        assert_eq!(urlencode("/articles/a-b_c.d~e"), "%2Farticles%2Fa-b_c.d~e");
        assert_eq!(urlencode("/a b?x=1&y"), "%2Fa%20b%3Fx%3D1%26y");
        assert_eq!(urlencode("/記事"), "%2F%E8%A8%98%E4%BA%8B");
    }
}
