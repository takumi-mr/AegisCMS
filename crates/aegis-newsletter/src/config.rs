//! `[newsletter]` の設定

use aegis_captcha::CaptchaConfig;
use aegis_core::{AegisError, Result};
use serde::{Deserialize, Serialize};

/// ボット除けの見えない欄の名前
pub const HONEYPOT_FIELD: &str = "website";
/// 確認リンクの有効期限（時間）の上限
const MAX_CONFIRM_TTL_HOURS: u64 = 24 * 14;

/// ニュースレター（`[newsletter]`）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NewsletterConfig {
    pub enabled: bool,
    /// 同じクライアント IP からの 1 時間あたりの申し込みの数の上限（0 で無制限）
    pub max_per_ip_per_hour: u64,
    /// 確認リンクの有効期限（時間）。過ぎた確認待ちは使えず、同じアドレスでもう一度申し込む
    pub confirm_ttl_hours: u64,
    /// 組み込みの文言（申し込みのページ・メールの確認ページ）の言語: `en` / `ja`
    pub locale: String,
    /// CAPTCHA（`[newsletter.captcha]`）。書かなければ使わない
    pub captcha: Option<CaptchaConfig>,
}

impl Default for NewsletterConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_per_ip_per_hour: 10,
            confirm_ttl_hours: 48,
            locale: "en".to_string(),
            captcha: None,
        }
    }
}

impl NewsletterConfig {
    /// 設定の検査。`enabled` のときに起動時に呼ぶ
    pub fn validate(&self) -> Result<()> {
        let err = |m: String| Err(AegisError::Config(format!("[newsletter] {}", m)));
        if !crate::messages::LOCALES.contains(&self.locale.as_str()) {
            return err(format!(
                "locale must be one of {:?} (got {:?})",
                crate::messages::LOCALES,
                self.locale
            ));
        }
        if self.confirm_ttl_hours == 0 || self.confirm_ttl_hours > MAX_CONFIRM_TTL_HOURS {
            return err(format!(
                "confirm_ttl_hours must be 1 to {}",
                MAX_CONFIRM_TTL_HOURS
            ));
        }
        if let Some(captcha) = &self.captcha {
            captcha.validate("[newsletter.captcha]")?;
        }
        Ok(())
    }
}
