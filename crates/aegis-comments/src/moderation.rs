//! 読者の投稿の審査。コメントと（今後の）商品レビューで共有する仕組み:
//! 承認制の設定（Admin Studio からの切り替えを含む）、投稿の検査（必須項目・長さ・ハニーポット）、
//! IP ごとの投稿数の制限、NG ワード・リンク数による振り分け、最初の状態の決定
//!
//! 最初の状態は WordPress と同じ考え方で決める:
//!
//! 1. スパムの語（`spam_words`）を含む → `spam`
//! 2. 承認待ちにする語（`hold_words`）を含む、リンクが `max_links` を超える → `pending`
//! 3. 承認制（`require_approval`）なら `pending`。ただし `auto_approve_known` のとき、同じ名前と
//!    メールアドレスで承認済みの投稿がある人は `approved`
//! 4. それ以外は `approved`
//!
//! この後に `check-comment` などのプラグインが、より厳しい状態（承認待ち・スパム）にできる

use std::collections::HashMap;

use aegis_core::{AegisError, CacheBackend, ModerationStatus, Result};
use serde::{Deserialize, Serialize};

/// 審査の設定（`[comments.moderation]`。レビューも同じ形を使う）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModerationConfig {
    /// 承認制（承認するまで公開しない）。Admin Studio から切り替えられる（そちらが優先）
    pub require_approval: bool,
    /// 承認制のとき、同じ名前とメールアドレスで承認済みの投稿がある人は承認を待たずに公開する
    /// （WordPress の「すでに承認されたコメントの投稿者のコメントを許可」）。Admin Studio から切り替えられる
    pub auto_approve_known: bool,
    /// 本文のリンク（`http://` / `https://` / `www.`）がこれより多いと承認待ちにする
    pub max_links: usize,
    /// 含むと承認待ちにする語（大文字・小文字を区別しない。名前・メールアドレス・本文を調べる）
    pub hold_words: Vec<String>,
    /// 含むとスパムにする語（同上）
    pub spam_words: Vec<String>,
    /// 同じクライアント IP からの 1 時間あたりの投稿数の上限（0 で無制限）
    pub max_per_ip_per_hour: u64,
}

impl Default for ModerationConfig {
    fn default() -> Self {
        Self {
            require_approval: true,
            auto_approve_known: true,
            max_links: 2,
            hold_words: Vec::new(),
            spam_words: Vec::new(),
            max_per_ip_per_hour: 10,
        }
    }
}

/// 承認制の設定の、Admin Studio から変えた値（`_aegis_settings`。`None` は設定ファイルの値）
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyOverrides {
    pub require_approval: Option<bool>,
    pub auto_approve_known: Option<bool>,
}

/// 実際に使う承認制の設定
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModerationPolicy {
    pub require_approval: bool,
    pub auto_approve_known: bool,
}

/// `_aegis_settings` に保存する名前（`<prefix>.require_approval` など。コメントは `comments`）
pub fn setting_names(prefix: &str) -> (String, String) {
    (
        format!("{}.require_approval", prefix),
        format!("{}.auto_approve_known", prefix),
    )
}

impl PolicyOverrides {
    pub fn from_settings(prefix: &str, settings: &HashMap<String, String>) -> Self {
        let (approval, known) = setting_names(prefix);
        let flag = |name: &str| settings.get(name).and_then(|v| v.parse::<bool>().ok());
        Self {
            require_approval: flag(&approval),
            auto_approve_known: flag(&known),
        }
    }
}

impl ModerationConfig {
    /// 設定ファイルの値に Admin Studio の値を重ねる
    pub fn policy(&self, overrides: &PolicyOverrides) -> ModerationPolicy {
        ModerationPolicy {
            require_approval: overrides.require_approval.unwrap_or(self.require_approval),
            auto_approve_known: overrides
                .auto_approve_known
                .unwrap_or(self.auto_approve_known),
        }
    }

    /// `section` は設定のセクション名（エラーメッセージ用。`[comments.moderation]`）
    pub fn validate(&self, section: &str) -> Result<()> {
        for word in self.hold_words.iter().chain(&self.spam_words) {
            if word.trim().is_empty() {
                return Err(AegisError::Config(format!(
                    "{}: hold_words / spam_words must not contain empty words",
                    section
                )));
            }
        }
        Ok(())
    }

    /// 内容から決まる状態（承認制より前の 1・2）。理由も返す
    pub fn screen(&self, texts: &[&str]) -> Option<(ModerationStatus, String)> {
        let haystack = texts.join("\n").to_lowercase();
        let found = |words: &[String]| {
            words
                .iter()
                .map(|w| w.trim().to_lowercase())
                .find(|w| !w.is_empty() && haystack.contains(w.as_str()))
        };
        if let Some(word) = found(&self.spam_words) {
            return Some((ModerationStatus::Spam, format!("spam word \"{}\"", word)));
        }
        if let Some(word) = found(&self.hold_words) {
            return Some((ModerationStatus::Pending, format!("hold word \"{}\"", word)));
        }
        let links = count_links(&haystack);
        if links > self.max_links {
            return Some((
                ModerationStatus::Pending,
                format!("{} links (more than {})", links, self.max_links),
            ));
        }
        None
    }
}

impl ModerationPolicy {
    /// 承認制による状態（3・4）。`known_author` は承認済みの投稿がある人か
    pub fn status_for(&self, known_author: bool) -> ModerationStatus {
        if !self.require_approval || (self.auto_approve_known && known_author) {
            ModerationStatus::Approved
        } else {
            ModerationStatus::Pending
        }
    }
}

/// 2 つの状態のうち厳しい方（スパム > 承認待ち > 承認済み > ゴミ箱）
pub fn stricter(a: ModerationStatus, b: ModerationStatus) -> ModerationStatus {
    let rank = |s: ModerationStatus| match s {
        ModerationStatus::Trash => 0,
        ModerationStatus::Approved => 1,
        ModerationStatus::Pending => 2,
        ModerationStatus::Spam => 3,
    };
    if rank(b) > rank(a) {
        b
    } else {
        a
    }
}

/// 本文のリンクの数（`http://`・`https://`・`www.` の出現数。小文字にしてから渡す）
pub fn count_links(lowercase_text: &str) -> usize {
    lowercase_text.matches("http://").count()
        + lowercase_text.matches("https://").count()
        + lowercase_text
            .match_indices("www.")
            .filter(|(i, _)| !lowercase_text[..*i].ends_with("://"))
            .count()
}

/// IP ごとの投稿数を数える。`max` を超えたら `false`（0 なら常に `true`）。
/// `scope` はキーの名前空間（`comments` / `reviews`）
pub async fn within_rate_limit(
    cache: &dyn CacheBackend,
    scope: &str,
    ip: Option<&str>,
    max: u64,
) -> Result<bool> {
    let Some(ip) = ip.filter(|_| max > 0) else {
        return Ok(true);
    };
    let count = cache
        .increment(&format!("{}:ip:{}", scope, ip), Some(3600))
        .await?;
    Ok(count <= max)
}

/// 投稿者の名前の最大文字数
pub const MAX_NAME_CHARS: usize = 100;

/// 投稿者の名前（前後の空白を除く）。空・長すぎる・制御文字は `Err`（メッセージは投稿者に見せる）
pub fn normalize_name(name: &str) -> std::result::Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Your name is required".to_string());
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(format!(
            "Your name is too long ({} characters at most)",
            MAX_NAME_CHARS
        ));
    }
    if name.chars().any(char::is_control) {
        return Err("Your name must not contain control characters".to_string());
    }
    Ok(name.to_string())
}

/// メールアドレス（前後の空白を除いて小文字にする）。形が正しくなければ `Err`
pub fn normalize_email(email: &str) -> std::result::Result<String, String> {
    let email = email.trim().to_lowercase();
    let valid = match email.split_once('@') {
        Some((local, domain)) => {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
                && !domain.contains('@')
                && email.len() <= 254
                && !email.chars().any(|c| c.is_whitespace() || c.is_control())
        }
        None => false,
    };
    if valid {
        Ok(email)
    } else {
        Err("A valid email address is required".to_string())
    }
}

/// 本文（前後の空白を除き、改行を `\n` に揃える）。空・`max_chars` を超える・NUL は `Err`
pub fn normalize_body(body: &str, max_chars: usize) -> std::result::Result<String, String> {
    let body = body.replace("\r\n", "\n").replace('\r', "\n");
    let body = body.trim();
    if body.is_empty() {
        return Err("The comment must not be empty".to_string());
    }
    if body.chars().count() > max_chars {
        return Err(format!(
            "The comment is too long ({} characters at most)",
            max_chars
        ));
    }
    if body
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err("The comment must not contain control characters".to_string());
    }
    Ok(body.to_string())
}

/// プレーンテキストの本文を HTML にする。すべてエスケープし、空行で段落（`<p>`）、改行で `<br>` にする
pub fn body_html(body: &str) -> String {
    body.split("\n\n")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| {
            let lines: Vec<String> = p.lines().map(escape_html).collect();
            format!("<p>{}</p>", lines.join("<br>"))
        })
        .collect()
}

fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_cache::MemoryCacheBackend;

    #[test]
    fn test_policy_follows_wordpress() {
        let config = ModerationConfig::default();
        let policy = config.policy(&PolicyOverrides::default());
        assert_eq!(policy.status_for(false), ModerationStatus::Pending);
        assert_eq!(policy.status_for(true), ModerationStatus::Approved);

        let no_known = config.policy(&PolicyOverrides {
            auto_approve_known: Some(false),
            ..Default::default()
        });
        assert_eq!(no_known.status_for(true), ModerationStatus::Pending);

        let open = config.policy(&PolicyOverrides {
            require_approval: Some(false),
            ..Default::default()
        });
        assert_eq!(open.status_for(false), ModerationStatus::Approved);
    }

    #[test]
    fn test_overrides_from_settings() {
        let settings = HashMap::from([
            ("comments.require_approval".to_string(), "false".to_string()),
            (
                "comments.auto_approve_known".to_string(),
                "oops".to_string(),
            ),
        ]);
        assert_eq!(
            PolicyOverrides::from_settings("comments", &settings),
            PolicyOverrides {
                require_approval: Some(false),
                auto_approve_known: None,
            }
        );
        assert_eq!(
            PolicyOverrides::from_settings("reviews", &settings),
            PolicyOverrides::default()
        );
    }

    #[test]
    fn test_screen_words_and_links() {
        let config = ModerationConfig {
            hold_words: vec!["Discount".to_string()],
            spam_words: vec!["casino".to_string()],
            ..Default::default()
        };
        assert_eq!(config.screen(&["Hanako", "Nice post!"]), None);
        assert_eq!(
            config.screen(&["Hanako", "Best CASINO here"]).unwrap().0,
            ModerationStatus::Spam
        );
        assert_eq!(
            config.screen(&["Hanako", "big discount"]).unwrap().0,
            ModerationStatus::Pending
        );
        // spam beats hold
        assert_eq!(
            config.screen(&["discount casino"]).unwrap().0,
            ModerationStatus::Spam
        );
        assert_eq!(
            config.screen(&["see https://a.example and www.b.example"]),
            None
        );
        assert_eq!(
            config
                .screen(&["https://a.example http://b.example www.c.example"])
                .unwrap()
                .0,
            ModerationStatus::Pending
        );
    }

    #[test]
    fn test_count_links_does_not_count_www_twice() {
        assert_eq!(count_links("https://www.example.com"), 1);
        assert_eq!(count_links("www.example.com and http://x"), 2);
        assert_eq!(count_links("no links"), 0);
    }

    #[test]
    fn test_stricter() {
        use ModerationStatus::*;
        assert_eq!(stricter(Approved, Pending), Pending);
        assert_eq!(stricter(Spam, Pending), Spam);
        assert_eq!(stricter(Pending, Approved), Pending);
    }

    #[test]
    fn test_normalize_inputs() {
        assert_eq!(normalize_name("  Hanako "), Ok("Hanako".to_string()));
        assert!(normalize_name(" ").is_err());
        assert!(normalize_name(&"a".repeat(MAX_NAME_CHARS + 1)).is_err());
        assert!(normalize_name("a\u{0}b").is_err());

        assert_eq!(
            normalize_email(" Hanako@Example.COM "),
            Ok("hanako@example.com".to_string())
        );
        for bad in [
            "",
            "hanako",
            "@example.com",
            "a@b",
            "a@.com",
            "a b@example.com",
        ] {
            assert!(normalize_email(bad).is_err(), "{bad}");
        }

        assert_eq!(normalize_body(" a\r\nb ", 10), Ok("a\nb".to_string()));
        assert!(normalize_body("  ", 10).is_err());
        assert!(normalize_body("あいうえおかきくけこさ", 10).is_err());
        assert!(normalize_body("a\u{7}b", 10).is_err());
    }

    #[test]
    fn test_body_html_escapes_everything() {
        assert_eq!(
            body_html("Hi <b>there</b> & \"you\"\nnext line\n\n\nNew paragraph"),
            "<p>Hi &lt;b&gt;there&lt;/b&gt; &amp; &quot;you&quot;<br>next line</p><p>New paragraph</p>"
        );
        assert_eq!(
            body_html("<script>alert(1)</script>"),
            "<p>&lt;script&gt;alert(1)&lt;/script&gt;</p>"
        );
    }

    #[tokio::test]
    async fn test_rate_limit_per_ip() {
        let cache = MemoryCacheBackend::new();
        for _ in 0..2 {
            assert!(
                within_rate_limit(&cache, "comments", Some("203.0.113.7"), 2)
                    .await
                    .unwrap()
            );
        }
        assert!(
            !within_rate_limit(&cache, "comments", Some("203.0.113.7"), 2)
                .await
                .unwrap()
        );
        // another address and another scope have their own counters
        assert!(
            within_rate_limit(&cache, "comments", Some("203.0.113.8"), 2)
                .await
                .unwrap()
        );
        assert!(within_rate_limit(&cache, "reviews", Some("203.0.113.7"), 2)
            .await
            .unwrap());
        // unlimited, or no address
        assert!(
            within_rate_limit(&cache, "comments", Some("203.0.113.7"), 0)
                .await
                .unwrap()
        );
        assert!(within_rate_limit(&cache, "comments", None, 1)
            .await
            .unwrap());
    }
}
