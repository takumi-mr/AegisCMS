use aegis_core::{AuditAction, AuditActor, AuditEvent, AuditOutcome};
use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;
use axum::http::HeaderMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use crate::auth::extract_session_id;
use crate::state::AppState;

/// リクエストから解決した監査用のコンテキスト（操作主体・クライアント情報）
/// 抽出は失敗しない。セッションが無い・無効な場合は `actor` が `None` になる
#[derive(Debug, Clone, Default)]
pub struct AuditContext {
    pub actor: Option<AuditActor>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

impl AuditContext {
    /// 操作主体とクライアント情報を埋めた監査イベントを生成する
    pub fn event(&self, action: AuditAction, outcome: AuditOutcome) -> AuditEvent {
        AuditEvent::new(action, outcome)
            .with_actor(self.actor.clone())
            .with_client(self.ip.clone(), self.user_agent.clone())
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

        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(addr)| addr.ip().to_string());
        let ip = if state.audit.trust_forwarded_for() {
            forwarded_for(&parts.headers).or(peer)
        } else {
            peer
        };

        let user_agent = parts
            .headers
            .get(axum::http::header::USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.chars().take(512).collect());

        Ok(Self {
            actor,
            ip,
            user_agent,
        })
    }
}

/// `X-Forwarded-For` の先頭（最初のクライアント）アドレスを取り出す
fn forwarded_for(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_forwarded_for_takes_first_address() {
        let mut headers = HeaderMap::new();
        assert_eq!(forwarded_for(&headers), None);

        headers.insert("x-forwarded-for", "203.0.113.7, 10.0.0.2".parse().unwrap());
        assert_eq!(forwarded_for(&headers), Some("203.0.113.7".to_string()));
    }
}
