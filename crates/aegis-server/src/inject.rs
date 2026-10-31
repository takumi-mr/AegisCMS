//! 公開ページの HTML への差し込み。同意バナー（[`crate::consent`]）など、サイトの機能がテーマに関係なく出す
//! `<head>` の内容を、応答が返る直前に足す。ハンドラーごとに足さずに 1 か所で済み、404 ページや
//! プラグイン・テーマが作ったページにも付く
//!
//! 対象は `text/html` の `GET` 応答だけ。API（`/api/`）、Admin Studio、アップロード、この機能のアセット
//! （`/_aegis/`）は触らない。差し込む内容が無いとき（機能がすべて無効）は、本文を読まずに素通しする

use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Request, State},
    http::{header, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::state::AppState;

/// 読み込む HTML の大きさの上限（超えるものは触らずにそのまま返せないので、サーバーの誤りにする）
const MAX_HTML_BYTES: usize = 16 * 1024 * 1024;

/// `<head>` に足す内容（無ければ空）
fn head_snippet(state: &AppState) -> String {
    crate::consent::head_html(&state.config)
}

/// `</body>` の前に足す内容（無ければ空）
fn body_snippet(state: &AppState) -> String {
    crate::consent::body_html(state)
}

/// 差し込まない URL のパス（ベースパスを除いたもの）
fn is_excluded(state: &AppState, path: &str) -> bool {
    let admin = state.config.server.admin_path();
    let admin = admin.trim_end_matches('/');
    let prefix_of = |prefix: &str| path == prefix || path.starts_with(&format!("{}/", prefix));
    prefix_of("/api")
        || prefix_of("/_aegis")
        || (!admin.is_empty() && prefix_of(admin))
        || prefix_of(
            state
                .config
                .storage
                .local
                .public_url_prefix
                .trim_end_matches('/'),
        )
}

/// 最初の `</head>`（大文字・小文字を区別しない）の前に `snippet` を入れる。`</head>` が無ければ `None`
pub fn insert_into_head(html: &str, snippet: &str) -> Option<String> {
    insert_at(html, html.to_ascii_lowercase().find("</head>")?, snippet)
}

/// 最後の `</body>`（大文字・小文字を区別しない）の前に `snippet` を入れる。`</body>` が無ければ `None`
/// （本文の中に `</body>` という文字列があっても、文書の終わりのものを選ぶ）
pub fn insert_into_body(html: &str, snippet: &str) -> Option<String> {
    insert_at(html, html.to_ascii_lowercase().rfind("</body>")?, snippet)
}

fn insert_at(html: &str, at: usize, snippet: &str) -> Option<String> {
    let mut out = String::with_capacity(html.len() + snippet.len());
    out.push_str(&html[..at]);
    out.push_str(snippet);
    out.push_str(&html[at..]);
    Some(out)
}

pub async fn inject_middleware(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    let wanted = *request.method() == Method::GET && !is_excluded(&state, request.uri().path());
    let response = next.run(request).await;
    if !wanted {
        return response;
    }
    let is_html = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.trim_start().to_ascii_lowercase().starts_with("text/html"));
    if !is_html {
        return response;
    }
    let head = head_snippet(&state);
    if head.is_empty() {
        return response;
    }
    let body_snippet = body_snippet(&state);

    let (mut parts, body) = response.into_parts();
    let bytes = match axum::body::to_bytes(body, MAX_HTML_BYTES).await {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::error!(
                "Could not read an HTML response to add the site's tags: {}",
                e
            );
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let injected = std::str::from_utf8(&bytes).ok().and_then(|html| {
        // `</head>` が無い（断片や、head を持たないページ）なら、スクリプトが無いので何も足さない
        let with_head = insert_into_head(html, &head)?;
        Some(insert_into_body(&with_head, &body_snippet).unwrap_or(with_head))
    });
    let body = match injected {
        Some(html) => {
            parts.headers.remove(header::CONTENT_LENGTH);
            Body::from(html)
        }
        None => Body::from(bytes),
    };
    Response::from_parts(parts, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_into_head() {
        let html = "<html><head><title>x</title></head><body></body></html>";
        assert_eq!(
            insert_into_head(html, "<b>").unwrap(),
            "<html><head><title>x</title><b></head><body></body></html>"
        );
        // case-insensitive, first one only
        let upper = "<HTML><HEAD></HEAD><body><p></HEAD></p></body></HTML>";
        assert_eq!(
            insert_into_head(upper, "<b>").unwrap(),
            "<HTML><HEAD><b></HEAD><body><p></HEAD></p></body></HTML>"
        );
        assert_eq!(insert_into_head("<p>fragment</p>", "<b>"), None);
        // the body: the last </body>, even when the text mentions one
        let page = "<body><pre>&lt;/body&gt; and </BODY> in a string</pre></body></html>";
        assert_eq!(
            insert_into_body(page, "<i>").unwrap(),
            "<body><pre>&lt;/body&gt; and </BODY> in a string</pre><i></body></html>"
        );
        assert_eq!(insert_into_body("<p>no body</p>", "<i>"), None);
        // multibyte text before the tag keeps its boundaries
        let japanese = "<head><title>日本語</title></head>";
        assert!(insert_into_head(japanese, "<b>")
            .unwrap()
            .contains("日本語</title><b></head>"));
    }
}
