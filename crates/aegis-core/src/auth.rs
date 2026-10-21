use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::Result;

/// 外部認証プロバイダ（IdP）やローカル認証から確立されたユーザーの共通アイデンティティ
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthIdentity {
    /// プロバイダ側での一意な識別子 (例: SAML NameID, OIDC sub, ローカルユーザーID)
    pub external_id: String,
    /// ユーザーのメールアドレス
    pub email: String,
    /// 表示名（ユーザー名）
    #[serde(default)]
    pub display_name: Option<String>,
    /// システムロール (例: "admin", "editor", "viewer")
    #[serde(default = "default_role")]
    pub role: String,
    /// 追加のプロバイダ固有クレーム/属性 (例: SAML 属性、組織ID、グループ一覧等)
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

fn default_role() -> String {
    "editor".to_string()
}

impl AuthIdentity {
    pub fn new(external_id: impl Into<String>, email: impl Into<String>) -> Self {
        Self {
            external_id: external_id.into(),
            email: email.into(),
            display_name: None,
            role: default_role(),
            metadata: HashMap::new(),
        }
    }
}

/// 認証リクエスト時の入力ペイロード（ID/PW、SAMLResponse、OIDC 認可コードなど）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuthCredentials {
    /// 認証方式の種類やアクション (例: "password", "callback", "token")
    pub kind: String,
    /// 主キーまたはユーザー名 (例: ユーザー名, メールアドレス)
    #[serde(default)]
    pub identifier: Option<String>,
    /// 認証シークレットまたはトークン (例: パスワード, 認可コード)
    #[serde(default)]
    pub secret: Option<String>,
    /// 生のペイロードデータ (例: SAMLResponse XML, コールバック POST データ等)
    #[serde(default)]
    pub raw_payload: Option<Vec<u8>>,
    /// 追加のパラメータ (例: redirect_uri, state, nonce)
    #[serde(default)]
    pub parameters: HashMap<String, String>,
}

/// 認証プロバイダが要求する外部リダイレクト情報（SAML / OAuth 等のフロー用）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthRedirect {
    /// リダイレクト先 URL (例: IdP の Single Sign-On URL)
    pub url: String,
    /// リダイレクト時の HTTP メソッド ("GET" または "POST")
    pub method: String,
    /// POST 時に送信するフォームパラメータ（SAML Request 等）
    #[serde(default)]
    pub form_fields: HashMap<String, String>,
}

/// 認証プロバイダを抽象化する基底トレイト
/// SAML 2.0, OpenID Connect, LDAP, ローカルパスワード認証などを同一インターフェースで扱います
#[async_trait]
pub trait AuthProvider: Send + Sync {
    /// プロバイダのユニーク識別子 (例: "local", "saml", "oidc")
    fn provider_id(&self) -> &str;

    /// ログインフローの開始情報（IdP へのリダイレクト URL 等）を生成する
    /// ローカルパスワード認証など外部フローを伴わない場合は None を返します
    async fn initiate_login(&self, callback_url: &str) -> Result<Option<AuthRedirect>> {
        let _ = callback_url;
        Ok(None)
    }

    /// 認証クレデンシャルまたはコールバックデータを検証し、ユーザーのアイデンティティを確立する
    async fn authenticate(&self, credentials: &AuthCredentials) -> Result<AuthIdentity>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AegisError;

    struct MockLocalAuthProvider;

    #[async_trait]
    impl AuthProvider for MockLocalAuthProvider {
        fn provider_id(&self) -> &str {
            "local"
        }

        async fn authenticate(&self, credentials: &AuthCredentials) -> Result<AuthIdentity> {
            let id = credentials.identifier.as_deref().unwrap_or_default();
            let secret = credentials.secret.as_deref().unwrap_or_default();

            if id == "admin@example.com" && secret == "correct_password" {
                let mut identity = AuthIdentity::new("usr_local_1", "admin@example.com");
                identity.role = "admin".to_string();
                identity.display_name = Some("Admin User".to_string());
                Ok(identity)
            } else {
                Err(AegisError::Authentication(
                    "Invalid credentials".to_string(),
                ))
            }
        }
    }

    struct MockSamlAuthProvider;

    #[async_trait]
    impl AuthProvider for MockSamlAuthProvider {
        fn provider_id(&self) -> &str {
            "saml"
        }

        async fn initiate_login(&self, callback_url: &str) -> Result<Option<AuthRedirect>> {
            let redirect = AuthRedirect {
                url: format!("https://idp.example.com/sso?acs={}", callback_url),
                method: "GET".to_string(),
                form_fields: HashMap::new(),
            };
            Ok(Some(redirect))
        }

        async fn authenticate(&self, credentials: &AuthCredentials) -> Result<AuthIdentity> {
            if let Some(raw) = &credentials.raw_payload {
                let xml = String::from_utf8_lossy(raw);
                if xml.contains("NameID: user@enterprise.com") {
                    let mut identity =
                        AuthIdentity::new("saml_user_enterprise", "user@enterprise.com");
                    identity.role = "editor".to_string();
                    identity
                        .metadata
                        .insert("department".to_string(), serde_json::json!("Engineering"));
                    return Ok(identity);
                }
            }
            Err(AegisError::Authentication(
                "Invalid SAML payload".to_string(),
            ))
        }
    }

    #[tokio::test]
    async fn test_mock_local_auth_provider() {
        let provider = MockLocalAuthProvider;
        assert_eq!(provider.provider_id(), "local");

        // initiate_login should be None for local
        let redirect = provider.initiate_login("/callback").await.unwrap();
        assert!(redirect.is_none());

        // Failed authentication
        let failed_creds = AuthCredentials {
            identifier: Some("admin@example.com".to_string()),
            secret: Some("wrong_password".to_string()),
            ..Default::default()
        };
        assert!(provider.authenticate(&failed_creds).await.is_err());

        // Successful authentication
        let success_creds = AuthCredentials {
            identifier: Some("admin@example.com".to_string()),
            secret: Some("correct_password".to_string()),
            ..Default::default()
        };
        let identity = provider.authenticate(&success_creds).await.unwrap();
        assert_eq!(identity.email, "admin@example.com");
        assert_eq!(identity.role, "admin");
        assert_eq!(identity.display_name.as_deref(), Some("Admin User"));
    }

    #[tokio::test]
    async fn test_mock_saml_auth_provider() {
        let provider = MockSamlAuthProvider;
        assert_eq!(provider.provider_id(), "saml");

        // initiate_login should return AuthRedirect
        let redirect = provider
            .initiate_login("https://cms.example.com/api/v1/auth/saml/callback")
            .await
            .unwrap()
            .expect("Expected AuthRedirect");
        assert_eq!(redirect.method, "GET");
        assert!(redirect.url.contains("https://idp.example.com/sso"));

        // Authenticate with valid SAML payload
        let valid_creds = AuthCredentials {
            kind: "callback".to_string(),
            raw_payload: Some(b"<Response><NameID: user@enterprise.com></Response>".to_vec()),
            ..Default::default()
        };
        let identity = provider.authenticate(&valid_creds).await.unwrap();
        assert_eq!(identity.email, "user@enterprise.com");
        assert_eq!(
            identity.metadata.get("department"),
            Some(&serde_json::json!("Engineering"))
        );
    }
}
