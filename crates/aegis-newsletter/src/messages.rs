//! 組み込みの文言（`[newsletter].locale`）: 申し込みのページ・確認と解除のページ・フォームの項目

/// 選べる言語
pub const LOCALES: &[&str] = &["en", "ja"];

/// 決まった文言。知らない `key` は空
pub fn text(locale: &str, key: &str) -> &'static str {
    let ja = locale == "ja";
    match key {
        "title" if ja => "ニュースレター",
        "title" => "Newsletter",
        "description" if ja => {
            "新しい記事のお知らせをメールでお届けします。申し込むと確認のメールが届きます。メールのリンクを開くと購読が始まります。"
        }
        "description" => {
            "Get new posts by email. After you sign up we send a confirmation email; open the link in it to start the subscription."
        }
        "email" if ja => "メールアドレス",
        "email" => "Email",
        "submit" if ja => "購読する",
        "submit" => "Subscribe",
        "sent" if ja => {
            "確認のメールを送りました。メールのリンクを開くと購読が始まります。届かないときは、迷惑メールのフォルダーもご確認ください。"
        }
        "sent" => {
            "We sent a confirmation email. Open the link in it to start the subscription. If it does not arrive, check your spam folder."
        }
        "invalid_email" if ja => "正しいメールアドレスを入力してください。",
        "invalid_email" => "Please enter a valid email address.",
        "rate_limited" if ja => "申し込みが多すぎます。しばらくしてからもう一度お試しください。",
        "rate_limited" => "Too many sign-ups from this address. Try again later.",
        "failed" if ja => "申し込めませんでした。しばらくしてからもう一度お試しください。",
        "failed" => "We could not sign you up. Please try again later.",
        "captcha_failed" if ja => "確認に失敗しました。もう一度お試しください。",
        "captcha_failed" => "The verification failed. Please try again.",
        "captcha_unavailable" if ja => {
            "確認を行えませんでした。しばらくしてからもう一度お試しください。"
        }
        "captcha_unavailable" => "The verification is unavailable. Please try again later.",
        "confirm_title" if ja => "購読の確認",
        "confirm_title" => "Confirm your subscription",
        "confirm_lead" if ja => "次のボタンを押すと、購読が始まります。",
        "confirm_lead" => "Press the button to start your subscription.",
        "confirm_button" if ja => "購読を確定する",
        "confirm_button" => "Confirm subscription",
        "confirmed_title" if ja => "購読を開始しました",
        "confirmed_title" => "You are subscribed",
        "confirmed" if ja => "ご登録ありがとうございます。新しいお知らせをメールでお届けします。",
        "confirmed" => "Thank you. You will get new posts by email.",
        "invalid_link" if ja => {
            "このリンクは無効か、有効期限が切れています。もう一度、申し込みからお試しください。"
        }
        "invalid_link" => "This link is invalid or has expired. Please sign up again.",
        "unsubscribe_title" if ja => "配信の停止",
        "unsubscribe_title" => "Unsubscribe",
        "unsubscribe_lead" if ja => "次のボタンを押すと、ニュースレターの配信を停止します。",
        "unsubscribe_lead" => "Press the button to stop the newsletter.",
        "unsubscribe_button" if ja => "配信を停止する",
        "unsubscribe_button" => "Unsubscribe",
        "unsubscribed_title" if ja => "配信を停止しました",
        "unsubscribed_title" => "You are unsubscribed",
        "unsubscribed" if ja => "ニュースレターの配信を停止しました。またのご利用をお待ちしています。",
        "unsubscribed" => "You will no longer get the newsletter. You are welcome back any time.",
        _ => "",
    }
}
