use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::{extract::State, http::StatusCode, routing::post, Form, Router};

use super::*;

type Seen = Arc<Mutex<Vec<HashMap<String, String>>>>;

/// 手元で動く siteverify。トークン `good` だけ通し、`broken` は 500、`garbage` は JSON でない応答を返す
async fn mock_siteverify() -> (String, Seen) {
    async fn handler(
        State(seen): State<Seen>,
        Form(form): Form<HashMap<String, String>>,
    ) -> (StatusCode, String) {
        let token = form.get("response").cloned().unwrap_or_default();
        seen.lock().unwrap().push(form);
        match token.as_str() {
            "good" => (StatusCode::OK, r#"{"success": true}"#.to_string()),
            "broken" => (StatusCode::INTERNAL_SERVER_ERROR, "oops".to_string()),
            "garbage" => (StatusCode::OK, "<html>".to_string()),
            _ => (
                StatusCode::OK,
                r#"{"success": false, "error-codes": ["invalid-input-response"]}"#.to_string(),
            ),
        }
    }
    let seen: Seen = Arc::default();
    let app = Router::new()
        .route("/siteverify", post(handler))
        .with_state(seen.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/siteverify", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, seen)
}

fn config(provider: Provider) -> CaptchaConfig {
    CaptchaConfig {
        provider,
        site_key: "site-key".to_string(),
        secret: "secret-key".to_string(),
        fail_open: false,
        timeout_secs: 5,
        verify_url: String::new(),
        script_url: String::new(),
        widget_class: String::new(),
        response_field: String::new(),
    }
}

fn captcha_for(url: &str, fail_open: bool) -> Captcha {
    Captcha::new(
        CaptchaConfig {
            verify_url: url.to_string(),
            fail_open,
            ..config(Provider::Turnstile)
        },
        "[test]",
    )
    .unwrap()
}

#[test]
fn test_presets_validate_and_name_the_response_field() {
    for (provider, field) in [
        (Provider::Turnstile, "cf-turnstile-response"),
        (Provider::Hcaptcha, "h-captcha-response"),
        (Provider::Recaptcha, "g-recaptcha-response"),
    ] {
        let config = config(provider);
        config.validate("[test]").unwrap();
        assert_eq!(config.response_field(), field);
    }
}

#[test]
fn test_validate_rejects_bad_settings() {
    let invalid = |mutate: fn(&mut CaptchaConfig)| {
        let mut c = config(Provider::Turnstile);
        mutate(&mut c);
        c.validate("[test]").unwrap_err().to_string()
    };
    assert!(invalid(|c| c.site_key = " ".into()).contains("site_key"));
    assert!(invalid(|c| c.secret = String::new()).contains("secret"));
    assert!(invalid(|c| c.secret = "env:AEGIS_CAPTCHA_TEST_UNSET".into()).contains("secret"));
    assert!(invalid(|c| c.timeout_secs = 0).contains("timeout_secs"));
    assert!(invalid(|c| c.timeout_secs = 31).contains("timeout_secs"));
    // 秘密を送る先は https（手元のホストを除く）
    assert!(invalid(|c| c.verify_url = "http://example.com/verify".into()).contains("verify_url"));
    assert!(invalid(|c| c.verify_url = "https://user@example.com/v".into()).contains("verify_url"));
    assert!(invalid(|c| c.script_url = "http://127.0.0.1/api.js".into()).contains("script_url"));
    assert!(invalid(|c| c.widget_class = "a\"b".into()).contains("widget_class"));
    // custom は 4 つとも必要
    let mut custom = config(Provider::Custom);
    assert!(custom
        .validate("[test]")
        .unwrap_err()
        .to_string()
        .contains("verify_url"));
    custom.verify_url = "https://captcha.example.com/verify".into();
    custom.script_url = "https://captcha.example.com/api.js".into();
    custom.widget_class = "my-captcha".into();
    assert!(custom
        .validate("[test]")
        .unwrap_err()
        .to_string()
        .contains("response_field"));
    custom.response_field = "my-captcha-response".into();
    custom.validate("[test]").unwrap();
    // 手元のホストへの http は認める
    let mut local = config(Provider::Turnstile);
    local.verify_url = "http://localhost:9000/siteverify".into();
    local.validate("[test]").unwrap();
}

#[test]
fn test_secret_can_come_from_the_environment() {
    std::env::set_var("AEGIS_CAPTCHA_TEST_SECRET", " from-env ");
    let mut c = config(Provider::Turnstile);
    c.secret = "env:AEGIS_CAPTCHA_TEST_SECRET".into();
    let captcha = Captcha::new(c, "[test]").unwrap();
    assert_eq!(captcha.secret, "from-env");
}

#[test]
fn test_widget_html_and_csp_hosts() {
    let captcha = Captcha::new(config(Provider::Turnstile), "[test]").unwrap();
    assert_eq!(
        captcha.widget_html(),
        "<script src=\"https://challenges.cloudflare.com/turnstile/v0/api.js\" async defer></script>\n\
         <div class=\"cf-turnstile\" data-sitekey=\"site-key\"></div>"
    );
    assert_eq!(captcha.csp_hosts(), ["challenges.cloudflare.com"]);

    // サイトキーはエスケープする
    let mut c = config(Provider::Hcaptcha);
    c.site_key = "a\"><script>".into();
    let html = Captcha::new(c, "[test]").unwrap().widget_html();
    assert!(html.contains("data-sitekey=\"a&quot;&gt;&lt;script&gt;\""));
    assert!(html.contains("class=\"h-captcha\""));

    let mut custom = config(Provider::Custom);
    custom.verify_url = "https://captcha.example.com/verify".into();
    custom.script_url = "https://cdn.example.com/api.js".into();
    custom.widget_class = "my-captcha".into();
    custom.response_field = "my-response".into();
    assert_eq!(
        Captcha::new(custom, "[test]").unwrap().csp_hosts(),
        ["cdn.example.com"]
    );
}

#[tokio::test]
async fn test_verify_passes_and_rejects() {
    let (url, seen) = mock_siteverify().await;
    let captcha = captcha_for(&url, false);

    captcha
        .verify(Some("good"), Some("203.0.113.9"))
        .await
        .unwrap();
    {
        let seen = seen.lock().unwrap();
        assert_eq!(seen[0]["secret"], "secret-key");
        assert_eq!(seen[0]["response"], "good");
        assert_eq!(seen[0]["remoteip"], "203.0.113.9");
    }
    // IP が分からないときは送らない
    captcha.verify(Some("good"), None).await.unwrap();
    assert!(!seen.lock().unwrap()[1].contains_key("remoteip"));

    assert_eq!(
        captcha.verify(Some("bad"), None).await,
        Err(CaptchaError::Failed)
    );
    // トークンが無い・空・長すぎるときは検証サービスに聞かない
    let before = seen.lock().unwrap().len();
    for token in [None, Some(""), Some("  ")] {
        assert_eq!(captcha.verify(token, None).await, Err(CaptchaError::Failed));
    }
    let long = "x".repeat(MAX_TOKEN_LEN + 1);
    assert_eq!(
        captcha.verify(Some(&long), None).await,
        Err(CaptchaError::Failed)
    );
    assert_eq!(seen.lock().unwrap().len(), before);
}

#[tokio::test]
async fn test_unavailable_service_fails_closed_by_default() {
    let (url, _) = mock_siteverify().await;
    let captcha = captcha_for(&url, false);
    // 500・JSON でない応答
    for token in ["broken", "garbage"] {
        assert_eq!(
            captcha.verify(Some(token), None).await,
            Err(CaptchaError::Unavailable)
        );
    }
    // つながらない
    let closed = {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        format!("http://{}/siteverify", listener.local_addr().unwrap())
    };
    assert_eq!(
        captcha_for(&closed, false).verify(Some("good"), None).await,
        Err(CaptchaError::Unavailable)
    );
}

#[tokio::test]
async fn test_fail_open_lets_unavailable_through_but_not_a_wrong_token() {
    let (url, _) = mock_siteverify().await;
    let captcha = captcha_for(&url, true);
    captcha.verify(Some("broken"), None).await.unwrap();
    captcha.verify(Some("garbage"), None).await.unwrap();
    // 検証サービスが「違う」と答えたものは、fail_open でも通さない
    assert_eq!(
        captcha.verify(Some("bad"), None).await,
        Err(CaptchaError::Failed)
    );
    assert_eq!(captcha.verify(None, None).await, Err(CaptchaError::Failed));
}
