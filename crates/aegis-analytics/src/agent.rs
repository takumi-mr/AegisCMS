//! リクエストの User-Agent・参照元・ページのパスの判定と整形

/// クローラー・監視・コマンドラインのツールとみなす User-Agent の文字列（小文字で比べる）
const BOT_MARKERS: [&str; 18] = [
    "bot",
    "crawl",
    "spider",
    "slurp",
    "curl",
    "wget",
    "python-",
    "go-http-client",
    "java/",
    "okhttp",
    "httpclient",
    "headless",
    "lighthouse",
    "pagespeed",
    "preview",
    "facebookexternalhit",
    "monitor",
    "uptime",
];

/// 集計に含めない User-Agent（空、またはクローラーなど）
pub fn is_bot(user_agent: &str) -> bool {
    let ua = user_agent.trim().to_ascii_lowercase();
    ua.is_empty() || BOT_MARKERS.iter().any(|m| ua.contains(m))
}

/// 端末の種類（`mobile` / `tablet` / `desktop`）
pub fn device(user_agent: &str) -> &'static str {
    let ua = user_agent.to_ascii_lowercase();
    if ua.contains("ipad")
        || ua.contains("tablet")
        || (ua.contains("android") && !ua.contains("mobile"))
    {
        "tablet"
    } else if ua.contains("mobi") || ua.contains("iphone") || ua.contains("android") {
        "mobile"
    } else {
        "desktop"
    }
}

/// 参照元の分類
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Referrer {
    /// 参照元なし（直接の訪問・ブックマーク、参照元を送らないサイトからの移動、読めない値）
    Direct,
    /// 自分のサイトの中での移動
    Internal,
    /// ほかのサイト（ホスト名、`www.` は除く）
    External(String),
}

/// `document.referrer` を分類する。`own_host` はリクエストの `Host`
pub fn classify_referrer(referrer: &str, own_host: &str) -> Referrer {
    let Ok(url) = url::Url::parse(referrer.trim()) else {
        return Referrer::Direct;
    };
    if !matches!(url.scheme(), "http" | "https") {
        return Referrer::Direct;
    }
    let Some(host) = url.host_str().map(str::to_ascii_lowercase) else {
        return Referrer::Direct;
    };
    let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
    let own = own_host
        .split(':')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let own = own.strip_prefix("www.").unwrap_or(&own);
    if host == own {
        Referrer::Internal
    } else {
        Referrer::External(host)
    }
}

/// ページのパスを集計用にする（クエリ・フラグメントを除き、`/` で始まらない・長すぎる・制御文字を含むものは `None`）
pub fn normalize_path(path: &str) -> Option<String> {
    let path = path.split(['?', '#']).next().unwrap_or_default().trim();
    if !path.starts_with('/') || path.starts_with("//") || path.chars().any(char::is_control) {
        return None;
    }
    if path.chars().count() > aegis_db::analytics::ANALYTICS_VALUE_MAX_CHARS {
        return None;
    }
    Some(path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const IPHONE: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1";
    const IPAD: &str = "Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15";
    const ANDROID_PHONE: &str =
        "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 Chrome/120 Mobile Safari/537.36";
    const ANDROID_TABLET: &str =
        "Mozilla/5.0 (Linux; Android 14; SM-X710) AppleWebKit/537.36 Chrome/120 Safari/537.36";
    const DESKTOP: &str =
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/120 Safari/537.36";

    #[test]
    fn test_devices() {
        assert_eq!(device(IPHONE), "mobile");
        assert_eq!(device(ANDROID_PHONE), "mobile");
        assert_eq!(device(IPAD), "tablet");
        assert_eq!(device(ANDROID_TABLET), "tablet");
        assert_eq!(device(DESKTOP), "desktop");
    }

    #[test]
    fn test_bots() {
        assert!(is_bot(""));
        assert!(is_bot("Mozilla/5.0 (compatible; Googlebot/2.1)"));
        assert!(is_bot("curl/8.5.0"));
        assert!(is_bot("Mozilla/5.0 HeadlessChrome/120"));
        assert!(!is_bot(DESKTOP));
        assert!(!is_bot(IPHONE));
    }

    #[test]
    fn test_classify_referrer() {
        assert_eq!(
            classify_referrer("https://www.google.com/search?q=x", "example.com"),
            Referrer::External("google.com".to_string())
        );
        assert_eq!(
            classify_referrer("https://example.com/a", "example.com:8080"),
            Referrer::Internal
        );
        assert_eq!(
            classify_referrer("https://www.example.com/a", "example.com"),
            Referrer::Internal
        );
        assert_eq!(classify_referrer("", "example.com"), Referrer::Direct);
        assert_eq!(
            classify_referrer("android-app://com.x", "example.com"),
            Referrer::Direct
        );
    }

    #[test]
    fn test_normalize_path() {
        assert_eq!(normalize_path("/a/b?x=1#top").as_deref(), Some("/a/b"));
        assert_eq!(normalize_path("/").as_deref(), Some("/"));
        assert_eq!(normalize_path("https://x/"), None);
        assert_eq!(normalize_path("//evil.example/"), None);
        assert_eq!(normalize_path("/a\nb"), None);
        assert_eq!(normalize_path(&format!("/{}", "a".repeat(300))), None);
    }
}
