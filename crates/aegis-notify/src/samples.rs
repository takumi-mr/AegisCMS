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
    "stock.low",
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
    "comment.approved",
    "forum.posted",
    "forum.pending",
    "forum.reported",
    "contact.received",
    "contact.auto_reply",
    "newsletter.confirm",
    "newsletter.issue",
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
        "stock.low" => json!({
            "product": { "id": "sample-product", "title": if ja { "マグカップ" } else { "Ceramic Mug" }, "stock": 4 },
            "threshold": 5,
            "order": { "number": "261025-SAMPLE" }
        }),
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
        "newsletter.confirm" => json!({
            "confirm_url": url("/newsletter/confirm?token=sample"), "expires_hours": 48,
        }),
        "newsletter.issue" => json!({
            "title": if ja { "秋のブレンドが入荷しました" } else { "The autumn blend is here" },
            "summary": if ja {
                "今年のコーヒー豆が届きました。深いコクとやわらかな酸味が特長です。"
            } else {
                "This year's beans have arrived, with a deep body and a soft acidity."
            },
            "url": url("/articles/autumn-blend"),
            "unsubscribe_url": url("/newsletter/unsubscribe?token=sample"),
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
        "comment.approved" => json!({
            "name": comment["author_name"],
            "content": content,
            "comment": comment,
            "content_url": url("/autumn-blend#comment-sample"),
            "unsubscribe_url": url("/comments/unsubscribe?token=sample"),
        }),
        "forum.posted" | "forum.pending" => json!({
            "category": {
                "id": "sample-cat",
                "name": if ja { "雑談" } else { "General" },
                "slug": "general",
            },
            "thread": {
                "id": "sample-thread",
                "title": if ja { "おすすめの抽出方法を教えてください" } else { "Favorite brewing method?" },
            },
            "post": {
                "id": "sample-post",
                "author_name": if ja { "佐藤 太郎" } else { "Taro Sato" },
                "body": if ja { "ハンドドリップとフレンチプレス、どちらがお好みですか？" } else { "Do you prefer pour-over or French press?" },
                "ip": "203.0.113.7",
                "is_reply": false,
            },
            "thread_url": url("/forum/general/sample-thread"),
            "moderation_url": if admin.is_empty() { String::new() } else { format!("{}#forum", admin) },
        }),
        "forum.reported" => json!({
            "category": {
                "id": "sample-cat",
                "name": if ja { "雑談" } else { "General" },
                "slug": "general",
            },
            "thread": {
                "id": "sample-thread",
                "title": if ja { "おすすめの抽出方法を教えてください" } else { "Favorite brewing method?" },
            },
            "post": {
                "id": "sample-post",
                "author_name": if ja { "佐藤 太郎" } else { "Taro Sato" },
                "body": if ja { "宣伝目的の書き込みです。" } else { "Unsolicited promotional link." },
            },
            "report": {
                "id": "sample-report",
                "target_type": "post",
                "target_id": "sample-post",
                "reason": if ja { "スパム・宣伝" } else { "Spam / advertising" },
                "reporter_ip": "203.0.113.8",
            },
            "thread_url": url("/forum/general/sample-thread"),
            "moderation_url": if admin.is_empty() { String::new() } else { format!("{}#forum", admin) },
        }),
        "contact.received" | "contact.auto_reply" => json!({
            "form": {
                "name": "default",
                "title": if ja { "お問い合わせ" } else { "Contact" },
            },
            "message": {
                "id": "sample-message",
                "name": customer,
                "email": "hanako@example.com",
                "subject": if ja { "法人向けプランについて" } else { "About the business plan" },
                "fields": [
                    { "name": "name", "label": if ja { "お名前" } else { "Name" }, "value": customer },
                    { "name": "email", "label": if ja { "メールアドレス" } else { "Email" }, "value": "hanako@example.com" },
                    { "name": "subject", "label": if ja { "件名" } else { "Subject" },
                      "value": if ja { "法人向けプランについて" } else { "About the business plan" } },
                    { "name": "message", "label": if ja { "お問い合わせ内容" } else { "Message" },
                      "value": if ja { "10 名で使う場合の見積もりをお願いします。\n請求書払いはできますか？" } else { "Could you send a quote for 10 users?\nDo you accept invoices?" } },
                ],
                "created_at": "2026-10-25T01:23:45Z",
                "ip": "203.0.113.7",
            },
            "admin_url": if admin.is_empty() { String::new() } else { format!("{}#contact", admin) },
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
