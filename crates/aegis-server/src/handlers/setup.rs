use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::setup::{is_initialized, run_init, InitOptions};
use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub struct SetupStatusResponse {
    pub initialized: bool,
    pub requires_setup: bool,
}

#[derive(Debug, Deserialize)]
pub struct SetupRequest {
    #[serde(default = "default_site_title")]
    pub site_title: String,
    pub admin_username: String,
    pub admin_password: String,
    pub admin_email: Option<String>,
    pub admin_display_name: Option<String>,
    #[serde(default = "default_sample_data")]
    pub create_sample_data: bool,
}

fn default_site_title() -> String {
    "AegisCMS".to_string()
}

fn default_sample_data() -> bool {
    true
}

/// `GET /api/v1/setup/status`
/// Check whether AegisCMS is initialized or requires initial setup.
pub async fn setup_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match is_initialized(state.db.as_ref()).await {
        Ok(initialized) => (
            StatusCode::OK,
            Json(json!(SetupStatusResponse {
                initialized,
                requires_setup: !initialized,
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "code": "STATUS_CHECK_FAILED",
                "error": format!("Failed to check setup status: {}", e),
            })),
        ),
    }
}

/// `POST /api/v1/setup`
/// Initialize AegisCMS with the first administrator account and initial data.
/// Returns 403 Forbidden if user accounts already exist.
pub async fn setup_init(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SetupRequest>,
) -> impl IntoResponse {
    // 1. Guard: reject if system is already initialized
    match is_initialized(state.db.as_ref()).await {
        Ok(true) => {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({
                    "code": "ALREADY_INITIALIZED",
                    "error": "System is already initialized: user accounts already exist",
                })),
            );
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "code": "STATUS_CHECK_FAILED",
                    "error": format!("Failed to verify system state: {}", e),
                })),
            );
        }
        Ok(false) => {}
    }

    // 2. Validate input
    let username = payload.admin_username.trim();
    if username.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "code": "VALIDATION_FAILED",
                "error": "Admin username cannot be empty",
            })),
        );
    }
    if payload.admin_password.len() < 8 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "code": "VALIDATION_FAILED",
                "error": "Admin password must be at least 8 characters long",
            })),
        );
    }

    let opts = InitOptions {
        site_title: payload.site_title,
        admin_username: username.to_string(),
        admin_password: payload.admin_password,
        admin_email: payload.admin_email,
        admin_display_name: payload.admin_display_name,
        create_sample_data: payload.create_sample_data,
    };

    // 3. Execute setup
    match run_init(&state.config, opts).await {
        Ok(result) => (
            StatusCode::CREATED,
            Json(json!({
                "success": true,
                "admin_user_id": result.admin_user_id,
                "admin_username": result.admin_username,
                "site_title": result.site_title,
                "sample_article_id": result.sample_article_id,
                "sample_page_id": result.sample_page_id,
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "code": "SETUP_FAILED",
                "error": format!("Initialization failed: {}", e),
            })),
        ),
    }
}

/// `GET /setup`
/// Standalone Web Installer interface.
/// Redirects to /admin/ if already initialized.
pub async fn setup_page(State(state): State<Arc<AppState>>) -> Response {
    if let Ok(true) = is_initialized(state.db.as_ref()).await {
        let admin_path = state.config.server.admin_path();
        return axum::response::Redirect::to(&format!("{}/", admin_path)).into_response();
    }

    let html = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>AegisCMS - Initial Setup</title>
  <style>
    .no-margin { margin-bottom: 0; }
    :root {
      --bg: #0f172a;
      --card-bg: #1e293b;
      --text: #f8fafc;
      --muted: #94a3b8;
      --primary: #38bdf8;
      --primary-hover: #0ea5e9;
      --border: #334155;
      --error: #f43f5e;
    }
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body {
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
      background-color: var(--bg);
      color: var(--text);
      display: flex;
      justify-content: center;
      align-items: center;
      min-height: 100vh;
      padding: 1.5rem;
    }
    .card {
      background: var(--card-bg);
      border: 1px solid var(--border);
      border-radius: 12px;
      padding: 2.5rem;
      width: 100%;
      max-width: 480px;
      box-shadow: 0 10px 25px -5px rgba(0, 0, 0, 0.4);
    }
    .logo {
      display: flex;
      align-items: center;
      gap: 0.75rem;
      font-size: 1.5rem;
      font-weight: 700;
      color: var(--primary);
      margin-bottom: 0.5rem;
    }
    .subtitle {
      color: var(--muted);
      font-size: 0.95rem;
      margin-bottom: 2rem;
    }
    .field {
      margin-bottom: 1.25rem;
    }
    label {
      display: block;
      font-size: 0.875rem;
      font-weight: 500;
      margin-bottom: 0.5rem;
    }
    input[type="text"], input[type="password"], input[type="email"] {
      width: 100%;
      padding: 0.75rem 1rem;
      background: #0f172a;
      border: 1px solid var(--border);
      border-radius: 6px;
      color: var(--text);
      font-size: 1rem;
      outline: none;
      transition: border-color 0.2s;
    }
    input:focus {
      border-color: var(--primary);
    }
    .checkbox-field {
      display: flex;
      align-items: center;
      gap: 0.5rem;
      margin-top: 1rem;
      margin-bottom: 1.5rem;
    }
    .checkbox-field input {
      accent-color: var(--primary);
      width: 1.1rem;
      height: 1.1rem;
    }
    button {
      width: 100%;
      padding: 0.875rem;
      background: var(--primary);
      color: #0f172a;
      border: none;
      border-radius: 6px;
      font-size: 1rem;
      font-weight: 600;
      cursor: pointer;
      transition: background 0.2s;
    }
    button:hover {
      background: var(--primary-hover);
    }
    button:disabled {
      opacity: 0.6;
      cursor: not-allowed;
    }
    .error-box {
      background: rgba(244, 63, 94, 0.15);
      border: 1px solid var(--error);
      color: var(--error);
      padding: 0.75rem 1rem;
      border-radius: 6px;
      margin-bottom: 1.25rem;
      display: none;
      font-size: 0.9rem;
    }
  </style>
</head>
<body>
  <div class="card">
    <div class="logo">
      <span>🛡️ AegisCMS</span>
    </div>
    <p class="subtitle">Complete the setup to start managing your website.</p>

    <div id="error" class="error-box"></div>

    <form id="setup-form">
      <div class="field">
        <label for="site_title">Site Title</label>
        <input type="text" id="site_title" value="AegisCMS" required>
      </div>
      <div class="field">
        <label for="admin_username">Admin Username</label>
        <input type="text" id="admin_username" value="admin" required>
      </div>
      <div class="field">
        <label for="admin_password">Admin Password (min. 8 characters)</label>
        <input type="password" id="admin_password" required minlength="8">
      </div>
      <div class="field">
        <label for="admin_email">Admin Email (optional)</label>
        <input type="email" id="admin_email" placeholder="admin@example.com">
      </div>
      <div class="checkbox-field">
        <input type="checkbox" id="sample_data" checked>
        <label for="sample_data" class="no-margin">Create sample content (welcome article & about page)</label>
      </div>
      <button type="submit" id="submit-btn">Complete Installation</button>
    </form>
  </div>

  <script src="/_aegis/setup.js" defer></script>
</body>
</html>"#;

    // Strict CSP: このページの `<style>` のハッシュを CSP に足す
    aegis_core::csp::record_styles_in(html);
    Html(html).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AegisConfig;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use std::collections::HashMap;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_setup_endpoints_and_guard() {
        let temp_dir = tempfile::tempdir().unwrap();
        let schemas_dir = temp_dir.path().join("schemas");
        tokio::fs::create_dir_all(&schemas_dir).await.unwrap();

        let article_schema = r#"
content_type: article
label: "Article"
fields:
  - name: excerpt
    type: string
"#;
        tokio::fs::write(schemas_dir.join("article.yaml"), article_schema)
            .await
            .unwrap();

        let db_file = temp_dir.path().join("test_setup_api.db");
        let mut config = AegisConfig::default();
        config.database.url = format!("sqlite://{}?mode=rwc", db_file.to_str().unwrap());
        config.paths.schemas_dir = schemas_dir;

        let db_adapter = aegis_db::create_database_adapter(&config.database)
            .await
            .unwrap();
        let schemas_vec = aegis_schema::SchemaLoader::load_dir(&config.paths.schemas_dir).unwrap();
        let schemas_map: HashMap<String, _> = schemas_vec
            .into_iter()
            .map(|s| (s.content_type.clone(), s))
            .collect();
        db_adapter.run_migrations(&schemas_map).await.unwrap();

        let state = Arc::new(
            crate::test_support::test_app_state_with_db(
                db_adapter,
                config.clone(),
                aegis_runtime::PluginSupervisor::new(),
                Vec::new(),
            )
            .await,
        );

        // 1. Check GET /api/v1/setup/status when no users exist
        let req = Request::get("/api/v1/setup/status")
            .body(Body::empty())
            .unwrap();
        let res = crate::app::router(&config, state.clone())
            .oneshot(req)
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1 << 16)
            .await
            .unwrap();
        let status_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(status_json["initialized"], false);
        assert_eq!(status_json["requires_setup"], true);

        // 2. Check GET /setup page
        let req = Request::get("/setup").body(Body::empty()).unwrap();
        let res = crate::app::router(&config, state.clone())
            .oneshot(req)
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        // The page has no inline script (CSP `script-src` needs no `'unsafe-inline'`); the script is a file
        let page = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let page = String::from_utf8_lossy(&page);
        assert!(
            page.contains(r#"<script src="/_cms/setup.js" defer></script>"#),
            "{page}"
        );
        assert!(!page.contains("<script>"), "{page}");

        // 3. Validation error: password too short
        let invalid_req = serde_json::json!({
            "site_title": "Test Site",
            "admin_username": "admin",
            "admin_password": "short"
        });
        let req = Request::post("/api/v1/setup")
            .header("content-type", "application/json")
            .body(Body::from(invalid_req.to_string()))
            .unwrap();
        let res = crate::app::router(&config, state.clone())
            .oneshot(req)
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);

        // 4. Successful setup
        let setup_payload = serde_json::json!({
            "site_title": "My Web Setup Site",
            "admin_username": "webadmin",
            "admin_password": "strong_password_123",
            "admin_email": "admin@example.com",
            "create_sample_data": true
        });
        let req = Request::post("/api/v1/setup")
            .header("content-type", "application/json")
            .body(Body::from(setup_payload.to_string()))
            .unwrap();
        let res = crate::app::router(&config, state.clone())
            .oneshot(req)
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let body = axum::body::to_bytes(res.into_body(), 1 << 16)
            .await
            .unwrap();
        let resp_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(resp_json["success"], true);
        assert_eq!(resp_json["admin_username"], "webadmin");

        // 5. Check GET /api/v1/setup/status now returns initialized: true
        let req = Request::get("/api/v1/setup/status")
            .body(Body::empty())
            .unwrap();
        let res = crate::app::router(&config, state.clone())
            .oneshot(req)
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1 << 16)
            .await
            .unwrap();
        let status_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(status_json["initialized"], true);
        assert_eq!(status_json["requires_setup"], false);

        // 6. Guard check: second POST /api/v1/setup must return 403 Forbidden
        let req = Request::post("/api/v1/setup")
            .header("content-type", "application/json")
            .body(Body::from(setup_payload.to_string()))
            .unwrap();
        let res = crate::app::router(&config, state.clone())
            .oneshot(req)
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
        let body = axum::body::to_bytes(res.into_body(), 1 << 16)
            .await
            .unwrap();
        let err_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(err_json["code"], "ALREADY_INITIALIZED");
    }
}
