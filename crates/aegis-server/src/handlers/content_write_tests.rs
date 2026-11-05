//! コンテンツの書き込み API の形（カスタムフィールドは `fields` の中）の結合テスト（実際のルーターを通す）

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
    editor: String,
}

async fn site() -> Site {
    let state = Arc::new(
        crate::test_support::test_app_state(
            AegisConfig::default(),
            PluginSupervisor::new(),
            Vec::new(),
        )
        .await,
    );
    let editor = state
        .sessions
        .create_session("editor1", "editor", Default::default(), None)
        .await
        .unwrap()
        .id;
    Site { state, editor }
}

impl Site {
    async fn send(&self, method: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header("authorization", format!("Bearer {}", self.editor));
        let body = match body {
            Some(value) => {
                request = request.header("content-type", "application/json");
                Body::from(value.to_string())
            }
            None => Body::empty(),
        };
        let response = crate::app::router(&self.state.config, self.state.clone())
            .oneshot(request.body(body).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let value = serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).to_string()));
        (status, value)
    }
}

#[tokio::test]
async fn test_custom_fields_are_written_and_read_under_fields() {
    let site = site().await;
    let (status, created) = site
        .send(
            "POST",
            "/api/v1/contents/page",
            Some(json!({
                "title": "Nested",
                "slug": "nested",
                "status": "draft",
                "blocks": [],
                "fields": {
                    "layout": "blank",
                    "custom_meta": { "seo": { "k": 1 } },
                    // スキーマに無いフィールドは保存しない
                    "not_in_the_schema": "dropped"
                }
            })),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(created["fields"]["layout"], "blank");
    assert_eq!(
        created["fields"]["custom_meta"],
        json!({ "seo": { "k": 1 } })
    );
    assert!(created["fields"].get("not_in_the_schema").is_none());
    // 書き込んだ形と読み取った形が同じ（応答にトップレベルのカスタムフィールドは無い）
    assert!(created.get("layout").is_none());

    let (status, read) = site
        .send(
            "GET",
            &format!("/api/v1/contents/page/{}", created["id"].as_str().unwrap()),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(read["fields"], created["fields"]);

    // PUT も同じ形。送らなかったフィールドは空になる（全体の置き換え）
    let (status, updated) = site
        .send(
            "PUT",
            &format!("/api/v1/contents/page/{}", created["id"].as_str().unwrap()),
            Some(json!({
                "title": "Nested",
                "slug": "nested",
                "blocks": [],
                "fields": { "layout": "default" }
            })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["fields"]["layout"], "default");
    assert!(updated["fields"].get("custom_meta").is_none());
}

#[tokio::test]
async fn test_the_old_flat_form_is_refused_instead_of_dropped() {
    let site = site().await;
    // 以前の形（カスタムフィールドをトップレベルに書く）は、黙って捨てずに拒否する
    let (status, body) = site
        .send(
            "POST",
            "/api/v1/contents/article",
            Some(json!({
                "title": "Flat",
                "slug": "flat",
                "status": "draft",
                "blocks": [],
                "layout": "blank"
            })),
        )
        .await;
    assert!(status.is_client_error(), "{status} {body}");
    assert!(
        body.to_string().contains("unknown field `layout`"),
        "the error names the field: {body}"
    );
    let (_, listed) = site
        .send("GET", "/api/v1/contents/article?status=draft", None)
        .await;
    assert!(
        !listed.to_string().contains("\"slug\":\"flat\""),
        "nothing was created: {listed}"
    );
}
