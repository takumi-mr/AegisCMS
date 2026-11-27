//! カスタムウィジェット（`widgets/<name>/`）の配信と、本文ブロック `core/widget` の HTML 描画。
//!
//! 運営者が React / Vue などでビルドしたミニアプリ（`index.html` + `assets/*.js`, `*.css` 等）を
//! `[paths].widgets_dir`（既定 `widgets/`）配下に配置すると、Admin Studio の記事エディタから
//! `core/widget` ブロックとして選択・埋め込みできる。
//!
//! - 公開ページでは `<iframe sandbox="allow-scripts allow-forms" ...>` に隔離してマウントする
//!   （`allow-same-origin` を付けないため、親ページの DOM や管理者セッションには触れられない）
//! - 配信エンドポイント `/widgets/{name}/{*path}` はパストラバーサルとシンボリックリンク脱出を防止する

use std::path::Path as FilePath;
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Serialize;

use crate::auth::authorize::RequireRole;
use crate::auth::ViewerSession;
use crate::state::AppState;

/// 既定の iframe の高さ（px）
pub const DEFAULT_WIDGET_HEIGHT: u32 = 400;
/// 許可する高さの下限・上限（px）
const MIN_WIDGET_HEIGHT: u32 = 100;
const MAX_WIDGET_HEIGHT: u32 = 3000;

/// Admin Studio 向けに返す利用可能ウィジェットの情報
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WidgetSummary {
    pub name: String,
}

/// ウィジェット名として安全か（ASCII 英数字・ハイフン・アンダースコアのみ、1〜64 文字）
pub fn is_valid_widget_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// `widgets_dir` 内で `index.html` を持つウィジェット名の一覧（昇順）
pub fn discover_widgets(widgets_dir: &FilePath) -> Vec<WidgetSummary> {
    let Ok(entries) = std::fs::read_dir(widgets_dir) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if is_valid_widget_name(&name) && path.join("index.html").is_file() {
            names.push(name);
        }
    }
    names.sort();
    names
        .into_iter()
        .map(|name| WidgetSummary { name })
        .collect()
}

/// `GET /api/v1/widgets` — エディタの選択肢として、配置済みのウィジェット一覧を返す
pub async fn list_widgets(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): ViewerSession,
) -> Json<Vec<WidgetSummary>> {
    Json(discover_widgets(&state.config.paths.widgets_dir))
}

/// `GET /widgets/{name}` / `GET /widgets/{name}/` — ウィジェットの `index.html` を配信
pub async fn get_widget_index(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> axum::response::Response {
    serve_widget_file(&state, &name, "index.html")
}

/// `GET /widgets/{name}/{*path}` — ウィジェット配下の静的ファイル（JS / CSS / 画像等）を配信
pub async fn get_widget_asset(
    State(state): State<Arc<AppState>>,
    Path((name, rel_path)): Path<(String, String)>,
) -> axum::response::Response {
    let trimmed = rel_path.trim_start_matches('/');
    let effective = if trimmed.is_empty() {
        "index.html"
    } else {
        trimmed
    };
    serve_widget_file(&state, &name, effective)
}

fn serve_widget_file(state: &AppState, name: &str, rel_path: &str) -> axum::response::Response {
    if !is_valid_widget_name(name) {
        return (StatusCode::BAD_REQUEST, "Invalid widget name").into_response();
    }

    let clean_path = FilePath::new(rel_path);
    for comp in clean_path.components() {
        if matches!(
            comp,
            std::path::Component::ParentDir
                | std::path::Component::Prefix(_)
                | std::path::Component::RootDir
        ) {
            return (StatusCode::BAD_REQUEST, "Invalid asset path").into_response();
        }
    }

    let widget_dir = state.config.paths.widgets_dir.join(name);
    let Ok(canonical_widget_dir) = std::fs::canonicalize(&widget_dir) else {
        return (StatusCode::NOT_FOUND, "Widget not found").into_response();
    };
    let target = widget_dir.join(clean_path);
    let Ok(canonical_target) = std::fs::canonicalize(&target) else {
        return (StatusCode::NOT_FOUND, "Widget asset not found").into_response();
    };
    // シンボリックリンクによる widgets/<name>/ 外への脱出を防止
    if !canonical_target.starts_with(&canonical_widget_dir) || !canonical_target.is_file() {
        return (StatusCode::NOT_FOUND, "Widget asset not found").into_response();
    }

    match std::fs::read(&canonical_target) {
        Ok(bytes) => {
            let mime = aegis_core::guess_content_type(
                canonical_target
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or(""),
            );
            let resp = (
                [
                    (header::CONTENT_TYPE, mime),
                    (header::CACHE_CONTROL, "public, max-age=3600"),
                    (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
                ],
                bytes,
            )
                .into_response();
            let mut tags = aegis_cache::CacheTags::new();
            tags.add_url(&format!("/widgets/{}/{}", name, rel_path));
            tags.add_tag("site-global");
            tags.add_dependency("widget", name);
            crate::cdn::apply_cache_tags(state, resp, &tags)
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read widget").into_response(),
    }
}

/// URL フラグメント用のパーセントエンコード（安全な ASCII 文字だけ残す）
fn encode_fragment(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for b in raw.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

/// 本文のブロック `core/widget`（`{"name": "...", "height": 400, "title": "...", "params": "..."}`）の HTML。
/// 名前が不正、または `widgets/<name>/index.html` が存在しない場合は空文字列を返す。
pub fn block_html(state: &AppState, attributes: &serde_json::Value) -> String {
    let name = attributes
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or("");
    if !is_valid_widget_name(name) {
        return String::new();
    }
    if !state
        .config
        .paths
        .widgets_dir
        .join(name)
        .join("index.html")
        .is_file()
    {
        return String::new();
    }

    let height = attributes
        .get("height")
        .and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_str().and_then(|s| s.trim().parse::<u64>().ok()))
        })
        .map(|h| (h as u32).clamp(MIN_WIDGET_HEIGHT, MAX_WIDGET_HEIGHT))
        .unwrap_or(DEFAULT_WIDGET_HEIGHT);

    let title = attributes
        .get("title")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(name);

    let params = attributes
        .get("params")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let base = state
        .config
        .server
        .normalized_base_path()
        .unwrap_or_default();
    let mut src = format!("{base}/widgets/{name}/index.html");
    if let Some(p) = params {
        src.push('#');
        src.push_str(&encode_fragment(p));
    }

    format!(
        "<div class=\"aegis-widget\" data-widget=\"{}\"><iframe class=\"aegis-widget-frame\" src=\"{}\" title=\"{}\" width=\"100%\" height=\"{}\" sandbox=\"allow-scripts allow-forms\" loading=\"lazy\" referrerpolicy=\"no-referrer\"></iframe></div>\n",
        aegis_ast::renderer::escape_html(name),
        aegis_ast::renderer::escape_html(&src),
        aegis_ast::renderer::escape_html(title),
        height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AegisConfig;
    use crate::test_support::test_state_with_config;
    use aegis_auth::SessionData;
    use aegis_runtime::PluginSupervisor;
    use serde_json::json;

    fn viewer() -> ViewerSession {
        RequireRole(SessionData {
            id: "ses_viewer".to_string(),
            user_id: "viewer1".to_string(),
            role: "viewer".to_string(),
            created_at: String::new(),
            expires_at: String::new(),
            metadata: Default::default(),
        })
    }

    #[tokio::test]
    async fn test_widget_discovery_serving_and_block_rendering() {
        let dir = std::env::temp_dir().join(format!("aegis_widget_test_{}", uuid::Uuid::new_v4()));
        let sim_dir = dir.join("price-sim");
        let sim_assets = sim_dir.join("assets");
        std::fs::create_dir_all(&sim_assets).unwrap();
        std::fs::write(
            sim_dir.join("index.html"),
            "<!doctype html><div id=app></div>",
        )
        .unwrap();
        std::fs::write(sim_assets.join("app.js"), "console.log('widget');").unwrap();
        // index.html が無いディレクトリは無視される
        std::fs::create_dir_all(dir.join("incomplete")).unwrap();
        std::fs::write(dir.join("secret.txt"), "secret").unwrap();

        let mut config = AegisConfig::default();
        config.paths.widgets_dir = dir.clone();
        let state = test_state_with_config(config, PluginSupervisor::new()).await;

        // 1. 一覧 API は index.html を持つウィジェットだけを返す
        let Json(widgets) = list_widgets(State(state.clone()), viewer()).await;
        assert_eq!(
            widgets,
            vec![WidgetSummary {
                name: "price-sim".to_string()
            }]
        );

        // 2. index.html と assets/app.js が正しい MIME で配信される
        let resp = get_widget_index(State(state.clone()), Path("price-sim".to_string())).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/html"
        );

        let resp = get_widget_asset(
            State(state.clone()),
            Path(("price-sim".to_string(), "assets/app.js".to_string())),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/javascript"
        );

        // 3. パストラバーサルと不正名は 400
        let resp = get_widget_asset(
            State(state.clone()),
            Path(("price-sim".to_string(), "../secret.txt".to_string())),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let resp = get_widget_index(State(state.clone()), Path("../etc".to_string())).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        // 4. block_html が安全な sandbox iframe を生成し、高さをクランプする
        let attrs = json!({
            "name": "price-sim",
            "title": "Price <Sim>",
            "height": 5000,
            "params": r#"{"plan":"pro"}"#,
        });
        let html = block_html(&state, &attrs);
        assert!(
            html.contains(r#"sandbox="allow-scripts allow-forms""#),
            "{html}"
        );
        assert!(!html.contains("allow-same-origin"), "{html}");
        assert!(html.contains(r#"height="3000""#), "{html}");
        assert!(html.contains(r#"title="Price &lt;Sim&gt;""#), "{html}");
        assert!(
            html.contains("/widgets/price-sim/index.html#%7B%22plan%22%3A%22pro%22%7D"),
            "{html}"
        );

        // 5. 存在しないウィジェットは空文字列
        let missing = json!({ "name": "nonexistent" });
        assert!(block_html(&state, &missing).is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
