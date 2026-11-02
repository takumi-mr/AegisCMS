//! `[contact]` の設定。フォームと項目は `[[contact.forms]]` / `[[contact.forms.fields]]` で決める

use std::collections::HashSet;

use aegis_core::{AegisError, Result};
use serde::{Deserialize, Serialize};

/// 項目の値の長さの上限（設定できる最大値）
const MAX_LENGTH_LIMIT: usize = 20_000;
/// 1 つのフォームの項目の数の上限
const MAX_FIELDS: usize = 30;
/// ボット除けの見えない欄の名前（項目の名前には使えない）
pub const HONEYPOT_FIELD: &str = "website";

/// 問い合わせフォーム（`[contact]`）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ContactConfig {
    pub enabled: bool,
    /// 届いた問い合わせを残す日数（0 は消さない）
    pub retention_days: u32,
    /// 同じクライアント IP からの 1 時間あたりの送信数の上限（0 で無制限）
    pub max_per_ip_per_hour: u64,
    /// 入力のリンク（`http://` / `https://` / `www.`）がこれより多いとスパムにする
    pub max_links: usize,
    /// 含むとスパムにする語（大文字・小文字を区別しない）
    pub spam_words: Vec<String>,
    /// 届いたことを運営者（`[notifications.email].staff_recipients` など）に知らせる
    pub notify_staff: bool,
    /// 組み込みの文言（入力の誤り・既定のフォームの見出しや項目名）の言語: `en` / `ja`
    pub locale: String,
    /// 添付ファイル（`type = "file"` の項目）1 つあたりの大きさの上限（MB）
    pub max_attachment_mb: u64,
    /// 添付できるファイルの拡張子（小文字・ドットなし。大文字・小文字を区別しない）
    pub attachment_extensions: Vec<String>,
    /// フォーム。省略時は名前・メールアドレス・件名・本文の `default` フォーム 1 つ
    pub forms: Vec<FormConfig>,
}

/// 添付できるファイルの拡張子の既定（画面で開くと危険な HTML・SVG・スクリプトは含めない）
pub const DEFAULT_ATTACHMENT_EXTENSIONS: [&str; 10] = [
    "jpg", "jpeg", "png", "gif", "webp", "pdf", "txt", "csv", "docx", "xlsx",
];
/// `max_attachment_mb` の上限
const MAX_ATTACHMENT_MB_LIMIT: u64 = 50;

impl Default for ContactConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            retention_days: 365,
            max_per_ip_per_hour: 5,
            max_links: 3,
            spam_words: Vec::new(),
            notify_staff: true,
            locale: "en".to_string(),
            max_attachment_mb: 5,
            attachment_extensions: DEFAULT_ATTACHMENT_EXTENSIONS
                .iter()
                .map(|e| e.to_string())
                .collect(),
            forms: vec![FormConfig::default()],
        }
    }
}

/// 1 つのフォーム（`[[contact.forms]]`）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FormConfig {
    /// URL（`/contact/{name}`）とブロックで使う名前（英小文字・数字・`-`・`_`）
    pub name: String,
    /// ページの見出し（空なら `locale` の既定の文言）
    pub title: String,
    /// フォームの上に出す説明（プレーンテキスト）
    pub description: String,
    /// 送信ボタンの文言（空なら `locale` の既定の文言）
    pub submit_label: String,
    /// 送信した後に出す文言（空なら `locale` の既定の文言）
    pub success_message: String,
    /// 送信した人に、受け付けたことをメールで知らせる（通知のテンプレート `contact.auto_reply`）
    pub auto_reply: bool,
    /// 受信箱・通知で件名として扱う項目の名前
    pub subject_field: String,
    /// 項目。空なら名前・メールアドレス・件名・本文（`locale` の言語）
    pub fields: Vec<FieldConfig>,
}

impl Default for FormConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            title: String::new(),
            description: String::new(),
            submit_label: String::new(),
            success_message: String::new(),
            auto_reply: false,
            subject_field: "subject".to_string(),
            fields: Vec::new(),
        }
    }
}

/// 項目の種類
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldKind {
    /// 1 行の文字
    #[default]
    Text,
    /// メールアドレス（形を確かめ、小文字にする。最初のメールアドレスの項目が送信者のアドレス）
    Email,
    /// 電話番号（数字・`+`・`-`・`(` `)`・空白）
    Tel,
    /// 複数行の文字
    Textarea,
    /// 選択肢（`options`）から 1 つ
    Select,
    /// チェックボックス（必須なら同意の確認などに使う）
    Checkbox,
    /// ファイルの添付（`max_attachment_mb` / `attachment_extensions`。HTML のフォームからだけ送れる）
    File,
}

/// フォームの 1 項目（`[[contact.forms.fields]]`）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldConfig {
    /// 送信される名前（英小文字・数字・`_`）。`name` は送信者の名前として受信箱に出す
    pub name: String,
    pub label: String,
    #[serde(rename = "type", default)]
    pub kind: FieldKind,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub placeholder: String,
    /// `select` の選択肢
    #[serde(default)]
    pub options: Vec<String>,
    /// 最大文字数（省略時は `textarea` が 5000、それ以外は 200）
    #[serde(default)]
    pub max_length: Option<usize>,
}

impl FieldConfig {
    fn new(name: &str, label: &str, kind: FieldKind, required: bool) -> Self {
        Self {
            name: name.to_string(),
            label: label.to_string(),
            kind,
            required,
            placeholder: String::new(),
            options: Vec::new(),
            max_length: None,
        }
    }

    pub fn max_chars(&self) -> usize {
        self.max_length.unwrap_or(match self.kind {
            FieldKind::Textarea => 5000,
            _ => 200,
        })
    }
}

/// 項目を書かなかったフォームの項目（`locale` の言語）
pub fn default_fields(locale: &str) -> Vec<FieldConfig> {
    let ja = locale == "ja";
    vec![
        FieldConfig::new(
            "name",
            if ja { "お名前" } else { "Name" },
            FieldKind::Text,
            true,
        ),
        FieldConfig::new(
            "email",
            if ja { "メールアドレス" } else { "Email" },
            FieldKind::Email,
            true,
        ),
        FieldConfig::new(
            "subject",
            if ja { "件名" } else { "Subject" },
            FieldKind::Text,
            false,
        ),
        FieldConfig::new(
            "message",
            if ja {
                "お問い合わせ内容"
            } else {
                "Message"
            },
            FieldKind::Textarea,
            true,
        ),
    ]
}

fn valid_identifier(value: &str, allow_dash: bool, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || (allow_dash && b == b'-')
        })
}

fn valid_extension(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 10
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

impl ContactConfig {
    /// 添付ファイルの大きさの上限（バイト）
    pub fn max_attachment_bytes(&self) -> usize {
        (self.max_attachment_mb as usize).saturating_mul(1024 * 1024)
    }

    /// 1 回の送信で受け付ける本文の大きさの上限（添付の最大数 × 上限 + 項目分の余裕）
    pub fn max_submission_bytes(&self) -> usize {
        let files = self
            .forms
            .iter()
            .map(|f| {
                f.fields
                    .iter()
                    .filter(|x| x.kind == FieldKind::File)
                    .count()
            })
            .max()
            .unwrap_or(0);
        self.max_attachment_bytes()
            .saturating_mul(files)
            .saturating_add(512 * 1024)
    }

    pub fn form(&self, name: &str) -> Option<&FormConfig> {
        self.forms.iter().find(|f| f.name == name)
    }

    /// 空の文言・項目を `locale` の既定で埋めたもの（`ContactService` はこれを使う）
    pub fn resolved(mut self) -> Self {
        let text = |key: &str| crate::messages::text(&self.locale, key).to_string();
        let (title, submit, success) = (text("title"), text("submit"), text("success"));
        let fields = default_fields(&self.locale);
        for form in &mut self.forms {
            if form.title.trim().is_empty() {
                form.title = title.clone();
            }
            if form.submit_label.trim().is_empty() {
                form.submit_label = submit.clone();
            }
            if form.success_message.trim().is_empty() {
                form.success_message = success.clone();
            }
            if form.fields.is_empty() {
                form.fields = fields.clone();
            }
        }
        self
    }

    /// 設定の検査。`enabled` のときに起動時に呼ぶ
    pub fn validate(&self) -> Result<()> {
        let err = |m: String| Err(AegisError::Config(format!("[contact] {}", m)));
        if !crate::messages::LOCALES.contains(&self.locale.as_str()) {
            return err(format!(
                "locale must be one of {:?} (got {:?})",
                crate::messages::LOCALES,
                self.locale
            ));
        }
        if self.max_attachment_mb == 0 || self.max_attachment_mb > MAX_ATTACHMENT_MB_LIMIT {
            return err(format!(
                "max_attachment_mb must be 1 to {}",
                MAX_ATTACHMENT_MB_LIMIT
            ));
        }
        if self.attachment_extensions.is_empty()
            || self
                .attachment_extensions
                .iter()
                .any(|e| !valid_extension(e))
        {
            return err(
                "attachment_extensions must be lowercase letters and digits without a dot"
                    .to_string(),
            );
        }
        let resolved = self.clone().resolved();
        if resolved.forms.is_empty() {
            return err("needs at least one [[contact.forms]]".to_string());
        }
        for word in &resolved.spam_words {
            if word.trim().is_empty() {
                return err("spam_words must not contain empty words".to_string());
            }
        }
        let mut names = HashSet::new();
        for form in &resolved.forms {
            if !valid_identifier(&form.name, true, 64) {
                return err(format!(
                    "form name {:?} must be lowercase letters, digits, '-' or '_' (64 characters at most)",
                    form.name
                ));
            }
            if !names.insert(form.name.as_str()) {
                return err(format!("form {:?} is defined twice", form.name));
            }
            form.validate()
                .or_else(|m| err(format!("form {:?}: {}", form.name, m)))?;
        }
        Ok(())
    }
}

impl FormConfig {
    fn validate(&self) -> std::result::Result<(), String> {
        if self.fields.is_empty() {
            return Err("needs at least one field".to_string());
        }
        if self.fields.len() > MAX_FIELDS {
            return Err(format!("has more than {} fields", MAX_FIELDS));
        }
        let mut names = HashSet::new();
        for field in &self.fields {
            if !valid_identifier(&field.name, false, 64) {
                return Err(format!(
                    "field name {:?} must be lowercase letters, digits or '_'",
                    field.name
                ));
            }
            if field.name == HONEYPOT_FIELD {
                return Err(format!(
                    "the field name {:?} is reserved for the spam trap",
                    HONEYPOT_FIELD
                ));
            }
            if !names.insert(field.name.as_str()) {
                return Err(format!("field {:?} is defined twice", field.name));
            }
            if field.label.trim().is_empty() {
                return Err(format!("field {:?} needs a label", field.name));
            }
            if field.kind == FieldKind::Select
                && (field.options.is_empty() || field.options.iter().any(|o| o.trim().is_empty()))
            {
                return Err(format!(
                    "select field {:?} needs non-empty options",
                    field.name
                ));
            }
            if field
                .max_length
                .is_some_and(|n| n == 0 || n > MAX_LENGTH_LIMIT)
            {
                return Err(format!(
                    "field {:?}: max_length must be 1 to {}",
                    field.name, MAX_LENGTH_LIMIT
                ));
            }
        }
        if self.subject_field != "subject"
            && !self.fields.iter().any(|f| f.name == self.subject_field)
        {
            return Err(format!(
                "subject_field {:?} is not one of the fields",
                self.subject_field
            ));
        }
        if self.auto_reply
            && !self
                .fields
                .iter()
                .any(|f| f.kind == FieldKind::Email && f.required)
        {
            return Err(
                "auto_reply needs a required field with type = \"email\" (the reply goes there)"
                    .to_string(),
            );
        }
        Ok(())
    }

    /// 送信者のメールアドレスの項目（最初のメールアドレスの項目）
    pub fn email_field(&self) -> Option<&FieldConfig> {
        self.fields.iter().find(|f| f.kind == FieldKind::Email)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(toml_text: &str) -> ContactConfig {
        #[derive(Deserialize)]
        struct Root {
            contact: ContactConfig,
        }
        toml::from_str::<Root>(toml_text).unwrap().contact
    }

    #[test]
    fn test_defaults_have_one_form() {
        let config = parse("[contact]\nenabled = true\n");
        assert!(config.validate().is_ok());
        let config = config.resolved();
        assert_eq!(config.forms.len(), 1);
        assert_eq!(config.forms[0].name, "default");
        assert_eq!(config.forms[0].title, "Contact");
        assert_eq!(config.forms[0].submit_label, "Send");
        assert_eq!(config.forms[0].fields.len(), 4);
    }

    #[test]
    fn test_japanese_defaults() {
        let config = parse("[contact]\nlocale = \"ja\"\n").resolved();
        let form = &config.forms[0];
        assert_eq!(form.title, "お問い合わせ");
        assert_eq!(form.submit_label, "送信する");
        assert_eq!(form.fields[0].label, "お名前");
        assert!(parse("[contact]\nlocale = \"fr\"\n").validate().is_err());
    }

    #[test]
    fn test_forms_from_toml() {
        let config = parse(
            r#"
            [contact]
            enabled = true

            [[contact.forms]]
            name = "sales"
            title = "導入のご相談"
            auto_reply = true
            subject_field = "plan"

            [[contact.forms.fields]]
            name = "company"
            label = "会社名"

            [[contact.forms.fields]]
            name = "email"
            label = "メールアドレス"
            type = "email"
            required = true

            [[contact.forms.fields]]
            name = "plan"
            label = "プラン"
            type = "select"
            options = ["Free", "Pro"]

            [[contact.forms]]
            name = "simple"
            "#,
        );
        assert!(config.validate().is_ok(), "{:?}", config.validate());
        let config = config.resolved();
        let sales = config.form("sales").unwrap();
        assert_eq!(sales.title, "導入のご相談");
        assert_eq!(sales.fields[2].kind, FieldKind::Select);
        assert_eq!(sales.submit_label, "Send");
        assert_eq!(sales.email_field().unwrap().name, "email");
        // 項目を書かないフォームは既定の項目
        assert_eq!(config.form("simple").unwrap().fields, default_fields("en"));
    }

    #[test]
    fn test_invalid_forms() {
        let check = |form: FormConfig| {
            ContactConfig {
                forms: vec![form],
                ..ContactConfig::default()
            }
            .validate()
        };
        let base = FormConfig {
            fields: default_fields("en"),
            ..FormConfig::default()
        };
        assert!(check(base.clone()).is_ok());
        assert!(check(FormConfig {
            name: "Bad Name".into(),
            ..base.clone()
        })
        .is_err());
        let mut honeypot = base.clone();
        honeypot.fields[0].name = "website".into();
        assert!(check(honeypot).is_err());
        let mut twice = base.clone();
        twice.fields[1].name = "name".into();
        assert!(check(twice).is_err());
        let mut select = base.clone();
        select.fields[2].kind = FieldKind::Select;
        assert!(check(select).is_err());
        let mut no_email = base.clone();
        no_email.auto_reply = true;
        no_email.fields[1].required = false;
        assert!(check(no_email).is_err());
        assert!(check(FormConfig {
            subject_field: "missing".into(),
            ..base.clone()
        })
        .is_err());
        let duplicate = ContactConfig {
            forms: vec![base.clone(), base],
            ..ContactConfig::default()
        };
        assert!(duplicate.validate().is_err());
    }
}
