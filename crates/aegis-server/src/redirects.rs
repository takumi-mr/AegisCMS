//! リダイレクト（旧 URL → 新 URL, 301 / 302）。Admin Studio で管理する規則と、slug を変えたときに残す自動の規則。
//!
//! - 規則は DB の設定（`_aegis_settings` の [`SETTING_KEY`]）に JSON で持つ（[`MAX_RULES`] 件まで）。起動時と
//!   変更の知らせ（`cluster::TOPIC_REDIRECTS`）でメモリに読み込む
//! - 規則を見るのは**ページが 404 になったときだけ**（`app::redirect_middleware`）。生きているページの URL は、
//!   規則があっても奪わない
//! - 一致は URL のパス（`[server].base_path` を除いたもの）で、パーセントエンコードを戻し、末尾の `/` を除いて比べる
//! - 規則の連なり（A → B、B → C）は作らない: 自動の規則を足すときに、向き先が古い URL の規則を新しい URL に付け替える。
//!   手で足すとき、たどると自分に戻る規則（ループ）は断る

use std::collections::HashSet;

use aegis_core::{AegisError, Result};
use aegis_db::DatabaseAdapter;
use serde::{Deserialize, Serialize};

/// 規則を入れる設定のキー
pub const SETTING_KEY: &str = "redirects.rules";
/// 規則の数の上限
pub const MAX_RULES: usize = 2000;
const MAX_FROM_LEN: usize = 512;
const MAX_TO_LEN: usize = 2000;
/// 規則をたどる回数の上限（ループの検出）
const MAX_HOPS: usize = 16;

/// リダイレクトの規則
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedirectRule {
    pub id: String,
    /// 旧 URL のパス（`/` で始まる。`[server].base_path` は含めない）
    pub from: String,
    /// 新しい URL: `/` で始まるパス（`base_path` を付けて返す）か、`http(s)://` の URL
    pub to: String,
    /// `301`（恒久）か `302`（一時）
    pub status: u16,
    /// slug の変更で自動に作った規則
    #[serde(default)]
    pub auto: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// 作る・変える規則の入力（検査前）
#[derive(Debug, Clone, Deserialize)]
pub struct RuleInput {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub status: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RedirectError {
    /// 入力の誤り（`field` は `from` / `to` / `status`）
    #[error("{message}")]
    Invalid {
        field: &'static str,
        message: String,
    },
    /// 同じ `from` の規則がある
    #[error("A redirect from this path already exists")]
    Duplicate,
    /// たどると `from` に戻る
    #[error("This redirect would loop back to itself")]
    Loop,
    #[error("Too many redirects (at most {MAX_RULES})")]
    TooMany,
    #[error("Redirect not found")]
    NotFound,
}

impl RedirectError {
    /// 機械が読む理由（API の `code`）
    pub fn code(&self) -> &'static str {
        match self {
            Self::Invalid { .. } => "invalid_redirect",
            Self::Duplicate => "duplicate_redirect",
            Self::Loop => "redirect_loop",
            Self::TooMany => "too_many_redirects",
            Self::NotFound => "redirect_not_found",
        }
    }
}

fn invalid(field: &'static str, message: &str) -> RedirectError {
    RedirectError::Invalid {
        field,
        message: message.to_string(),
    }
}

/// `%XX` を戻す（UTF-8 にならないときは入力のまま）
fn percent_decode(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(byte) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| path.to_string())
}

/// 比べるための形: パーセントエンコードを戻し、末尾の `/` を除く（ルート `/` はそのまま）
pub fn canonical_path(path: &str) -> String {
    let decoded = percent_decode(path);
    let trimmed = decoded.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}

/// `from` の検査と整形。`reserved` はリダイレクトを置けない先頭（`/api`・管理画面など）
pub fn normalize_from(
    raw: &str,
    reserved: &[String],
) -> std::result::Result<String, RedirectError> {
    let raw = raw.trim();
    if !raw.starts_with('/') || raw.starts_with("//") {
        return Err(invalid("from", "must be a path starting with '/'"));
    }
    if raw.contains(['?', '#']) {
        return Err(invalid(
            "from",
            "must not contain '?' or '#' (match the path only)",
        ));
    }
    if raw.len() > MAX_FROM_LEN || raw.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(invalid(
            "from",
            "must be 512 characters or fewer, without spaces or control characters",
        ));
    }
    let path = canonical_path(raw);
    if path == "/" {
        return Err(invalid("from", "the home page cannot be redirected"));
    }
    if path
        .split('/')
        .any(|segment| segment == "." || segment == "..")
    {
        return Err(invalid("from", "must not contain '.' or '..' segments"));
    }
    if reserved
        .iter()
        .any(|r| path == *r || path.starts_with(&format!("{}/", r)))
    {
        return Err(invalid("from", "this path is reserved by the server"));
    }
    Ok(path)
}

/// `to` の検査。`/` で始まるパス（`//` は不可）か、`http://` / `https://` の URL
pub fn validate_to(raw: &str) -> std::result::Result<String, RedirectError> {
    let raw = raw.trim();
    let is_path = raw.starts_with('/') && !raw.starts_with("//") && !raw.contains('\\');
    let is_url = raw.starts_with("https://") || raw.starts_with("http://");
    if !(is_path || is_url) {
        return Err(invalid(
            "to",
            "must be a path starting with '/' or an http(s) URL",
        ));
    }
    if raw.len() > MAX_TO_LEN || raw.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(invalid(
            "to",
            "must be 2000 characters or fewer, without spaces or control characters",
        ));
    }
    if is_url
        && raw
            .split('/')
            .nth(2)
            .is_none_or(|host| host.is_empty() || host.contains('@'))
    {
        return Err(invalid(
            "to",
            "must be an http(s) URL with a host and no user info",
        ));
    }
    Ok(raw.to_string())
}

fn validate_status(status: Option<u16>) -> std::result::Result<u16, RedirectError> {
    match status.unwrap_or(301) {
        s @ (301 | 302) => Ok(s),
        _ => Err(invalid("status", "must be 301 or 302")),
    }
}

/// `to` がこのサイトのパスなら、その比較用の形（クエリ・フラグメントを除く）
fn local_target(to: &str) -> Option<String> {
    if !to.starts_with('/') {
        return None;
    }
    let path = to.split(['?', '#']).next().unwrap_or(to);
    Some(canonical_path(path))
}

/// `Location` ヘッダーに入れる形: ASCII の制御文字・空白・`"<>\`{}|^` と ASCII 以外を `%XX` にする（`%` はそのまま）
pub fn encode_location(url: &str) -> String {
    let mut out = String::with_capacity(url.len());
    for byte in url.bytes() {
        if byte.is_ascii_graphic() && !b"\"<>\\`{}|^".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{:02X}", byte));
        }
    }
    out
}

/// すべての規則
#[derive(Debug, Clone, Default)]
pub struct RedirectTable {
    rules: Vec<RedirectRule>,
}

impl RedirectTable {
    /// DB から読む。読めない・壊れているときは警告して空にする（サイトは止めない）
    pub async fn load(db: &dyn DatabaseAdapter) -> Self {
        match db.list_settings().await {
            Ok(settings) => match settings.get(SETTING_KEY) {
                Some(json) => match serde_json::from_str::<Vec<RedirectRule>>(json) {
                    Ok(rules) => Self { rules },
                    Err(e) => {
                        tracing::warn!("Could not read the saved redirects, ignoring them: {}", e);
                        Self::default()
                    }
                },
                None => Self::default(),
            },
            Err(e) => {
                tracing::warn!("Could not load the redirects: {}", e);
                Self::default()
            }
        }
    }

    pub async fn save(&self, db: &dyn DatabaseAdapter) -> Result<()> {
        if self.rules.is_empty() {
            return db.delete_setting(SETTING_KEY).await;
        }
        let json = serde_json::to_string(&self.rules).map_err(|e| {
            AegisError::Serialization(format!("Could not serialize redirects: {}", e))
        })?;
        db.set_setting(SETTING_KEY, &json).await
    }

    pub fn rules(&self) -> &[RedirectRule] {
        &self.rules
    }

    /// このパスの規則（比べる形は [`canonical_path`]）
    pub fn resolve(&self, path: &str) -> Option<&RedirectRule> {
        let path = canonical_path(path);
        self.rules.iter().find(|r| r.from == path)
    }

    /// `from` から規則をたどって `from` に戻るか（`ignore` の規則は無いものとして）
    fn loops(&self, from: &str, to: &str, ignore: Option<&str>) -> bool {
        let mut seen = HashSet::new();
        let mut next = local_target(to);
        for _ in 0..MAX_HOPS {
            let Some(current) = next else {
                return false;
            };
            if current == from {
                return true;
            }
            if !seen.insert(current.clone()) {
                return true;
            }
            next = self
                .rules
                .iter()
                .find(|r| r.from == current && Some(r.id.as_str()) != ignore)
                .and_then(|r| local_target(&r.to));
        }
        true
    }

    /// 手で規則を足す
    pub fn add(
        &mut self,
        input: &RuleInput,
        reserved: &[String],
        id: String,
        now: &str,
    ) -> std::result::Result<RedirectRule, RedirectError> {
        let from = normalize_from(&input.from, reserved)?;
        let to = validate_to(&input.to)?;
        let status = validate_status(input.status)?;
        if self.rules.len() >= MAX_RULES {
            return Err(RedirectError::TooMany);
        }
        if self.rules.iter().any(|r| r.from == from) {
            return Err(RedirectError::Duplicate);
        }
        if self.loops(&from, &to, None) {
            return Err(RedirectError::Loop);
        }
        let rule = RedirectRule {
            id,
            from,
            to,
            status,
            auto: false,
            created_at: now.to_string(),
            updated_at: now.to_string(),
        };
        self.rules.push(rule.clone());
        Ok(rule)
    }

    /// 規則を変える（自動の規則を変えると、手で管理する規則になる）
    pub fn update(
        &mut self,
        id: &str,
        input: &RuleInput,
        reserved: &[String],
        now: &str,
    ) -> std::result::Result<RedirectRule, RedirectError> {
        let from = normalize_from(&input.from, reserved)?;
        let to = validate_to(&input.to)?;
        let status = validate_status(input.status)?;
        let index = self
            .rules
            .iter()
            .position(|r| r.id == id)
            .ok_or(RedirectError::NotFound)?;
        if self.rules.iter().any(|r| r.from == from && r.id != id) {
            return Err(RedirectError::Duplicate);
        }
        if self.loops(&from, &to, Some(id)) {
            return Err(RedirectError::Loop);
        }
        let rule = &mut self.rules[index];
        rule.from = from;
        rule.to = to;
        rule.status = status;
        rule.auto = false;
        rule.updated_at = now.to_string();
        Ok(rule.clone())
    }

    pub fn remove(&mut self, id: &str) -> Option<RedirectRule> {
        let index = self.rules.iter().position(|r| r.id == id)?;
        Some(self.rules.remove(index))
    }

    /// slug の変更で、旧 URL（`old`）から新しい URL（`new`）への規則を自動で足す。連なりを作らないよう、
    /// 向き先が `old` の規則を `new` に付け替え、`new` を `from` にする規則（今は生きているページの URL）は消す。
    /// 数が上限なら、いちばん古い自動の規則を消して入れる（自動の規則が無ければ足さない）。足したら規則を返す
    pub fn add_auto(
        &mut self,
        old: &str,
        new: &str,
        id: String,
        now: &str,
    ) -> Option<RedirectRule> {
        let (old, new) = (canonical_path(old), canonical_path(new));
        if old == new || old == "/" || new == "/" {
            return None;
        }
        self.rules.retain(|r| r.from != new && r.from != old);
        for rule in &mut self.rules {
            if local_target(&rule.to).as_deref() == Some(old.as_str()) {
                rule.to = new.clone();
                rule.updated_at = now.to_string();
            }
        }
        if self.rules.len() >= MAX_RULES {
            let oldest = self
                .rules
                .iter()
                .enumerate()
                .filter(|(_, r)| r.auto)
                .min_by(|(_, a), (_, b)| a.created_at.cmp(&b.created_at))
                .map(|(i, _)| i)?;
            self.rules.remove(oldest);
        }
        let rule = RedirectRule {
            id,
            from: old,
            to: new,
            status: 301,
            auto: true,
            created_at: now.to_string(),
            updated_at: now.to_string(),
        };
        self.rules.push(rule.clone());
        Some(rule)
    }
}

/// リダイレクトを置けない先頭のパス（`/api`・管理画面・アップロード）
pub fn reserved_prefixes(config: &crate::config::AegisConfig) -> Vec<String> {
    let mut reserved = vec!["/api".to_string(), "/uploads".to_string()];
    let admin = config.server.admin_path();
    let admin = admin.trim_end_matches('/');
    if !admin.is_empty() {
        reserved.push(admin.to_string());
    }
    reserved
}

/// コンテンツの種類ごとの、公開ページのパス（slug が変わったとき、旧 URL を新しい URL へ転送する対象）
pub fn public_paths(
    config: &crate::config::AegisConfig,
    content_type: &str,
    slug: &str,
) -> Vec<String> {
    if content_type == "article" {
        vec![format!("/articles/{}", slug), format!("/{}", slug)]
    } else if content_type == "page" {
        vec![format!("/{}", slug)]
    } else if content_type == config.commerce.product_type {
        vec![format!("/products/{}", slug)]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
#[path = "redirects_tests.rs"]
mod tests;
