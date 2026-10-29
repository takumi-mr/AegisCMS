//! ログイン・ユーザー・セッション（`[auth]`）: パスワード（Argon2id）、ローカル / OpenID Connect /
//! SAML 2.0 の認証プロバイダ、ユーザー管理、アカウント登録、セッション、ログイン試行の制限、ロール

pub mod config;
pub mod invitation;
pub mod local;
#[cfg(feature = "oidc")]
pub mod oidc;
/// テスト用の OpenID Connect IdP（`mock-idp` フィーチャー）
#[cfg(any(all(test, feature = "oidc"), feature = "mock-idp"))]
pub mod oidc_mock;
pub mod password;
pub mod password_reset;
pub mod rate_limit;
pub mod registration;
#[cfg(feature = "saml")]
pub mod saml;
pub mod session;
pub mod sso;
pub mod totp;
pub mod users;

use sha2::{Digest, Sha256};

pub use config::{
    AuthConfig, InvitationConfig, LoginRateLimitConfig, OidcConfig, PasswordResetConfig,
    RegistrationConfig, SamlConfig,
};
pub use invitation::InvitationService;
pub use local::LocalAuthProvider;
pub use password_reset::PasswordResetService;
pub use rate_limit::LoginRateLimiter;
pub use session::{SessionData, SessionService};
pub use totp::*;

/// OpenID Connect の `AuthProvider::provider_id`（ID 連携テーブルのプロバイダ名にも使う）
pub const OIDC_PROVIDER: &str = "oidc";
/// IdP からのコールバック（`AuthCredentials::parameters` にクエリ文字列）を表す `AuthCredentials::kind`
pub const OIDC_CALLBACK_CREDENTIALS: &str = "oidc_callback";

/// ID 連携テーブルの subject。IdP の `sub` は Issuer ごとにしか一意でないため Issuer と組み合わせる
pub fn oidc_subject(issuer: &str, sub: &str) -> String {
    format!("{}#{}", issuer.trim_end_matches('/'), sub)
}

/// SAML の `AuthProvider::provider_id`（ID 連携テーブルのプロバイダ名にも使う）
pub const SAML_PROVIDER: &str = "saml";
/// ACS への POST（`AuthCredentials::parameters` にフォームの `SAMLResponse`）を表す `AuthCredentials::kind`
pub const SAML_RESPONSE_CREDENTIALS: &str = "saml_response";

/// SAML の ID 連携テーブルの subject。NameID は IdP ごとにしか一意でないため IdP の Entity ID と組み合わせる
pub fn saml_subject(idp_entity_id: &str, name_id: &str) -> String {
    format!("{}#{}", idp_entity_id.trim(), name_id)
}

/// システムロール（権限の強い順）
pub const ROLES: [&str; 3] = ["admin", "editor", "viewer"];

/// ロールの強さ（`admin` ⊃ `editor` ⊃ `viewer`）。`ROLES` 以外のロールは権限なし
pub fn role_level(role: &str) -> u8 {
    match role {
        "admin" => ADMIN,
        "editor" => EDITOR,
        "viewer" => VIEWER,
        _ => 0,
    }
}

pub const VIEWER: u8 = 1;
pub const EDITOR: u8 = 2;
pub const ADMIN: u8 = 3;

/// `role_level` の逆（`VIEWER` 未満は `viewer`）
pub fn role_name(level: u8) -> &'static str {
    match level {
        ADMIN => "admin",
        EDITOR => "editor",
        _ => "viewer",
    }
}

/// セッション ID（Bearer トークン）を監査ログに残すための指紋。
/// トークン自体は認証情報なので記録せず、SHA-256 の先頭 16 桁のみを使って同一セッションを突き合わせる
pub fn session_fingerprint(session_id: &str) -> String {
    let digest = Sha256::digest(session_id.as_bytes());
    hex::encode(digest)[..16].to_string()
}
