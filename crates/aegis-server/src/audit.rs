//! リクエストから監査ログの操作主体・クライアント情報を取り出す extractor

use aegis_audit::AuditOrigin;
use aegis_core::AuditActor;
use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;
use axum::http::HeaderMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use crate::auth::extract_session_id;
use crate::state::AppState;

/// リクエストから解決した監査用のコンテキスト（`AuditOrigin`: 操作主体・クライアント情報）。
/// 抽出は失敗しない。セッションが無い・無効な場合は `actor` が `None` になる。
/// `Deref` で `AuditOrigin` として使える（`audit.event(...)`、`&AuditOrigin` を受け取る関数）
#[derive(Debug, Clone, Default)]
pub struct AuditContext(pub AuditOrigin);

impl std::ops::Deref for AuditContext {
    type Target = AuditOrigin;

    fn deref(&self) -> &AuditOrigin {
        &self.0
    }
}

impl FromRequestParts<Arc<AppState>> for AuditContext {
    type Rejection = Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        if !state.audit.is_enabled() {
            return Ok(Self::default());
        }

        let actor = match extract_session_id(&parts.headers) {
            Some(id) => match state.sessions.get_session(&id).await {
                Ok(Some(session)) => Some(AuditActor {
                    user_id: session.user_id,
                    role: session.role,
                }),
                Ok(None) => None,
                Err(e) => {
                    tracing::warn!("Audit: failed to resolve session: {}", e);
                    None
                }
            },
            None => None,
        };

        let ip = client_ip(
            parts,
            state.audit.trust_forwarded_for(),
            state.audit.forwarded_for_hops(),
        );

        let user_agent = parts
            .headers
            .get(axum::http::header::USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.chars().take(512).collect());

        Ok(Self(AuditOrigin {
            actor,
            ip,
            user_agent,
        }))
    }
}

/// リクエストのクライアント IP（`[audit].trust_forwarded_for` が有効なら `X-Forwarded-For` の先頭を優先）
/// 抽出は失敗しない。接続情報が無い場合は `None` になる
#[derive(Debug, Clone, Default)]
pub struct ClientIp(pub Option<String>);

impl FromRequestParts<Arc<AppState>> for ClientIp {
    type Rejection = Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self(client_ip(
            parts,
            state.audit.trust_forwarded_for(),
            state.audit.forwarded_for_hops(),
        )))
    }
}

fn client_ip(parts: &Parts, trust_forwarded_for: bool, hops: u32) -> Option<String> {
    let peer = parts
        .extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(addr)| addr.ip().to_string());
    if trust_forwarded_for {
        forwarded_for(&parts.headers, hops).or(peer)
    } else {
        peer
    }
}

/// `X-Forwarded-For` から、信頼できるプロキシ `hops` 段分を右端から除いたアドレスを取り出す。
/// 先頭はクライアントが自由に書けるので使わない（プロキシは右に追記する）。
/// 段数より短い・読めないときは `None`（呼び出し側は接続元アドレスを使う）
fn forwarded_for(headers: &HeaderMap, hops: u32) -> Option<String> {
    // ヘッダーが複数行に分かれていても、届いた順につなげる
    let mut addresses: Vec<String> = Vec::new();
    for value in headers.get_all("x-forwarded-for") {
        let value = value.to_str().ok()?;
        addresses.extend(value.split(',').map(|s| s.trim().to_string()));
    }
    let index = addresses.len().checked_sub(hops.max(1) as usize)?;
    let address = addresses.swap_remove(index);
    // アドレスとして読めるものだけ（キャッシュのキーや監査ログに任意の文字列を入れさせない）
    address
        .parse::<std::net::IpAddr>()
        .ok()
        .map(|ip| ip.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_forwarded_for_counts_trusted_hops_from_the_right() {
        let mut headers = HeaderMap::new();
        assert_eq!(forwarded_for(&headers, 1), None);

        // クライアントが先頭に偽の値を書いても、プロキシが右に足した実 IP が採用される
        headers.insert(
            "x-forwarded-for",
            "198.51.100.99, 203.0.113.7".parse().unwrap(),
        );
        assert_eq!(forwarded_for(&headers, 1), Some("203.0.113.7".to_string()));
        // プロキシが 2 段（CDN → LB）なら右から 2 番目
        headers.insert(
            "x-forwarded-for",
            "198.51.100.99, 203.0.113.7, 10.0.0.2".parse().unwrap(),
        );
        assert_eq!(forwarded_for(&headers, 2), Some("203.0.113.7".to_string()));
        // 段数より短いヘッダーは信用しない
        assert_eq!(forwarded_for(&headers, 4), None);
        // 複数行のヘッダーもつなげて数える
        headers.append("x-forwarded-for", "192.0.2.1".parse().unwrap());
        assert_eq!(forwarded_for(&headers, 1), Some("192.0.2.1".to_string()));
        // アドレスでない値は使わない
        let mut junk = HeaderMap::new();
        junk.insert("x-forwarded-for", "1.2.3.4, <script>".parse().unwrap());
        assert_eq!(forwarded_for(&junk, 1), None);
    }
}
