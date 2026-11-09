//! 公開ページの閲覧の制限（会員レベル）。docs/members.md 15。
//!
//! 記事・ページの `access`（必要なレベル）と閲覧者（Cookie / Bearer の会員のセッション）から、本文を
//! 見せるかを決める。判定そのものは `aegis_members::may_view`。ここは、閲覧者を引いて、見せないときの案内を作る。

use axum::http::{header, HeaderMap, HeaderValue};
use axum::response::Response;
use std::collections::HashMap;

use crate::state::AppState;
use aegis_members::Viewer;

/// 記事・ページのカスタムフィールド名（必要なレベル。未設定・`public` は誰でも）
pub const ACCESS_FIELD: &str = "access";

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

/// ログイン中の有効な会員のレベル。未ログイン・停止中・会員アカウントが無効なら `None`
async fn member_level(state: &AppState, headers: &HeaderMap) -> Option<String> {
    let service = state.members.as_ref()?;
    let token = crate::handlers::members::session_token(state, headers)?;
    let session = service.verify_session(&token).await.ok()??;
    let member = service.get_profile(&session.member_id).await.ok()?;
    (member.status == "active").then_some(member.level)
}

/// `fields["access"]` を見て、この閲覧者に本文を見せてよいかを決める。
/// `path` は案内のログインのリンクが、ログイン後に戻る先（サイト内のパス）
pub async fn gate(
    state: &AppState,
    headers: &HeaderMap,
    fields: &HashMap<String, serde_json::Value>,
    path: &str,
) -> Gate {
    let required = fields.get(ACCESS_FIELD).and_then(|v| v.as_str());
    if aegis_members::is_public(required) {
        return Gate::Open;
    }
    let required = required.map(str::trim).unwrap_or_default();

    let level = member_level(state, headers).await;
    let viewer = match level.as_deref() {
        Some(level) => Viewer::Member { level },
        None => Viewer::Guest,
    };
    let levels = &state.config.members.levels;
    if aegis_members::may_view(levels, Some(required), viewer) {
        return Gate::Allowed;
    }
    Gate::Denied {
        notice_html: notice(state, required, level.is_some(), path),
    }
}

/// 本文の代わりに出す案内。ゲストにはログインと会員登録、レベルが足りない会員には必要なレベルと、
/// 設定していれば `[members].upgrade_url` のリンクを出す
fn notice(state: &AppState, required: &str, logged_in: bool, path: &str) -> String {
    let locale = &state.config.theme.locale;
    let t = |key: &str| aegis_theme::messages::translate(locale, key, &[]);
    let escape = aegis_ast::renderer::escape_html;
    let members = &state.config.members;

    let mut links = Vec::new();
    let message = if logged_in {
        let params = [("level".to_string(), required.to_string())];
        if !members.upgrade_url.trim().is_empty() {
            links.push(format!(
                r#"<a href="{}">{}</a>"#,
                escape(members.upgrade_url.trim()),
                t("access.upgrade")
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
        t("access.members_only")
    };

    let links = if links.is_empty() {
        String::new()
    } else {
        format!(r#"<p class="aegis-access-links">{}</p>"#, links.join(" · "))
    };
    format!(r#"<div class="aegis-access-notice" role="note"><p>{message}</p>{links}</div>"#)
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
