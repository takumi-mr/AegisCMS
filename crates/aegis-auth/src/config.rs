//! `[auth]` の設定（ログイン試行の制限・OpenID Connect）

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthConfig {
    pub login_rate_limit: LoginRateLimitConfig,
    pub oidc: OidcConfig,
}

/// OpenID Connect（認可コードフロー + PKCE）による外部 IdP ログイン
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OidcConfig {
    pub enabled: bool,
    /// ログイン画面のボタンに表示する名前
    pub display_name: String,
    /// IdP の Issuer（`{issuer_url}/.well-known/openid-configuration` からエンドポイントを取得する）
    pub issuer_url: String,
    pub client_id: String,
    pub client_secret: String,
    /// IdP に登録するコールバック URL（`https://cms.example.com/api/v1/auth/oidc/callback`）
    pub redirect_url: String,
    pub scopes: Vec<String>,
    /// AegisCMS のユーザー名にするクレーム（無ければ `email`）
    pub username_claim: String,
    /// 未登録のユーザーを初回ログイン時に作成する（`false` なら IdP と紐付け済みのユーザーのみ）
    pub auto_create_users: bool,
    /// クレームからロールが決まらない場合のロール（新規ユーザーの初期ロール）
    pub default_role: String,
    /// ロールの決定に使うクレーム（例: `groups`）。空ならロールは AegisCMS のユーザーアカウントで決まる
    pub role_claim: String,
    /// `role_claim` の値 → ロール。複数一致した場合は最も強いロール
    pub role_mapping: std::collections::HashMap<String, String>,
    /// ログイン後に戻る Admin Studio の URL（セッショントークンを `#session=` で渡す）
    pub post_login_redirect: String,
}

impl Default for OidcConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            display_name: "SSO".to_string(),
            issuer_url: String::new(),
            client_id: String::new(),
            client_secret: String::new(),
            redirect_url: String::new(),
            scopes: vec![
                "openid".to_string(),
                "profile".to_string(),
                "email".to_string(),
            ],
            username_claim: "preferred_username".to_string(),
            auto_create_users: true,
            default_role: "viewer".to_string(),
            role_claim: String::new(),
            role_mapping: std::collections::HashMap::new(),
            post_login_redirect: "/admin/".to_string(),
        }
    }
}

/// `POST /api/v1/auth/session` のログイン失敗回数の制限。
/// 失敗回数はキャッシュバックエンド（`[cache]`）で数えるため、`redis` なら全サーバーで共有される
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoginRateLimitConfig {
    pub enabled: bool,
    /// 同じユーザー名で `window_secs` 内にこの回数失敗するとロックする（0 で無制限）
    pub max_failures_per_user: u64,
    /// 同じクライアント IP から `window_secs` 内にこの回数失敗するとロックする（0 で無制限）
    pub max_failures_per_ip: u64,
    /// 失敗回数を数える期間（秒）。最初の失敗から数え、過ぎるとリセットされる
    pub window_secs: u64,
    /// ロックの継続時間（秒）。ロック中は正しいパスワードでもログインできない
    pub lockout_secs: u64,
}

impl Default for LoginRateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_failures_per_user: 5,
            max_failures_per_ip: 20,
            window_secs: 900,
            lockout_secs: 900,
        }
    }
}
