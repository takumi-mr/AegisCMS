//! `[members]` の設定

use aegis_core::{AegisError, Result};
use serde::{Deserialize, Serialize};

/// 新規の会員登録の受付（`[members].registration`）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemberRegistration {
    /// 誰でも登録できる
    #[default]
    Open,
    /// 新規の登録だけを止める（登録済みの顧客は、ログインして使える）
    Closed,
}

/// 会員アカウント（`[members]`。docs/members.md）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MembersConfig {
    /// 会員アカウント（会員登録・ログイン）を使う。`false` なら会員の API は 404（ゲスト購入は使える）
    pub enabled: bool,
    pub registration: MemberRegistration,
    /// ログインの有効期間（日）
    pub session_days: u64,
    /// パスワードの再設定（メールで送るリンク）を使う。`[notifications]` が有効で、
    /// `[members].public_url` が設定されているときだけ動く（リンクは Host ヘッダーではなく、この設定から作る）
    pub password_reset: bool,
    /// 会員登録のあと、メールの確認リンクを開くまでログインさせない。確認メールを送れること
    /// （`[notifications]` が有効で、`[members].public_url` がある）が要る
    pub require_email_verification: bool,
    /// 会員のメール（再設定・確認）のリンクの元になるサイトの URL（例 `https://example.com`）。
    /// 注文確認のリンクに使う `[commerce].public_url` とは別に持つ
    pub public_url: String,
    /// 会員の権限レベル（低 → 高の順）。先頭が新規会員の既定。高いレベルは低いレベルの内容も見られる。
    /// 記事・ページの `access` で必要なレベルを指定する（docs/members.md 15）
    pub levels: Vec<String>,
    /// 権限レベルが足りない会員に出す「アップグレード」の案内の URL（サイト内のパス `/...` か `https://...`）。
    /// 空なら案内のリンクを出さない
    pub upgrade_url: String,
    /// 購入（記事の単独購入・サブスク。docs/members.md 16）
    pub purchase: PurchaseConfig,
}

/// 購入（`[members.purchase]`。docs/members.md 16.2）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PurchaseConfig {
    /// 購入を使う。会員アカウント（`[members].enabled`）と `[members].public_url` が要る
    pub enabled: bool,
    /// 記事の単独購入に使う通貨（ISO 4217 の小文字）。`purchase_price` はこの通貨の最小単位
    pub currency: String,
    pub stripe: PurchaseStripeConfig,
}

impl Default for PurchaseConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            currency: "jpy".to_string(),
            stripe: PurchaseStripeConfig::default(),
        }
    }
}

/// 購入に使う Stripe（`[members.purchase.stripe]`）。キーは環境変数でも渡せる。
/// Webhook のエンドポイントはショップとは別（`/api/v1/members/webhooks/stripe`）なので、署名シークレットも別
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PurchaseStripeConfig {
    /// シークレットキー。空なら環境変数 `AEGIS_STRIPE_SECRET_KEY`
    pub secret_key: String,
    /// Webhook の署名シークレット（`whsec_...`）。空なら環境変数 `AEGIS_MEMBERS_STRIPE_WEBHOOK_SECRET`
    pub webhook_secret: String,
    /// API の URL（テスト用のモックサーバーに向けるときだけ変える）
    pub api_base: String,
}

impl Default for PurchaseStripeConfig {
    fn default() -> Self {
        Self {
            secret_key: String::new(),
            webhook_secret: String::new(),
            api_base: "https://api.stripe.com".to_string(),
        }
    }
}

fn value_or_env(value: &str, env: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        std::env::var(env).unwrap_or_default().trim().to_string()
    } else {
        value.to_string()
    }
}

impl PurchaseStripeConfig {
    pub fn secret_key(&self) -> String {
        value_or_env(&self.secret_key, "AEGIS_STRIPE_SECRET_KEY")
    }

    pub fn webhook_secret(&self) -> String {
        value_or_env(&self.webhook_secret, "AEGIS_MEMBERS_STRIPE_WEBHOOK_SECRET")
    }
}

impl PurchaseConfig {
    /// 設定の誤りを起動時に断る。`members_enabled` / `public_url` は `[members]` の値
    pub fn validate(&self, members_enabled: bool, public_url: &str) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        let bad = |msg: &str| Err(AegisError::Config(format!("[members.purchase] {msg}")));
        if !members_enabled {
            return bad("needs [members].enabled = true (a purchase belongs to a member)");
        }
        let url = public_url.trim();
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return bad(
                "needs [members].public_url (the return URL of the payment page is built from it)",
            );
        }
        if !(self.currency.len() == 3 && self.currency.chars().all(|c| c.is_ascii_lowercase())) {
            return bad(&format!(
                "currency must be a lowercase ISO 4217 code such as \"jpy\" (got {:?})",
                self.currency
            ));
        }
        if self.stripe.secret_key().is_empty() || self.stripe.webhook_secret().is_empty() {
            return bad(
                "needs [members.purchase.stripe].secret_key and webhook_secret                  (or AEGIS_STRIPE_SECRET_KEY / AEGIS_MEMBERS_STRIPE_WEBHOOK_SECRET)",
            );
        }
        Ok(())
    }
}

impl Default for MembersConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            registration: MemberRegistration::Open,
            session_days: 30,
            password_reset: true,
            require_email_verification: false,
            public_url: String::new(),
            levels: vec!["free".to_string(), "paid".to_string()],
            upgrade_url: String::new(),
            purchase: PurchaseConfig::default(),
        }
    }
}

impl MembersConfig {
    /// メールを使う設定と、メールを送れる状態の食い違いを調べる。確認を必須にしているのに確認メールを
    /// 送れないと、誰もログインできなくなるので、起動時にエラーにする
    pub fn check_mail(&self, mail_available: bool) -> Result<()> {
        if !(self.enabled && self.require_email_verification) {
            return Ok(());
        }
        if !mail_available {
            return Err(AegisError::Config(
                "[members].require_email_verification = true needs [notifications] enabled \
                 (the confirmation email cannot be sent otherwise)"
                    .to_string(),
            ));
        }
        if self.public_url.trim().is_empty() {
            return Err(AegisError::Config(
                "[members].require_email_verification = true needs [members].public_url \
                 (the link in the confirmation email is built from it)"
                    .to_string(),
            ));
        }
        Ok(())
    }

    /// 設定の誤りを起動時に断る（使わない設定なら見ない）
    pub fn validate(&self) -> Result<()> {
        if self.enabled && !(1..=365).contains(&self.session_days) {
            return Err(AegisError::Config(
                "[members].session_days must be between 1 and 365".to_string(),
            ));
        }
        self.validate_levels()?;
        self.purchase.validate(self.enabled, &self.public_url)?;
        let url = self.upgrade_url.trim();
        let local = url.starts_with('/') && !url.starts_with("//") && !url.contains('\\');
        if !(url.is_empty() || local || url.starts_with("https://")) {
            return Err(AegisError::Config(
                "[members].upgrade_url must be empty, a path starting with / or an https:// URL"
                    .to_string(),
            ));
        }
        Ok(())
    }

    /// `levels` の誤りを断る。1 件以上・重複なし・英小文字と数字と `-` `_` の 1〜32 文字（`public` は予約）。
    /// 使わない設定でも、記事の `access` の判定に使うので、有効でなくても見る
    pub fn validate_levels(&self) -> Result<()> {
        let bad = |msg: &str| Err(AegisError::Config(format!("[members].levels {msg}")));
        if self.levels.is_empty() {
            return bad("must have at least one level");
        }
        for (i, level) in self.levels.iter().enumerate() {
            let valid = (1..=32).contains(&level.len())
                && level
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
            if !valid {
                return bad(&format!(
                    "has an invalid level '{level}' (use 1-32 characters of a-z, 0-9, - and _)"
                ));
            }
            if level == "public" {
                return bad("cannot use 'public' (it means everyone)");
            }
            if self.levels[..i].contains(level) {
                return bad(&format!("has the level '{level}' twice"));
            }
        }
        Ok(())
    }

    /// 新規会員のレベル（`levels` の先頭）
    pub fn default_level(&self) -> &str {
        self.levels.first().map(String::as_str).unwrap_or("free")
    }

    /// ログインの有効期間（秒）
    pub fn session_ttl_secs(&self) -> u64 {
        self.session_days.saturating_mul(24 * 3600)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_are_validated() {
        let with = |levels: &[&str]| MembersConfig {
            levels: levels.iter().map(|l| l.to_string()).collect(),
            ..MembersConfig::default()
        };
        with(&["free", "paid"]).validate_levels().unwrap();
        assert_eq!(with(&["basic", "pro"]).default_level(), "basic");
        for bad in [
            &[][..],
            &["free", "free"],
            &["Free"],
            &["public"],
            &[""],
            &["has space"],
            &["a-very-long-level-name-that-goes-over-the-limit"],
        ] {
            assert!(with(bad).validate_levels().is_err(), "{bad:?}");
        }
        // 使わない設定でも、レベルの誤りは起動時に断る
        assert!(with(&[]).validate().is_err());
    }

    #[test]
    fn purchase_config_is_validated_only_when_enabled() {
        let base = || MembersConfig {
            enabled: true,
            public_url: "https://example.com".to_string(),
            purchase: PurchaseConfig {
                enabled: true,
                currency: "jpy".to_string(),
                stripe: PurchaseStripeConfig {
                    secret_key: "sk_test_x".to_string(),
                    webhook_secret: "whsec_x".to_string(),
                    ..PurchaseStripeConfig::default()
                },
            },
            ..MembersConfig::default()
        };
        base().validate().unwrap();

        let mut no_members = base();
        no_members.enabled = false;
        assert!(no_members.validate().is_err());
        let mut no_url = base();
        no_url.public_url = String::new();
        assert!(no_url.validate().is_err());
        let mut bad_currency = base();
        bad_currency.purchase.currency = "JPY".to_string();
        assert!(bad_currency.validate().is_err());
        let mut no_secret = base();
        no_secret.purchase.stripe.webhook_secret = String::new();
        // 環境変数にも無いときだけ誤り
        if std::env::var("AEGIS_MEMBERS_STRIPE_WEBHOOK_SECRET").is_err() {
            assert!(no_secret.validate().is_err());
        }

        // 使わない設定なら見ない
        let mut off = base();
        off.purchase.enabled = false;
        off.purchase.currency = "??".to_string();
        off.validate().unwrap();
    }

    #[test]
    fn upgrade_url_is_a_local_path_or_https() {
        let with = |url: &str| MembersConfig {
            upgrade_url: url.to_string(),
            ..MembersConfig::default()
        };
        for ok in ["", "/plans", "https://example.com/plans"] {
            with(ok).validate().unwrap();
        }
        for bad in [
            "//evil.example",
            "http://example.com",
            "javascript:alert(1)",
            "plans",
            "/\\evil",
        ] {
            assert!(with(bad).validate().is_err(), "{bad}");
        }
    }

    /// 確認を必須にしているのにメールを送れない設定は、起動時に断る（誰もログインできなくなるため）
    #[test]
    fn email_verification_needs_mail_and_a_public_url() {
        let off = MembersConfig::default();
        assert!(!off.require_email_verification);
        off.check_mail(false).unwrap();

        let on = MembersConfig {
            enabled: true,
            require_email_verification: true,
            public_url: "https://example.com".to_string(),
            ..MembersConfig::default()
        };
        on.check_mail(true).unwrap();
        assert!(on.check_mail(false).is_err());
        let no_url = MembersConfig {
            public_url: "  ".to_string(),
            ..on
        };
        assert!(no_url.check_mail(true).is_err());

        // 会員アカウントを使わない設定なら、見ない
        let unused = MembersConfig {
            enabled: false,
            require_email_verification: true,
            ..MembersConfig::default()
        };
        unused.check_mail(false).unwrap();
    }

    #[test]
    fn members_section_defaults_parses_and_validates() {
        // 書かなければ使わない（既定は無効。登録は開いている・30 日）
        let config: MembersConfig = toml::from_str("").unwrap();
        assert!(!config.enabled);
        assert_eq!(config.registration, MemberRegistration::Open);
        assert_eq!(config.session_days, 30);
        config.validate().unwrap();

        let config: MembersConfig = toml::from_str(
            "enabled = true
registration = \"closed\"
session_days = 7
",
        )
        .unwrap();
        assert_eq!(config.registration, MemberRegistration::Closed);
        assert_eq!(config.session_ttl_secs(), 7 * 24 * 3600);
        config.validate().unwrap();

        // 範囲外の日数は設定の誤りとして拒否する（使わない設定なら見ない）
        for days in [0, 366] {
            let bad: MembersConfig = toml::from_str(&format!(
                "enabled = true
session_days = {days}
"
            ))
            .unwrap();
            assert!(bad.validate().is_err(), "{days}");
        }
        let unused: MembersConfig = toml::from_str("session_days = 0").unwrap();
        unused.validate().unwrap();

        // 知らない値は拒否する
        assert!(toml::from_str::<MembersConfig>("registration = \"invite\"").is_err());
    }
}
