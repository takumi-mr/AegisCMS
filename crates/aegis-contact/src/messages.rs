//! 組み込みの文言（`[contact].locale`）: 入力の誤り、送信できなかった理由、既定のフォームの見出しなど

/// 選べる言語
pub const LOCALES: &[&str] = &["en", "ja"];

/// 決まった文言。知らない `key` は空
pub fn text(locale: &str, key: &str) -> &'static str {
    let ja = locale == "ja";
    match key {
        "title" if ja => "お問い合わせ",
        "title" => "Contact",
        "submit" if ja => "送信する",
        "submit" => "Send",
        "success" if ja => "お問い合わせありがとうございます。内容を確認してご連絡します。",
        "success" => "Thank you. We have received your message.",
        "check_fields" if ja => "入力内容を確認してください。",
        "check_fields" => "Please check the fields below.",
        "rate_limited" if ja => "送信が多すぎます。しばらくしてからもう一度お試しください。",
        "rate_limited" => "Too many messages from this address. Try again later.",
        "failed" if ja => "送信できませんでした。しばらくしてからもう一度お試しください。",
        "failed" => "Your message could not be sent. Please try again later.",
        _ => "",
    }
}

/// 項目の誤りの文言。`code` は `required` / `too_long` / `invalid_email` / `invalid_tel` /
/// `invalid_option` / `invalid_text`、`checkbox` はチェックボックスの必須
pub fn field_error(locale: &str, code: &str, label: &str, max_chars: usize) -> String {
    if locale == "ja" {
        match code {
            "required_checkbox" => format!("「{}」にチェックを入れてください", label),
            "required" => format!("「{}」を入力してください", label),
            "too_long" => format!("「{}」は {} 文字以内で入力してください", label, max_chars),
            "invalid_email" => format!("「{}」に正しいメールアドレスを入力してください", label),
            "invalid_tel" => format!("「{}」に電話番号を入力してください", label),
            "invalid_option" => format!("「{}」を一覧から選んでください", label),
            _ => format!("「{}」に使えない文字が含まれています", label),
        }
    } else {
        match code {
            "required_checkbox" => format!("Please check \"{}\"", label),
            "required" => format!("\"{}\" is required", label),
            "too_long" => format!(
                "\"{}\" is too long ({} characters at most)",
                label, max_chars
            ),
            "invalid_email" => format!("\"{}\" must be a valid email address", label),
            "invalid_tel" => format!("\"{}\" must be a phone number", label),
            "invalid_option" => format!("Please choose \"{}\" from the list", label),
            _ => format!("\"{}\" must not contain control characters", label),
        }
    }
}
