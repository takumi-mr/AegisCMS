//! お知らせバナーの API と、公開ページへの差し込みの結合テスト（実際のルーターを通す）

use std::sync::Arc;

use aegis_runtime::PluginSupervisor;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::config::AegisConfig;
use crate::state::AppState;

struct Site {
    state: Arc<AppState>,
    admin: String,
    editor: String,
}

struct Reply {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    body: String,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or(Value::Null)
    }
}

async fn site(configure: impl FnOnce(&mut AegisConfig)) -> Site {
    let mut config = AegisConfig::default();
    config.audit.enabled = true;
    configure(&mut config);
    let state = Arc::new(
        crate::test_support::test_app_state(config, PluginSupervisor::new(), Vec::new()).await,
    );
    let session = |user: &'static str, role: &'static str| {
        let state = state.clone();
        async move {
            state
                .sessions
                .create_session(user, role, Default::default(), None)
                .await
                .unwrap()
                .id
        }
    };
    let admin = session("admin1", "admin").await;
    let editor = session("editor1", "editor").await;
    Site {
        state,
        admin,
        editor,
    }
}

impl Site {
    async fn send(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
        token: Option<&str>,
    ) -> Reply {
        let mut request = Request::builder().method(method).uri(path);
        if body.is_some() {
            request = request.header("content-type", "application/json");
        }
        if let Some(token) = token {
            request = request.header("authorization", format!("Bearer {}", token));
        }
        let response = crate::app::router(&self.state.config, self.state.clone())
            .oneshot(
                request
                    .body(Body::from(body.map(|b| b.to_string()).unwrap_or_default()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        Reply {
            status,
            headers,
            body: String::from_utf8_lossy(&bytes).to_string(),
        }
    }

    async fn page(&self) -> String {
        self.send("GET", "/", None, None).await.body
    }

    async fn add(&self, body: Value) -> Reply {
        self.send("POST", "/api/v1/notices", Some(body), Some(&self.admin))
            .await
    }

    async fn list(&self) -> Value {
        self.send("GET", "/api/v1/notices", None, Some(&self.admin))
            .await
            .json()
    }
}

#[tokio::test]
async fn test_api_manages_notices_and_refuses_bad_ones() {
    let site = site(|_| {}).await;
    // admin only
    assert_eq!(
        site.send("GET", "/api/v1/notices", None, Some(&site.editor))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        site.send("GET", "/api/v1/notices", None, None).await.status,
        StatusCode::UNAUTHORIZED
    );

    let created = site
        .add(json!({
            "message": "Maintenance tonight", "link_url": "/status", "link_label": "Status",
            "style": "warning", "start_at": "2026-01-01T00:00:00+09:00", "end_at": "2999-01-01T00:00:00Z"
        }))
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let notice = created.json();
    assert_eq!(notice["status"], "active");
    assert_eq!(notice["dismissible"], true);
    assert_eq!(notice["enabled"], true);
    assert_eq!(notice["start_at"], "2025-12-31T15:00:00.000000Z");
    let id = notice["id"].as_str().unwrap().to_string();

    // refused, with the field
    for (body, field) in [
        (json!({ "message": "" }), "message"),
        (json!({ "message": "x", "style": "rainbow" }), "style"),
        (
            json!({ "message": "x", "link_url": "javascript:alert(1)" }),
            "link_url",
        ),
        (
            json!({ "message": "x", "start_at": "tomorrow" }),
            "start_at",
        ),
        (
            json!({ "message": "x", "start_at": "2026-02-01T00:00:00Z", "end_at": "2026-01-01T00:00:00Z" }),
            "end_at",
        ),
    ] {
        let reply = site.add(body.clone()).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{}", body);
        assert_eq!(reply.json()["code"], "invalid_notice");
        assert_eq!(reply.json()["field"], field, "{}", body);
    }

    // change: a later start makes it "scheduled"; disabling makes it "disabled"
    let scheduled = site
        .send(
            "PUT",
            &format!("/api/v1/notices/{}", id),
            Some(json!({ "message": "Later", "start_at": "2999-01-01T00:00:00Z" })),
            Some(&site.admin),
        )
        .await;
    assert_eq!(scheduled.status, StatusCode::OK, "{}", scheduled.body);
    assert_eq!(scheduled.json()["status"], "scheduled");
    assert_eq!(scheduled.json()["style"], "info");
    let disabled = site
        .send(
            "PUT",
            &format!("/api/v1/notices/{}", id),
            Some(json!({ "message": "Off", "enabled": false })),
            Some(&site.admin),
        )
        .await;
    assert_eq!(disabled.json()["status"], "disabled");
    let expired = site
        .send(
            "PUT",
            &format!("/api/v1/notices/{}", id),
            Some(json!({ "message": "Over", "end_at": "2020-01-01T00:00:00Z" })),
            Some(&site.admin),
        )
        .await;
    assert_eq!(expired.json()["status"], "expired");
    assert_eq!(
        site.send(
            "PUT",
            "/api/v1/notices/missing",
            Some(json!({ "message": "x" })),
            Some(&site.admin)
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );

    let list = site.list().await;
    assert_eq!(list["total"], 1);
    assert_eq!(list["notices"][0]["message"], "Over");

    // delete
    let deleted = site
        .send(
            "DELETE",
            &format!("/api/v1/notices/{}", id),
            None,
            Some(&site.admin),
        )
        .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    assert_eq!(site.list().await["total"], 0);
    assert_eq!(
        site.send(
            "DELETE",
            &format!("/api/v1/notices/{}", id),
            None,
            Some(&site.admin)
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn test_an_active_notice_is_added_right_after_the_body_tag() {
    let site = site(|_| {}).await;
    assert!(
        !site.page().await.contains("aegis-notice"),
        "nothing without notices"
    );

    site.add(json!({
        "message": "We are <b>closed</b> today", "link_url": "/status", "style": "alert"
    }))
    .await;
    let page = site.page().await;
    let lower = page.to_ascii_lowercase();
    let body_open = lower.find("<body").unwrap();
    let after_open = body_open + lower[body_open..].find('>').unwrap() + 1;
    let notice = page
        .find("<div class=\"aegis-notices\"")
        .unwrap_or_else(|| panic!("{}", page));
    // it is the first thing in the body
    assert_eq!(notice, after_open + page[after_open..].find('<').unwrap());
    assert!(
        page[after_open..notice].trim().is_empty(),
        "{}",
        &page[after_open..notice]
    );
    assert!(page.contains("aegis-notice aegis-notice-alert"));
    assert!(page.contains(r#"role="alert""#));
    // the message is escaped; the link has the built-in label
    assert!(
        page.contains("We are &lt;b&gt;closed&lt;&#x2f;b&gt; today")
            || page.contains("We are &lt;b&gt;closed&lt;/b&gt; today"),
        "{}",
        page
    );
    assert!(page.contains(">Learn more</a>"));
    assert!(page.contains("data-aegis-notice-close hidden"));
    // the head has the assets, once
    let head_end = lower.find("</head>").unwrap();
    let script = page.find("/_aegis/notices.js").unwrap();
    assert!(script < head_end);
    assert_eq!(page.matches("/_aegis/notices.js").count(), 1);
}

#[tokio::test]
async fn test_notices_follow_their_period_and_switch() {
    let site = site(|_| {}).await;
    let add = |body: Value| site.add(body);
    add(json!({ "message": "Past one", "end_at": "2020-01-01T00:00:00Z" })).await;
    add(json!({ "message": "Future one", "start_at": "2999-01-01T00:00:00Z" })).await;
    add(json!({ "message": "Draft one", "enabled": false })).await;
    // all three are saved, none is shown, and the page has no notice tags at all
    assert_eq!(site.list().await["total"], 3);
    let page = site.page().await;
    assert!(!page.contains("aegis-notice"), "{}", page);
    assert!(!page.contains("notices.js"));

    add(json!({ "message": "Current one", "start_at": "2020-01-01T00:00:00Z", "end_at": "2999-01-01T00:00:00Z" }))
        .await;
    let page = site.page().await;
    assert!(page.contains("Current one"));
    assert!(
        !page.contains("Past one") && !page.contains("Future one") && !page.contains("Draft one")
    );
}

#[tokio::test]
async fn test_at_most_three_notices_and_dismissible_ones_have_a_close_button() {
    let site = site(|_| {}).await;
    for (i, dismissible) in [true, false, true, true].into_iter().enumerate() {
        site.add(json!({ "message": format!("Notice {}", i + 1), "dismissible": dismissible }))
            .await;
    }
    let page = site.page().await;
    assert!(page.contains("Notice 1") && page.contains("Notice 2") && page.contains("Notice 3"));
    assert!(!page.contains("Notice 4"));
    // only the dismissible ones (1 and 3) have a close button
    assert_eq!(page.matches("data-aegis-notice-close").count(), 2);
    assert_eq!(page.matches("data-aegis-notice=").count(), 3);
}

#[tokio::test]
async fn test_editing_a_notice_gives_it_a_new_version() {
    let site = site(|_| {}).await;
    let id = site.add(json!({ "message": "Version one" })).await.json()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let rev_of = |page: &str| {
        let start =
            page.find("data-aegis-notice-rev=\"").unwrap() + "data-aegis-notice-rev=\"".len();
        page[start..start + page[start..].find('"').unwrap()].to_string()
    };
    let before = rev_of(&site.page().await);
    std::thread::sleep(std::time::Duration::from_millis(5));
    site.send(
        "PUT",
        &format!("/api/v1/notices/{}", id),
        Some(json!({ "message": "Version two" })),
        Some(&site.admin),
    )
    .await;
    let page = site.page().await;
    assert!(page.contains("Version two"));
    assert_ne!(rev_of(&page), before);
}

#[tokio::test]
async fn test_notices_are_added_to_html_pages_only() {
    let site = site(|_| {}).await;
    site.add(json!({ "message": "Hello" })).await;
    for path in ["/api/v1/health", "/feed.xml", "/_aegis/notices.js"] {
        let reply = site.send("GET", path, None, None).await;
        assert!(!reply.body.contains("class=\"aegis-notices\""), "{}", path);
    }
    // the 404 page gets them, like every public page
    let missing = site.send("GET", "/does-not-exist", None, None).await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    assert!(missing.body.contains("Hello"));
}

#[tokio::test]
async fn test_assets_are_served() {
    let site = site(|_| {}).await;
    let script = site.send("GET", "/_aegis/notices.js", None, None).await;
    assert_eq!(script.status, StatusCode::OK);
    assert_eq!(
        script.headers["content-type"],
        "application/javascript; charset=utf-8"
    );
    assert!(script.body.contains("aegis_notices"));
    let style = site.send("GET", "/_aegis/notices.css", None, None).await;
    assert_eq!(style.headers["content-type"], "text/css; charset=utf-8");
    assert!(style.body.contains(".aegis-notice-warning"));
}

#[tokio::test]
async fn test_the_base_path_is_in_the_asset_urls() {
    let site = site(|config| config.server.base_path = "/cms".to_string()).await;
    site.add(json!({ "message": "Hello" })).await;
    let page = site.page().await;
    assert!(
        page.contains("href=\"/cms/_aegis/notices.css?v="),
        "{}",
        page
    );
    assert!(page.contains("src=\"/cms/_aegis/notices.js?v="));
}

#[tokio::test]
async fn test_a_theme_can_replace_the_notices() {
    let dir = tempfile::tempdir().unwrap();
    let templates = dir.path().join("mine").join("templates");
    std::fs::create_dir_all(&templates).unwrap();
    std::fs::write(
        templates.join("notices.html"),
        r#"{% for n in fields.notices %}<aside class="mine-{{ n.style }}" data-aegis-notice="{{ n.id }}" data-aegis-notice-rev="{{ n.rev }}">{{ n.message }}</aside>{% endfor %}"#,
    )
    .unwrap();
    let mut state = crate::test_support::test_app_state(
        AegisConfig::default(),
        PluginSupervisor::new(),
        Vec::new(),
    )
    .await;
    state.theme = Arc::new(aegis_theme::ThemeEngine::new(dir.path(), "mine"));
    let state = Arc::new(state);
    let admin = state
        .sessions
        .create_session("admin1", "admin", Default::default(), None)
        .await
        .unwrap()
        .id;
    let site = Site {
        state,
        admin: admin.clone(),
        editor: admin,
    };
    site.add(json!({ "message": "Custom look", "style": "success" }))
        .await;
    let page = site.page().await;
    assert!(page.contains(r#"class="mine-success""#), "{}", page);
    assert!(page.contains("Custom look"));
    assert!(!page.contains("aegis-notices"));
}

#[tokio::test]
async fn test_a_broken_notices_template_leaves_the_page_working() {
    let dir = tempfile::tempdir().unwrap();
    let templates = dir.path().join("mine").join("templates");
    std::fs::create_dir_all(&templates).unwrap();
    std::fs::write(templates.join("notices.html"), "{% if %} broken").unwrap();
    let mut state = crate::test_support::test_app_state(
        AegisConfig::default(),
        PluginSupervisor::new(),
        Vec::new(),
    )
    .await;
    state.theme = Arc::new(aegis_theme::ThemeEngine::new(dir.path(), "mine"));
    let state = Arc::new(state);
    let admin = state
        .sessions
        .create_session("admin1", "admin", Default::default(), None)
        .await
        .unwrap()
        .id;
    let site = Site {
        state,
        admin: admin.clone(),
        editor: admin,
    };
    site.add(json!({ "message": "Hello" })).await;
    let reply = site.send("GET", "/", None, None).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert!(!reply.body.contains("Hello"));
}

#[tokio::test]
async fn test_notices_are_saved_and_loaded_again_and_changes_are_audited() {
    let site = site(|_| {}).await;
    site.add(json!({ "message": "Persistent" })).await;
    let reloaded = crate::notices::NoticeList::load(site.state.db.as_ref()).await;
    assert_eq!(reloaded.items()[0].message, "Persistent");

    let events = site
        .send(
            "GET",
            "/api/v1/audit-logs?action=notice.create",
            None,
            Some(&site.admin),
        )
        .await;
    assert_eq!(events.status, StatusCode::OK, "{}", events.body);
    assert!(events.body.contains("notice.create"), "{}", events.body);

    // the last one removed leaves no setting behind
    let id = site.list().await["notices"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    site.send(
        "DELETE",
        &format!("/api/v1/notices/{}", id),
        None,
        Some(&site.admin),
    )
    .await;
    let settings = site.state.db.list_settings().await.unwrap();
    assert!(!settings.contains_key(crate::notices::SETTING_KEY));
}
