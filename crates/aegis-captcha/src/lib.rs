//! フォームの CAPTCHA 検証（任意）。Cloudflare Turnstile・hCaptcha・reCAPTCHA は、どれも同じ仕組みで動く:
//!
//! 1. ページにスクリプトとウィジェットを置くと、ウィジェットがフォームに隠し欄（トークン）を足す
//! 2. サーバーが `secret` とトークンを `siteverify` の URL に `POST` し、`{"success": true}` なら通す
//!
//! なので提供元ごとの実装は持たず、設定の既定値（[`Provider`]）だけを切り替える。`provider = "custom"` なら
//! 同じ形の別のサービスにも使える。設定が無ければ何も動かない（フォーム側は [`Captcha`] を作らない）
//!
//! 検証サービスに届かない・読めないときは、既定では断る（fail-closed）。`fail_open = true` で通す

use std::time::Duration;

use aegis_core::{AegisError, Result};
use serde::{Deserialize, Serialize};

/// トークンの長さの上限（超えるものは検証せずに断る。reCAPTCHA のトークンは 2 KB ほど）
const MAX_TOKEN_LEN: usize = 4096;
/// `timeout_secs` の上限
const MAX_TIMEOUT_SECS: u64 = 30;

/// 提供元。`siteverify` の URL・スクリプト・ウィジェットの既定値が決まる
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    /// Cloudflare Turnstile
    Turnstile,
    /// hCaptcha
    Hcaptcha,
    /// Google reCAPTCHA（v2 のチェックボックス）
    Recaptcha,
    /// 上の 3 つと同じ形の別のサービス（`verify_url`・`script_url`・`widget_class`・`response_field` がすべて必要）
    Custom,
}

/// 提供元ごとの既定値
struct Preset {
    verify_url: &'static str,
    script_url: &'static str,
    widget_class: &'static str,
    response_field: &'static str,
    /// ブラウザーが読み込む・つなぐ先（CSP に足すホスト）
    csp_hosts: &'static [&'static str],
}

impl Provider {
    fn preset(self) -> Preset {
        match self {
            Provider::Turnstile => Preset {
                verify_url: "https://challenges.cloudflare.com/turnstile/v0/siteverify",
                script_url: "https://challenges.cloudflare.com/turnstile/v0/api.js",
                widget_class: "cf-turnstile",
                response_field: "cf-turnstile-response",
                csp_hosts: &["challenges.cloudflare.com"],
            },
            Provider::Hcaptcha => Preset {
                verify_url: "https://api.hcaptcha.com/siteverify",
                script_url: "https://js.hcaptcha.com/1/api.js",
                widget_class: "h-captcha",
                response_field: "h-captcha-response",
                csp_hosts: &["hcaptcha.com", "*.hcaptcha.com"],
            },
            Provider::Recaptcha => Preset {
                verify_url: "https://www.google.com/recaptcha/api/siteverify",
                script_url: "https://www.google.com/recaptcha/api.js",
                widget_class: "g-recaptcha",
                response_field: "g-recaptcha-response",
                csp_hosts: &["www.google.com", "www.gstatic.com"],
            },
            Provider::Custom => Preset {
                verify_url: "",
                script_url: "",
                widget_class: "",
                response_field: "",
                csp_hosts: &[],
            },
        }
    }
}

/// `[contact.captcha]` など、フォームごとの CAPTCHA の設定
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptchaConfig {
    pub provider: Provider,
    /// サイトキー（公開してよい。ウィジェットの `data-sitekey`）
    #[serde(default)]
    pub site_key: String,
    /// シークレットキー。`"env:VAR_NAME"` で環境変数から読める（設定ファイルに書かないため）
    #[serde(default, skip_serializing)]
    pub secret: String,
    /// 検証サービスに届かないときに通す（既定は断る）
    #[serde(default)]
    pub fail_open: bool,
    /// 検証サービスの応答を待つ秒数
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    /// `siteverify` の URL（空なら提供元の既定値）
    #[serde(default)]
    pub verify_url: String,
    /// ウィジェットのスクリプトの URL（空なら提供元の既定値）
    #[serde(default)]
    pub script_url: String,
    /// ウィジェットの `div` の class（空なら提供元の既定値）
    #[serde(default)]
    pub widget_class: String,
    /// トークンが入るフォームの欄の名前（空なら提供元の既定値）
    #[serde(default)]
    pub response_field: String,
}

fn default_timeout_secs() -> u64 {
    5
}

/// `"env:VAR_NAME"` なら環境変数の値、それ以外はそのまま。空・未設定は `None`
fn resolve_secret(configured: &str) -> Option<String> {
    let trimmed = configured.trim();
    let value = match trimmed.strip_prefix("env:") {
        Some(name) => std::env::var(name.trim()).ok()?,
        None => trimmed.to_string(),
    };
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn is_local_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "[::1]")
}

/// URL の `host`。`https://` はユーザー情報・ポートなしのものだけ、`http://` は手元のホスト
/// （`allow_local_http` のとき。ポート可。テストと開発用）だけを認める。認めない URL は `None`
fn host_of(url: &str, allow_local_http: bool) -> Option<&str> {
    let (rest, https) = match url.strip_prefix("https://") {
        Some(rest) => (rest, true),
        None => (url.strip_prefix("http://")?, false),
    };
    let authority = rest.split(['/', '?', '#']).next()?;
    if authority.is_empty() || authority.contains(['@', '\\', ' ']) {
        return None;
    }
    if https {
        return (!authority.contains(':')).then_some(authority);
    }
    let host = authority.rsplit_once(':').map_or(authority, |(h, _)| h);
    (allow_local_http && is_local_host(host)).then_some(host)
}

impl CaptchaConfig {
    fn verify_url(&self) -> &str {
        non_empty(&self.verify_url, self.provider.preset().verify_url)
    }

    fn script_url(&self) -> &str {
        non_empty(&self.script_url, self.provider.preset().script_url)
    }

    fn widget_class(&self) -> &str {
        non_empty(&self.widget_class, self.provider.preset().widget_class)
    }

    /// トークンが入るフォームの欄の名前
    pub fn response_field(&self) -> &str {
        non_empty(&self.response_field, self.provider.preset().response_field)
    }

    /// 設定の検査。`enabled` のときに起動時に呼ぶ（`context` は `[contact.captcha]` のような設定の場所）
    pub fn validate(&self, context: &str) -> Result<()> {
        let err = |m: String| Err(AegisError::Config(format!("{} {}", context, m)));
        if self.site_key.trim().is_empty() {
            return err("site_key is required".to_string());
        }
        if resolve_secret(&self.secret).is_none() {
            return err(
                "secret is required (a value, or \"env:VAR_NAME\" with the variable set)"
                    .to_string(),
            );
        }
        if self.timeout_secs == 0 || self.timeout_secs > MAX_TIMEOUT_SECS {
            return err(format!("timeout_secs must be 1 to {}", MAX_TIMEOUT_SECS));
        }
        for (name, value) in [
            ("verify_url", self.verify_url()),
            ("script_url", self.script_url()),
        ] {
            if value.is_empty() {
                return err(format!("{} is required for provider \"custom\"", name));
            }
            // 検証の URL は秘密を送るので、手元のホスト以外は https だけ
            if host_of(value, name == "verify_url").is_none() {
                return err(format!(
                    "{} must be an https:// URL without user info or a port (got {:?})",
                    name, value
                ));
            }
        }
        for (name, value) in [
            ("widget_class", self.widget_class()),
            ("response_field", self.response_field()),
        ] {
            if value.is_empty() {
                return err(format!("{} is required for provider \"custom\"", name));
            }
            if !value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            {
                return err(format!(
                    "{} may contain only letters, digits, '-' and '_' (got {:?})",
                    name, value
                ));
            }
        }
        Ok(())
    }
}

fn non_empty<'a>(value: &'a str, default: &'a str) -> &'a str {
    let value = value.trim();
    if value.is_empty() {
        default
    } else {
        value
    }
}

/// 検証に通らなかった理由
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CaptchaError {
    /// トークンが無い・正しくない・期限切れ（送信した人の側の問題）
    #[error("Captcha verification failed")]
    Failed,
    /// 検証サービスに届かない・応答を読めない（`fail_open = false` のとき）
    #[error("Captcha verification is unavailable. Try again later.")]
    Unavailable,
}

#[derive(Deserialize)]
struct VerifyResponse {
    #[serde(default)]
    success: bool,
}

/// CAPTCHA の検証とウィジェットの HTML
pub struct Captcha {
    config: CaptchaConfig,
    secret: String,
    http: reqwest::Client,
}

impl Captcha {
    /// `config` を検査して作る。`context` は設定の場所（エラーの文言用）
    pub fn new(config: CaptchaConfig, context: &str) -> Result<Self> {
        config.validate(context)?;
        let secret = resolve_secret(&config.secret).unwrap_or_default();
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            // 秘密を持って別のホストへ飛ばされないようにする
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| AegisError::Config(format!("Failed to build HTTP client: {}", e)))?;
        Ok(Self {
            config,
            secret,
            http,
        })
    }

    /// トークンが入るフォームの欄の名前
    pub fn response_field(&self) -> &str {
        self.config.response_field()
    }

    /// フォームの中に置くスクリプトとウィジェットの HTML（エスケープ済み）
    pub fn widget_html(&self) -> String {
        format!(
            "<script src=\"{}\" async defer></script>\n<div class=\"{}\" data-sitekey=\"{}\"></div>",
            escape_attr(self.config.script_url()),
            escape_attr(self.config.widget_class()),
            escape_attr(self.config.site_key.trim())
        )
    }

    /// ウィジェットが読み込む・つなぐ先のホスト（CSP の `script-src` などに足す）
    pub fn csp_hosts(&self) -> Vec<String> {
        let preset = self.config.provider.preset();
        if self.config.provider != Provider::Custom {
            return preset.csp_hosts.iter().map(|h| h.to_string()).collect();
        }
        host_of(self.config.script_url(), false)
            .map(|h| vec![h.to_string()])
            .unwrap_or_default()
    }

    /// トークンを検証する。`remote_ip` は分かれば送る（提供元の判定の助けになる）。
    /// 検証サービスに届かないときは `fail_open` に従い、通すか [`CaptchaError::Unavailable`] を返す
    pub async fn verify(
        &self,
        token: Option<&str>,
        remote_ip: Option<&str>,
    ) -> std::result::Result<(), CaptchaError> {
        let token = token.map(str::trim).unwrap_or_default();
        if token.is_empty() || token.len() > MAX_TOKEN_LEN {
            return Err(CaptchaError::Failed);
        }
        let mut form = vec![("secret", self.secret.as_str()), ("response", token)];
        if let Some(ip) = remote_ip.filter(|ip| !ip.is_empty()) {
            form.push(("remoteip", ip));
        }
        match self.siteverify(&form).await {
            Ok(true) => Ok(()),
            Ok(false) => Err(CaptchaError::Failed),
            Err(reason) if self.config.fail_open => {
                tracing::warn!(
                    "Captcha service unavailable, letting the request through (fail_open): {}",
                    reason
                );
                Ok(())
            }
            Err(reason) => {
                tracing::error!(
                    "Captcha service unavailable, rejecting the request: {}",
                    reason
                );
                Err(CaptchaError::Unavailable)
            }
        }
    }

    async fn siteverify(&self, form: &[(&str, &str)]) -> std::result::Result<bool, String> {
        let response = self
            .http
            .post(self.config.verify_url())
            .form(form)
            .send()
            .await
            .map_err(|e| e.without_url().to_string())?;
        if !response.status().is_success() {
            return Err(format!("siteverify returned {}", response.status()));
        }
        let body: VerifyResponse = response
            .json()
            .await
            .map_err(|e| format!("unreadable siteverify response: {}", e.without_url()))?;
        Ok(body.success)
    }
}

fn escape_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests;
