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
        "upload_too_large" if ja => "添付ファイルが大きすぎます。小さいファイルを選んでください。",
        "upload_too_large" => "The attached files are too large. Please choose smaller files.",
        "captcha_failed" if ja => "確認に失敗しました。もう一度お試しください。",
        "captcha_failed" => "The verification failed. Please try again.",
        "captcha_unavailable" if ja => {
            "確認を行えませんでした。しばらくしてからもう一度お試しください。"
        }
        "captcha_unavailable" => "The verification is unavailable. Please try again later.",
        _ => "",
    }
}

/// 添付ファイルの誤りの文言。`code` は `required` / `file_too_large`（`detail` は上限の MB）/
/// `invalid_file_type`（`detail` は選べる拡張子）
pub fn file_error(locale: &str, code: &str, label: &str, detail: &str) -> String {
    if locale == "ja" {
        match code {
            "required" => format!("「{}」のファイルを選んでください", label),
            "file_too_large" => {
                format!("「{}」のファイルは {} MB 以下にしてください", label, detail)
            }
            _ => format!("「{}」には {} のファイルだけ添付できます", label, detail),
        }
    } else {
        match code {
            "required" => format!("Please attach a file to \"{}\"", label),
            "file_too_large" => format!(
                "The file for \"{}\" must be {} MB or smaller",
                label, detail
            ),
            _ => format!("\"{}\" accepts these file types only: {}", label, detail),
        }
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
