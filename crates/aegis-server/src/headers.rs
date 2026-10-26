//! HTTP 共通ページヘッダーおよびブラウザー向け防御ヘッダーの設定管理。
//! DB の `_aegis_settings` に保存された設定（CSP、X-Frame-Options、X-Content-Type-Options 等）を
//! 公開サイトおよび管理画面を含む HTTP レスポンスに反映する。

use std::collections::HashMap;

use aegis_db::DatabaseAdapter;
use axum::http::{header::HeaderName, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};

/// `_aegis_settings` のキー名
pub const CSP_SETTING: &str = "headers.content_security_policy";
pub const X_FRAME_OPTIONS_SETTING: &str = "headers.x_frame_options";
pub const X_CONTENT_TYPE_OPTIONS_SETTING: &str = "headers.x_content_type_options";
pub const REFERRER_POLICY_SETTING: &str = "headers.referrer_policy";
pub const PERMISSIONS_POLICY_SETTING: &str = "headers.permissions_policy";
pub const COOP_SETTING: &str = "headers.cross_origin_opener_policy";
pub const CUSTOM_HEADERS_SETTING: &str = "headers.custom_headers";

/// デフォルトの防御ヘッダー値
pub const DEFAULT_CONTENT_SECURITY_POLICY: &str =
    "default-src 'self'; script-src 'self' 'unsafe-inline' 'unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https:; font-src 'self' data: https:; connect-src 'self'; frame-ancestors 'self'; object-src 'none'; base-uri 'self'; form-action 'self';";
pub const DEFAULT_X_FRAME_OPTIONS: &str = "SAMEORIGIN";
pub const DEFAULT_X_CONTENT_TYPE_OPTIONS: &str = "nosniff";
pub const DEFAULT_REFERRER_POLICY: &str = "strict-origin-when-cross-origin";
pub const DEFAULT_PERMISSIONS_POLICY: &str = "camera=(), microphone=(), geolocation=()";
pub const DEFAULT_CROSS_ORIGIN_OPENER_POLICY: &str = "same-origin";

/// 最大文字数制限
pub const MAX_HEADER_NAME_CHARS: usize = 100;
pub const MAX_HEADER_VALUE_CHARS: usize = 4096;
pub const MAX_CUSTOM_HEADERS_COUNT: usize = 50;

/// 任意のカスタム HTTP ヘッダー（キーと値のペア）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomHeader {
    pub name: String,
    pub value: String,
}

/// デフォルト値の公開情報
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeaderDefaults {
    pub content_security_policy: String,
    pub x_frame_options: String,
    pub x_content_type_options: String,
    pub referrer_policy: String,
    pub permissions_policy: String,
    pub cross_origin_opener_policy: String,
}

impl Default for HeaderDefaults {
    fn default() -> Self {
        Self {
            content_security_policy: DEFAULT_CONTENT_SECURITY_POLICY.to_string(),
            x_frame_options: DEFAULT_X_FRAME_OPTIONS.to_string(),
            x_content_type_options: DEFAULT_X_CONTENT_TYPE_OPTIONS.to_string(),
            referrer_policy: DEFAULT_REFERRER_POLICY.to_string(),
            permissions_policy: DEFAULT_PERMISSIONS_POLICY.to_string(),
            cross_origin_opener_policy: DEFAULT_CROSS_ORIGIN_OPENER_POLICY.to_string(),
        }
    }
}

/// Admin Studio から変更した共通ヘッダー設定（None はデフォルト値を使用）
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeaderOverrides {
    /// Content-Security-Policy（None はデフォルト。空文字や "disabled" で無効化）
    pub content_security_policy: Option<String>,
    /// X-Frame-Options（None は "SAMEORIGIN"。空文字や "disabled" で無効化）
    pub x_frame_options: Option<String>,
    /// X-Content-Type-Options（None は "nosniff"。空文字や "disabled" で無効化）
    pub x_content_type_options: Option<String>,
    /// Referrer-Policy（None は "strict-origin-when-cross-origin"。空文字や "disabled" で無効化）
    pub referrer_policy: Option<String>,
    /// Permissions-Policy（None はデフォルト。空文字や "disabled" で無効化）
    pub permissions_policy: Option<String>,
    /// Cross-Origin-Opener-Policy（None は "same-origin"。空文字や "disabled" で無効化）
    pub cross_origin_opener_policy: Option<String>,
    /// 任意のカスタムヘッダー一覧
    pub custom_headers: Vec<CustomHeader>,
}

impl HeaderOverrides {
    pub fn from_settings(settings: &HashMap<String, String>) -> Self {
        let custom_headers = settings
            .get(CUSTOM_HEADERS_SETTING)
            .and_then(|raw| serde_json::from_str::<Vec<CustomHeader>>(raw).ok())
            .unwrap_or_default();

        Self {
            content_security_policy: settings.get(CSP_SETTING).cloned(),
            x_frame_options: settings.get(X_FRAME_OPTIONS_SETTING).cloned(),
            x_content_type_options: settings.get(X_CONTENT_TYPE_OPTIONS_SETTING).cloned(),
            referrer_policy: settings.get(REFERRER_POLICY_SETTING).cloned(),
            permissions_policy: settings.get(PERMISSIONS_POLICY_SETTING).cloned(),
            cross_origin_opener_policy: settings.get(COOP_SETTING).cloned(),
            custom_headers,
        }
    }

    /// DB から読み込む。読めなければ警告してデフォルト値で動かす
    pub async fn load(db: &dyn DatabaseAdapter) -> Self {
        match db.list_settings().await {
            Ok(settings) => Self::from_settings(&settings),
            Err(e) => {
                tracing::warn!("Could not load header settings, using defaults: {}", e);
                Self::default()
            }
        }
    }

    /// 有効なヘッダー値を決定する（"disabled" や空文字は無効化とみなす）
    fn resolve_val<'a>(custom: Option<&'a str>, default: &'a str) -> Option<&'a str> {
        match custom {
            Some(v) => {
                let trimmed = v.trim();
                if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("disabled") {
                    None
                } else {
                    Some(trimmed)
                }
            }
            None => Some(default),
        }
    }

    pub fn effective_csp(&self) -> Option<&str> {
        Self::resolve_val(
            self.content_security_policy.as_deref(),
            DEFAULT_CONTENT_SECURITY_POLICY,
        )
    }

    pub fn effective_x_frame_options(&self) -> Option<&str> {
        Self::resolve_val(self.x_frame_options.as_deref(), DEFAULT_X_FRAME_OPTIONS)
    }

    pub fn effective_x_content_type_options(&self) -> Option<&str> {
        Self::resolve_val(
            self.x_content_type_options.as_deref(),
            DEFAULT_X_CONTENT_TYPE_OPTIONS,
        )
    }

    pub fn effective_referrer_policy(&self) -> Option<&str> {
        Self::resolve_val(self.referrer_policy.as_deref(), DEFAULT_REFERRER_POLICY)
    }

    pub fn effective_permissions_policy(&self) -> Option<&str> {
        Self::resolve_val(
            self.permissions_policy.as_deref(),
            DEFAULT_PERMISSIONS_POLICY,
        )
    }

    pub fn effective_cross_origin_opener_policy(&self) -> Option<&str> {
        Self::resolve_val(
            self.cross_origin_opener_policy.as_deref(),
            DEFAULT_CROSS_ORIGIN_OPENER_POLICY,
        )
    }

    /// レスポンスの HeaderMap に防御ヘッダーおよびカスタムヘッダーを注入する
    pub fn apply_to_headers(&self, headers: &mut HeaderMap) {
        if let Some(val) = self.effective_csp() {
            insert_if_valid(headers, "content-security-policy", val);
        }
        if let Some(val) = self.effective_x_frame_options() {
            insert_if_valid(headers, "x-frame-options", val);
        }
        if let Some(val) = self.effective_x_content_type_options() {
            insert_if_valid(headers, "x-content-type-options", val);
        }
        if let Some(val) = self.effective_referrer_policy() {
            insert_if_valid(headers, "referrer-policy", val);
        }
        if let Some(val) = self.effective_permissions_policy() {
            insert_if_valid(headers, "permissions-policy", val);
        }
        if let Some(val) = self.effective_cross_origin_opener_policy() {
            insert_if_valid(headers, "cross-origin-opener-policy", val);
        }

        // 任意のカスタムヘッダー
        for custom in &self.custom_headers {
            let name = custom.name.trim();
            let value = custom.value.trim();
            if !name.is_empty() && !value.is_empty() {
                insert_if_valid(headers, name, value);
            }
        }
    }
}

fn insert_if_valid(headers: &mut HeaderMap, name: &str, val: &str) {
    if let (Ok(header_name), Ok(header_val)) = (
        HeaderName::from_bytes(name.as_bytes()),
        HeaderValue::from_str(val),
    ) {
        headers.insert(header_name, header_val);
    }
}

/// ヘッダー名の検証
pub fn validate_header_name(name: &str) -> Result<(), &'static str> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Header name cannot be empty");
    }
    if trimmed.len() > MAX_HEADER_NAME_CHARS {
        return Err("Header name is too long (max 100 characters)");
    }
    if trimmed.contains('\r') || trimmed.contains('\n') {
        return Err("Header name contains newline characters");
    }
    // RFC 7230 token characters: visible ascii except delimiters
    let is_valid = trimmed.bytes().all(|b| {
        b.is_ascii_alphanumeric()
            || matches!(
                b,
                b'!' | b'#'
                    | b'$'
                    | b'%'
                    | b'&'
                    | b'\''
                    | b'*'
                    | b'+'
                    | b'-'
                    | b'.'
                    | b'^'
                    | b'_'
                    | b'`'
                    | b'|'
                    | b'~'
            )
    });
    if !is_valid {
        return Err("Header name contains invalid characters");
    }
    Ok(())
}

/// ヘッダー値の検証
pub fn validate_header_value(val: &str) -> Result<(), &'static str> {
    if val.len() > MAX_HEADER_VALUE_CHARS {
        return Err("Header value is too long (max 4096 characters)");
    }
    if val.contains('\r') || val.contains('\n') {
        return Err("Header value cannot contain newline characters (CRLF)");
    }
    // HeaderValue should be visible ASCII / Latin-1 without control chars
    if val.bytes().any(|b| (b < 32 && b != 9) || b == 127) {
        return Err("Header value contains invalid control characters");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_headers_applied() {
        let overrides = HeaderOverrides::default();
        let mut headers = HeaderMap::new();
        overrides.apply_to_headers(&mut headers);

        assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");
        assert_eq!(headers.get("x-frame-options").unwrap(), "SAMEORIGIN");
        assert_eq!(
            headers.get("referrer-policy").unwrap(),
            "strict-origin-when-cross-origin"
        );
        assert!(headers.contains_key("content-security-policy"));
        assert!(headers.contains_key("permissions-policy"));
        assert_eq!(
            headers.get("cross-origin-opener-policy").unwrap(),
            "same-origin"
        );
    }

    #[test]
    fn test_custom_override_and_disabled() {
        let overrides = HeaderOverrides {
            x_frame_options: Some("DENY".to_string()),
            x_content_type_options: Some("disabled".to_string()),
            custom_headers: vec![CustomHeader {
                name: "X-Custom-Test".to_string(),
                value: "hello-world".to_string(),
            }],
            ..Default::default()
        };

        let mut headers = HeaderMap::new();
        overrides.apply_to_headers(&mut headers);

        assert_eq!(headers.get("x-frame-options").unwrap(), "DENY");
        assert!(!headers.contains_key("x-content-type-options"));
        assert_eq!(headers.get("x-custom-test").unwrap(), "hello-world");
    }

    #[test]
    fn test_validation() {
        assert!(validate_header_name("X-Custom-Header").is_ok());
        assert!(validate_header_name("X-Header\r\nBad").is_err());
        assert!(validate_header_name("").is_err());

        assert!(validate_header_value("nosniff").is_ok());
        assert!(validate_header_value("val\r\nSet-Cookie: evil").is_err());
    }
}
