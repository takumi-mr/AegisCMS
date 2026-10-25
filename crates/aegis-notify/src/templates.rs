//! 通知の本文のテンプレート（MiniJinja）。
//!
//! 通知の種類ごとに `<種類>.subject`（件名）と `<種類>.txt`（本文、プレーンテキスト）を使う。運営者向け
//! （`Recipient::Staff`）は `<種類>.staff.subject` / `<種類>.staff.txt` を先に探す。
//! 無ければ種類の最後の `.` 以降を外した名前を順に探す（`audit.session.create` → `audit.session` → `audit`）。
//!
//! HTML メール（`with_html`）は、同じ名前の `<種類>.html` があればそれを、無ければ共通のレイアウト
//! `_layout.html` を使う。レイアウトにはプレーンテキストの本文を HTML にしたもの（`body_html`。段落・改行・
//! URL のリンク）を渡すので、種類ごとに HTML を書かなくてもすべての通知が HTML になる。`.html` は自動でエスケープする。
//!
//! テンプレートは Admin Studio で編集したもの（`set_overrides`）、`[notifications].templates_dir` のファイル、
//! 組み込み（`locale` の言語）の順に探す

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use aegis_core::{AegisError, Recipient, Result};
use minijinja::{context, AutoEscape, Environment, ErrorKind, Value};
use serde::Serialize;

/// 組み込みテンプレートの言語
pub const LOCALES: &[&str] = &["en", "ja"];

/// HTML メールの共通レイアウト（言語に関係なく 1 つ）
pub const LAYOUT: &str = "_layout.html";

/// 編集できるテンプレートの拡張子
const EXTENSIONS: &[&str] = &[".subject", ".txt", ".html"];

/// 1 つのテンプレートの大きさの上限（Admin Studio から保存するもの）
pub const MAX_TEMPLATE_BYTES: usize = 60_000;

macro_rules! builtin_templates {
    ($($name:literal),* $(,)?) => {
        /// 組み込みテンプレートの名前（`_layout.html` を除く。どの言語にもある）
        pub const BUILTIN_NAMES: &[&str] = &[$($name),*];

        fn builtin(locale: &str, name: &str) -> Option<&'static str> {
            match (locale, name) {
                (_, LAYOUT) => Some(include_str!("templates/_layout.html")),
                $(
                    ("en", $name) => Some(include_str!(concat!("templates/en/", $name))),
                    ("ja", $name) => Some(include_str!(concat!("templates/ja/", $name))),
                )*
                _ => None,
            }
        }
    };
}

builtin_templates!(
    "_summary.txt",
    "_footer.txt",
    "order.placed.subject",
    "order.placed.txt",
    "order.placed.staff.subject",
    "order.placed.staff.txt",
    "order.paid.subject",
    "order.paid.txt",
    "order.shipped.subject",
    "order.shipped.txt",
    "order.cancelled.subject",
    "order.cancelled.txt",
    "order.cancelled.staff.subject",
    "order.cancelled.staff.txt",
    "stock.low.staff.subject",
    "stock.low.staff.txt",
    "test.subject",
    "test.txt",
    "account.verify.subject",
    "account.verify.txt",
    "account.registered.staff.subject",
    "account.registered.staff.txt",
    "account.password_reset.subject",
    "account.password_reset.txt",
    "account.approved.subject",
    "account.approved.txt",
    "account.invite.subject",
    "account.invite.txt",
    "content.published.subject",
    "content.published.txt",
    "content.unpublished.subject",
    "content.unpublished.txt",
    "_comment_summary.txt",
    "comment.posted.subject",
    "comment.posted.txt",
    "comment.pending.subject",
    "comment.pending.txt",
    "comment.pending.staff.subject",
    "comment.pending.staff.txt",
    "comment.replied.subject",
    "comment.replied.txt",
    "comment.approved.subject",
    "comment.approved.txt",
    "_audit_title.txt",
    "audit.subject",
    "audit.txt",
);

/// テンプレート名として安全か（ディレクトリの外を読ませない）
fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// Admin Studio から編集できる名前か（`<種類>.subject` / `.txt` / `.html`、部品の `_*.txt` など）
pub fn is_editable_name(name: &str) -> bool {
    name.len() <= 200
        && safe_name(name)
        && EXTENSIONS
            .iter()
            .any(|ext| name.len() > ext.len() && name.ends_with(ext))
}

/// テンプレートの出どころ
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TemplateSource {
    /// Admin Studio で編集したもの（DB の `_aegis_settings`）
    Custom,
    /// `[notifications].templates_dir` のファイル
    File,
    /// 組み込み
    Builtin,
}

/// テンプレートの一覧の 1 件
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TemplateInfo {
    pub name: String,
    pub source: TemplateSource,
}

type Overrides = Arc<RwLock<HashMap<String, String>>>;

/// `name` の中身を探す（編集したもの → ファイル → 組み込み）
fn load_source(
    overrides: Option<&Overrides>,
    dir: Option<&Path>,
    locale: &str,
    name: &str,
) -> std::io::Result<Option<(TemplateSource, String)>> {
    if !safe_name(name) {
        return Ok(None);
    }
    if let Some(overrides) = overrides {
        let overrides = overrides
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(source) = overrides.get(name) {
            return Ok(Some((TemplateSource::Custom, source.clone())));
        }
    }
    if let Some(found) = load_default(dir, locale, name)? {
        return Ok(Some(found));
    }
    Ok(None)
}

/// 編集したものを除いた中身（元に戻したときに使われるもの）
fn load_default(
    dir: Option<&Path>,
    locale: &str,
    name: &str,
) -> std::io::Result<Option<(TemplateSource, String)>> {
    if let Some(dir) = dir {
        match std::fs::read_to_string(dir.join(name)) {
            Ok(source) => return Ok(Some((TemplateSource::File, source))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(builtin(locale, name).map(|s| (TemplateSource::Builtin, s.to_string())))
}

pub struct Templates {
    locale: String,
    dir: Option<PathBuf>,
    html: bool,
    overrides: Overrides,
    env: RwLock<Environment<'static>>,
}

/// 作った件名と本文
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    pub subject: String,
    pub body: String,
    /// HTML の本文（`with_html(true)` のとき）
    pub html: Option<String>,
}

fn environment(overrides: Overrides, dir: Option<PathBuf>, locale: String) -> Environment<'static> {
    let mut env = Environment::new();
    // HTML のテンプレートだけエスケープする（件名・本文はプレーンテキスト）
    env.set_auto_escape_callback(|name| {
        if name.ends_with(".html") {
            AutoEscape::Html
        } else {
            AutoEscape::None
        }
    });
    env.set_trim_blocks(true);
    env.set_lstrip_blocks(true);
    env.set_keep_trailing_newline(true);
    // 最小単位の整数の金額を通貨の表記にする（`{{ order.total | money(order.currency) }}`）
    env.add_filter("money", |amount: Value, currency: String| -> String {
        match i64::try_from(amount.clone()) {
            Ok(amount) => aegis_core::format_money(amount, &currency),
            Err(_) => amount.to_string(),
        }
    });
    env.set_loader(move |name| {
        load_source(Some(&overrides), dir.as_deref(), &locale, name)
            .map(|found| found.map(|(_, source)| source))
            .map_err(|e| {
                minijinja::Error::new(
                    ErrorKind::InvalidOperation,
                    format!("Failed to read notification template {}: {}", name, e),
                )
            })
    });
    env
}

impl Templates {
    pub fn new(locale: &str, templates_dir: Option<&Path>) -> Self {
        let locale = if LOCALES.contains(&locale) {
            locale.to_string()
        } else {
            "en".to_string()
        };
        let dir: Option<PathBuf> = templates_dir.map(Path::to_path_buf);
        let overrides: Overrides = Arc::default();
        Self {
            env: RwLock::new(environment(overrides.clone(), dir.clone(), locale.clone())),
            locale,
            dir,
            html: false,
            overrides,
        }
    }

    /// HTML の本文も作る
    pub fn with_html(mut self, html: bool) -> Self {
        self.html = html;
        self
    }

    pub fn html(&self) -> bool {
        self.html
    }

    pub fn locale(&self) -> &str {
        &self.locale
    }

    /// 同じ設定で、編集したテンプレートだけを差し替えたもの（保存前のプレビュー用）
    pub fn preview(&self, overrides: HashMap<String, String>) -> Self {
        let templates = Self::new(&self.locale, self.dir.as_deref()).with_html(self.html);
        let mut merged = self.overrides();
        merged.extend(overrides);
        templates.set_overrides(merged);
        templates
    }

    /// Admin Studio で編集したテンプレート（名前 → 中身）をすべて差し替える
    pub fn set_overrides(&self, overrides: HashMap<String, String>) {
        *self
            .overrides
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = overrides;
        // 読み込み済みのテンプレートを捨てて読み直させる
        self.env
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear_templates();
    }

    pub fn overrides(&self) -> HashMap<String, String> {
        self.overrides
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// テンプレートの中身と出どころ
    pub fn source(&self, name: &str) -> Result<Option<(TemplateSource, String)>> {
        load_source(
            Some(&self.overrides),
            self.dir.as_deref(),
            &self.locale,
            name,
        )
        .map_err(|e| AegisError::Internal(format!("Notification template {}: {}", name, e)))
    }

    /// 編集したものを除いた中身（ファイルか組み込み。元に戻すと使われる）
    pub fn default_source(&self, name: &str) -> Result<Option<(TemplateSource, String)>> {
        if !safe_name(name) {
            return Ok(None);
        }
        load_default(self.dir.as_deref(), &self.locale, name)
            .map_err(|e| AegisError::Internal(format!("Notification template {}: {}", name, e)))
    }

    /// 使えるテンプレートの一覧（組み込み・`templates_dir` のファイル・編集したもの。名前順）
    pub fn list(&self) -> Vec<TemplateInfo> {
        let mut names: Vec<String> = BUILTIN_NAMES.iter().map(|n| n.to_string()).collect();
        names.push(LAYOUT.to_string());
        if let Some(dir) = &self.dir {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    if let Some(name) = entry.file_name().to_str() {
                        if is_editable_name(name) {
                            names.push(name.to_string());
                        }
                    }
                }
            }
        }
        names.extend(self.overrides().into_keys());
        names.sort();
        names.dedup();
        names
            .into_iter()
            .filter_map(|name| {
                let (source, _) = self.source(&name).ok()??;
                Some(TemplateInfo { name, source })
            })
            .collect()
    }

    /// 宛先向けの件名と本文を作る。テンプレートが無ければ `Ok(None)`（その通知は送らない）
    pub fn render(
        &self,
        kind: &str,
        recipient: &Recipient,
        context: &serde_json::Value,
    ) -> Result<Option<Rendered>> {
        if !safe_name(kind) {
            return Err(AegisError::Internal(format!(
                "Invalid notification kind {:?}",
                kind
            )));
        }
        let mut candidates = Vec::new();
        let mut base = Some(kind);
        while let Some(name) = base {
            if matches!(recipient, Recipient::Staff) {
                candidates.push(format!("{}.staff", name));
            }
            candidates.push(name.to_string());
            base = name.rsplit_once('.').map(|(parent, _)| parent);
        }
        let ctx = Value::from_serialize(context);
        let env = self
            .env
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for (index, base) in candidates.iter().enumerate() {
            let subject = match env.get_template(&format!("{}.subject", base)) {
                Ok(template) => template,
                Err(e) if e.kind() == ErrorKind::TemplateNotFound => continue,
                Err(e) => return Err(template_error(base, e)),
            };
            let body = env
                .get_template(&format!("{}.txt", base))
                .map_err(|e| template_error(base, e))?;
            let subject = subject.render(&ctx).map_err(|e| template_error(base, e))?;
            let body = body.render(&ctx).map_err(|e| template_error(base, e))?;
            // 件名は 1 行
            let subject = subject.split_whitespace().collect::<Vec<_>>().join(" ");
            let body = tidy(&body);
            let html = if self.html {
                // 件名・本文と同じ名前まで（運営者向けの本文に顧客向けの HTML を使わない）
                Some(self.render_html(&env, &candidates[..=index], &subject, &body, &ctx)?)
            } else {
                None
            };
            return Ok(Some(Rendered {
                subject,
                body,
                html,
            }));
        }
        Ok(None)
    }

    fn render_html(
        &self,
        env: &Environment<'static>,
        candidates: &[String],
        subject: &str,
        body: &str,
        ctx: &Value,
    ) -> Result<String> {
        let (body_html, footer_html) = crate::html::text_to_html(body);
        let ctx = context! {
            subject => subject,
            body => body,
            body_html => Value::from_safe_string(body_html),
            footer_html => Value::from_safe_string(footer_html),
            locale => self.locale.as_str(),
            ..ctx.clone()
        };
        for base in candidates {
            let name = format!("{}.html", base);
            match env.get_template(&name) {
                Ok(template) => {
                    return template.render(&ctx).map_err(|e| template_error(&name, e));
                }
                Err(e) if e.kind() == ErrorKind::TemplateNotFound => {}
                Err(e) => return Err(template_error(&name, e)),
            }
        }
        env.get_template(LAYOUT)
            .and_then(|template| template.render(&ctx))
            .map_err(|e| template_error(LAYOUT, e))
    }
}

/// テンプレートの文法を確かめる（保存の前）
pub fn check_syntax(name: &str, source: &str) -> Result<()> {
    let mut env = Environment::new();
    env.add_template_owned(name.to_string(), source.to_string())
        .map(|_| ())
        .map_err(|e| AegisError::Validation(format!("{:#}", e)))
}

fn template_error(name: &str, e: minijinja::Error) -> AegisError {
    AegisError::Internal(format!("Notification template '{}': {:#}", name, e))
}

/// 3 行以上続く空行を 1 行にまとめ、前後の空白を落とす（条件分岐で空いた行を詰める）
fn tidy(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut blank = 0;
    for line in body.trim().lines() {
        let line = line.trim_end();
        if line.is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn order(status: &str) -> serde_json::Value {
        json!({
            "site_name": "Aegis Coffee",
            "site_url": "https://shop.example.com",
            "order": {
                "number": "261023-ABC234",
                "status": status,
                "currency": "jpy",
                "lines": [
                    { "title": "Ceramic Mug", "quantity": 2, "unit_price": 1200, "line_total": 2400 }
                ],
                "adjustments": [{ "label": "Shipping", "amount": 600 }],
                "subtotal": 2400,
                "total": 3000,
                "customer_name": "Hanako",
                "customer_email": "hanako@example.com",
                "shipping_address": {
                    "name": "", "postal_code": "100-0001", "region": "Tokyo",
                    "city": "Chiyoda", "line1": "1-1 Marunouchi", "line2": "Apt 3"
                }
            },
            "order_url": "https://shop.example.com/orders/1?token=abc",
            "payment_instructions": "Bank transfer to 1234567",
        })
    }

    #[test]
    fn renders_builtin_order_placed_in_each_locale() {
        let customer = Recipient::person("hanako@example.com", "Hanako");
        let en = Templates::new("en", None)
            .render("order.placed", &customer, &order("pending_payment"))
            .unwrap()
            .unwrap();
        assert_eq!(
            en.subject,
            "Thank you for your order (261023-ABC234) - Aegis Coffee"
        );
        assert!(en.body.starts_with("Dear Hanako,"));
        assert!(en.body.contains("Bank transfer to 1234567"));
        assert!(en.body.contains("- Ceramic Mug x 2: ¥2,400"));
        assert!(en.body.contains("Total: ¥3,000"));
        assert!(en
            .body
            .contains("https://shop.example.com/orders/1?token=abc"));
        assert!(!en.body.contains("\n\n\n"));
        assert!(en.body.contains("\n1-1 Marunouchi Apt 3\n"), "{}", en.body);
        assert!(
            en.body
                .ends_with("--\nAegis Coffee\nhttps://shop.example.com\n"),
            "{}",
            en.body
        );

        let ja = Templates::new("ja", None)
            .render("order.placed", &customer, &order("paid"))
            .unwrap()
            .unwrap();
        assert!(ja.subject.contains("ご注文ありがとうございます"));
        assert!(ja.body.contains("お支払いを確認しました"));
        assert!(!ja.body.contains("お支払い方法"));
    }

    #[test]
    fn staff_get_their_own_template_and_fall_back_to_the_shared_one() {
        let templates = Templates::new("en", None);
        let staff = templates
            .render("order.placed", &Recipient::Staff, &order("pending_payment"))
            .unwrap()
            .unwrap();
        assert_eq!(staff.subject, "New order 261023-ABC234 (¥3,000)");
        assert!(staff.body.contains("hanako@example.com"));
        // order.paid に運営者向けは無いので顧客向けと同じ
        let paid = templates
            .render("order.paid", &Recipient::Staff, &order("paid"))
            .unwrap()
            .unwrap();
        assert!(paid.subject.starts_with("We have received your payment"));
    }

    #[test]
    fn unknown_kinds_render_nothing_and_unsafe_names_are_rejected() {
        let templates = Templates::new("en", None);
        let customer = Recipient::person("a@example.com", "");
        assert_eq!(
            templates
                .render("order.refunded", &customer, &json!({}))
                .unwrap(),
            None
        );
        assert!(templates
            .render("../secrets", &customer, &json!({}))
            .is_err());
    }

    #[test]
    fn files_in_templates_dir_override_the_builtin_ones() {
        let dir = std::env::temp_dir().join(format!("aegis-notify-tpl-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("order.shipped.subject"),
            "Shipped: {{ order.number }}",
        )
        .unwrap();
        std::fs::write(
            dir.join("order.shipped.txt"),
            "On its way!\n{% include \"_summary.txt\" %}",
        )
        .unwrap();
        let rendered = Templates::new("en", Some(&dir))
            .render(
                "order.shipped",
                &Recipient::person("a@example.com", ""),
                &order("shipped"),
            )
            .unwrap()
            .unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(rendered.subject, "Shipped: 261023-ABC234");
        // 上書きしていない部品（_summary.txt）は組み込みのもの
        assert!(rendered
            .body
            .starts_with("On its way!\nOrder number: 261023-ABC234"));
    }

    fn audit_event(
        outcome: &str,
        actor: serde_json::Value,
        details: serde_json::Value,
    ) -> serde_json::Value {
        json!({
            "site_name": "Aegis Coffee",
            "admin_url": "https://shop.example.com/admin/",
            "title": "",
            "audit": {
                "action": "session.create",
                "outcome": outcome,
                "occurred_at": "2026-10-23T05:00:00.000000Z",
                "actor": actor,
                "resource_type": "session",
                "resource_id": null,
                "ip": "203.0.113.7",
                "user_agent": "Mozilla/5.0",
                "details": details,
            }
        })
    }

    #[test]
    fn audit_events_use_the_generic_audit_template() {
        let ja = Templates::new("ja", None);
        let login = ja
            .render(
                "audit.session.create",
                &Recipient::Staff,
                &audit_event(
                    "success",
                    json!({ "user_id": "root", "role": "admin" }),
                    json!({ "provider": "local" }),
                ),
            )
            .unwrap()
            .unwrap();
        assert_eq!(login.subject, "【Aegis Coffee】ログイン（成功）：root");
        assert!(
            login.body.contains("ユーザー: root（admin）"),
            "{}",
            login.body
        );
        assert!(login.body.contains("IP アドレス: 203.0.113.7"));
        assert!(login.body.contains("  provider: local"));
        assert!(login
            .body
            .contains("監査ログ: https://shop.example.com/admin/"));

        let locked = ja
            .render(
                "audit.session.create",
                &Recipient::Staff,
                &audit_event(
                    "failure",
                    serde_json::Value::Null,
                    json!({ "username": "root", "error": "Invalid password", "locked": "user" }),
                ),
            )
            .unwrap()
            .unwrap();
        assert_eq!(
            locked.subject,
            "【Aegis Coffee】ログインをロックしました（root）"
        );
        assert!(locked.body.contains("ユーザー名: root"), "{}", locked.body);
        assert!(locked.body.contains("  locked: user"));

        // ルールの見出し（title）が優先
        let mut titled = audit_event(
            "success",
            json!({ "user_id": "root", "role": "admin" }),
            json!({}),
        );
        titled["title"] = json!("管理者がログインしました");
        let en = Templates::new("en", None)
            .render("audit.session.create", &Recipient::Staff, &titled)
            .unwrap()
            .unwrap();
        assert_eq!(en.subject, "[Aegis Coffee] 管理者がログインしました");
        assert!(
            en.body
                .starts_with("管理者がログインしました\n\nTime: 2026-10-23T05:00:00.000000Z (UTC)"),
            "{}",
            en.body
        );
    }

    #[test]
    fn renders_the_account_templates_in_each_locale() {
        let person = Recipient::person("hanako@example.com", "Hanako");
        let verify = json!({
            "site_name": "Aegis Coffee",
            "site_url": "https://shop.example.com",
            "username": "hanako",
            "display_name": null,
            "verify_url": "https://cms.example.com/admin/#verify-email=abc123",
            "expires_hours": 24,
        });
        for locale in LOCALES {
            let rendered = Templates::new(locale, None)
                .render("account.verify", &person, &verify)
                .unwrap()
                .unwrap();
            assert!(
                rendered.subject.contains("Aegis Coffee"),
                "{}",
                rendered.subject
            );
            assert!(
                rendered.body.starts_with("hanako") || rendered.body.starts_with("Hello hanako")
            );
            assert!(rendered
                .body
                .contains("\nhttps://cms.example.com/admin/#verify-email=abc123\n"));
            assert!(rendered.body.contains("24"));
            assert!(!rendered.body.contains("\n\n\n"), "{}", rendered.body);
        }

        let mut registered = json!({
            "site_name": "Aegis Coffee",
            "admin_url": "https://cms.example.com/admin/",
            "username": "hanako",
            "display_name": "Hanako",
            "email": "hanako@example.com",
            "role": "viewer",
            "pending_approval": true,
        });
        let en = Templates::new("en", None)
            .render("account.registered", &Recipient::Staff, &registered)
            .unwrap()
            .unwrap();
        assert_eq!(
            en.subject,
            "New account hanako awaiting approval - Aegis Coffee"
        );
        assert!(en
            .body
            .contains("Display name: Hanako\nEmail: hanako@example.com\n"));
        assert!(en.body.contains("enable it under Settings > Users"));
        assert!(en.body.contains("https://cms.example.com/admin/"));
        registered["pending_approval"] = json!(false);
        let ja = Templates::new("ja", None)
            .render("account.registered", &Recipient::Staff, &registered)
            .unwrap()
            .unwrap();
        assert_eq!(ja.subject, "【Aegis Coffee】新しいアカウント hanako");
        assert!(!ja.body.contains("無効の状態"));
        // Only staff get the "registered" notice
        assert!(Templates::new("en", None)
            .render("account.registered", &person, &registered)
            .unwrap()
            .is_none());
    }

    #[test]
    fn renders_the_content_templates_in_each_locale() {
        let author = Recipient::person("hanako@example.com", "Hanako");
        let mut data = json!({
            "site_name": "Aegis Coffee",
            "site_url": "https://shop.example.com",
            "username": "hanako",
            "display_name": "Hanako",
            "at": "2099-03-01T00:00:00Z",
            "content": { "label": "Articles", "title": "Spring Sale", "slug": "spring-sale" },
            "content_url": "https://shop.example.com/spring-sale",
        });
        for locale in LOCALES {
            for kind in ["content.published", "content.unpublished"] {
                let rendered = Templates::new(locale, None)
                    .render(kind, &author, &data)
                    .unwrap()
                    .unwrap();
                assert!(
                    rendered.subject.contains("Spring Sale"),
                    "{}",
                    rendered.subject
                );
                assert!(rendered.body.contains("Hanako"), "{}", rendered.body);
                assert!(rendered.body.contains("2099-03-01T00:00:00Z"));
                assert!(
                    !rendered.body.contains(
                        "


"
                    ),
                    "{}",
                    rendered.body
                );
            }
        }
        let en = Templates::new("en", None)
            .render("content.published", &author, &data)
            .unwrap()
            .unwrap();
        assert_eq!(en.subject, "Published: Spring Sale - Aegis Coffee");
        assert!(en.body.contains(
            "
https://shop.example.com/spring-sale
"
        ));

        // Without a public URL the link line is left out
        data["content_url"] = json!("");
        let ja = Templates::new("ja", None)
            .render("content.published", &author, &data)
            .unwrap()
            .unwrap();
        assert_eq!(ja.subject, "【Aegis Coffee】「Spring Sale」を公開しました");
        assert!(!ja.body.contains("spring-sale"), "{}", ja.body);
        assert!(
            !ja.body.contains(
                "


"
            ),
            "{}",
            ja.body
        );
    }

    #[test]
    fn renders_the_comment_templates_in_each_locale() {
        let author = Recipient::person("hanako@example.com", "Hanako");
        let no_blank_runs = |text: &str| {
            !text.contains(
                "


",
            )
        };
        let data = json!({
            "site_name": "Aegis Coffee",
            "site_url": "https://shop.example.com",
            "username": "hanako",
            "display_name": "Hanako",
            "comment": {
                "id": "c1",
                "author_name": "Taro",
                "author_email": "taro@example.com",
                "body": "Great beans!",
                "ip": "203.0.113.7",
                "is_reply": false,
            },
            "content": { "label": "Articles", "title": "Spring Sale", "slug": "spring-sale" },
            "content_url": "https://shop.example.com/spring-sale#comment-c1",
            "moderation_url": "https://shop.example.com/admin/#comments",
        });
        for locale in LOCALES {
            let templates = Templates::new(locale, None);
            for (kind, recipient) in [
                ("comment.posted", &author),
                ("comment.pending", &author),
                ("comment.pending", &Recipient::Staff),
            ] {
                let rendered = templates.render(kind, recipient, &data).unwrap().unwrap();
                assert!(
                    rendered.subject.contains("Spring Sale"),
                    "{}",
                    rendered.subject
                );
                assert!(rendered.body.contains("Great beans!"), "{}", rendered.body);
                assert!(rendered.body.contains("Taro"), "{}", rendered.body);
                assert!(no_blank_runs(&rendered.body), "{}", rendered.body);
            }
            let staff = templates
                .render("comment.pending", &Recipient::Staff, &data)
                .unwrap()
                .unwrap();
            assert!(staff.body.contains("taro@example.com"));
            assert!(staff.body.contains("203.0.113.7"));
            assert!(staff
                .body
                .contains("https://shop.example.com/admin/#comments"));
            assert!(!staff.body.contains("Hanako"), "{}", staff.body);
        }

        let en = Templates::new("en", None)
            .render("comment.posted", &author, &data)
            .unwrap()
            .unwrap();
        assert_eq!(en.subject, "New comment on \"Spring Sale\" - Aegis Coffee");
        assert!(en
            .body
            .contains("Taro commented on your Articles \"Spring Sale\"."));

        let replied = json!({
            "site_name": "Aegis Coffee",
            "site_url": "https://shop.example.com",
            "name": "Hanako",
            "comment": { "body": "Which roast?" },
            "reply": { "author_name": "Shop", "body": "Medium.", "staff": true },
            "content": { "title": "Spring Sale" },
            "content_url": "https://shop.example.com/spring-sale#comment-c2",
            "unsubscribe_url": "https://shop.example.com/comments/unsubscribe?id=c1&token=t",
        });
        for locale in LOCALES {
            let rendered = Templates::new(locale, None)
                .render("comment.replied", &author, &replied)
                .unwrap()
                .unwrap();
            assert!(rendered.body.contains("Medium."));
            assert!(rendered.body.contains("Which roast?"));
            assert!(rendered.body.contains("token=t"));
            assert!(no_blank_runs(&rendered.body), "{}", rendered.body);
        }
        let ja = Templates::new("ja", None)
            .render("comment.replied", &author, &replied)
            .unwrap()
            .unwrap();
        assert_eq!(
            ja.subject,
            "【Aegis Coffee】「Spring Sale」のあなたのコメントに返信がありました"
        );
        assert!(
            ja.body.contains("Shop（Aegis Coffee） さんが返信しました"),
            "{}",
            ja.body
        );
    }

    #[test]
    fn renders_the_comment_approved_template_in_each_locale() {
        let author = Recipient::person("hanako@example.com", "Hanako");
        let approved = json!({
            "site_name": "Aegis Coffee",
            "site_url": "https://shop.example.com",
            "name": "Hanako",
            "comment": { "body": "Which roast?" },
            "content": { "title": "Spring Sale" },
            "content_url": "https://shop.example.com/spring-sale#comment-c1",
            "unsubscribe_url": "https://shop.example.com/comments/unsubscribe?id=c1&token=t",
        });
        for locale in LOCALES {
            let rendered = Templates::new(locale, None)
                .render("comment.approved", &author, &approved)
                .unwrap()
                .unwrap();
            assert!(rendered.subject.contains("Spring Sale"));
            assert!(rendered.body.contains("Which roast?"));
            assert!(rendered.body.contains("#comment-c1"));
            assert!(rendered.body.contains("token=t"));
            assert!(!rendered.body.contains("\n\n\n"), "{}", rendered.body);
        }
        let ja = Templates::new("ja", None)
            .render("comment.approved", &author, &approved)
            .unwrap()
            .unwrap();
        assert_eq!(
            ja.subject,
            "【Aegis Coffee】「Spring Sale」へのあなたのコメントが承認されました"
        );
    }

    #[test]
    fn html_uses_the_layout_and_escapes_the_values() {
        let author = Recipient::person("a@example.com", "");
        let context = json!({
            "site_name": "Aegis <Coffee>",
            "site_url": "https://shop.example.com",
            "name": "Taro",
            "comment": { "body": "<script>alert(1)</script>" },
            "reply": { "author_name": "Shop", "body": "See https://shop.example.com/a?x=1&y=2", "staff": false },
            "content": { "title": "Spring" },
            "content_url": "",
            "unsubscribe_url": "",
        });
        let plain = Templates::new("en", None)
            .render("comment.replied", &author, &context)
            .unwrap()
            .unwrap();
        assert_eq!(plain.html, None);

        let rendered = Templates::new("en", None)
            .with_html(true)
            .render("comment.replied", &author, &context)
            .unwrap()
            .unwrap();
        let html = rendered.html.unwrap();
        assert!(html.starts_with("<!DOCTYPE html>"), "{}", html);
        assert!(html.contains("<html lang=\"en\">"));
        assert!(rendered.subject.contains("Aegis <Coffee>"));
        assert!(html.contains("Aegis &lt;Coffee&gt;</title>"), "{}", html);
        // 本文の値もサイト名もエスケープされる
        assert!(
            html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"),
            "{}",
            html
        );
        assert!(!html.contains("<script>"));
        assert!(html.contains(">Aegis &lt;Coffee&gt;</a>"), "{}", html);
        // URL はリンクになり、--- から後はフッター
        assert!(html.contains("<a href=\"https://shop.example.com/a?x=1&amp;y=2\""));
        assert!(html.contains("color:#71717a;font-size:12px"));
        // プレーンテキストの本文は変わらない
        assert_eq!(rendered.body, plain.body);
    }

    #[test]
    fn kind_html_templates_override_the_layout_only_for_their_own_recipients() {
        let templates = Templates::new("en", None).with_html(true);
        templates.set_overrides(HashMap::from([(
            "order.placed.html".to_string(),
            "{% extends \"_layout.html\" %}{% block content %}<h1>{{ order.number }}</h1>{{ body_html }}{% endblock %}"
                .to_string(),
        )]));
        let customer = templates
            .render(
                "order.placed",
                &Recipient::person("hanako@example.com", "Hanako"),
                &order("paid"),
            )
            .unwrap()
            .unwrap()
            .html
            .unwrap();
        assert!(customer.contains("<h1>261023-ABC234</h1>"), "{}", customer);
        assert!(customer.contains("<p style="));
        // 運営者向けは本文が別（order.placed.staff.txt）なので、顧客向けの HTML を使わない
        let staff = templates
            .render("order.placed", &Recipient::Staff, &order("paid"))
            .unwrap()
            .unwrap()
            .html
            .unwrap();
        assert!(!staff.contains("<h1>"), "{}", staff);
    }

    #[test]
    fn overrides_take_precedence_and_can_be_replaced() {
        let dir = std::env::temp_dir().join(format!("aegis-notify-ovr-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("order.shipped.subject"),
            "File: {{ order.number }}",
        )
        .unwrap();
        std::fs::write(dir.join("custom.note.txt"), "x").unwrap();
        let templates = Templates::new("en", Some(&dir));
        let customer = Recipient::person("a@example.com", "");
        let subject = |t: &Templates| {
            t.render("order.shipped", &customer, &order("shipped"))
                .unwrap()
                .unwrap()
                .subject
        };
        assert_eq!(subject(&templates), "File: 261023-ABC234");

        templates.set_overrides(HashMap::from([(
            "order.shipped.subject".to_string(),
            "Custom: {{ order.number }}".to_string(),
        )]));
        // 読み込み済みのものも差し替わる
        assert_eq!(subject(&templates), "Custom: 261023-ABC234");
        assert_eq!(
            templates
                .source("order.shipped.subject")
                .unwrap()
                .unwrap()
                .0,
            TemplateSource::Custom
        );
        assert_eq!(
            templates.default_source("order.shipped.subject").unwrap(),
            Some((TemplateSource::File, "File: {{ order.number }}".to_string()))
        );

        // 保存前のプレビューは元のものを変えない
        let preview = templates.preview(HashMap::from([(
            "order.shipped.subject".to_string(),
            "Preview".to_string(),
        )]));
        assert_eq!(subject(&preview), "Preview");
        assert_eq!(subject(&templates), "Custom: 261023-ABC234");

        let list = templates.list();
        let source_of = |name: &str| list.iter().find(|t| t.name == name).map(|t| t.source);
        assert_eq!(
            source_of("order.shipped.subject"),
            Some(TemplateSource::Custom)
        );
        assert_eq!(
            source_of("order.shipped.txt"),
            Some(TemplateSource::Builtin)
        );
        assert_eq!(source_of("custom.note.txt"), Some(TemplateSource::File));
        assert_eq!(source_of(LAYOUT), Some(TemplateSource::Builtin));

        templates.set_overrides(HashMap::new());
        assert_eq!(subject(&templates), "File: 261023-ABC234");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn checks_names_and_syntax() {
        for name in [
            "order.placed.subject",
            "_footer.txt",
            "_layout.html",
            "audit.session.create.html",
        ] {
            assert!(is_editable_name(name), "{}", name);
        }
        for name in [
            "../x.txt",
            ".txt",
            "order.placed",
            "a/b.txt",
            "x.exe",
            ".hidden.txt",
        ] {
            assert!(!is_editable_name(name), "{}", name);
        }
        check_syntax("a.txt", "Hello {{ name }}{% if x %}!{% endif %}").unwrap();
        let error = check_syntax("a.txt", "Hello {% if x %}")
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("syntax error") || error.contains("unexpected"),
            "{}",
            error
        );
    }
}
