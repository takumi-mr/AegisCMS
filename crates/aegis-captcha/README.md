# aegis-captcha

**English** | [日本語](README.ja.md)

A crate for CAPTCHA verification on form submissions (Cloudflare Turnstile, hCaptcha, Google reCAPTCHA, and custom siteverify-compatible services).

## Overview

`aegis-captcha` provides CAPTCHA verification to protect public forms such as contact forms, comment posting, user registration, and newsletter sign-up from bots and spam.

The three major services (Turnstile / hCaptcha / reCAPTCHA) all use the same token verification protocol (a widget generated in the browser, and a POST with a secret from the server to the `siteverify` URL), so you can change the provider just by switching the setting (`provider`).

## Responsibilities and Features

- **Multi-provider support**:
    - `cloudflare` / `turnstile`: Cloudflare Turnstile.
    - `hcaptcha`: hCaptcha.
    - `recaptcha`: Google reCAPTCHA v2 / v3.
    - `custom`: any `siteverify`-compatible endpoint.
- **Verification and security policy**:
    - Server-side verification of the response token submitted by the client.
    - **fail-closed** (default): for safety, requests are rejected when the external CAPTCHA API fails or times out.
    - **fail-open**: with `fail_open = true`, you can run a fallback mode that lets input through during a temporary outage.
- **Template and header support**:
    - Generates the client-side `<script>` tag and widget HTML for each provider.
    - Provides a host list so that the domains each provider needs (`challenges.cloudflare.com`, `hcaptcha.com`, `google.com`, etc.) are added automatically to the Content-Security-Policy (CSP).

## Usage Example

```rust
use aegis_captcha::{Captcha, CaptchaConfig, Provider};

// 1. Initialize the config
let config = CaptchaConfig {
    provider: Provider::Turnstile,
    site_key: "0x4AAAAAA...".into(),
    secret_key: "0x4AAAAAA...".into(),
    ..Default::default()
};
let captcha = Captcha::new(config);

// 2. Get the widget HTML for the template
let widget_html = captcha.widget_html();

// 3. Verify the form submission token
let token = "user-submitted-captcha-response";
let client_ip = Some("203.0.113.1");
let result = captcha.verify(token, client_ip).await;
assert!(result.is_ok());
```
