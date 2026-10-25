use aegis_core::create_test_signature;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use ring::signature;
use serde_json::Value;
use std::io::{Cursor, Write};
use std::path::PathBuf;
use std::sync::Arc;
use tower::ServiceExt;

fn make_multipart_body(
    boundary: &str,
    field_name: &str,
    file_name: &str,
    file_bytes: &[u8],
) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(
        format!(
            "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\nContent-Type: application/zip\r\n\r\n",
            field_name, file_name
        )
        .as_bytes(),
    );
    body.extend_from_slice(file_bytes);
    body.extend_from_slice(format!("\r\n--{}--\r\n", boundary).as_bytes());
    body
}

fn build_plugin_zip(
    name: &str,
    version: &str,
    min_aegis_ver: Option<&str>,
    key_pair: Option<&signature::Ed25519KeyPair>,
) -> Vec<u8> {
    let mut zip_buf = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut zip_buf));
        let options = zip::write::SimpleFileOptions::default();

        let min_ver_str = min_aegis_ver
            .map(|v| format!(", \"min_aegis_version\": \"{}\"", v))
            .unwrap_or_default();
        let manifest_content = format!(
            r#"{{"name": "{}", "version": "{}", "entrypoint": "plugin.wasm"{}}}"#,
            name, version, min_ver_str
        );

        writer
            .start_file(format!("{}/plugin.manifest.json", name), options)
            .unwrap();
        writer.write_all(manifest_content.as_bytes()).unwrap();

        writer
            .start_file(format!("{}/plugin.wasm", name), options)
            .unwrap();
        writer.write_all(b"\0asm\x01\0\0\0").unwrap();

        if let Some(kp) = key_pair {
            let sig = create_test_signature(manifest_content.as_bytes(), kp, "manifest");
            writer
                .start_file(format!("{}/signature.json", name), options)
                .unwrap();
            writer
                .write_all(serde_json::to_string_pretty(&sig).unwrap().as_bytes())
                .unwrap();
        }

        writer.finish().unwrap();
    }
    zip_buf
}

fn build_theme_zip(
    id: &str,
    name: &str,
    version: &str,
    min_aegis_ver: Option<&str>,
    key_pair: Option<&signature::Ed25519KeyPair>,
) -> Vec<u8> {
    let mut zip_buf = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut zip_buf));
        let options = zip::write::SimpleFileOptions::default();

        let min_ver_str = min_aegis_ver
            .map(|v| format!(", \"min_aegis_version\": \"{}\"", v))
            .unwrap_or_default();
        let manifest_content = format!(
            r#"{{"id": "{}", "name": "{}", "version": "{}"{}}}"#,
            id, name, version, min_ver_str
        );

        writer
            .start_file(format!("{}/theme.json", id), options)
            .unwrap();
        writer.write_all(manifest_content.as_bytes()).unwrap();

        writer
            .start_file(format!("{}/templates/base.html", id), options)
            .unwrap();
        writer
            .write_all(b"<html><body>{% block content %}{% endblock %}</body></html>")
            .unwrap();

        writer
            .start_file(format!("{}/assets/style.css", id), options)
            .unwrap();
        writer.write_all(b"body { background: #000; }").unwrap();

        if let Some(kp) = key_pair {
            let sig = create_test_signature(manifest_content.as_bytes(), kp, "manifest");
            writer
                .start_file(format!("{}/signature.json", id), options)
                .unwrap();
            writer
                .write_all(serde_json::to_string_pretty(&sig).unwrap().as_bytes())
                .unwrap();
        }

        writer.finish().unwrap();
    }
    zip_buf
}

async fn test_site() -> (Arc<crate::state::AppState>, String, PathBuf, PathBuf) {
    let test_id = format!("{}_{}", std::process::id(), uuid::Uuid::new_v4());
    let plugins_dir = std::env::temp_dir().join(format!("aegis_srv_plugins_{}", test_id));
    let themes_dir = std::env::temp_dir().join(format!("aegis_srv_themes_{}", test_id));
    let _ = std::fs::remove_dir_all(&plugins_dir);
    let _ = std::fs::remove_dir_all(&themes_dir);
    std::fs::create_dir_all(&plugins_dir).unwrap();
    std::fs::create_dir_all(&themes_dir).unwrap();

    let mut config = crate::config::AegisConfig::default();
    config.paths.plugins_dir = plugins_dir.clone();

    let theme_engine = aegis_theme::ThemeEngine::new(&themes_dir, "default");
    let supervisor = aegis_runtime::PluginSupervisor::new();

    let loader = crate::plugin_registry::PluginLoader::new(
        aegis_runtime::WasmEngine::new().unwrap(),
        Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())).into(),
        plugins_dir.clone(),
        config.runtime.default_resource_limits(),
    );

    let mut state =
        crate::test_support::test_app_state(config.clone(), supervisor, Vec::new()).await;
    state.theme = Arc::new(theme_engine);
    state.plugin_registry = Arc::new(crate::plugin_registry::PluginRegistry::new(
        Some(loader),
        Vec::new(),
    ));

    let admin_session = state
        .sessions
        .create_session("admin1", "admin", Default::default(), None)
        .await
        .unwrap();

    (Arc::new(state), admin_session.id, plugins_dir, themes_dir)
}

#[tokio::test]
async fn test_install_plugin_zip_success_and_incompatible_version() {
    let (state, token, plugins_dir, themes_dir) = test_site().await;
    let router = crate::app::router(&state.config, state.clone());

    // 1. Success case: install a valid plugin zip
    let zip_bytes = build_plugin_zip("matrix-helper", "1.2.0", Some("0.1.0"), None);
    let boundary = "---------------------------974767299852498929531610575";
    let body = make_multipart_body(boundary, "file", "matrix-helper.zip", &zip_bytes);

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/plugins/install")
        .header("authorization", format!("Bearer {}", token))
        .header(
            "content-type",
            format!("multipart/form-data; boundary={}", boundary),
        )
        .body(Body::from(body))
        .unwrap();

    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let val: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(val["name"], "matrix-helper");
    assert_eq!(val["is_installed"], true);
    assert_eq!(val["manifest"]["version"], "1.2.0");

    // Check files actually installed
    let installed_manifest = plugins_dir
        .join("matrix-helper")
        .join("plugin.manifest.json");
    assert!(installed_manifest.is_file());
    assert!(plugins_dir
        .join("matrix-helper")
        .join("plugin.wasm")
        .is_file());

    // 2. Incompatible version rejection
    let bad_ver_zip = build_plugin_zip("future-plugin", "2.0.0", Some("99.0.0"), None);
    let body = make_multipart_body(boundary, "file", "future-plugin.zip", &bad_ver_zip);

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/plugins/install")
        .header("authorization", format!("Bearer {}", token))
        .header(
            "content-type",
            format!("multipart/form-data; boundary={}", boundary),
        )
        .body(Body::from(body))
        .unwrap();

    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let err_val: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(err_val["error"]
        .as_str()
        .unwrap()
        .contains("Package requires Aegis version >="));

    let _ = std::fs::remove_dir_all(&plugins_dir);
    let _ = std::fs::remove_dir_all(&themes_dir);
}

#[tokio::test]
async fn test_install_theme_zip_success_and_validation() {
    let (state, token, plugins_dir, themes_dir) = test_site().await;
    let router = crate::app::router(&state.config, state.clone());

    // 1. Install valid theme ZIP
    let zip_bytes = build_theme_zip("neon-zen", "Neon Zen Theme", "1.0.0", Some("0.1.0"), None);
    let boundary = "---------------------------boundary12345";
    let body = make_multipart_body(boundary, "file", "neon-zen.zip", &zip_bytes);

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/themes/install")
        .header("authorization", format!("Bearer {}", token))
        .header(
            "content-type",
            format!("multipart/form-data; boundary={}", boundary),
        )
        .body(Body::from(body))
        .unwrap();

    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let val: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(val["id"], "neon-zen");
    assert_eq!(val["name"], "Neon Zen Theme");
    assert_eq!(val["has_templates"], true);
    assert_eq!(val["has_assets"], true);

    // Verify files on disk
    let theme_manifest = themes_dir.join("neon-zen").join("theme.json");
    assert!(theme_manifest.is_file());
    assert!(themes_dir
        .join("neon-zen")
        .join("templates/base.html")
        .is_file());

    // 2. Reject incompatible min_aegis_version
    let future_theme = build_theme_zip("quantum", "Quantum Theme", "1.0.0", Some("99.9.9"), None);
    let body = make_multipart_body(boundary, "file", "quantum.zip", &future_theme);

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/themes/install")
        .header("authorization", format!("Bearer {}", token))
        .header(
            "content-type",
            format!("multipart/form-data; boundary={}", boundary),
        )
        .body(Body::from(body))
        .unwrap();

    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let _ = std::fs::remove_dir_all(&plugins_dir);
    let _ = std::fs::remove_dir_all(&themes_dir);
}

#[tokio::test]
async fn test_install_signed_plugin() {
    let (state, token, plugins_dir, themes_dir) = test_site().await;
    let router = crate::app::router(&state.config, state.clone());

    let rng = ring::rand::SystemRandom::new();
    let pkcs8 = signature::Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
    let key_pair = signature::Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();

    let signed_zip = build_plugin_zip("signed-plugin", "1.0.0", None, Some(&key_pair));
    let boundary = "---------------------------boundary_sig";
    let body = make_multipart_body(boundary, "file", "signed.zip", &signed_zip);

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/plugins/install")
        .header("authorization", format!("Bearer {}", token))
        .header(
            "content-type",
            format!("multipart/form-data; boundary={}", boundary),
        )
        .body(Body::from(body))
        .unwrap();

    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let val: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(val["name"], "signed-plugin");

    let _ = std::fs::remove_dir_all(&plugins_dir);
    let _ = std::fs::remove_dir_all(&themes_dir);
}
