//! 公開ページに出る識別子の接頭辞（`[server].namespace`）。
//!
//! 組み込みのテンプレート・スクリプト・スタイル・テーマ・プラグインは、CSS のクラス名・`id`・`data-*` 属性・
//! Cookie と localStorage のキー・JS のグローバル名・配信パスを `aegis-` / `aegis_` / `Aegis…` / `/_aegis/` の形で
//! 書いている。応答が返る直前に 1 か所でこれを設定の接頭辞に置き換えるので、HTML を作る側（各クレート・
//! プラグイン・自作テーマ）は接頭辞を意識しなくてよく、HTML・CSS・JS で同じ置き換えが掛かるため互いに食い違わない
//!
//! 対象は `text/html`・`text/css`・JavaScript の応答。API（`/api/`）、Admin Studio、アップロードは触らない。
//! 接頭辞が `aegis` なら何もしない

use std::borrow::Cow;
use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::state::AppState;

/// 読み込む応答の大きさの上限
const MAX_BODY_BYTES: usize = 16 * 1024 * 1024;

/// 置き換えの元になる接頭辞
const SOURCE: &str = "aegis";

/// 先頭の 1 文字を大文字にする（`cms` → `Cms`）
fn capitalized(ns: &str) -> String {
    let mut chars = ns.chars();
    match chars.next() {
        Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

/// `text` の中の `aegis` 接頭辞の識別子を `ns` に置き換える。置き換える所が無ければ借用のまま返す
///
/// - `aegis-…` `aegis_…` `aegis:…`（クラス名・属性・Cookie・イベント名）: 直前が英数字・`/` でないとき
/// - `/_aegis/`（配信パス）
/// - `Aegis` + 大文字（`AegisConsent` など。製品名の `AegisCMS` は除く）
/// - `AEGIS_…`（定数名）
pub fn rewrite<'a>(text: &'a str, ns: &str) -> Cow<'a, str> {
    if ns == SOURCE {
        return Cow::Borrowed(text);
    }
    let bytes = text.as_bytes();
    let mut out: Option<String> = None;
    let mut copied = 0;
    let mut i = 0;
    while i + SOURCE.len() <= bytes.len() {
        let head = &bytes[i..i + SOURCE.len()];
        let after = bytes.get(i + SOURCE.len()).copied();
        let before = if i == 0 { None } else { Some(bytes[i - 1]) };
        let replacement = if head == b"aegis" {
            let separated = matches!(after, Some(b'-' | b'_' | b':'))
                && !matches!(before, Some(b'/'))
                && !before.is_some_and(|b| b.is_ascii_alphanumeric());
            let asset_path = before == Some(b'_') && after == Some(b'/');
            (separated || asset_path).then(|| ns.to_string())
        } else if head == b"Aegis" {
            let camel = after.is_some_and(|b| b.is_ascii_uppercase())
                && !bytes[i + SOURCE.len()..].starts_with(b"CMS");
            camel.then(|| capitalized(ns))
        } else if head == b"AEGIS" {
            (after == Some(b'_')).then(|| ns.to_ascii_uppercase())
        } else {
            None
        };
        match replacement {
            Some(r) => {
                let out = out.get_or_insert_with(|| String::with_capacity(text.len()));
                out.push_str(&text[copied..i]);
                out.push_str(&r);
                i += SOURCE.len();
                copied = i;
            }
            None => i += 1,
        }
    }
    match out {
        Some(mut out) => {
            out.push_str(&text[copied..]);
            Cow::Owned(out)
        }
        None => Cow::Borrowed(text),
    }
}

/// 置き換える応答の種類（HTML・CSS・JavaScript）
fn is_rewritable(content_type: &str) -> bool {
    let kind = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    matches!(
        kind.as_str(),
        "text/html" | "text/css" | "text/javascript" | "application/javascript"
    )
}

/// 触らない URL のパス（ベースパスを除いたもの）
fn is_excluded(state: &AppState, path: &str) -> bool {
    let admin = state.config.server.admin_path();
    let uploads = state.config.storage.local.public_url_prefix.clone();
    [
        "/api",
        admin.trim_end_matches('/'),
        uploads.trim_end_matches('/'),
    ]
    .iter()
    .any(|prefix| {
        !prefix.is_empty() && (path == *prefix || path.starts_with(&format!("{prefix}/")))
    })
}

pub async fn namespace_middleware(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    let ns = state.config.server.namespace();
    let wanted = ns != SOURCE && !is_excluded(&state, request.uri().path());
    let response = next.run(request).await;
    if !wanted {
        return response;
    }
    let rewritable = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(is_rewritable);
    if !rewritable {
        return response;
    }

    let (mut parts, body) = response.into_parts();
    let bytes = match axum::body::to_bytes(body, MAX_BODY_BYTES).await {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::error!("Could not read a response to rename its identifiers: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let rewritten = std::str::from_utf8(&bytes)
        .ok()
        .and_then(|text| match rewrite(text, &ns) {
            Cow::Owned(changed) => Some(changed),
            Cow::Borrowed(_) => None,
        });
    let body = match rewritten {
        Some(text) => {
            // 本文が変わるので、元の本文に付いていた長さと ETag は合わない
            parts.headers.remove(header::CONTENT_LENGTH);
            parts.headers.remove(header::ETAG);
            Body::from(text)
        }
        None => Body::from(bytes),
    };
    Response::from_parts(parts, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str) -> String {
        rewrite(text, "x7").into_owned()
    }

    #[test]
    fn test_renames_class_names_attributes_and_keys() {
        assert_eq!(
            run(
                r#"<div class="aegis-consent aegis-consent-pos-top" data-aegis-consent-root id="aegis-cart">"#
            ),
            r#"<div class="x7-consent x7-consent-pos-top" data-x7-consent-root id="x7-cart">"#
        );
        assert_eq!(
            run(".aegis-card:hover{} localStorage.getItem('aegis_cart_v1') document.cookie='aegis_consent=1'"),
            ".x7-card:hover{} localStorage.getItem('x7_cart_v1') document.cookie='x7_consent=1'"
        );
        assert_eq!(
            run("new CustomEvent('aegis:consent')"),
            "new CustomEvent('x7:consent')"
        );
    }

    #[test]
    fn test_renames_asset_paths_and_globals() {
        assert_eq!(
            run(r#"<script src="/cms/_aegis/consent.js?v=1" defer></script>"#),
            r#"<script src="/cms/_x7/consent.js?v=1" defer></script>"#
        );
        assert_eq!(
            run("window.AegisConsent = window.AegisCart; window.AEGIS_CART_I18N; window.__aegis_requote"),
            "window.X7Consent = window.X7Cart; window.X7_CART_I18N; window.__x7_requote"
        );
    }

    #[test]
    fn test_leaves_other_text_alone() {
        // 製品名・単語・URL・英数字に続く名前は識別子ではない
        let text = "Powered by AegisCMS. Aegis is a shield. https://example.com/aegis-cms/ plaegis-x aegis.toml";
        assert_eq!(rewrite(text, "x7"), Cow::Borrowed(text));
        assert_eq!(
            rewrite("class=\"aegis-a\"", "aegis"),
            Cow::Borrowed("class=\"aegis-a\"")
        );
    }

    #[test]
    fn test_multibyte_text_keeps_its_boundaries() {
        assert_eq!(
            run("日本語 <p class=\"aegis-tags\">タグ</p> 終わり"),
            "日本語 <p class=\"x7-tags\">タグ</p> 終わり"
        );
    }
}
