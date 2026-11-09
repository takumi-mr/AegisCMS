//! 公開ページの閲覧の制限（会員レベル）。docs/members.md 15。
//!
//! 記事・ページの `access`（必要なレベル）と閲覧者（Cookie / Bearer の会員のセッション）から、本文を
//! 見せるかを決める。判定そのものは `aegis_members::may_view`。ここは、閲覧者を引いて、見せないときの案内を作る。

use axum::http::{header, HeaderMap, HeaderValue};
use axum::response::Response;
use std::collections::HashMap;

use crate::state::AppState;
use aegis_members::Viewer;

pub use aegis_schema::ACCESS_FIELD;

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
    (member.status == "active").then_some((member.id, member.level))
}

/// 購入が使えるか（会員アカウントと `[members.purchase]` がどちらも有効）
pub fn purchase_enabled(state: &AppState) -> bool {
    state.members.is_some() && state.config.members.purchase.enabled
}

/// `fields["access"]` を見て、この閲覧者に本文を見せてよいかを決める。
/// `path` は案内のログインのリンクが、ログイン後に戻る先（サイト内のパス）。
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
/// 購入の権利は調べない（プレビューは「購入していない会員」の姿を出す）
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
