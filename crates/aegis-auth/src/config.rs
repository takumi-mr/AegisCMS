//! `[auth]` の設定（ログイン試行の制限・OpenID Connect・アカウント登録）

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthConfig {
    pub login_rate_limit: LoginRateLimitConfig,
    pub oidc: OidcConfig,
    pub registration: RegistrationConfig,
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

/// Admin Studio のログイン画面からのアカウント登録（`POST /api/v1/auth/registration`）。
/// メールアドレスに届く確認リンクを開くとアカウントができる。確認メールは通知（`[notifications]`）で送る
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RegistrationConfig {
    pub enabled: bool,
    /// 登録したユーザーのロール（`admin` は指定できない）
    pub default_role: String,
    /// `true` なら確認後のアカウントを無効の状態で作り、admin が有効にするまでログインできない
    pub require_approval: bool,
    /// 登録できるメールアドレスのドメイン（`example.com` ならサブドメインも可）。空ならどのドメインでも登録できる
    pub allowed_email_domains: Vec<String>,
    /// 確認リンクの有効期間（秒）
    pub token_ttl_secs: u64,
    /// 同じクライアント IP から 1 時間に受け付ける登録の数（0 で無制限）
    pub max_per_ip_per_hour: u64,
    /// 確認リンクの開き先の Admin Studio の URL（例: `https://cms.example.com/admin/`）。
    /// 空なら `[commerce].public_url` + `/admin/`。リクエストの Host からは作らない（確認メールのリンクを偽装させない）
    pub admin_url: String,
}

impl Default for RegistrationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_role: "viewer".to_string(),
            require_approval: true,
            allowed_email_domains: Vec::new(),
            token_ttl_secs: 86_400,
            max_per_ip_per_hour: 5,
            admin_url: String::new(),
        }
    }
}

impl RegistrationConfig {
    /// 設定値の検証（`enabled` のときのみ）
    pub fn validate(&self) -> aegis_core::Result<()> {
        if !self.enabled {
            return Ok(());
        }
        let role = self.default_role.as_str();
        if !crate::ROLES.contains(&role) || role == "admin" {
            return Err(aegis_core::AegisError::Config(format!(
                "[auth.registration].default_role = {:?}: use \"editor\" or \"viewer\"",
                role
            )));
        }
        if self.token_ttl_secs == 0 {
            return Err(aegis_core::AegisError::Config(
                "[auth.registration].token_ttl_secs must be greater than 0".to_string(),
            ));
        }
        Ok(())
    }

    /// メールアドレスのドメインが `allowed_email_domains` に合うか
    pub fn allows_email(&self, email: &str) -> bool {
        if self.allowed_email_domains.is_empty() {
            return true;
        }
        let Some((_, domain)) = email.rsplit_once('@') else {
            return false;
        };
        let domain = domain.to_ascii_lowercase();
        self.allowed_email_domains.iter().any(|allowed| {
            let allowed = allowed.trim().trim_start_matches('@').to_ascii_lowercase();
            !allowed.is_empty() && (domain == allowed || domain.ends_with(&format!(".{}", allowed)))
        })
    }
}
