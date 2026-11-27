//! プラグインのブロックの `<iframe>`（外部のページの埋め込み）についてのサイトの設定（`[embed]`）。
//!
//! - **管理者が許可するホスト**（`frame_domains` と、Admin Studio の **Settings › Headers** の
//!   `embed_frame_domains`）: マニフェストで `capabilities.embed.site_domains` を宣言したプラグインの
//!   ブロックに限り、このホストの `<iframe>` をサニタイズで残し、CSP の `frame-src` に足す。
//!   どのホストを許すかを決めるのは管理者だけで、投稿者（本文）やプラグインは足せない
//! - **同意を待つ**（`consent`・`consent_exempt`）: `[consent]` を有効にしていると、公開ページ・プレビューの
//!   プラグインのブロックの `<iframe>` を、`consent` の区分の同意まで読み込まない（`src` を `data-src` にして
//!   同意のスクリプトに任せる）。同意を待つ間は、ブロックの先頭に「Cookie の設定」ボタンつきの案内を出す。
//!   コンテンツ API・フィードの HTML は変えない（ヘッドレスのクライアントは自分で同意を扱う）

use aegis_core::manifest::is_valid_frame_domain;
use aegis_core::{AegisError, Result};
use serde::{Deserialize, Serialize};

use crate::consent::ConsentConfig;
use crate::state::AppState;

/// 許可できるホストの数の上限（設定ファイル・Admin Studio のそれぞれ）
pub const MAX_SITE_FRAME_DOMAINS: usize = 50;

/// `[embed]`
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbedConfig {
    /// 管理者が許可するホスト（`capabilities.embed.site_domains` を宣言したプラグインのブロックにだけ効く）
    #[serde(default)]
    pub frame_domains: Vec<String>,
    /// プラグインのブロックの `<iframe>` を、この同意の区分が許可されるまで読み込まない（空なら待たない）
    #[serde(default)]
    pub consent: String,
    /// 同意を待たないホスト
    #[serde(default)]
    pub consent_exempt: Vec<String>,
}

/// 許可するホストの一覧の検査（形式と数）。`what` はエラーに出す名前
pub fn validate_hosts(what: &str, hosts: &[String]) -> std::result::Result<(), String> {
    if hosts.len() > MAX_SITE_FRAME_DOMAINS {
        return Err(format!(
            "{} allows at most {} hosts",
            what, MAX_SITE_FRAME_DOMAINS
        ));
    }
    if let Some(bad) = hosts.iter().find(|h| !is_valid_frame_domain(h)) {
        return Err(format!(
            "{}: {:?} is not a host name (lowercase, e.g. \"forms.example.com\"; no scheme, port, path or wildcard)",
            what, bad
        ));
    }
    Ok(())
}

impl EmbedConfig {
    /// 設定の検査。起動時に呼ぶ
    pub fn validate(&self, consent: &ConsentConfig) -> Result<()> {
        let err = |m: String| Err(AegisError::Config(format!("[embed] {}", m)));
        if let Err(m) = validate_hosts("frame_domains", &self.frame_domains) {
            return err(m);
        }
        if let Err(m) = validate_hosts("consent_exempt", &self.consent_exempt) {
            return err(m);
        }
        if self.consent.is_empty() {
            if !self.consent_exempt.is_empty() {
                return err("consent_exempt needs consent".to_string());
            }
            return Ok(());
        }
        if !consent.enabled {
            return err(format!(
                "consent = {:?} needs [consent] enabled = true",
                self.consent
            ));
        }
        if !consent.categories().iter().any(|c| c.id == self.consent) {
            return err(format!(
                "consent = {:?} is not one of the [consent] categories",
                self.consent
            ));
        }
        Ok(())
    }

    /// 同意を待つときの区分（`[consent]` が無効なら待たない）
    pub fn consent_category<'a>(&'a self, consent: &ConsentConfig) -> Option<&'a str> {
        (consent.enabled && !self.consent.is_empty()).then_some(self.consent.as_str())
    }
}

/// URL（`https://example.com:8443/cms` など）のホスト名（英小文字。ポートとユーザー情報を除く）
pub fn url_host(url: &str) -> Option<String> {
    let rest = url.trim().split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit('@').next()?;
    let host = match host.strip_prefix('[') {
        Some(v6) => v6.split(']').next()?,
        None => host.split(':').next()?,
    };
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

/// このサイト自身のホスト（`[seo]` の正規 URL・Admin Studio の **SEO & Bot** のもの）。
/// 許可するホストに入れると、汎用の埋め込み（`allow-scripts allow-same-origin`）が同じオリジンの
/// ページを開き、`sandbox` を外して親のページを操作できてしまうため、許可するホストから除く
pub fn own_host(state: &AppState) -> Option<String> {
    state.seo().canonical_base_url.as_deref().and_then(url_host)
}

/// 管理者が許可したホスト（設定ファイルと Admin Studio の和。重複なし・並べ替え済み）。
/// このサイト自身のホスト（[`own_host`]）は、書かれていても除く
pub fn site_frame_domains(state: &AppState) -> Vec<String> {
    let own = own_host(state);
    let mut hosts = state.config.embed.frame_domains.clone();
    hosts.extend(state.headers().embed_frame_domains);
    hosts.retain(|h| own.as_deref() != Some(h.as_str()));
    hosts.sort();
    hosts.dedup();
    hosts
}

/// 公開ページに出すプラグインのブロックの HTML（サニタイズ済み、`div.aegis-plugin-block` で包んだもの）の
/// `<iframe>` を、同意まで読み込まない形にする。待つものがあれば、ブロックの先頭に案内を入れる
pub fn hold_for_consent(state: &AppState, block_html: String) -> String {
    let Some(category) = state.config.embed.consent_category(&state.config.consent) else {
        return block_html;
    };
    let (held, count) = aegis_ast::renderer::hold_frames_for_consent(
        &block_html,
        category,
        &state.config.embed.consent_exempt,
    );
    if count == 0 {
        return held;
    }
    let locale = state.config.theme.locale.as_str();
    let note = format!(
        "<p class=\"aegis-consent-note\" data-aegis-consent-note=\"{}\">{} <button type=\"button\" class=\"aegis-consent-note-button\" data-aegis-consent-open>{}</button></p>",
        aegis_ast::renderer::escape_html(category),
        aegis_theme::messages::translate(locale, "consent.embed_waiting", &[]),
        aegis_theme::messages::translate(locale, "consent.reopen", &[]),
    );
    // ブロックを包む `div` の開始タグ（属性の値はエスケープ済みで `>` を含まない）の直後に入れる
    match held.find('>') {
        Some(end) => format!("{}{}{}", &held[..=end], note, &held[end + 1..]),
        None => format!("{}{}", note, held),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consent::ConsentCategory;

    fn consent(enabled: bool) -> ConsentConfig {
        ConsentConfig {
            enabled,
            categories: [("marketing", ""), ("video", "Video")]
                .into_iter()
                .map(|(id, label)| ConsentCategory {
                    id: id.to_string(),
                    label: label.to_string(),
                    description: String::new(),
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn reads_the_host_of_a_url() {
        assert_eq!(
            url_host("https://Example.com/cms").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            url_host("https://example.com:8443").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            url_host("https://u@example.com/").as_deref(),
            Some("example.com")
        );
        assert_eq!(url_host("http://[::1]:8080/").as_deref(), Some("::1"));
        assert_eq!(url_host("example.com"), None);
    }

    #[test]
    fn validates_hosts_and_the_consent_category() {
        let ok = EmbedConfig {
            frame_domains: vec!["forms.example.com".to_string()],
            consent: "video".to_string(),
            consent_exempt: vec!["www.openstreetmap.org".to_string()],
        };
        assert!(ok.validate(&consent(true)).is_ok());
        assert_eq!(ok.consent_category(&consent(true)), Some("video"));
        assert!(EmbedConfig::default().validate(&consent(false)).is_ok());

        let bad = |config: EmbedConfig, consent: ConsentConfig| config.validate(&consent).is_err();
        for host in [
            "https://a.example",
            "*.example.com",
            "a.example:8443",
            "A.example",
            "a",
        ] {
            assert!(
                bad(
                    EmbedConfig {
                        frame_domains: vec![host.to_string()],
                        ..Default::default()
                    },
                    consent(false)
                ),
                "{}",
                host
            );
        }
        let too_many = (0..=MAX_SITE_FRAME_DOMAINS)
            .map(|i| format!("h{}.example.com", i))
            .collect();
        assert!(bad(
            EmbedConfig {
                frame_domains: too_many,
                ..Default::default()
            },
            consent(false)
        ));
        // 区分は [consent] にあるもの。[consent] が無効なら書けない
        let with_consent = |id: &str| EmbedConfig {
            consent: id.to_string(),
            ..Default::default()
        };
        assert!(bad(with_consent("analytics"), consent(true)));
        assert!(bad(with_consent("marketing"), consent(false)));
        assert!(bad(
            EmbedConfig {
                consent_exempt: vec!["a.example.com".to_string()],
                ..Default::default()
            },
            consent(true)
        ));
        // [consent] が無効なら待たない
        assert_eq!(
            with_consent("marketing").consent_category(&consent(false)),
            None
        );
    }
}
