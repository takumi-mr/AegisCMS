//! 通知の本文のテンプレート（MiniJinja、プレーンテキスト）。
//!
//! 通知の種類ごとに `<種類>.subject`（件名）と `<種類>.txt`（本文）を使う。運営者向け
//! （`Recipient::Staff`）は `<種類>.staff.subject` / `<種類>.staff.txt` を先に探す。
//! 無ければ種類の最後の `.` 以降を外した名前を順に探す（`audit.session.create` → `audit.session` → `audit`）。
//! `[notifications].templates_dir` にあるファイルを優先し、無ければ組み込み（`locale` の言語）を使う

use std::path::{Path, PathBuf};

use aegis_core::{AegisError, Recipient, Result};
use minijinja::{AutoEscape, Environment, ErrorKind, Value};

/// 組み込みテンプレートの言語
pub const LOCALES: &[&str] = &["en", "ja"];

macro_rules! builtin_templates {
    ($($name:literal),* $(,)?) => {
        fn builtin(locale: &str, name: &str) -> Option<&'static str> {
            match (locale, name) {
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
    "test.subject",
    "test.txt",
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

pub struct Templates {
    env: Environment<'static>,
}

/// 作った件名と本文
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    pub subject: String,
    pub body: String,
}

impl Templates {
    pub fn new(locale: &str, templates_dir: Option<&Path>) -> Self {
        let mut env = Environment::new();
        env.set_auto_escape_callback(|_| AutoEscape::None);
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
        let locale = if LOCALES.contains(&locale) {
            locale.to_string()
        } else {
            "en".to_string()
        };
        let dir: Option<PathBuf> = templates_dir.map(Path::to_path_buf);
        env.set_loader(move |name| {
            if !safe_name(name) {
                return Ok(None);
            }
            if let Some(dir) = &dir {
                match std::fs::read_to_string(dir.join(name)) {
                    Ok(source) => return Ok(Some(source)),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => {
                        return Err(minijinja::Error::new(
                            ErrorKind::InvalidOperation,
                            format!("Failed to read notification template {}: {}", name, e),
                        ))
                    }
                }
            }
            Ok(builtin(&locale, name).map(str::to_string))
        });
        Self { env }
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
        for base in candidates {
            let subject = match self.env.get_template(&format!("{}.subject", base)) {
                Ok(template) => template,
                Err(e) if e.kind() == ErrorKind::TemplateNotFound => continue,
                Err(e) => return Err(template_error(&base, e)),
            };
            let body = self
                .env
                .get_template(&format!("{}.txt", base))
                .map_err(|e| template_error(&base, e))?;
            let subject = subject.render(&ctx).map_err(|e| template_error(&base, e))?;
            let body = body.render(&ctx).map_err(|e| template_error(&base, e))?;
            return Ok(Some(Rendered {
                // 件名は 1 行
                subject: subject.split_whitespace().collect::<Vec<_>>().join(" "),
                body: tidy(&body),
            }));
        }
        Ok(None)
    }
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
}
