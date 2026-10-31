//! リダイレクトの API と、404 のときの転送、slug を変えたときの自動の規則の結合テスト（実際のルーターを通す）

use std::sync::Arc;

use aegis_db::ContentRecord;
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

    fn location(&self) -> &str {
        self.headers
            .get("location")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
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

    async fn get(&self, path: &str) -> Reply {
        self.send("GET", path, None, None).await
    }

    async fn add(&self, from: &str, to: &str, status: u16) -> Reply {
        self.send(
            "POST",
            "/api/v1/redirects",
            Some(json!({ "from": from, "to": to, "status": status })),
            Some(&self.admin),
        )
        .await
    }

    async fn list(&self) -> Value {
        self.send("GET", "/api/v1/redirects", None, Some(&self.admin))
            .await
            .json()
    }

    async fn publish_article(&self, slug: &str, status: &str) {
        let schema = self.state.schemas.read().await["article"].clone();
        self.state
            .db
            .insert_content(
                &schema,
                &ContentRecord {
                    id: format!("id-{}", slug),
                    content_type: "article".to_string(),
                    title: "Hello".to_string(),
                    slug: slug.to_string(),
                    status: status.to_string(),
                    blocks_json: json!([{ "name": "core/paragraph", "text": "Body" }]).to_string(),
                    author_id: None,
                    published_at: Some("2026-10-01T00:00:00Z".to_string()),
                    unpublish_at: None,
                    created_at: "2026-10-01T00:00:00Z".to_string(),
                    updated_at: "2026-10-01T00:00:00Z".to_string(),
                    custom_fields: Default::default(),
                },
            )
            .await
            .unwrap();
    }

    /// Saves the article under `to_slug`. The old URL keeps serving the published version until a
    /// save with `publish` puts the new version live
    async fn rename(&self, from_slug: &str, to_slug: &str, status: &str, publish: bool) -> Reply {
        self.send(
            "PUT",
            &format!("/api/v1/contents/article/{}", from_slug),
            Some(json!({
                "title": "Hello",
                "slug": to_slug,
                "status": status,
                "publish": publish,
                "blocks": [{ "name": "core/paragraph", "text": "Body" }]
            })),
            Some(&self.editor),
        )
        .await
    }
}

#[tokio::test]
async fn test_api_manages_rules_and_refuses_bad_ones() {
    let site = site(|_| {}).await;

    // admin only
    let denied = site
        .send("GET", "/api/v1/redirects", None, Some(&site.editor))
        .await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN);
    assert_eq!(
        site.get("/api/v1/redirects").await.status,
        StatusCode::UNAUTHORIZED
    );

    let created = site.add("/old/", "/new", 301).await;
    assert_eq!(created.status, StatusCode::CREATED);
    let rule = created.json();
    assert_eq!(rule["from"], "/old");
    assert_eq!(rule["status"], 301);
    assert_eq!(rule["auto"], false);
    let id = rule["id"].as_str().unwrap().to_string();

    // refused: duplicate, loop, reserved path, bad target, bad status
    let duplicate = site.add("/old", "/other", 301).await;
    assert_eq!(duplicate.status, StatusCode::CONFLICT);
    assert_eq!(duplicate.json()["code"], "duplicate_redirect");
    let looping = site.add("/new", "/old", 301).await;
    assert_eq!(looping.status, StatusCode::BAD_REQUEST);
    assert_eq!(looping.json()["code"], "redirect_loop");
    for (from, to, status, field) in [
        ("/api/v1/contents", "/x", 301, "from"),
        ("/admin/page", "/x", 301, "from"),
        ("/?a=1", "/x", 301, "from"),
        ("/x", "//evil.example", 301, "to"),
        ("/x", "javascript:alert(1)", 301, "to"),
        ("/x", "/y", 307, "status"),
    ] {
        let reply = site.add(from, to, status).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{} -> {}", from, to);
        assert_eq!(reply.json()["code"], "invalid_redirect");
        assert_eq!(reply.json()["field"], field, "{} -> {}", from, to);
    }

    // change and list (search by either side)
    let updated = site
        .send(
            "PUT",
            &format!("/api/v1/redirects/{}", id),
            Some(json!({ "from": "/old", "to": "https://example.com/new", "status": 302 })),
            Some(&site.admin),
        )
        .await;
    assert_eq!(updated.status, StatusCode::OK);
    assert_eq!(updated.json()["status"], 302);
    site.add("/another", "/somewhere", 301).await;
    let all = site.list().await;
    assert_eq!(all["total"], 2);
    let found = site
        .send(
            "GET",
            "/api/v1/redirects?q=EXAMPLE.com",
            None,
            Some(&site.admin),
        )
        .await
        .json();
    assert_eq!(found["total"], 1);
    assert_eq!(found["redirects"][0]["from"], "/old");
    assert_eq!(
        site.send(
            "PUT",
            "/api/v1/redirects/missing",
            Some(json!({"from": "/a", "to": "/b"})),
            Some(&site.admin)
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );

    // delete
    let deleted = site
        .send(
            "DELETE",
            &format!("/api/v1/redirects/{}", id),
            None,
            Some(&site.admin),
        )
        .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    assert_eq!(site.list().await["total"], 1);
    assert_eq!(
        site.send(
            "DELETE",
            &format!("/api/v1/redirects/{}", id),
            None,
            Some(&site.admin)
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn test_missing_pages_are_redirected_but_live_pages_are_not() {
    let site = site(|config| config.server.base_path = "/cms".to_string()).await;
    site.publish_article("hello", "published").await;
    site.add("/old-page", "/articles/hello", 301).await;
    site.add("/temp", "/articles/hello", 302).await;
    site.add("/away", "https://example.com/new?from=cms#top", 301)
        .await;
    site.add("/記事", "/articles/hello", 301).await;
    // A rule for a URL that serves a page does nothing
    site.add("/articles/hello", "/elsewhere", 301).await;

    // The router sees paths with the base path already stripped; `Location` gets it back
    let moved = site.get("/old-page").await;
    assert_eq!(moved.status, StatusCode::MOVED_PERMANENTLY);
    assert_eq!(moved.location(), "/cms/articles/hello");
    assert_eq!(moved.headers["cache-control"], "public, max-age=3600");
    // trailing slash and query string
    let with_query = site.get("/old-page/?utm_source=x&a=1").await;
    assert_eq!(with_query.status, StatusCode::MOVED_PERMANENTLY);
    assert_eq!(
        with_query.location(),
        "/cms/articles/hello?utm_source=x&a=1"
    );

    let temporary = site.get("/temp").await;
    assert_eq!(temporary.status, StatusCode::FOUND);
    assert_eq!(temporary.headers["cache-control"], "no-cache");

    // External targets are used as they are; their own query wins, the fragment stays last
    let external = site.get("/away").await;
    assert_eq!(external.location(), "https://example.com/new?from=cms#top");
    let keeps_fragment = site.add("/frag", "/articles/hello#comments", 301).await;
    assert_eq!(keeps_fragment.status, StatusCode::CREATED);
    assert_eq!(
        site.get("/frag?x=1").await.location(),
        "/cms/articles/hello?x=1#comments"
    );

    // Percent-encoded request, non-ASCII rule
    assert_eq!(
        site.get("/%E8%A8%98%E4%BA%8B").await.status,
        StatusCode::MOVED_PERMANENTLY
    );

    // The live article stays; unknown paths stay 404; other methods are not redirected
    assert_eq!(site.get("/articles/hello").await.status, StatusCode::OK);
    assert_eq!(
        site.get("/nothing-here").await.status,
        StatusCode::NOT_FOUND
    );
    let post = site.send("POST", "/old-page", None, None).await;
    assert_ne!(post.status, StatusCode::MOVED_PERMANENTLY);

    // After deleting the rule the old URL is a plain 404 again
    let id = site.list().await["redirects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["from"] == "/old-page")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    site.send(
        "DELETE",
        &format!("/api/v1/redirects/{}", id),
        None,
        Some(&site.admin),
    )
    .await;
    assert_eq!(site.get("/old-page").await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_publishing_a_new_slug_keeps_redirects_without_chains() {
    let site = site(|_| {}).await;
    site.publish_article("hello", "published").await;

    // Saved but not published again: the old URL is still the live one, so nothing is redirected yet
    let saved = site.rename("hello", "hello-2", "published", false).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.body);
    assert_eq!(site.get("/articles/hello").await.status, StatusCode::OK);
    assert_eq!(site.list().await["total"], 0);

    // Publishing the new version moves the URL: the old one is redirected
    let published = site.rename("hello-2", "hello-2", "published", true).await;
    assert_eq!(published.status, StatusCode::OK, "{}", published.body);
    let old_article = site.get("/articles/hello").await;
    assert_eq!(old_article.status, StatusCode::MOVED_PERMANENTLY);
    assert_eq!(old_article.location(), "/articles/hello-2");
    assert_eq!(site.get("/hello").await.location(), "/hello-2");
    assert_eq!(site.get("/articles/hello-2").await.status, StatusCode::OK);
    let rules = site.list().await;
    assert_eq!(rules["total"], 2);
    assert!(rules["redirects"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["auto"] == true));

    // Renaming again points the first URL at the newest one (no chain)
    site.rename("hello-2", "hello-3", "published", true).await;
    assert_eq!(
        site.get("/articles/hello").await.location(),
        "/articles/hello-3"
    );
    assert_eq!(
        site.get("/articles/hello-2").await.location(),
        "/articles/hello-3"
    );
    assert_eq!(site.list().await["total"], 4);

    // Going back to the first slug: its URL is live again, so the rule that left it is gone
    site.rename("hello-3", "hello", "published", true).await;
    assert_eq!(site.get("/articles/hello").await.status, StatusCode::OK);
    assert_eq!(
        site.get("/articles/hello-3").await.location(),
        "/articles/hello"
    );
    assert_eq!(
        site.get("/articles/hello-2").await.location(),
        "/articles/hello"
    );
}

#[tokio::test]
async fn test_unpublished_content_leaves_no_redirect() {
    let site = site(|_| {}).await;
    site.publish_article("draft-post", "draft").await;
    let renamed = site
        .rename("draft-post", "draft-post-2", "draft", false)
        .await;
    assert_eq!(renamed.status, StatusCode::OK, "{}", renamed.body);
    assert_eq!(site.list().await["total"], 0);

    // Taking a published article down while renaming it: the new URL would be a 404 too
    site.publish_article("live", "published").await;
    site.rename("live", "live-2", "draft", false).await;
    assert_eq!(site.list().await["total"], 0);
}

#[tokio::test]
async fn test_rules_are_saved_and_loaded_again() {
    let site = site(|_| {}).await;
    site.add("/old", "/new", 301).await;
    let reloaded = crate::redirects::RedirectTable::load(site.state.db.as_ref()).await;
    assert_eq!(reloaded.resolve("/old").unwrap().to, "/new");

    // The last rule removed leaves no setting behind
    let id = site.list().await["redirects"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    site.send(
        "DELETE",
        &format!("/api/v1/redirects/{}", id),
        None,
        Some(&site.admin),
    )
    .await;
    let settings = site.state.db.list_settings().await.unwrap();
    assert!(!settings.contains_key(crate::redirects::SETTING_KEY));
}

#[tokio::test]
async fn test_a_scheduled_publication_with_a_new_slug_keeps_a_redirect() {
    let site = site(|_| {}).await;
    site.publish_article("hello", "published").await;
    let due = chrono::Utc::now() + chrono::Duration::hours(1);
    let scheduled = site
        .send(
            "PUT",
            "/api/v1/contents/article/hello",
            Some(json!({
                "title": "Hello",
                "slug": "hello-later",
                "status": "scheduled",
                "published_at": due.to_rfc3339(),
                "blocks": [{ "name": "core/paragraph", "text": "Body" }]
            })),
            Some(&site.editor),
        )
        .await;
    assert_eq!(scheduled.status, StatusCode::OK, "{}", scheduled.body);
    // Until the time comes, the old URL is the live one
    assert_eq!(site.get("/articles/hello").await.status, StatusCode::OK);
    assert_eq!(site.list().await["total"], 0);

    let published =
        crate::scheduler::publish_due_contents(&site.state, due + chrono::Duration::minutes(1))
            .await;
    assert_eq!(published, 1);
    assert_eq!(
        site.get("/articles/hello-later").await.status,
        StatusCode::OK
    );
    let old = site.get("/articles/hello").await;
    assert_eq!(old.status, StatusCode::MOVED_PERMANENTLY);
    assert_eq!(old.location(), "/articles/hello-later");
}
