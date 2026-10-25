//! テンプレートのプレビューとテスト送信に使う見本の値（Admin Studio）。実際の通知と同じ形の値を、
//! 架空の注文・コメント・ユーザーで作る

use serde_json::{json, Value};

/// 見本の値がある通知の種類（組み込みテンプレートのある種類。監査イベントは `audit.session.create` で代表する）
pub const SAMPLE_KINDS: &[&str] = &[
    "test",
    "order.placed",
    "order.paid",
    "order.shipped",
    "order.cancelled",
    "account.verify",
    "account.registered",
    "account.password_reset",
    "account.approved",
    "account.invite",
    "content.published",
    "content.unpublished",
    "comment.posted",
    "comment.pending",
    "comment.replied",
    "audit.session.create",
];

/// 見本の値（サイトの情報 `site_name` などは含まない。`NotificationService` が加える）。
/// 知らない種類は空のオブジェクト
pub fn sample_data(kind: &str, locale: &str, site_url: &str) -> Value {
    let ja = locale == "ja";
    let customer = if ja { "山田 花子" } else { "Hanako Yamada" };
    let url = |path: &str| {
        if site_url.is_empty() {
            String::new()
        } else {
            format!("{}{}", site_url.trim_end_matches('/'), path)
        }
    };
    let admin = url("/admin/");
    let order = |status: &str, note: &str| {
        json!({
            "number": "261025-SAMPLE",
            "status": status,
            "currency": if ja { "jpy" } else { "usd" },
            "created_at": "2026-10-25T01:23:45Z",
            "paid_at": if status == "pending_payment" { Value::Null } else { json!("2026-10-25T02:00:00Z") },
            "note": note,
            "lines": [
                { "title": if ja { "マグカップ" } else { "Ceramic Mug" }, "sku": "MUG-01", "quantity": 2,
                  "unit_price": if ja { 1200 } else { 1500 }, "line_total": if ja { 2400 } else { 3000 } },
                { "title": if ja { "コーヒー豆 200g" } else { "Coffee Beans 200g" }, "sku": "BEAN-200", "quantity": 1,
                  "unit_price": 1800, "line_total": 1800 }
            ],
            "adjustments": [{ "label": if ja { "送料" } else { "Shipping" }, "amount": if ja { 600 } else { 500 }, "kind": "shipping" }],
            "subtotal": if ja { 4200 } else { 4800 },
            "total": if ja { 4800 } else { 5300 },
            "customer_name": customer,
            "customer_email": "hanako@example.com",
            "shipping_address": {
                "name": "",
                "postal_code": if ja { "100-0001" } else { "94105" },
                "region": if ja { "東京都" } else { "CA" },
                "city": if ja { "千代田区" } else { "San Francisco" },
                "line1": if ja { "丸の内 1-1" } else { "1 Market St" },
                "line2": if ja { "サンプルビル 3F" } else { "Suite 300" },
                "phone": "000-0000-0000"
            }
        })
    };
    let content = json!({
        "id": "sample-article",
        "content_type": "article",
        "label": if ja { "記事" } else { "Article" },
        "title": if ja { "秋の新作ブレンドのご紹介" } else { "Introducing our autumn blend" },
        "slug": "autumn-blend",
    });
    let comment = json!({
        "author_name": if ja { "佐藤 太郎" } else { "Taro Sato" },
        "author_email": "taro@example.com",
        "ip": "203.0.113.7",
        "body": if ja { "香りがとても良かったです。\n次の入荷はいつですか？" } else { "Lovely aroma.\nWhen will it be back in stock?" },
        "created_at": "2026-10-25T03:00:00Z",
        "is_reply": false,
    });
    match kind {
        "order.placed" => json!({
            "order": order("pending_payment", ""),
            "order_url": url("/orders/sample?token=sample"),
            "payment_instructions": if ja { "〇〇銀行 本店 普通 1234567 にお振り込みください。" } else { "Please transfer the total to account 1234567 at Example Bank." },
            "payment_due": "2026-11-01T00:00:00Z",
        }),
        "order.paid" => json!({ "order": order("paid", "") }),
        "order.shipped" => json!({
            "order": order("shipped", if ja { "ヤマト運輸 追跡番号 1234-5678-9012" } else { "UPS tracking number 1Z999AA10123456784" }),
        }),
        "order.cancelled" => {
            json!({ "order": order("cancelled", ""), "reason": "admin", "was_paid": true })
        }
        "account.verify" => json!({
            "username": "hanako", "display_name": customer,
            "verify_url": url("/admin/#verify=sample"), "expires_hours": 24,
        }),
        "account.registered" => json!({
            "username": "hanako", "display_name": customer, "email": "hanako@example.com",
            "role": "editor", "pending_approval": true,
        }),
        "account.password_reset" => json!({
            "username": "hanako", "display_name": customer,
            "reset_url": url("/admin/#reset=sample"), "expires_hours": 1,
        }),
        "account.approved" => json!({
            "username": "hanako", "display_name": customer, "login_url": admin,
        }),
        "account.invite" => json!({
            "email": "hanako@example.com", "display_name": customer, "role": "editor",
            "invite_url": url("/admin/#invite=sample"), "expires_days": 7,
        }),
        "content.published" | "content.unpublished" => json!({
            "content": content,
            "at": "2026-10-25T09:00:00Z",
            "content_url": if kind == "content.published" { json!(url("/autumn-blend")) } else { json!("") },
            "username": "hanako", "display_name": customer,
        }),
        "comment.posted" | "comment.pending" => json!({
            "content": content,
            "comment": comment,
            "content_url": url("/autumn-blend#comment-sample"),
            "moderation_url": if admin.is_empty() { String::new() } else { format!("{}#comments", admin) },
            "username": "hanako", "display_name": customer,
        }),
        "comment.replied" => json!({
            "name": comment["author_name"],
            "content": content,
            "comment": comment,
            "reply": {
                "author_name": customer,
                "body": if ja { "ありがとうございます！来週入荷予定です。" } else { "Thank you! It will be back next week." },
                "staff": true,
            },
            "content_url": url("/autumn-blend#comment-sample-reply"),
            "unsubscribe_url": url("/comments/unsubscribe?token=sample"),
        }),
        "audit.session.create" => json!({
            "title": "",
            "audit": {
                "action": "session.create",
                "outcome": "success",
                "occurred_at": "2026-10-25T05:00:00.000000Z",
                "actor": { "user_id": "root", "role": "admin" },
                "resource_type": "session",
                "resource_id": null,
                "ip": "203.0.113.7",
                "user_agent": "Mozilla/5.0",
                "details": { "provider": "local" },
            }
        }),
        _ => json!({}),
    }
}
