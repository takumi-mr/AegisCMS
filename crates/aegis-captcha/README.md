# aegis-captcha

フォーム送信時の CAPTCHA 検証（Cloudflare Turnstile, hCaptcha, Google reCAPTCHA, カスタム siteverify 互換）クレート。

## 概要

`aegis-captcha` は、お問い合わせフォーム、コメント投稿、ユーザー登録、ニュースレター登録等の公開フォームをボットやスパムから保護するための CAPTCHA 検証機能を提供します。

主要な 3 大サービス（Turnstile / hCaptcha / reCAPTCHA）はすべて同様のトークン検証プロトコル（ブラウザ上のウィジェット生成と、サーバーから `siteverify` URL へのシークレット付き POST）を採用しているため、設定（`provider`）の切り替えだけでプロバイダを変更できます。

## 主な責務・提供機能

- **マルチプロバイダ対応**:
  - `cloudflare` / `turnstile`: Cloudflare Turnstile。
  - `hcaptcha`: hCaptcha。
  - `recaptcha`: Google reCAPTCHA v2 / v3。
  - `custom`: 任意の `siteverify` 互換エンドポイント。
- **検証とセキュリティポリシー**:
  - クライアントから送信されたレスポンストークンのサーバーサイド検証。
  - **fail-closed** (既定): 外部 CAPTCHA API の障害時やタイムアウト時は安全のため拒否。
  - **fail-open**: `fail_open = true` で一時的な障害時に入力を通過させるフォールバック運用も可能。
- **テンプレート・ヘッダー支援**:
  - 各プロバイダに応じたクライアント側 `<script>` タグおよびウィジェット HTML の生成。
  - プロバイダに必要なドメイン（`challenges.cloudflare.com`, `hcaptcha.com`, `google.com` 等）を Content-Security-Policy (CSP) に自動付与するためのホストリスト提供。

## 使用例

```rust
use aegis_captcha::{Captcha, CaptchaConfig, Provider};

// 1. 設定の初期化
let config = CaptchaConfig {
    provider: Provider::Turnstile,
    site_key: "0x4AAAAAA...".into(),
    secret_key: "0x4AAAAAA...".into(),
    ..Default::default()
};
let captcha = Captcha::new(config);

// 2. テンプレート用ウィジェット HTML の取得
let widget_html = captcha.widget_html();

// 3. フォーム送信トークンの検証
let token = "user-submitted-captcha-response";
let client_ip = Some("203.0.113.1");
let result = captcha.verify(token, client_ip).await;
assert!(result.is_ok());
```
