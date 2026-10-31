//! お知らせバナー（メンテナンス・セールの告知など）。Admin Studio の Settings › Notices で、メッセージ・リンク・
//! 見た目の種類・表示する期間・閉じられるかを決める。公開ページの `<body>` の直後に、テーマの
//! `notices.html`（無ければ組み込み）で描いたバナーを差し込む（[`crate::inject`]）
//!
//! - お知らせは DB の設定（`_aegis_settings` の [`SETTING_KEY`]）に JSON で持つ（[`MAX_NOTICES`] 件まで）。
//!   起動時と変更の知らせ（`cluster::TOPIC_NOTICES`）でメモリに読み込む
//! - 期間（`start_at` 〜 `end_at`）は、**ページを返すときの時刻**で判定する。CDN にキャッシュされたページには、
//!   キャッシュの期限まで古い状態が残る。変更したときは、サイト全体のキャッシュをパージする
//! - 同時に出すのは、期間内の有効なお知らせのうち、作った順に [`MAX_SHOWN`] 件まで
//! - 閉じた記録はブラウザーの `localStorage` に残す（`/_aegis/notices.js`）。お知らせを編集すると版（`updated_at`）が
//!   変わるので、閉じた人にもう一度出る

use aegis_core::{AegisError, PageRenderContext, Result};
use aegis_db::DatabaseAdapter;
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::config::AegisConfig;
use crate::state::AppState;

const SCRIPT: &str = include_str!("../assets/notices.js");
const STYLE: &str = include_str!("../assets/notices.css");

/// お知らせを入れる設定のキー
pub const SETTING_KEY: &str = "notices.items";
/// お知らせの数の上限
pub const MAX_NOTICES: usize = 20;
/// 同時に出すお知らせの数の上限
pub const MAX_SHOWN: usize = 3;
const MAX_MESSAGE_CHARS: usize = 500;
const MAX_LABEL_CHARS: usize = 60;
const MAX_URL_CHARS: usize = 2000;
/// 見た目の種類
pub const STYLES: &[&str] = &["info", "success", "warning", "alert"];

/// お知らせ
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notice {
    pub id: String,
    pub message: String,
    /// リンク先（`/` で始まるパスか `http(s)://`）。空ならリンクなし
    #[serde(default)]
    pub link_url: String,
    /// リンクの文字（空ならテーマの既定の文）
    #[serde(default)]
    pub link_label: String,
    /// `info` / `success` / `warning` / `alert`
    pub style: String,
    /// この日時から表示する（RFC 3339。省略は「すぐ」）
    #[serde(default)]
    pub start_at: Option<String>,
    /// この日時まで表示する（RFC 3339。省略は「ずっと」）
    #[serde(default)]
    pub end_at: Option<String>,
    /// 訪問者が閉じられる
    pub dismissible: bool,
    /// `false` なら期間内でも出さない（下書き）
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// 作る・変えるお知らせの入力（検査前）
#[derive(Debug, Clone, Deserialize)]
pub struct NoticeInput {
    pub message: String,
    #[serde(default)]
    pub link_url: String,
    #[serde(default)]
    pub link_label: String,
    #[serde(default = "default_style")]
    pub style: String,
    #[serde(default)]
    pub start_at: Option<String>,
    #[serde(default)]
    pub end_at: Option<String>,
    #[serde(default = "yes")]
    pub dismissible: bool,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn default_style() -> String {
    "info".to_string()
}

fn yes() -> bool {
    true
}

/// 今のお知らせの状態
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NoticeStatus {
    /// 今、表示される
    Active,
    /// 開始の日時を待っている
    Scheduled,
    /// 終了の日時を過ぎた
    Expired,
    /// 無効にしている
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NoticeError {
    /// 入力の誤り（`field` は `message` / `link_url` / `link_label` / `style` / `start_at` / `end_at`）
    #[error("{message}")]
    Invalid {
        field: &'static str,
        message: String,
    },
    #[error("Too many notices (at most {MAX_NOTICES})")]
    TooMany,
    #[error("Notice not found")]
    NotFound,
}

impl NoticeError {
    /// 機械が読む理由（API の `code`）
    pub fn code(&self) -> &'static str {
        match self {
            Self::Invalid { .. } => "invalid_notice",
            Self::TooMany => "too_many_notices",
            Self::NotFound => "notice_not_found",
        }
    }
}

fn invalid(field: &'static str, message: &str) -> NoticeError {
    NoticeError::Invalid {
        field,
        message: message.to_string(),
    }
}

/// RFC 3339 の日時を UTC の決まった形にする
fn normalize_time(
    field: &'static str,
    value: &Option<String>,
) -> std::result::Result<Option<String>, NoticeError> {
    let Some(text) = value.as_deref().map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(None);
    };
    DateTime::parse_from_rfc3339(text)
        .map(|t| {
            Some(
                t.with_timezone(&Utc)
                    .to_rfc3339_opts(SecondsFormat::Micros, true),
            )
        })
        .map_err(|_| {
            invalid(
                field,
                "must be a date and time in RFC 3339 (e.g. 2026-12-01T09:00:00+09:00)",
            )
        })
}

fn no_control(text: &str) -> bool {
    !text.chars().any(char::is_control)
}

impl NoticeInput {
    /// 検査して、整えた入力を返す（前後の空白を除き、日時を UTC にそろえる）
    fn normalized(&self) -> std::result::Result<NoticeInput, NoticeError> {
        let message = self.message.trim();
        if message.is_empty() {
            return Err(invalid("message", "must not be empty"));
        }
        if message.chars().count() > MAX_MESSAGE_CHARS || !no_control(message) {
            return Err(invalid(
                "message",
                "must be 500 characters or fewer, on one line without control characters",
            ));
        }
        let link_url = self.link_url.trim();
        if !link_url.is_empty() {
            let is_path = link_url.starts_with('/')
                && !link_url.starts_with("//")
                && !link_url.contains('\\');
            let is_url = link_url.starts_with("https://") || link_url.starts_with("http://");
            if !(is_path || is_url)
                || link_url.chars().count() > MAX_URL_CHARS
                || link_url
                    .chars()
                    .any(|c| c.is_control() || c.is_whitespace())
            {
                return Err(invalid(
                    "link_url",
                    "must be a path starting with '/' or an http(s) URL, without spaces",
                ));
            }
        }
        let link_label = self.link_label.trim();
        if link_label.chars().count() > MAX_LABEL_CHARS || !no_control(link_label) {
            return Err(invalid("link_label", "must be 60 characters or fewer"));
        }
        if !STYLES.contains(&self.style.as_str()) {
            return Err(invalid("style", "must be info, success, warning or alert"));
        }
        let start_at = normalize_time("start_at", &self.start_at)?;
        let end_at = normalize_time("end_at", &self.end_at)?;
        if let (Some(start), Some(end)) = (&start_at, &end_at) {
            if end <= start {
                return Err(invalid("end_at", "must be after start_at"));
            }
        }
        Ok(NoticeInput {
            message: message.to_string(),
            link_url: link_url.to_string(),
            link_label: link_label.to_string(),
            style: self.style.clone(),
            start_at,
            end_at,
            dismissible: self.dismissible,
            enabled: self.enabled,
        })
    }
}

impl Notice {
    pub fn status(&self, now: DateTime<Utc>) -> NoticeStatus {
        if !self.enabled {
            return NoticeStatus::Disabled;
        }
        let parse = |t: &Option<String>| {
            t.as_deref()
                .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
                .map(|t| t.with_timezone(&Utc))
        };
        if parse(&self.start_at).is_some_and(|start| now < start) {
            return NoticeStatus::Scheduled;
        }
        if parse(&self.end_at).is_some_and(|end| now >= end) {
            return NoticeStatus::Expired;
        }
        NoticeStatus::Active
    }

    /// テンプレートとスクリプトに渡す版。編集すると変わるので、閉じた人にもう一度出る
    fn rev(&self) -> &str {
        &self.updated_at
    }
}

/// すべてのお知らせ（作った順）
#[derive(Debug, Clone, Default)]
pub struct NoticeList {
    items: Vec<Notice>,
}

impl NoticeList {
    /// DB から読む。読めない・壊れているときは警告して空にする（サイトは止めない）
    pub async fn load(db: &dyn DatabaseAdapter) -> Self {
        match db.list_settings().await {
            Ok(settings) => match settings.get(SETTING_KEY) {
                Some(json) => match serde_json::from_str::<Vec<Notice>>(json) {
                    Ok(items) => Self { items },
                    Err(e) => {
                        tracing::warn!("Could not read the saved notices, ignoring them: {}", e);
                        Self::default()
                    }
                },
                None => Self::default(),
            },
            Err(e) => {
                tracing::warn!("Could not load the notices: {}", e);
                Self::default()
            }
        }
    }

    pub async fn save(&self, db: &dyn DatabaseAdapter) -> Result<()> {
        if self.items.is_empty() {
            return db.delete_setting(SETTING_KEY).await;
        }
        let json = serde_json::to_string(&self.items).map_err(|e| {
            AegisError::Serialization(format!("Could not serialize notices: {}", e))
        })?;
        db.set_setting(SETTING_KEY, &json).await
    }

    pub fn items(&self) -> &[Notice] {
        &self.items
    }

    pub fn add(
        &mut self,
        input: &NoticeInput,
        id: String,
        now: &str,
    ) -> std::result::Result<Notice, NoticeError> {
        let input = input.normalized()?;
        if self.items.len() >= MAX_NOTICES {
            return Err(NoticeError::TooMany);
        }
        let notice = Notice {
            id,
            message: input.message,
            link_url: input.link_url,
            link_label: input.link_label,
            style: input.style,
            start_at: input.start_at,
            end_at: input.end_at,
            dismissible: input.dismissible,
            enabled: input.enabled,
            created_at: now.to_string(),
            updated_at: now.to_string(),
        };
        self.items.push(notice.clone());
        Ok(notice)
    }

    pub fn update(
        &mut self,
        id: &str,
        input: &NoticeInput,
        now: &str,
    ) -> std::result::Result<Notice, NoticeError> {
        let input = input.normalized()?;
        let notice = self
            .items
            .iter_mut()
            .find(|n| n.id == id)
            .ok_or(NoticeError::NotFound)?;
        notice.message = input.message;
        notice.link_url = input.link_url;
        notice.link_label = input.link_label;
        notice.style = input.style;
        notice.start_at = input.start_at;
        notice.end_at = input.end_at;
        notice.dismissible = input.dismissible;
        notice.enabled = input.enabled;
        notice.updated_at = now.to_string();
        Ok(notice.clone())
    }

    pub fn remove(&mut self, id: &str) -> Option<Notice> {
        let index = self.items.iter().position(|n| n.id == id)?;
        Some(self.items.remove(index))
    }

    /// 今表示するお知らせ（期間内で有効なもの。作った順に [`MAX_SHOWN`] 件まで）
    pub fn active(&self, now: DateTime<Utc>) -> Vec<&Notice> {
        self.items
            .iter()
            .filter(|n| n.status(now) == NoticeStatus::Active)
            .take(MAX_SHOWN)
            .collect()
    }
}

/// テンプレート `notices.html` に渡す値（`fields`）
fn template_fields(active: &[&Notice]) -> serde_json::Value {
    json!({
        "notices": active
            .iter()
            .map(|n| json!({
                "id": n.id,
                "message": n.message,
                "link_url": n.link_url,
                "link_label": n.link_label,
                "style": n.style,
                "dismissible": n.dismissible,
                "rev": n.rev(),
            }))
            .collect::<Vec<_>>(),
        "preview": false,
    })
}

/// 公開ページの `<head>` に差し込む HTML（表示するお知らせが無ければ空）
pub fn head_html(config: &AegisConfig, any_active: bool) -> String {
    if !any_active {
        return String::new();
    }
    let base = config.server.normalized_base_path().unwrap_or_default();
    let version = env!("CARGO_PKG_VERSION");
    format!(
        "<link rel=\"stylesheet\" href=\"{base}/_aegis/notices.css?v={version}\">\n\
         <script src=\"{base}/_aegis/notices.js?v={version}\" defer></script>\n",
        base = base,
        version = version,
    )
}

/// 公開ページの `<body>` の直後に差し込むバナーの HTML（テーマの `notices.html`）。表示するお知らせが無ければ空。
/// テンプレートが壊れていて描けないときも空にする（お知らせが出なくても、ページは返す）
pub fn body_html(state: &AppState, active: &[&Notice]) -> String {
    if active.is_empty() {
        return String::new();
    }
    let mut fields = std::collections::HashMap::new();
    if let Some(object) = template_fields(active).as_object() {
        fields.extend(object.clone());
    }
    let ctx = PageRenderContext {
        template: "notices.html".to_string(),
        content_type: "notices".to_string(),
        site_title: state.site().title,
        fields,
        ..Default::default()
    };
    match state.theme.render("notices.html", &ctx) {
        Ok(html) => html,
        Err(e) => {
            tracing::error!("Could not render the notices (notices.html): {}", e);
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
#[path = "notices_tests.rs"]
mod tests;
