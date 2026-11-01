//! `aegis plugin dev <dir>`: Wasm プラグインの開発・テスト用サーバー。
//!
//! データベース・管理者ログイン・外部ネットワークを必要とせず、
//! プラグイン単体（`plugin.manifest.json` と `plugin.wasm`）を即座に検証・デバッグできます。
//!
//! 主な機能:
//! - Wasmtime サンドボックス上でのフック関数の直接実行 (`POST /__dev/api/invoke`)
//! - Fuel（CPU命令数）・実行時間（ms）・Wasm ログ（`host.log`）の収集と可視化
//! - KVS / HTTP / 設定値（`config_get`）のモックと、KV変更・送信HTTPリクエストの追跡
//! - フロントエンド系フック（`on-render-page`, `render-blocks`, `transform-content-ast`）の HTML ライブプレビュー (`POST /__dev/api/preview`)
//! - プラグインソースコードの自動ビルド（Rust / AssemblyScript / TinyGo）とホットリロード (`GET /__dev/version`)
//! - Plugin Builder UI（React 19 + Vite）との連携用 REST API

use std::collections::{BTreeMap, HashMap};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime};

use aegis_core::{PluginManifest, Result as AegisResult};
use aegis_runtime::{
    HostServices, HttpRequest, HttpResponse, PluginLogEntry, PluginServices, WasmEngine,
    WasmPluginInstance,
};
use anyhow::{bail, Context};
use async_trait::async_trait;
use axum::extract::{Path as UrlPath, Query, Request, State};
use axum::http::{header, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::watch;

/// `aegis plugin dev` のコマンドライン引数
#[derive(clap::Args, Debug, Clone)]
pub struct DevArgs {
    /// プラグインのディレクトリ（plugin.manifest.json と plugin.wasm を含む）
    pub dir: PathBuf,
    /// バインド先ホストアドレス
    #[arg(short = 'H', long, default_value = "127.0.0.1")]
    pub host: String,
    /// 待ち受けポート番号
    #[arg(short, long, default_value_t = 8091)]
    pub port: u16,
    /// 自動ビルドコマンド（省略時は Cargo.toml や package.json から自動検出）
    #[arg(long)]
    pub build_command: Option<String>,
}

/// 書き込み・更新 API で要求するセキュリティヘッダー
const WRITE_HEADER: &str = "x-aegis-dev";
/// 読み書きを許可するファイルの最大サイズ（4 MiB）
const MAX_FILE_BYTES: usize = 4 << 20;
/// ホットリロード用のロングポーリング最大待機時間
const LONG_POLL_TIMEOUT: Duration = Duration::from_secs(25);
/// ファイル変更監視間隔
const WATCH_INTERVAL: Duration = Duration::from_millis(300);

/// モック HTTP 通信ルール
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockHttpRule {
    /// URL パターン（例: `https://api.slack.com/*` または `*`）
    pub url_pattern: String,
    /// 返却する HTTP ステータスコード
    pub status: u16,
    /// 返却するレスポンスボディ
    pub body: String,
    /// 返却する HTTP ヘッダー
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
}

/// 開発サーバー専用のモックホストサービス
#[derive(Clone)]
pub struct DevHostServices {
    /// KVS の現時点でのストアデータ
    pub kv: Arc<Mutex<HashMap<String, String>>>,
    /// フック実行中に変更（書き込み）された KVS のキーバリュー差分
    pub kv_mutations: Arc<Mutex<HashMap<String, String>>>,
    /// モック HTTP レスポンス設定
    pub mock_http_rules: Arc<Mutex<Vec<MockHttpRule>>>,
    /// フック実行中にプラグインが送信した HTTP リクエスト一覧
    pub captured_http_requests: Arc<Mutex<Vec<HttpRequest>>>,
    /// プラグイン設定値（config_get で参照されるキーバリュー）
    pub settings: Arc<RwLock<HashMap<String, String>>>,
}

impl DevHostServices {
    pub fn new() -> Self {
        Self {
            kv: Arc::new(Mutex::new(HashMap::new())),
            kv_mutations: Arc::new(Mutex::new(HashMap::new())),
            mock_http_rules: Arc::new(Mutex::new(Vec::new())),
            captured_http_requests: Arc::new(Mutex::new(Vec::new())),
            settings: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 各フック呼び出し前のリセット
    pub fn reset_session(&self) {
        if let Ok(mut mutations) = self.kv_mutations.lock() {
            mutations.clear();
        }
        if let Ok(mut requests) = self.captured_http_requests.lock() {
            requests.clear();
        }
    }
}

#[async_trait]
impl HostServices for DevHostServices {
    async fn kv_get(&self, key: &str) -> AegisResult<Option<String>> {
        let kv = self
            .kv
            .lock()
            .map_err(|_| aegis_core::AegisError::Internal("KV store mutex poisoned".to_string()))?;
        Ok(kv.get(key).cloned())
    }

    async fn kv_set(&self, key: &str, value: &str) -> AegisResult<()> {
        if let Ok(mut kv) = self.kv.lock() {
            kv.insert(key.to_string(), value.to_string());
        }
        if let Ok(mut mutations) = self.kv_mutations.lock() {
            mutations.insert(key.to_string(), value.to_string());
        }
        Ok(())
    }

    async fn http_fetch(&self, _plugin: &str, request: HttpRequest) -> AegisResult<HttpResponse> {
        if let Ok(mut reqs) = self.captured_http_requests.lock() {
            reqs.push(request.clone());
        }

        // モックルールと URL を突き合わせ
        if let Ok(rules) = self.mock_http_rules.lock() {
            for rule in rules.iter() {
                if pattern_matches(&rule.url_pattern, &request.url) {
                    return Ok(HttpResponse {
                        status: rule.status,
                        headers: rule.headers.clone(),
                        body: rule.body.clone(),
                    });
                }
            }
        }

        // マッチするモックルールが無い場合は既定で 200 OK の空 JSON
        Ok(HttpResponse {
            status: 200,
            headers: BTreeMap::new(),
            body: "{}".to_string(),
        })
    }

    fn plugin_setting(&self, _plugin: &str, key: &str) -> Option<String> {
        self.settings.read().ok().and_then(|s| s.get(key).cloned())
    }
}

/// 単純なワイルドカードパターンマッチング (`*` を許容)
fn pattern_matches(pattern: &str, url: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        url.starts_with(prefix)
    } else {
        pattern == url
    }
}

/// 自動ビルド状態
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BuildStatus {
    pub is_building: bool,
    pub last_error: Option<String>,
    pub last_built_at: Option<u64>,
    pub command: Option<String>,
}

/// 開発サーバーの共有状態
#[derive(Clone)]
struct DevState {
    dir: PathBuf,
    engine: WasmEngine,
    version: watch::Receiver<u64>,
    loopback: bool,
    host_services: DevHostServices,
    build_status: Arc<RwLock<BuildStatus>>,
}

/// プラグインディレクトリからビルドコマンドを自動検出
fn detect_build_command(dir: &Path) -> Option<String> {
    // 1. plugin.manifest.json の "dev": { "build_command": "..." }
    if let Ok(manifest_bytes) = std::fs::read(dir.join("plugin.manifest.json")) {
        if let Ok(val) = serde_json::from_slice::<Value>(&manifest_bytes) {
            if let Some(cmd) = val
                .get("dev")
                .and_then(|d| d.get("build_command"))
                .and_then(|c| c.as_str())
            {
                return Some(cmd.to_string());
            }
        }
    }
    // 2. Cargo.toml (Rust プラグイン)
    if dir.join("Cargo.toml").is_file() {
        return Some("cargo build --target wasm32-unknown-unknown".to_string());
    }
    // 3. package.json (AssemblyScript プラグイン)
    if dir.join("package.json").is_file() {
        if let Ok(pkg_bytes) = std::fs::read(dir.join("package.json")) {
            if let Ok(val) = serde_json::from_slice::<Value>(&pkg_bytes) {
                if val.get("scripts").and_then(|s| s.get("asbuild")).is_some() {
                    return Some("npm run asbuild".to_string());
                }
                if val.get("scripts").and_then(|s| s.get("build")).is_some() {
                    return Some("npm run build".to_string());
                }
            }
        }
    }
    None
}

/// ビルドコマンドの非同期実行
async fn trigger_build(
    dir: PathBuf,
    cmd_str: String,
    status: Arc<RwLock<BuildStatus>>,
) -> Result<(), String> {
    {
        let mut st = status.write().map_err(|e| e.to_string())?;
        st.is_building = true;
        st.command = Some(cmd_str.clone());
    }

    #[cfg(target_os = "windows")]
    let output_res = tokio::process::Command::new("cmd")
        .args(["/C", &cmd_str])
        .current_dir(&dir)
        .output()
        .await;

    #[cfg(not(target_os = "windows"))]
    let output_res = tokio::process::Command::new("sh")
        .args(["-c", &cmd_str])
        .current_dir(&dir)
        .output()
        .await;

    let mut st = status.write().map_err(|e| e.to_string())?;
    st.is_building = false;
    st.last_built_at = Some(current_timestamp_millis());

    match output_res {
        Ok(output) => {
            if output.status.success() {
                st.last_error = None;
                Ok(())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
                let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
                let err_msg = if !stderr.is_empty() { stderr } else { stdout };
                st.last_error = Some(err_msg.clone());
                Err(err_msg)
            }
        }
        Err(e) => {
            let err_msg = format!("Failed to execute '{}': {}", cmd_str, e);
            st.last_error = Some(err_msg.clone());
            Err(err_msg)
        }
    }
}

/// `plugin dev` サーバーの起動
pub async fn run(args: DevArgs) -> anyhow::Result<()> {
    let dir = args
        .dir
        .canonicalize()
        .with_context(|| format!("Plugin directory not found: {}", args.dir.display()))?;

    let manifest_path = dir.join("plugin.manifest.json");
    if !manifest_path.is_file() {
        bail!(
            "{} has no plugin.manifest.json (is it an AegisCMS plugin directory?)",
            dir.display()
        );
    }

    let loopback = is_loopback_host(&args.host);
    let engine = WasmEngine::new()?;
    let host_services = DevHostServices::new();

    let detected_cmd = args.build_command.or_else(|| detect_build_command(&dir));
    let build_status = Arc::new(RwLock::new(BuildStatus {
        is_building: false,
        last_error: None,
        last_built_at: None,
        command: detected_cmd.clone(),
    }));

    let (version_tx, version_rx) = watch::channel(current_timestamp_millis());

    // ファイル変更監視タスク（Wasm やマニフェスト、ソースコード等の更新を検知して通知・ビルド）
    let watch_dir = dir.clone();
    let watch_status = build_status.clone();
    let watch_tx = version_tx.clone();
    tokio::spawn(async move {
        let mut last_modified = highest_mtime(&watch_dir);
        loop {
            tokio::time::sleep(WATCH_INTERVAL).await;
            let (current, is_source_changed) = check_mtime_and_sources(&watch_dir);
            if current > last_modified {
                last_modified = current;

                // ソースファイルの変更があり、ビルドコマンドが存在する場合は自動ビルドを実行
                if is_source_changed {
                    let cmd_to_run = watch_status.read().ok().and_then(|s| s.command.clone());
                    if let Some(cmd) = cmd_to_run {
                        let _ = trigger_build(watch_dir.clone(), cmd, watch_status.clone()).await;
                    }
                }

                let _ = watch_tx.send(current_timestamp_millis());
            }
        }
    });

    let state = DevState {
        dir: dir.clone(),
        engine,
        version: version_rx,
        loopback,
        host_services,
        build_status,
    };

    let app = Router::new()
        .route("/", get(welcome_page))
        .route("/__dev/version", get(version_handler))
        .route("/__dev/api/plugin", get(api_plugin_info))
        .route("/__dev/api/fixtures", get(api_fixtures))
        .route("/__dev/api/invoke", post(api_invoke_hook))
        .route("/__dev/api/preview", post(api_generate_preview))
        .route("/__dev/api/build", post(api_trigger_build))
        .route("/__dev/api/files", get(api_files_list))
        .route(
            "/__dev/api/files/{*path}",
            get(api_read_file)
                .put(api_write_file)
                .delete(api_delete_file),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            guard_middleware,
        ))
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", args.host, args.port).parse()?;
    println!("--------------------------------------------------");
    println!("  AegisCMS Plugin Development Server");
    println!("  Plugin:  {}", dir.display());
    if let Some(ref cmd) = detected_cmd {
        println!("  Build:   {}", cmd);
    }
    println!("  Listen:  http://{}", addr);
    println!("  API:     http://{}/__dev/api/plugin", addr);
    println!("--------------------------------------------------");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn current_timestamp_millis() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn highest_mtime(dir: &Path) -> u64 {
    check_mtime_and_sources(dir).0
}

fn check_mtime_and_sources(dir: &Path) -> (u64, bool) {
    let mut highest = 0;
    let mut source_changed = false;

    for entry in walkdir::WalkDir::new(dir)
        .max_depth(5)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        let path = entry.path();
        if path
            .components()
            .any(|c| c.as_os_str() == "target" || c.as_os_str() == "node_modules")
        {
            continue;
        }

        if let Ok(metadata) = path.metadata() {
            if let Ok(modified) = metadata.modified() {
                let millis = modified
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                if millis > highest {
                    highest = millis;
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        if matches!(ext, "rs" | "ts" | "js" | "go" | "wat") {
                            source_changed = true;
                        }
                    }
                }
            }
        }
    }
    (highest, source_changed)
}

fn is_loopback_host(host: &str) -> bool {
    let name = if let Some(rest) = host.strip_prefix('[') {
        rest.split(']').next().unwrap_or("")
    } else {
        host.split(':').next().unwrap_or("")
    };
    matches!(name, "localhost" | "127.0.0.1" | "::1")
}

// ---------------------------------------------------------------------------
// セキュリティガード・ミドルウェア
// ---------------------------------------------------------------------------

async fn guard_middleware(State(state): State<DevState>, req: Request, next: Next) -> Response {
    // OPTIONS プリフライト
    if req.method() == Method::OPTIONS {
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .header(
                header::ACCESS_CONTROL_ALLOW_METHODS,
                "GET, POST, PUT, DELETE, OPTIONS",
            )
            .header(header::ACCESS_CONTROL_ALLOW_HEADERS, "*")
            .body(axum::body::Body::empty())
            .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "error").into_response());
    }

    // DNS リバインディング防止
    if state.loopback {
        let host = req
            .headers()
            .get(header::HOST)
            .and_then(|h| h.to_str().ok())
            .unwrap_or("");
        if !is_loopback_host(host) {
            return (StatusCode::FORBIDDEN, "Unexpected Host header").into_response();
        }
    }

    // ミューテーションリクエストのヘッダー検証
    if !matches!(*req.method(), Method::GET | Method::HEAD)
        && !req.headers().contains_key(WRITE_HEADER)
    {
        return (
            StatusCode::FORBIDDEN,
            format!("Missing {WRITE_HEADER} header"),
        )
            .into_response();
    }

    let mut res = next.run(req).await;
    res.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        header::HeaderValue::from_static("*"),
    );
    res
}

// ---------------------------------------------------------------------------
// API ハンドラ
// ---------------------------------------------------------------------------

async fn welcome_page(State(state): State<DevState>) -> Html<String> {
    let manifest_res = std::fs::read_to_string(state.dir.join("plugin.manifest.json"));
    let manifest: Value = manifest_res
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| json!({}));

    let name = manifest
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown-plugin");
    let version = manifest
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("0.0.0");

    Html(format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>AegisCMS Plugin Dev - {name}</title>
    <style>
        body {{ font-family: system-ui, -apple-system, sans-serif; line-height: 1.6; max-width: 800px; margin: 40px auto; padding: 0 20px; color: #24292f; }}
        h1 {{ border-bottom: 2px solid #eaeef2; padding-bottom: 12px; }}
        pre {{ background: #f6f8fa; padding: 16px; border-radius: 6px; overflow-x: auto; }}
        code {{ font-family: ui-monospace, SFMono-Regular, monospace; }}
        .badge {{ display: inline-block; padding: 4px 8px; border-radius: 4px; font-weight: 600; background: #ddf4ff; color: #0969da; }}
        ul {{ padding-left: 20px; }}
        li {{ margin-bottom: 8px; }}
    </style>
</head>
<body>
    <h1>AegisCMS Plugin Development Server</h1>
    <p>Target: <span class="badge">{name} v{version}</span></p>
    <h3>Available Endpoints</h3>
    <ul>
        <li><code>GET /__dev/api/plugin</code> - プラグインメタデータ、ケイパビリティ、エクスポート一覧、ビルド状態</li>
        <li><code>GET /__dev/api/fixtures</code> - 各フック検証用の標準モックデータプリセット</li>
        <li><code>POST /__dev/api/invoke</code> - フックの直接実行（ログ、Fuel、実行時間、KV/HTTPキャプチャ）</li>
        <li><code>POST /__dev/api/preview</code> - フロントエンド系フックの HTML プレビュー合成</li>
        <li><code>POST /__dev/api/build</code> - 手動ビルド実行トリガー</li>
        <li><code>GET /__dev/version</code> - ホットリロード用ロングポーリング</li>
        <li><code>GET /__dev/api/files</code> - プラグインファイル一覧</li>
    </ul>
</body>
</html>"#
    ))
}

#[derive(Deserialize)]
struct VersionQuery {
    since: Option<u64>,
}

async fn version_handler(
    State(mut state): State<DevState>,
    Query(query): Query<VersionQuery>,
) -> Json<Value> {
    let current = *state.version.borrow();
    if let Some(since) = query.since {
        if current > since {
            return Json(json!({ "version": current }));
        }
        // ロングポーリング待機
        let _ = tokio::time::timeout(LONG_POLL_TIMEOUT, async {
            while state.version.changed().await.is_ok() {
                let v = *state.version.borrow();
                if v > since {
                    return;
                }
            }
        })
        .await;
    }
    Json(json!({ "version": *state.version.borrow() }))
}

/// `GET /__dev/api/plugin`
async fn api_plugin_info(
    State(state): State<DevState>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let manifest_bytes = std::fs::read(state.dir.join("plugin.manifest.json"))
        .map_err(|e| (StatusCode::NOT_FOUND, format!("Manifest not found: {e}")))?;
    let manifest: PluginManifest = serde_json::from_slice(&manifest_bytes).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid manifest JSON: {e}"),
        )
    })?;

    let wasm_file = state.dir.join(&manifest.entrypoint);
    let (wasm_exists, wasm_size, exports) = if wasm_file.is_file() {
        let bytes = std::fs::read(&wasm_file).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to read Wasm: {e}"),
            )
        })?;
        let size = bytes.len();
        let plugin_services: PluginServices = PluginServices(Arc::new(state.host_services.clone()));
        match WasmPluginInstance::from_bytes(
            state.engine,
            manifest.clone(),
            &bytes,
            plugin_services,
        ) {
            Ok(inst) => (true, size, inst.exports()),
            Err(_) => (true, size, Vec::new()),
        }
    } else {
        (false, 0, Vec::new())
    };

    let build_status = state
        .build_status
        .read()
        .map(|s| s.clone())
        .unwrap_or_default();

    Ok(Json(json!({
        "manifest": manifest,
        "entrypoint": manifest.entrypoint,
        "wasm_exists": wasm_exists,
        "wasm_size_bytes": wasm_size,
        "exports": exports,
        "build_status": build_status,
        "updated_at": current_timestamp_millis(),
    })))
}

/// `GET /__dev/api/fixtures`
async fn api_fixtures() -> Json<Value> {
    Json(json!({
        "on-before-save": {
            "post": {
                "id": "post-sample-1",
                "title": "Welcome to AegisCMS",
                "slug": "welcome-to-aegiscms",
                "author_id": "user-admin",
                "blocks_json": "[]"
            }
        },
        "transform-content-ast": {
            "blocks": [
                {
                    "type": "paragraph",
                    "content": "This is a paragraph with a code block below."
                },
                {
                    "type": "code",
                    "language": "rust",
                    "code": "fn main() {\n    println!(\"Hello Aegis!\");\n}"
                }
            ]
        },
        "render-blocks": {
            "blocks": [
                {
                    "type": "official-blocks/button",
                    "props": {
                        "label": "Get Started",
                        "url": "https://example.com"
                    }
                },
                {
                    "type": "paragraph",
                    "content": "Regular unhandled block"
                }
            ]
        },
        "on-render-page": {
            "context": {
                "site_title": "My Aegis Site",
                "template": "post.html",
                "extra_head": "",
                "extra_css": "",
                "extra_js": "",
                "items": [
                    {
                        "id": "post-1",
                        "slug": "getting-started",
                        "title": "Getting Started with AegisCMS",
                        "html": "<article class=\"post\"><h1>Getting Started</h1><p>Welcome to our new site built on AegisCMS.</p><pre><code class=\"language-rust\">fn hello() {}</code></pre></article>"
                    }
                ]
            }
        },
        "calculate-order": {
            "order": {
                "items": [
                    {
                        "product_id": "prod-1",
                        "title": "Aegis T-Shirt",
                        "unit_price": 3000,
                        "quantity": 2
                    }
                ],
                "subtotal": 6000,
                "shipping_fee": 500,
                "discount_amount": 0,
                "total": 6500,
                "currency": "JPY",
                "coupon_code": "WELCOME10"
            }
        },
        "on-order-status-changed": {
            "change": {
                "order_id": "ord-2026-001",
                "previous_status": "pending",
                "new_status": "paid",
                "total": 6500,
                "customer_email": "customer@example.com"
            }
        },
        "on-content-published": {
            "content": {
                "id": "post-sample-1",
                "title": "New Features Released",
                "slug": "new-features-released",
                "url": "http://localhost:8080/posts/new-features-released"
            }
        },
        "check-comment": {
            "comment": {
                "content_id": "post-sample-1",
                "author_name": "Friendly Reader",
                "author_email": "reader@example.com",
                "content": "Thanks for this amazing article!",
                "ip": "192.168.1.1"
            }
        },
        "send-notification": {
            "notification": {
                "channel": "plugin:slack",
                "recipient": "#general",
                "subject": "System Alert",
                "body": "Daily backup completed successfully."
            }
        },
        "on-user-authenticated": {
            "auth": {
                "user_id": "user-123",
                "username": "takumi",
                "roles": ["admin"],
                "metadata": {}
            }
        }
    }))
}

/// フック実行リクエスト
#[derive(Deserialize)]
struct InvokeRequest {
    /// 呼び出すフック名（例: "on-before-save"）
    hook: String,
    /// エクスポート関数名（省略時はフック名のハイフンをアンダースコアに変換）
    export: Option<String>,
    /// フックへの入力データ（任意の JSON または文字列）
    input: Value,
    /// 実行時の動的設定値
    #[serde(default)]
    config: HashMap<String, String>,
    /// 実行前の KV 初期データ
    #[serde(default)]
    kv: HashMap<String, String>,
    /// モック HTTP ルール
    #[serde(default)]
    mock_http: Vec<MockHttpRule>,
}

/// フック実行レスポンス
#[derive(Serialize)]
struct InvokeResponse {
    success: bool,
    output: Option<Value>,
    raw_output: Option<String>,
    error: Option<String>,
    fuel_consumed: u64,
    duration_ms: f64,
    logs: Vec<PluginLogEntry>,
    kv_mutations: HashMap<String, String>,
    http_requests: Vec<HttpRequest>,
}

/// `POST /__dev/api/invoke`
async fn api_invoke_hook(
    State(state): State<DevState>,
    Json(payload): Json<InvokeRequest>,
) -> Result<Json<InvokeResponse>, (StatusCode, String)> {
    let manifest_bytes = std::fs::read(state.dir.join("plugin.manifest.json"))
        .map_err(|e| (StatusCode::NOT_FOUND, format!("Manifest not found: {e}")))?;
    let manifest: PluginManifest = serde_json::from_slice(&manifest_bytes).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid manifest JSON: {e}"),
        )
    })?;

    let wasm_file = state.dir.join(&manifest.entrypoint);
    if !wasm_file.is_file() {
        return Err((
            StatusCode::NOT_FOUND,
            format!("Wasm binary not found: {}", wasm_file.display()),
        ));
    }

    let wasm_bytes = std::fs::read(&wasm_file).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to read Wasm: {e}"),
        )
    })?;

    // セッション状態（KV・HTTPモック・設定）の初期化
    state.host_services.reset_session();
    if let Ok(mut kv_store) = state.host_services.kv.lock() {
        kv_store.clear();
        for (k, v) in payload.kv {
            let full_key = if let Some(storage) = &manifest.capabilities.storage {
                format!("{}:{}", storage.kv_namespace, k)
            } else {
                k
            };
            kv_store.insert(full_key, v);
        }
    }
    if let Ok(mut settings) = state.host_services.settings.write() {
        *settings = payload.config;
    }
    if let Ok(mut rules) = state.host_services.mock_http_rules.lock() {
        *rules = payload.mock_http;
    }

    let plugin_services: PluginServices = PluginServices(Arc::new(state.host_services.clone()));
    let instance = match WasmPluginInstance::from_bytes(
        state.engine,
        manifest.clone(),
        &wasm_bytes,
        plugin_services,
    ) {
        Ok(inst) => inst,
        Err(e) => {
            return Ok(Json(InvokeResponse {
                success: false,
                output: None,
                raw_output: None,
                error: Some(format!("Module initialization failed: {e}")),
                fuel_consumed: 0,
                duration_ms: 0.0,
                logs: Vec::new(),
                kv_mutations: HashMap::new(),
                http_requests: Vec::new(),
            }));
        }
    };

    let export_name = payload
        .export
        .unwrap_or_else(|| payload.hook.replace('-', "_"));

    // 入力データのバイナリ化
    let input_bytes = match payload.input {
        Value::String(s) => s.into_bytes(),
        other => serde_json::to_vec(&other).unwrap_or_default(),
    };

    let raw_res = instance
        .call_hook_raw(&payload.hook, &export_name, &input_bytes)
        .await;

    let (mutations, requests) = {
        let m = state
            .host_services
            .kv_mutations
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default();
        let r = state
            .host_services
            .captured_http_requests
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default();
        (m, r)
    };

    match raw_res {
        Ok(details) => {
            let raw_str = details
                .output
                .as_ref()
                .map(|b| String::from_utf8_lossy(b).into_owned());
            let json_val: Option<Value> = details
                .output
                .as_ref()
                .and_then(|b| serde_json::from_slice(b).ok());

            Ok(Json(InvokeResponse {
                success: true,
                output: json_val,
                raw_output: raw_str,
                error: None,
                fuel_consumed: details.fuel_consumed,
                duration_ms: details.duration_ms,
                logs: details.logs,
                kv_mutations: mutations,
                http_requests: requests,
            }))
        }
        Err(e) => Ok(Json(InvokeResponse {
            success: false,
            output: None,
            raw_output: None,
            error: Some(e.to_string()),
            fuel_consumed: 0,
            duration_ms: 0.0,
            logs: Vec::new(),
            kv_mutations: mutations,
            http_requests: requests,
        })),
    }
}

// ---------------------------------------------------------------------------
// プレビュー生成 & ビルドトリガー API
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct PreviewRequest {
    hook: String,
    output: Value,
}

#[derive(Serialize)]
struct PreviewResponse {
    html: String,
}

/// `POST /__dev/api/preview`
async fn api_generate_preview(Json(payload): Json<PreviewRequest>) -> Json<PreviewResponse> {
    let html = render_preview_document(&payload.hook, &payload.output);
    Json(PreviewResponse { html })
}

/// フック実行結果から完全なプレビュー HTML を生成
fn render_preview_document(hook: &str, output: &Value) -> String {
    let mut title = "Plugin Live Preview".to_string();
    let mut extra_head = String::new();
    let mut extra_css = String::new();
    let mut extra_js = String::new();
    let mut body_content = String::new();

    match hook {
        "on-render-page" => {
            if let Some(obj) = output.as_object() {
                if let Some(t) = obj.get("site_title").and_then(|v| v.as_str()) {
                    title = t.to_string();
                }
                if let Some(h) = obj.get("extra_head").and_then(|v| v.as_str()) {
                    extra_head.push_str(h);
                }
                if let Some(c) = obj.get("extra_css").and_then(|v| v.as_str()) {
                    extra_css.push_str(c);
                }
                if let Some(j) = obj.get("extra_js").and_then(|v| v.as_str()) {
                    extra_js.push_str(j);
                }
                if let Some(items) = obj.get("items").and_then(|v| v.as_array()) {
                    for item in items {
                        if let Some(html) = item.get("html").and_then(|v| v.as_str()) {
                            body_content.push_str(html);
                        } else if let Some(item_title) = item.get("title").and_then(|v| v.as_str())
                        {
                            body_content.push_str(&format!("<h2>{}</h2>", item_title));
                        }
                    }
                }
            }
        }
        "render-blocks" => {
            if let Some(html_list) = output.as_array() {
                body_content.push_str("<div class=\"rendered-blocks\">");
                for item in html_list {
                    if let Some(html) = item.as_str() {
                        body_content.push_str("<div class=\"block-item\">");
                        body_content.push_str(html);
                        body_content.push_str("</div>");
                    }
                }
                body_content.push_str("</div>");
            }
        }
        "transform-content-ast" => {
            if let Ok(blocks) = serde_json::from_value::<Vec<aegis_ast::BlockNode>>(output.clone())
            {
                body_content.push_str(&aegis_ast::HtmlRenderer::render_nodes(&blocks));
            } else {
                body_content.push_str(&format!(
                    "<pre><code>{}</code></pre>",
                    serde_json::to_string_pretty(output).unwrap_or_default()
                ));
            }
        }
        _ => {
            body_content.push_str(&format!(
                "<div class=\"preview-card\"><h3>Hook Output: {}</h3><pre><code>{}</code></pre></div>",
                hook,
                serde_json::to_string_pretty(output).unwrap_or_default()
            ));
        }
    }

    if body_content.is_empty() {
        body_content = "<p class=\"empty-notice\">No content generated by plugin.</p>".to_string();
    }

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{title}</title>
    <style>
        :root {{
            --font-sans: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
            --color-bg: #ffffff;
            --color-text: #24292f;
            --color-primary: #0969da;
            --color-border: #d0d7de;
        }}
        body {{
            font-family: var(--font-sans);
            color: var(--color-text);
            background-color: var(--color-bg);
            line-height: 1.6;
            margin: 0;
            padding: 32px 24px;
            max-width: 800px;
            margin-left: auto;
            margin-right: auto;
        }}
        h1, h2, h3, h4 {{ color: #1f2328; margin-top: 1.5em; margin-bottom: 0.5em; }}
        pre {{ background: #f6f8fa; padding: 14px; border-radius: 6px; overflow-x: auto; border: 1px solid var(--color-border); }}
        code {{ font-family: ui-monospace, SFMono-Regular, monospace; font-size: 13px; }}
        img {{ max-width: 100%; height: auto; }}
        .rendered-blocks {{ display: flex; flex-direction: column; gap: 16px; margin: 20px 0; }}
        .preview-card {{ border: 1px solid var(--color-border); border-radius: 8px; padding: 20px; background: #f6f8fa; }}
        .empty-notice {{ color: #6e7781; font-style: italic; text-align: center; margin: 40px 0; }}
    </style>
    <style>{extra_css}</style>
    {extra_head}
</head>
<body>
    <article>
        {body_content}
    </article>
    <script>{extra_js}</script>
</body>
</html>"#
    )
}

/// `POST /__dev/api/build`
async fn api_trigger_build(
    State(state): State<DevState>,
) -> Result<Json<BuildStatus>, (StatusCode, String)> {
    let cmd = {
        let st = state
            .build_status
            .read()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        st.command.clone()
    };

    if let Some(cmd_str) = cmd {
        let _ = trigger_build(state.dir.clone(), cmd_str, state.build_status.clone()).await;
    }

    let st = state
        .build_status
        .read()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .clone();
    Ok(Json(st))
}

// ---------------------------------------------------------------------------
// ファイル操作 API
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct FileItem {
    path: String,
    size: u64,
    is_dir: bool,
}

fn resolve_safe_path(dir: &Path, rel: &str) -> Result<PathBuf, (StatusCode, String)> {
    let rel_clean = rel.trim_start_matches(['/', '\\']);
    if rel_clean.contains("..") {
        return Err((
            StatusCode::BAD_REQUEST,
            "Path traversal forbidden".to_string(),
        ));
    }
    let target = dir.join(rel_clean);
    let mut existing = target.as_path();
    while !existing.exists() {
        existing = existing
            .parent()
            .ok_or_else(|| (StatusCode::BAD_REQUEST, "Invalid path".to_string()))?;
    }
    let real = existing
        .canonicalize()
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid path: {e}")))?;
    if !real.starts_with(dir) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Path leaves plugin directory".to_string(),
        ));
    }
    Ok(target)
}

/// `GET /__dev/api/files`
async fn api_files_list(
    State(state): State<DevState>,
) -> Result<Json<Vec<FileItem>>, (StatusCode, String)> {
    let mut items = Vec::new();
    for entry in walkdir::WalkDir::new(&state.dir)
        .max_depth(5)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        let path = entry.path();
        if path == state.dir {
            continue;
        }
        if let Ok(rel) = path.strip_prefix(&state.dir) {
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            let is_dir = entry.file_type().is_dir();
            let size = if is_dir {
                0
            } else {
                entry.metadata().map(|m| m.len()).unwrap_or(0)
            };
            items.push(FileItem {
                path: rel_str,
                size,
                is_dir,
            });
        }
    }
    items.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Json(items))
}

/// `GET /__dev/api/files/{*path}`
async fn api_read_file(
    State(state): State<DevState>,
    UrlPath(rel): UrlPath<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let path = resolve_safe_path(&state.dir, &rel)?;
    let bytes = std::fs::read(&path).map_err(|e| (StatusCode::NOT_FOUND, format!("{rel}: {e}")))?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            "File is too large".to_string(),
        ));
    }
    let content = String::from_utf8(bytes).map_err(|_| {
        (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "File is not UTF-8 text".to_string(),
        )
    })?;
    Ok(Json(json!({ "path": rel, "content": content })))
}

#[derive(Deserialize)]
struct WriteFilePayload {
    content: String,
}

/// `PUT /__dev/api/files/{*path}`
async fn api_write_file(
    State(state): State<DevState>,
    UrlPath(rel): UrlPath<String>,
    Json(payload): Json<WriteFilePayload>,
) -> Result<StatusCode, (StatusCode, String)> {
    let path = resolve_safe_path(&state.dir, &rel)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to create parent dirs: {e}"),
            )
        })?;
    }
    std::fs::write(&path, payload.content.as_bytes()).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to write file: {e}"),
        )
    })?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /__dev/api/files/{*path}`
async fn api_delete_file(
    State(state): State<DevState>,
    UrlPath(rel): UrlPath<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let path = resolve_safe_path(&state.dir, &rel)?;
    if path.is_file() {
        std::fs::remove_file(&path).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to delete file: {e}"),
            )
        })?;
    }
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// テスト
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use tower::ServiceExt;

    fn setup_test_plugin_dir() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("my-test-plugin");
        std::fs::create_dir_all(&dir).unwrap();

        let manifest = r#"{
            "name": "my-test-plugin",
            "version": "1.0.0",
            "entrypoint": "plugin.wasm",
            "capabilities": {
                "hooks": ["on-before-save"]
            },
            "resource_limits": {
                "max_memory_mb": 16,
                "max_fuel_units": 100000,
                "execution_timeout_ms": 250
            }
        }"#;
        std::fs::write(dir.join("plugin.manifest.json"), manifest).unwrap();

        let wat = r#"
        (module
            (import "host" "log" (func $log (param i32 i32 i32)))
            (memory (export "memory") 1)
            (data (i32.const 256) "{\"title\":\"Dev server transformed\"}")
            (data (i32.const 512) "Plugin log emitted")
            (func (export "allocate") (param $len i32) (result i32)
                i32.const 16
            )
            (func (export "on_before_save") (param $ptr i32) (param $len i32) (result i64)
                (call $log (i32.const 1) (i32.const 512) (i32.const 18))
                (i64.or
                    (i64.shl (i64.const 256) (i64.const 32))
                    (i64.const 34)
                )
            )
        )
        "#;
        let wasm = wat::parse_str(wat).unwrap();
        std::fs::write(dir.join("plugin.wasm"), wasm).unwrap();

        tmp
    }

    fn create_test_router(dir: PathBuf) -> Router {
        let engine = WasmEngine::new().unwrap();
        let host_services = DevHostServices::new();
        let (_tx, rx) = watch::channel(0);
        let build_status = Arc::new(RwLock::new(BuildStatus::default()));
        let state = DevState {
            dir,
            engine,
            version: rx,
            loopback: true,
            host_services,
            build_status,
        };

        Router::new()
            .route("/", get(welcome_page))
            .route("/__dev/version", get(version_handler))
            .route("/__dev/api/plugin", get(api_plugin_info))
            .route("/__dev/api/fixtures", get(api_fixtures))
            .route("/__dev/api/invoke", post(api_invoke_hook))
            .route("/__dev/api/preview", post(api_generate_preview))
            .route("/__dev/api/build", post(api_trigger_build))
            .route("/__dev/api/files", get(api_files_list))
            .route(
                "/__dev/api/files/{*path}",
                get(api_read_file)
                    .put(api_write_file)
                    .delete(api_delete_file),
            )
            .layer(middleware::from_fn_with_state(
                state.clone(),
                guard_middleware,
            ))
            .with_state(state)
    }

    #[tokio::test]
    async fn test_api_plugin_info() {
        let tmp = setup_test_plugin_dir();
        let dir = tmp.path().join("my-test-plugin");
        let app = create_test_router(dir);

        let req = Request::builder()
            .uri("/__dev/api/plugin")
            .header("Host", "localhost:8091")
            .body(Body::empty())
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let body = to_bytes(res.into_body(), 1024 * 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["manifest"]["name"], "my-test-plugin");
        assert_eq!(json["wasm_exists"], true);
        assert!(json["exports"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "on_before_save"));
    }

    #[tokio::test]
    async fn test_api_fixtures() {
        let tmp = setup_test_plugin_dir();
        let dir = tmp.path().join("my-test-plugin");
        let app = create_test_router(dir);

        let req = Request::builder()
            .uri("/__dev/api/fixtures")
            .header("Host", "127.0.0.1:8091")
            .body(Body::empty())
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let body = to_bytes(res.into_body(), 1024 * 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("on-before-save").is_some());
        assert!(json.get("transform-content-ast").is_some());
        assert!(json.get("calculate-order").is_some());
    }

    #[tokio::test]
    async fn test_api_invoke_hook_with_guard_and_logs() {
        let tmp = setup_test_plugin_dir();
        let dir = tmp.path().join("my-test-plugin");
        let app = create_test_router(dir);

        // 1. Without WRITE_HEADER -> 403 Forbidden
        let req_denied = Request::builder()
            .method(Method::POST)
            .uri("/__dev/api/invoke")
            .header("Host", "localhost:8091")
            .header("Content-Type", "application/json")
            .body(Body::from(r#"{"hook":"on-before-save","input":{}}"#))
            .unwrap();
        let res = app.clone().oneshot(req_denied).await.unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);

        // 2. With WRITE_HEADER -> 200 OK and execution details
        let req_ok = Request::builder()
            .method(Method::POST)
            .uri("/__dev/api/invoke")
            .header("Host", "localhost:8091")
            .header(WRITE_HEADER, "1")
            .header("Content-Type", "application/json")
            .body(Body::from(
                r#"{"hook":"on-before-save","input":{"title":"Original"}}"#,
            ))
            .unwrap();
        let res = app.oneshot(req_ok).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let body = to_bytes(res.into_body(), 1024 * 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["success"], true);
        assert_eq!(json["output"]["title"], "Dev server transformed");
        assert!(json["fuel_consumed"].as_u64().unwrap() > 0);
        let logs = json["logs"].as_array().unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0]["message"], "Plugin log emitted");
    }

    #[tokio::test]
    async fn test_api_generate_preview() {
        let tmp = setup_test_plugin_dir();
        let dir = tmp.path().join("my-test-plugin");
        let app = create_test_router(dir);

        let preview_payload = json!({
            "hook": "on-render-page",
            "output": {
                "site_title": "Custom Preview Title",
                "extra_css": ".highlight { color: red; }",
                "items": [
                    {
                        "title": "My Article",
                        "html": "<p>Article text here</p>"
                    }
                ]
            }
        });

        let req = Request::builder()
            .method(Method::POST)
            .uri("/__dev/api/preview")
            .header("Host", "localhost:8091")
            .header(WRITE_HEADER, "1")
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::to_vec(&preview_payload).unwrap()))
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let body = to_bytes(res.into_body(), 1024 * 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        let html = json["html"].as_str().unwrap();
        assert!(html.contains("Custom Preview Title"));
        assert!(html.contains(".highlight { color: red; }"));
        assert!(html.contains("<p>Article text here</p>"));
    }

    #[tokio::test]
    async fn test_api_invoke_captures_kv_and_http() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("host-test-plugin");
        std::fs::create_dir_all(&dir).unwrap();

        let manifest = r#"{
            "name": "host-test-plugin",
            "version": "1.0.0",
            "entrypoint": "plugin.wasm",
            "capabilities": {
                "hooks": ["on-before-save"],
                "network": { "allowed_domains": ["api.slack.com"] },
                "storage": { "kv_namespace": "test_ns" }
            },
            "resource_limits": {
                "max_memory_mb": 16,
                "max_fuel_units": 100000,
                "execution_timeout_ms": 250
            }
        }"#;
        std::fs::write(dir.join("plugin.manifest.json"), manifest).unwrap();

        let req_json =
            "{\"method\":\"POST\",\"url\":\"https://api.slack.com/webhook\",\"body\":\"alert\"}";
        let wat = format!(
            r#"
            (module
                (import "host" "kv_set" (func $kv_set (param i32 i32 i32 i32) (result i32)))
                (import "host" "http_fetch" (func $http_fetch (param i32 i32) (result i64)))
                (memory (export "memory") 1)
                (data (i32.const 256) "counter")
                (data (i32.const 272) "42")
                (data (i32.const 300) "{req_data}")
                (func (export "allocate") (param $len i32) (result i32)
                    i32.const 16
                )
                (func (export "on_before_save") (param $ptr i32) (param $len i32) (result i64)
                    (drop (call $kv_set (i32.const 256) (i32.const 7) (i32.const 272) (i32.const 2)))
                    (drop (call $http_fetch (i32.const 300) (i32.const {req_len})))
                    (i64.const 0)
                )
            )
            "#,
            req_data = req_json.replace('"', "\\\""),
            req_len = req_json.len()
        );
        let wasm = wat::parse_str(wat).unwrap();
        std::fs::write(dir.join("plugin.wasm"), wasm).unwrap();

        let app = create_test_router(dir);

        let invoke_payload = serde_json::json!({
            "hook": "on-before-save",
            "input": {},
            "mock_http": [
                {
                    "url_pattern": "https://api.slack.com/*",
                    "status": 200,
                    "body": "{\"ok\":true}"
                }
            ]
        });

        let req = Request::builder()
            .method(Method::POST)
            .uri("/__dev/api/invoke")
            .header("Host", "localhost:8091")
            .header(WRITE_HEADER, "1")
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::to_vec(&invoke_payload).unwrap()))
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let body = to_bytes(res.into_body(), 1024 * 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["success"], true);

        // KV mutation is captured with namespace
        assert_eq!(json["kv_mutations"]["test_ns:counter"], "42");

        // HTTP request is captured
        let http_reqs = json["http_requests"].as_array().unwrap();
        assert_eq!(http_reqs.len(), 1);
        assert_eq!(http_reqs[0]["method"], "POST");
        assert_eq!(http_reqs[0]["url"], "https://api.slack.com/webhook");
    }

    #[tokio::test]
    async fn test_files_api_prevents_traversal() {
        let tmp = setup_test_plugin_dir();
        let dir = tmp.path().join("my-test-plugin");
        let app = create_test_router(dir);

        let req = Request::builder()
            .uri("/__dev/api/files/../../secret.txt")
            .header("Host", "localhost:8091")
            .body(Body::empty())
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }
}
