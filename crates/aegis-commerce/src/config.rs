//! `[commerce]` の設定

use std::collections::HashMap;

use aegis_core::{AegisError, Result};
use serde::{Deserialize, Serialize};

/// 決済の方法（`[commerce].provider`）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PaymentProviderKind {
    /// 銀行振込・代引きなど。管理者が入金を確認して「支払い済み」にする
    #[default]
    Manual,
    /// Stripe Checkout（決済ページへ移動し、Webhook で入金を確認する）
    Stripe,
}

impl PaymentProviderKind {
    /// 注文の `payment_provider` に保存する名前
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Stripe => "stripe",
        }
    }
}

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
        Ok(())
    }

    /// ログインの有効期間（秒）
    pub fn session_ttl_secs(&self) -> u64 {
        self.session_days.saturating_mul(24 * 3600)
    }
}

/// EC（カート・注文・在庫・決済）。商品は `product_type` のスキーマのコンテンツ
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CommerceConfig {
    pub enabled: bool,
    /// 商品のコンテンツタイプ。`price`（integer, 必須）と、任意で `stock`（integer）フィールドを持つこと
    pub product_type: String,
    /// ISO 4217 の通貨コード（小文字）。金額はすべてこの通貨の最小単位の整数
    pub currency: String,
    /// サポートする通貨コード一覧（小文字）。空の場合は `[currency]` のみ
    #[serde(default)]
    pub supported_currencies: Vec<String>,
    /// 基軸通貨（`currency`）1 単位に対する各通貨の換算レート（例: JPY基軸なら `{"usd": 0.0067}`）。
    /// 基軸通貨自身のレートは自動で 1.0 として扱われる
    #[serde(default)]
    pub currency_rates: HashMap<String, f64>,
    /// 公開サイトの URL（`https://shop.example.com`、サブパス運用なら `[server].base_path` 込み）。
    /// 決済後の戻り先に使う。空ならリクエストの Host / X-Forwarded-Proto から組み立てる
    pub public_url: String,
    pub provider: PaymentProviderKind,
    /// 支払い待ちの注文が在庫を確保しておく時間（分）。過ぎると注文を取り消して在庫を戻す
    pub reservation_minutes: u64,
    /// 1 商品あたりの最大数量
    pub max_quantity_per_item: u32,
    /// 1 注文の最大商品数（行数）
    pub max_items: usize,
    /// 同じクライアント IP からの 1 時間あたりの注文数の上限（0 で無制限）。在庫の買い占めを防ぐ
    pub max_orders_per_ip_per_hour: u64,
    /// Notify staff when a tracked product or variant reaches this remaining stock. `None` disables alerts.
    pub low_stock_threshold: Option<u32>,
    pub manual: ManualPaymentConfig,
    pub stripe: StripeConfig,
}

impl Default for CommerceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            product_type: "product".to_string(),
            currency: "jpy".to_string(),
            supported_currencies: Vec::new(),
            currency_rates: HashMap::new(),
            public_url: String::new(),
            provider: PaymentProviderKind::Manual,
            reservation_minutes: 60,
            max_quantity_per_item: 99,
            max_items: 50,
            max_orders_per_ip_per_hour: 20,
            low_stock_threshold: Some(5),
            manual: ManualPaymentConfig::default(),
            stripe: StripeConfig::default(),
        }
    }
}

impl CommerceConfig {
    /// 利用可能な通貨一覧（`currency` を必ず含む、小文字）
    pub fn effective_supported_currencies(&self) -> Vec<String> {
        let base = self.currency.to_ascii_lowercase();
        if self.supported_currencies.is_empty() {
            vec![base]
        } else {
            let mut list = Vec::with_capacity(self.supported_currencies.len() + 1);
            if !self
                .supported_currencies
                .iter()
                .any(|c| c.eq_ignore_ascii_case(&base))
            {
                list.push(base);
            }
            for c in &self.supported_currencies {
                let code = c.to_ascii_lowercase();
                if !list.contains(&code) {
                    list.push(code);
                }
            }
            list
        }
    }

    /// 基軸通貨に対する換算レートを取得する。基軸通貨自身なら 1.0
    pub fn get_currency_rate(&self, target_currency: &str) -> Option<f64> {
        if target_currency.eq_ignore_ascii_case(&self.currency) {
            return Some(1.0);
        }
        self.currency_rates
            .get(&target_currency.to_ascii_lowercase())
            .copied()
            .filter(|rate| *rate > 0.0 && rate.is_finite())
    }

    /// 設定値の検証（`enabled` のときのみ）
    pub fn validate(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        let valid_currency =
            self.currency.len() == 3 && self.currency.chars().all(|c| c.is_ascii_lowercase());
        if !valid_currency {
            return Err(AegisError::Config(format!(
                "[commerce].currency must be a lowercase ISO 4217 code such as \"jpy\" (got {:?})",
                self.currency
            )));
        }
        for cur in &self.supported_currencies {
            let valid = cur.len() == 3 && cur.chars().all(|c| c.is_ascii_lowercase());
            if !valid {
                return Err(AegisError::Config(format!(
                    "[commerce].supported_currencies contains invalid currency code {:?} (expected 3 lowercase letters)",
                    cur
                )));
            }
        }
        for (cur, rate) in &self.currency_rates {
            if *rate <= 0.0 || !rate.is_finite() {
                return Err(AegisError::Config(format!(
                    "[commerce].currency_rates[{:?}] must be a positive finite number (got {})",
                    cur, rate
                )));
            }
        }
        if self.reservation_minutes == 0 || self.max_quantity_per_item == 0 || self.max_items == 0 {
            return Err(AegisError::Config(
                "[commerce].reservation_minutes, max_quantity_per_item and max_items must be at least 1"
                    .to_string(),
            ));
        }
        if self.provider == PaymentProviderKind::Stripe {
            if self.stripe.secret_key().is_empty() || self.stripe.webhook_secret().is_empty() {
                return Err(AegisError::Config(
                    "[commerce].provider = \"stripe\" needs [commerce.stripe].secret_key and webhook_secret \
                     (or AEGIS_STRIPE_SECRET_KEY / AEGIS_STRIPE_WEBHOOK_SECRET)"
                        .to_string(),
                ));
            }
            // Stripe Checkout のセッションは 30 分〜24 時間で期限切れにできる
            if !(30..=24 * 60).contains(&self.reservation_minutes) {
                return Err(AegisError::Config(
                    "[commerce].reservation_minutes must be between 30 and 1440 with Stripe"
                        .to_string(),
                ));
            }
        }
        Ok(())
    }
}

/// 銀行振込・代引きなど（`provider = "manual"`）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ManualPaymentConfig {
    /// 注文完了ページに表示する支払い方法の案内（振込先など）
    pub instructions: String,
    /// 支払い期限（時間）。過ぎた未払いの注文は取り消して在庫を戻す（`reservation_minutes` の代わり）
    pub payment_due_hours: u64,
}

impl Default for ManualPaymentConfig {
    fn default() -> Self {
        Self {
            instructions: "ご注文ありがとうございます。お支払い方法はメールでご案内します。"
                .to_string(),
            payment_due_hours: 72,
        }
    }
}

/// Stripe Checkout（`provider = "stripe"`）。キーは環境変数でも渡せる
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StripeConfig {
    /// シークレットキー（`sk_live_...` / `sk_test_...`）。空なら環境変数 `AEGIS_STRIPE_SECRET_KEY`
    pub secret_key: String,
    /// Webhook の署名シークレット（`whsec_...`）。空なら環境変数 `AEGIS_STRIPE_WEBHOOK_SECRET`
    pub webhook_secret: String,
    /// API の URL（テスト用のモックサーバーに向けるときだけ変える）
    pub api_base: String,
    /// コンビニ払い・銀行振込など、決済ページの完了後に支払われる方法の支払い期限（時間）。
    /// その間は在庫を確保したままにする
    pub async_payment_due_hours: u64,
}

impl Default for StripeConfig {
    fn default() -> Self {
        Self {
            secret_key: String::new(),
            webhook_secret: String::new(),
            api_base: "https://api.stripe.com".to_string(),
            async_payment_due_hours: 72,
        }
    }
}

fn config_or_env(value: &str, env: &str) -> String {
    if value.trim().is_empty() {
        std::env::var(env).unwrap_or_default().trim().to_string()
    } else {
        value.trim().to_string()
    }
}

impl StripeConfig {
    pub fn secret_key(&self) -> String {
        config_or_env(&self.secret_key, "AEGIS_STRIPE_SECRET_KEY")
    }

    pub fn webhook_secret(&self) -> String {
        config_or_env(&self.webhook_secret, "AEGIS_STRIPE_WEBHOOK_SECRET")
    }
}

#[cfg(test)]
mod tests {
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

    use super::*;

    #[test]
    fn test_commerce_config_validation() {
        let mut commerce = CommerceConfig::default();
        assert!(commerce.validate().is_ok(), "disabled is always valid");
        commerce.enabled = true;
        assert!(commerce.validate().is_ok());
        commerce.currency = "JPY".to_string();
        assert!(commerce.validate().is_err());
        commerce.currency = "usd".to_string();
        commerce.provider = PaymentProviderKind::Stripe;
        commerce.stripe.secret_key = "sk_test_x".to_string();
        commerce.stripe.webhook_secret = "whsec_x".to_string();
        assert!(commerce.validate().is_ok());
        commerce.reservation_minutes = 10;
        assert!(commerce.validate().is_err());
        commerce.reservation_minutes = 60;
        commerce.supported_currencies = vec!["eur".to_string(), "INVALID".to_string()];
        assert!(commerce.validate().is_err());
        commerce.supported_currencies = vec!["eur".to_string(), "jpy".to_string()];
        commerce.currency_rates.insert("eur".to_string(), 0.92);
        commerce.currency_rates.insert("jpy".to_string(), -1.0);
        assert!(commerce.validate().is_err());
        commerce.currency_rates.insert("jpy".to_string(), 150.0);
        assert!(commerce.validate().is_ok());

        assert_eq!(
            commerce.effective_supported_currencies(),
            vec!["usd".to_string(), "eur".to_string(), "jpy".to_string()]
        );
        assert_eq!(commerce.get_currency_rate("usd"), Some(1.0));
        assert_eq!(commerce.get_currency_rate("eur"), Some(0.92));
        assert_eq!(commerce.get_currency_rate("jpy"), Some(150.0));
        assert_eq!(commerce.get_currency_rate("gbp"), None);
    }
}
