use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use super::*;
use crate::state::AppState;

fn enabled() -> ConsentConfig {
    ConsentConfig {
        enabled: true,
        ..ConsentConfig::default()
    }
}

fn category(id: &str, label: &str) -> ConsentCategory {
    ConsentCategory {
        id: id.to_string(),
        label: label.to_string(),
        description: String::new(),
    }
}

#[test]
fn test_defaults_are_valid_and_off() {
    let config = ConsentConfig::default();
    assert!(!config.enabled);
    config.validate().unwrap();
    let ids: Vec<String> = config.categories().into_iter().map(|c| c.id).collect();
    assert_eq!(ids, ["analytics", "marketing"]);
    assert_eq!(
        (config.layout.as_str(), config.position(), config.backdrop()),
        ("banner", "bottom", false)
    );
}

#[test]
fn test_invalid_settings_are_refused() {
    let invalid = |mutate: fn(&mut ConsentConfig)| {
        let mut config = enabled();
        mutate(&mut config);
        config.validate().unwrap_err().to_string()
    };
    assert!(invalid(|c| c.cookie_days = 0).contains("cookie_days"));
    assert!(invalid(|c| c.cookie_days = 400).contains("cookie_days"));
    assert!(invalid(|c| c.layout = "popup".into()).contains("layout"));
    // a position has to belong to the layout
    assert!(invalid(|c| c.position = "left".into()).contains("position"));
    assert!(invalid(|c| c.position = "bottom-right".into()).contains("position"));
    assert!(invalid(|c| {
        c.layout = "box".into();
        c.position = "bottom".into();
    })
    .contains("position"));
    assert!(invalid(|c| {
        c.layout = "modal".into();
        c.position = "top".into();
    })
    .contains("position"));
    assert!(invalid(|c| c.delay_ms = 10_001).contains("delay_ms"));
    assert!(invalid(|c| c.policy_url = "javascript:alert(1)".into()).contains("policy_url"));
    assert!(invalid(|c| c.policy_url = "//evil.example/p".into()).contains("policy_url"));
    assert!(invalid(|c| c.policy_url = "/privacy policy".into()).contains("spaces"));
    assert!(invalid(|c| c.message = "x".repeat(601)).contains("message"));
    assert!(invalid(|c| c.categories = vec![category("Analytics", "")]).contains("category id"));
    assert!(invalid(|c| c.categories = vec![category("1st", "x")]).contains("category id"));
    assert!(invalid(|c| c.categories = vec![category("necessary", "x")]).contains("always on"));
    assert!(invalid(|c| {
        c.categories = vec![category("ads", "Ads"), category("ads", "Ads again")]
    })
    .contains("twice"));
    // Only the four known ids have a built-in name
    assert!(invalid(|c| c.categories = vec![category("ads", "")]).contains("needs a label"));
    assert!(invalid(|c| {
        c.categories = (0..=MAX_CATEGORIES)
            .map(|i| category(&format!("c{}", i), "x"))
            .collect()
    })
    .contains("at most"));
    // fine
    let mut ok = enabled();
    ok.policy_url = "/privacy".into();
    ok.categories = vec![category("analytics", ""), category("ads", "Advertising")];
    ok.validate().unwrap();
    ok.policy_url = "https://example.com/privacy".into();
    ok.validate().unwrap();
    for (layout, position) in [
        ("banner", "top"),
        ("box", "top-left"),
        ("box", "bottom-left"),
        ("modal", "center"),
    ] {
        ok.layout = layout.into();
        ok.position = position.into();
        ok.validate().unwrap();
    }
}

#[test]
fn test_each_layout_has_its_default_position_and_backdrop() {
    let with = |layout: &str| ConsentConfig {
        layout: layout.into(),
        ..enabled()
    };
    assert_eq!(
        (with("banner").position(), with("banner").backdrop()),
        ("bottom", false)
    );
    assert_eq!(
        (with("box").position(), with("box").backdrop()),
        ("bottom-right", false)
    );
    assert_eq!(
        (with("modal").position(), with("modal").backdrop()),
        ("center", true)
    );
    // both can be set
    let mut dim = with("banner");
    dim.backdrop = Some(true);
    assert!(dim.backdrop());
    let mut clear = with("modal");
    clear.backdrop = Some(false);
    assert!(!clear.backdrop());
    let mut top = with("banner");
    top.position = "top".into();
    assert_eq!(top.position(), "top");
}

#[test]
fn test_settings_parse_from_toml() {
    let config: ConsentConfig = toml::from_str(
        r#"
        enabled = true
        layout = "box"
        position = "bottom-left"
        backdrop = true
        delay_ms = 1500
        policy_url = "/privacy"
        [[categories]]
        id = "analytics"
        [[categories]]
        id = "video"
        label = "Videos"
        "#,
    )
    .unwrap();
    config.validate().unwrap();
    assert_eq!(config.position(), "bottom-left");
    assert!(config.backdrop());
    assert_eq!(config.delay_ms, 1500);
    assert_eq!(config.categories.len(), 2);
}

#[test]
fn test_the_revision_changes_when_the_categories_or_revision_change() {
    let base = enabled();
    assert_eq!(base.rev(), enabled().rev());
    let mut renamed_label = enabled();
    renamed_label.categories = vec![category("analytics", "Stats"), category("marketing", "Ads")];
    // Only ids count: a new label does not ask everyone again
    assert_eq!(base.rev(), renamed_label.rev());
    let mut more = enabled();
    more.categories = vec![
        category("analytics", ""),
        category("marketing", ""),
        category("ads", "Ads"),
    ];
    assert_ne!(base.rev(), more.rev());
    let mut bumped = enabled();
    bumped.revision = 2;
    assert_ne!(base.rev(), bumped.rev());
    // The layout does not ask again either
    let mut other_layout = enabled();
    other_layout.layout = "modal".into();
    assert_eq!(base.rev(), other_layout.rev());
    assert!(base.rev().starts_with("1-"));
}

#[test]
fn test_the_head_has_the_assets_and_a_safe_json_config() {
    let mut config = crate::config::AegisConfig::default();
    assert_eq!(head_html(&config), "");
    config.consent = enabled();
    config.consent.layout = "modal".into();
    config.consent.delay_ms = 800;
    config.consent.categories = vec![category("analytics", ""), category("a</script>b", "x")];
    config.server.base_path = "/cms".to_string();
    let head = head_html(&config);
    let version = env!("CARGO_PKG_VERSION");
    assert!(head.contains(&format!(
        "<link rel=\"stylesheet\" href=\"/cms/_aegis/consent.css?v={}\">",
        version
    )));
    assert!(head.contains(&format!(
        "<script src=\"/cms/_aegis/consent.js?v={}\" defer></script>",
        version
    )));
    // The config is data, not code, and cannot end its own tag
    let marker = "id=\"aegis-consent-config\">";
    let json_start = head.find(marker).unwrap() + marker.len();
    let json_end = head[json_start..].find("</script>").unwrap() + json_start;
    let json_text = &head[json_start..json_end];
    assert!(head.contains("type=\"application/json\""));
    assert!(!json_text.contains('<') && !json_text.contains('>') && !json_text.contains('&'));
    let parsed: serde_json::Value = serde_json::from_str(json_text).unwrap();
    assert_eq!(parsed["days"], 180);
    assert_eq!(parsed["delay"], 800);
    assert_eq!(parsed["modal"], true);
    assert_eq!(parsed["categories"][0], "analytics");
    assert_eq!(parsed["categories"][1], "a</script>b");
    assert_eq!(parsed["rev"], config.consent.rev());
    // The wording and the look belong to the theme's template, not to the script's settings
    assert!(parsed.get("texts").is_none());
}

fn test_state(
    configure: impl FnOnce(&mut crate::config::AegisConfig),
) -> impl std::future::Future<Output = AppState> {
    let mut config = crate::config::AegisConfig::default();
    config.consent = enabled();
    configure(&mut config);
    crate::test_support::test_app_state(config, aegis_runtime::PluginSupervisor::new(), Vec::new())
}

async fn state(configure: impl FnOnce(&mut crate::config::AegisConfig)) -> Arc<AppState> {
    Arc::new(test_state(configure).await)
}

/// A state whose theme is a directory with the given `consent.html`
async fn state_with_theme(consent_html: &str) -> (Arc<AppState>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let templates = dir.path().join("mine").join("templates");
    std::fs::create_dir_all(&templates).unwrap();
    std::fs::write(templates.join("consent.html"), consent_html).unwrap();
    let mut state = test_state(|_| {}).await;
    state.theme = Arc::new(aegis_theme::ThemeEngine::new(dir.path(), "mine"));
    (Arc::new(state), dir)
}

async fn get(state: &Arc<AppState>, path: &str) -> (StatusCode, axum::http::HeaderMap, String) {
    let response = crate::app::router(&state.config, state.clone())
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, headers, String::from_utf8_lossy(&bytes).to_string())
}

#[tokio::test]
async fn test_public_pages_get_the_tags_in_the_head_and_the_banner_before_the_body_ends() {
    let state = state(|_| {}).await;
    for path in ["/", "/articles", "/does-not-exist"] {
        let (_, headers, body) = get(&state, path).await;
        assert!(
            headers["content-type"]
                .to_str()
                .unwrap()
                .starts_with("text/html"),
            "{}",
            path
        );
        let lower = body.to_ascii_lowercase();
        let head_end = lower.find("</head>").unwrap();
        let body_end = lower.rfind("</body>").unwrap();
        let script = body
            .find("/_aegis/consent.js")
            .unwrap_or_else(|| panic!("{}: {}", path, body));
        assert!(script < head_end, "{}", path);
        assert_eq!(
            body.matches("id=\"aegis-consent-config\"").count(),
            1,
            "{}",
            path
        );
        // the banner (built-in consent.html) is just before the end of the page
        let banner = body
            .find("data-aegis-consent-root")
            .unwrap_or_else(|| panic!("{}: {}", path, body));
        assert!(head_end < banner && banner < body_end, "{}", path);
        assert_eq!(
            body.matches("data-aegis-consent-root").count(),
            1,
            "{}",
            path
        );
        assert!(
            body.contains("aegis-consent-layout-banner aegis-consent-pos-bottom"),
            "{}",
            body
        );
        // the length header (if any) matches the new body
        if let Some(length) = headers.get("content-length") {
            assert_eq!(
                length.to_str().unwrap().parse::<usize>().unwrap(),
                body.len()
            );
        }
    }
}

#[tokio::test]
async fn test_the_layout_settings_reach_the_banner() {
    let state = state(|config| {
        config.consent.layout = "box".into();
        config.consent.position = "top-left".into();
        config.consent.backdrop = Some(true);
        config.consent.policy_url = "/privacy".into();
        config.consent.message = "Hello </b> there".into();
        config.consent.categories = vec![
            category("analytics", ""),
            category("video", "Embedded <videos>"),
        ];
    })
    .await;
    let html = body_html(&state);
    assert!(
        html.contains("aegis-consent-layout-box aegis-consent-pos-top-left aegis-consent-backdrop"),
        "{}",
        html
    );
    assert!(!html.contains("aria-modal"));
    // The message and names are escaped; the policy link is there
    assert!(
        html.contains("Hello &lt;&#x2f;b&gt; there") || html.contains("Hello &lt;/b&gt; there"),
        "{}",
        html
    );
    assert!(html.contains("Embedded &lt;videos&gt;"));
    // (the template engine writes "/" as &#x2f;, which a browser reads as "/")
    assert!(
        html.contains(r#"<a class="aegis-consent-policy" href="&#x2f;privacy">Privacy policy</a>"#),
        "{}",
        html
    );
    // Names left out come from the built-in texts; every category has a checkbox
    assert!(html.contains("<strong>Analytics</strong>"));
    assert!(html.contains("Helps us understand how the site is used."));
    assert!(html.contains(r#"data-aegis-consent-category="analytics""#));
    assert!(html.contains(r#"data-aegis-consent-category="video""#));
    // modal: the dialog says so
    let modal = self::state(|config| config.consent.layout = "modal".into()).await;
    let html = body_html(&modal);
    assert!(
        html.contains("aegis-consent-layout-modal aegis-consent-pos-center aegis-consent-backdrop")
    );
    assert!(html.contains(r#"aria-modal="true""#));
    // The settings button can be left out
    let none = self::state(|config| config.consent.reopen_button = false).await;
    assert!(!body_html(&none).contains("data-aegis-consent-reopen"));
}

#[tokio::test]
async fn test_the_built_in_texts_follow_the_theme_language() {
    let state = state(|config| config.theme.locale = "ja".into()).await;
    let html = body_html(&state);
    assert!(
        html.contains("すべて拒否") && html.contains("すべて許可"),
        "{}",
        html
    );
    assert!(html.contains("<strong>アクセス解析</strong>"));
    assert!(!html.contains("Reject all"));
}

#[tokio::test]
async fn test_a_theme_can_replace_the_banner() {
    let (state, _dir) = state_with_theme(
        r#"<aside data-aegis-consent-root hidden class="mine-{{ fields.consent.layout }}">
  <p>{{ site_title }}: {{ fields.message or "custom words" }}</p>
  {% for c in fields.categories %}<label><input type="checkbox" data-aegis-consent-category="{{ c.id }}"> {{ c.id }}</label>{% endfor %}
  <button data-aegis-consent-action="accept-all">OK</button>
</aside>"#,
    )
    .await;
    let (_, _, body) = get(&state, "/").await;
    assert!(
        body.contains(r#"<aside data-aegis-consent-root hidden class="mine-banner">"#),
        "{}",
        body
    );
    assert!(body.contains("custom words"));
    assert!(body.contains(r#"data-aegis-consent-category="marketing""#));
    // The built-in markup is gone, the script and its settings are still added
    assert!(!body.contains("aegis-consent-title"));
    assert!(body.contains("/_aegis/consent.js"));
}

#[tokio::test]
async fn test_a_broken_banner_template_leaves_the_page_working() {
    let (state, _dir) = state_with_theme("{% if %} broken").await;
    assert_eq!(body_html(&state), "");
    let (status, _, body) = get(&state, "/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body.contains("data-aegis-consent-root"));
    // The head still has the script: with no banner it just never asks, so nothing waiting for consent runs
    assert!(body.contains("/_aegis/consent.js"));
}

#[tokio::test]
async fn test_nothing_is_added_when_consent_is_off() {
    let state = state(|config| config.consent.enabled = false).await;
    let (_, _, body) = get(&state, "/").await;
    assert!(!body.contains("aegis-consent"));
    assert_eq!(body_html(&state), "");
    assert_eq!(
        get(&state, "/_aegis/consent.js").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(&state, "/_aegis/consent.css").await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn test_assets_are_served_with_the_right_types() {
    let state = state(|_| {}).await;
    let (status, headers, body) = get(&state, "/_aegis/consent.js").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers["content-type"],
        "application/javascript; charset=utf-8"
    );
    assert_eq!(headers["x-content-type-options"], "nosniff");
    assert_eq!(headers["cache-control"], "public, max-age=3600");
    assert!(body.contains("window.AegisConsent"));
    let (status, headers, body) = get(&state, "/_aegis/consent.css").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "text/css; charset=utf-8");
    assert!(body.contains(".aegis-consent"));
}

#[tokio::test]
async fn test_api_admin_and_assets_are_left_alone() {
    let state = state(|_| {}).await;
    let (_, _, body) = get(&state, "/api/v1/health").await;
    assert!(!body.contains("aegis-consent"));
    // the feed and the sitemap are XML
    let (_, _, feed) = get(&state, "/feed.xml").await;
    assert!(!feed.contains("aegis-consent"));
    // the asset files themselves
    let (_, _, script) = get(&state, "/_aegis/consent.js").await;
    assert!(!script.contains("id=\"aegis-consent-config\"></script>"));
}

#[tokio::test]
async fn test_the_base_path_is_in_the_asset_urls() {
    let state = state(|config| config.server.base_path = "/cms".to_string()).await;
    let (_, _, body) = get(&state, "/").await;
    assert!(
        body.contains("href=\"/cms/_aegis/consent.css?v="),
        "{}",
        body
    );
    assert!(body.contains("src=\"/cms/_aegis/consent.js?v="));
}

/// The names a theme's consent.html and the script agree on (documented in docs/consent.md)
const HOOKS: &[&str] = &[
    "data-aegis-consent-root",
    "data-aegis-consent-action=\"accept-all\"",
    "data-aegis-consent-action=\"reject-all\"",
    "data-aegis-consent-action=\"customize\"",
    "data-aegis-consent-action=\"save\"",
    "data-aegis-consent-details",
    "data-aegis-consent-category",
    "data-aegis-consent-reopen",
];

#[tokio::test]
async fn test_the_built_in_banner_and_the_script_use_the_same_names() {
    let state = state(|_| {}).await;
    let html = body_html(&state);
    let script = script();
    for hook in HOOKS {
        assert!(
            html.contains(hook),
            "the built-in consent.html lacks {}",
            hook
        );
        // the script knows each name (an action by its value, the attributes by their names)
        let name = hook.split('=').next().unwrap();
        assert!(script.contains(name), "consent.js does not use {}", name);
    }
    for action in ["accept-all", "reject-all", "customize", "save"] {
        assert!(
            script.contains(action),
            "consent.js does not handle {}",
            action
        );
    }
    assert!(script.contains("data-aegis-consent-open"));
    assert!(script.contains("window.AegisConsent"));
    assert!(script.contains("aegis:consent"));
    // The root and the details start hidden; the script shows them
    assert!(html.contains("data-aegis-consent-root role=\"dialog\""));
    assert!(html.contains("data-aegis-consent-details hidden"));
}
