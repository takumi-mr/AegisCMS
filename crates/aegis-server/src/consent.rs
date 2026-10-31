//! Cookie の同意バナー（`[consent]`）。公開ページの `<head>` にスクリプトとスタイル（`/_aegis/consent.js` /
//! `consent.css`）と設定（JSON）を、`</body>` の前にバナーの HTML を差し込み、訪問者の選択（first-party の
//! Cookie `aegis_consent`）を、同意が要るスクリプト・iframe（`data-aegis-consent="<区分>"`）の実行に結びつける。
//! HTML への差し込みは [`crate::inject`]
//!
//! - バナーの HTML は**テーマのテンプレート `consent.html`**（無ければ組み込み）で描く。文言・マークアップ・
//!   見た目はテーマが決め、スクリプトは `data-aegis-consent-*` の属性で要素を見つけて動かす
//! - どう表示するかは設定（`layout`・`position`・`backdrop`・`delay_ms`）で決まり、テンプレートに渡される
//! - 区分（`categories`）は設定ファイルで決める。必須（`necessary`）は常にオンで、同意の対象にしない
//! - スクリプトと CSS は外部ファイル、設定は実行しない JSON（`type="application/json"`）なので、`script-src 'self'`
//!   だけの厳しい CSP でも動く
//! - 区分の ID か `revision` が変わると、保存済みの選択は無効になって、もう一度聞く

use aegis_core::{AegisError, PageRenderContext, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::config::AegisConfig;
use crate::state::AppState;

const SCRIPT: &str = include_str!("../assets/consent.js");
const STYLE: &str = include_str!("../assets/consent.css");

/// 区分の数の上限
const MAX_CATEGORIES: usize = 10;
/// Cookie を残す日数の上限（ブラウザーの上限もこのくらい）
const MAX_COOKIE_DAYS: u32 = 395;
/// 表示を遅らせる時間の上限（ミリ秒）
const MAX_DELAY_MS: u32 = 10_000;
/// 組み込みの文言がある区分（名前と説明をテーマの `t()` が持つ）
pub const KNOWN_CATEGORIES: &[&str] = &["analytics", "marketing", "preferences", "functional"];

/// バナーの形
pub const LAYOUTS: &[&str] = &["banner", "box", "modal"];

/// 形ごとの、選べる位置（先頭が既定）
fn positions(layout: &str) -> &'static [&'static str] {
    match layout {
        "box" => &["bottom-right", "bottom-left", "top-right", "top-left"],
        "modal" => &["center"],
        _ => &["bottom", "top"],
    }
}

/// 同意の対象にする区分（`[[consent.categories]]`）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsentCategory {
    /// 英小文字で始まり、英小文字・数字・`-`・`_`（32 文字以内）。`data-aegis-consent` に書く名前
    pub id: String,
    /// 名前（空なら、`analytics` などの組み込みの名前。テーマの言語）
    #[serde(default)]
    pub label: String,
    /// 説明（空なら、組み込みの説明）
    #[serde(default)]
    pub description: String,
}

/// Cookie の同意バナー（`[consent]`）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConsentConfig {
    pub enabled: bool,
    /// 選択を残す日数
    pub cookie_days: u32,
    /// バナーの形: `banner`（画面の端の帯）/ `box`（隅のカード）/ `modal`（画面の中央。背景を暗くする）
    pub layout: String,
    /// 位置。`banner` は `bottom` / `top`、`box` は `bottom-right` / `bottom-left` / `top-right` / `top-left`、
    /// `modal` は `center`。空なら形の既定
    pub position: String,
    /// 背景を暗くしてページを操作できなくする。省略時は `modal` だけ `true`
    pub backdrop: Option<bool>,
    /// 表示を遅らせる時間（ミリ秒）
    pub delay_ms: u32,
    /// プライバシーポリシーの URL（`/` で始まるパスか `http(s)://`）。空ならリンクを出さない
    pub policy_url: String,
    /// バナーの説明文（空ならテーマの既定の文）
    pub message: String,
    /// 選択したあとに、いつでも設定を開き直せる小さなボタンを出す
    pub reopen_button: bool,
    /// 選択を無効にしてもう一度聞きたいときに増やす
    pub revision: u32,
    /// 同意の対象にする区分。省略時は `analytics` と `marketing`
    pub categories: Vec<ConsentCategory>,
}

impl Default for ConsentConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            cookie_days: 180,
            layout: "banner".to_string(),
            position: String::new(),
            backdrop: None,
            delay_ms: 0,
            policy_url: String::new(),
            message: String::new(),
            reopen_button: true,
            revision: 1,
            categories: Vec::new(),
        }
    }
}

fn valid_category_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 32
        && bytes[0].is_ascii_lowercase()
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
}

/// FNV-1a（バージョンが変わっても同じ値になる軽いハッシュ）
fn fnv1a(text: &str) -> u32 {
    text.bytes().fold(0x811c_9dc5_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    })
}

impl ConsentConfig {
    /// 設定の検査。`enabled` のときに起動時に呼ぶ
    pub fn validate(&self) -> Result<()> {
        let err = |m: String| Err(AegisError::Config(format!("[consent] {}", m)));
        if self.cookie_days == 0 || self.cookie_days > MAX_COOKIE_DAYS {
            return err(format!("cookie_days must be 1 to {}", MAX_COOKIE_DAYS));
        }
        if !LAYOUTS.contains(&self.layout.as_str()) {
            return err(format!(
                "layout must be one of {:?} (got {:?})",
                LAYOUTS, self.layout
            ));
        }
        if !self.position.is_empty() && !positions(&self.layout).contains(&self.position.as_str()) {
            return err(format!(
                "position for layout {:?} must be one of {:?} (got {:?})",
                self.layout,
                positions(&self.layout),
                self.position
            ));
        }
        if self.delay_ms > MAX_DELAY_MS {
            return err(format!("delay_ms must be 0 to {}", MAX_DELAY_MS));
        }
        let policy = self.policy_url.trim();
        if !policy.is_empty()
            && !(policy.starts_with("https://")
                || policy.starts_with("http://")
                || (policy.starts_with('/') && !policy.starts_with("//")))
        {
            return err(
                "policy_url must be a path starting with '/' or an http(s) URL".to_string(),
            );
        }
        if policy.chars().any(|c| c.is_control() || c.is_whitespace()) {
            return err("policy_url must not contain spaces".to_string());
        }
        if self.message.chars().count() > 600 {
            return err("message must be 600 characters or fewer".to_string());
        }
        if self.categories.len() > MAX_CATEGORIES {
            return err(format!(
                "at most {} categories can be defined",
                MAX_CATEGORIES
            ));
        }
        let mut seen = std::collections::HashSet::new();
        for category in &self.categories {
            if !valid_category_id(&category.id) {
                return err(format!(
                    "category id {:?} must be lowercase letters, digits, '-' or '_', starting with a letter (32 characters at most)",
                    category.id
                ));
            }
            if category.id == "necessary" {
                return err(
                    "\"necessary\" is always on and is not a category you can define".to_string(),
                );
            }
            if !seen.insert(category.id.as_str()) {
                return err(format!("category {:?} is defined twice", category.id));
            }
            if category.label.trim().is_empty() && !KNOWN_CATEGORIES.contains(&category.id.as_str())
            {
                return err(format!(
                    "category {:?} needs a label (only {} have a built-in name)",
                    category.id,
                    KNOWN_CATEGORIES.join(", ")
                ));
            }
            if category.label.chars().count() > 60 || category.description.chars().count() > 300 {
                return err(format!(
                    "category {:?}: label is 60 characters and description 300 characters at most",
                    category.id
                ));
            }
        }
        Ok(())
    }

    /// 位置（空なら形の既定）
    pub fn position(&self) -> &str {
        if self.position.is_empty() {
            positions(&self.layout)[0]
        } else {
            &self.position
        }
    }

    /// 背景を暗くするか（省略時は `modal` だけ）
    pub fn backdrop(&self) -> bool {
        self.backdrop.unwrap_or(self.layout == "modal")
    }

    /// 区分（省略時は `analytics` と `marketing`。名前と説明が空なら、テンプレートが組み込みのものを使う）
    pub fn categories(&self) -> Vec<ConsentCategory> {
        if self.categories.is_empty() {
            ["analytics", "marketing"]
                .iter()
                .map(|id| ConsentCategory {
                    id: id.to_string(),
                    label: String::new(),
                    description: String::new(),
                })
                .collect()
        } else {
            self.categories.clone()
        }
    }

    /// 保存した選択の版。区分の ID の並びか `revision` が変わると別の版になる
    pub fn rev(&self) -> String {
        let ids: Vec<String> = self.categories().into_iter().map(|c| c.id).collect();
        format!("{}-{:08x}", self.revision, fnv1a(&ids.join(",")))
    }

    /// スクリプトに渡す設定（見た目と文言はテンプレートが持つので、動きに要るものだけ）
    fn script_config(&self) -> serde_json::Value {
        json!({
            "rev": self.rev(),
            "days": self.cookie_days,
            "delay": self.delay_ms,
            "modal": self.layout == "modal",
            "categories": self.categories().into_iter().map(|c| c.id).collect::<Vec<_>>(),
        })
    }

    /// テンプレート `consent.html` に渡す値（`fields`）。`preview` ならテーマ開発のプレビュー（最初から表示する）
    pub fn template_fields(&self, preview: bool) -> serde_json::Value {
        json!({
            "consent": {
                "layout": self.layout,
                "position": self.position(),
                "backdrop": self.backdrop(),
                "policy_url": self.policy_url.trim(),
                "reopen_button": self.reopen_button,
                "delay_ms": self.delay_ms,
                "cookie_days": self.cookie_days,
            },
            "message": self.message.trim(),
            "categories": self.categories(),
            "preview": preview,
        })
    }
}

/// `<script type="application/json">` の中に置ける JSON（`<` `>` `&` と行の区切りを `\uXXXX` にする）
fn json_for_script(value: &serde_json::Value) -> String {
    value
        .to_string()
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

/// 公開ページの `<head>` に差し込む HTML（無効なら空）
pub fn head_html(config: &AegisConfig) -> String {
    if !config.consent.enabled {
        return String::new();
    }
    let base = config.server.normalized_base_path().unwrap_or_default();
    let version = env!("CARGO_PKG_VERSION");
    format!(
        "<link rel=\"stylesheet\" href=\"{base}/_aegis/consent.css?v={version}\">\n\
         <script type=\"application/json\" id=\"aegis-consent-config\">{json}</script>\n\
         <script src=\"{base}/_aegis/consent.js?v={version}\" defer></script>\n",
        base = base,
        version = version,
        json = json_for_script(&config.consent.script_config()),
    )
}

/// 公開ページの `</body>` の前に差し込むバナーの HTML（テーマの `consent.html`）。無効なら空。
/// テンプレートが壊れていて描けないときも空にする（バナーが出なくても、同意が要るものは動かないだけで安全）
pub fn body_html(state: &AppState) -> String {
    if !state.config.consent.enabled {
        return String::new();
    }
    let mut fields = std::collections::HashMap::new();
    if let Some(object) = state.config.consent.template_fields(false).as_object() {
        fields.extend(object.clone());
    }
    let ctx = PageRenderContext {
        template: "consent.html".to_string(),
        content_type: "consent".to_string(),
        site_title: state.site().title,
        fields,
        ..Default::default()
    };
    match state.theme.render("consent.html", &ctx) {
        Ok(html) => html,
        Err(e) => {
            tracing::error!("Could not render the consent banner (consent.html): {}", e);
            String::new()
        }
    }
}

/// 配信するスクリプト
pub fn script() -> &'static str {
    SCRIPT
}

/// 配信するスタイル
pub fn style() -> &'static str {
    STYLE
}

#[cfg(test)]
#[path = "consent_tests.rs"]
mod tests;
