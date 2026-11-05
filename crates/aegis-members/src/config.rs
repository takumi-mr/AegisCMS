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

#[cfg(test)]
mod tests {
    use super::*;

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
