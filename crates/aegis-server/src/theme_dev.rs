//! `aegis-server theme dev <dir>`: テーマの開発用サーバー。
//!
//! DB・プラグイン・認証を使わず、テーマのフォルダーをそのまま読んでサンプルのデータで描画する。
//! テンプレート・アセット・サンプルを保存すると、開いているブラウザーが自動で読み込み直す
//! （エディターは VS Code などに任せてもよい）。`/__dev/api/*` は Theme Builder UI（`theme-builder/`）用の
//! JSON API で、テーマのファイルの読み書きと、未保存の内容でのプレビューもできる。
//!
//! 開発用なので、既定では `127.0.0.1` だけで待ち受ける。ファイルを書けるのは、ループバックで
//! 待ち受けているときだけ（ほかのページからの書き込み・DNS リバインディングを防ぐ検査つき）。
//! 手順は `docs/theme_authoring_guide.md`

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use aegis_core::PageRenderContext;
use aegis_theme::{ThemeEngine, ThemeManifest};
use anyhow::{bail, Context};
use axum::extract::{Path as UrlPath, Query, Request, State};
use axum::http::{header, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use tokio::sync::watch;
use tower_http::services::ServeDir;

/// `theme dev` の引数
#[derive(clap::Args, Debug)]
pub struct DevArgs {
    /// Theme directory (contains theme.json, templates/ and assets/), e.g. themes/trailhead
    pub dir: PathBuf,
    #[arg(short = 'H', long, default_value = "127.0.0.1")]
    pub host: String,
    #[arg(short, long, default_value_t = 8090)]
    pub port: u16,
    /// Language code for rendering (e.g. "en" or "ja")
    #[arg(short = 'l', long, default_value = "en")]
    pub locale: String,
}

/// 開いているページの再読み込みを待つ長さ（これを過ぎたら同じ版を返し、ページ側がつなぎ直す）
const LONG_POLL: Duration = Duration::from_secs(25);
const WATCH_INTERVAL: Duration = Duration::from_millis(300);

/// ページに差し込む自動再読み込みのスクリプト
const RELOAD_SCRIPT: &str = "<script>(async()=>{let v='';for(;;){try{const r=await fetch('/__dev/version'+(v===''?'':'?since='+v));const j=await r.json();if(v!==''&&String(j.version)!==v){location.reload();return}v=String(j.version)}catch(e){await new Promise(r=>setTimeout(r,1000))}}})()</script>";

#[derive(Clone)]
struct DevState {
    dir: PathBuf,
    engine: ThemeEngine,
    version: watch::Receiver<u64>,
    /// ループバックで待ち受けているか。そうでなければファイルの書き込み API は使えない
    loopback: bool,
}

/// 書き込み API に付けるヘッダー。ほかのサイトのページは勝手に付けられない（CORS の事前確認が要る）ので、
/// そこからの書き込みを防げる
const WRITE_HEADER: &str = "x-aegis-dev";

/// 描画できる 1 ページ分のデータ
#[derive(Debug, Clone)]
struct Sample {
    name: String,
    /// `file`（`samples/<name>.json`）か `builtin`（テンプレートごとの既定）
    source: &'static str,
    context: PageRenderContext,
}

#[derive(Debug, Serialize)]
struct SampleInfo {
    name: String,
    template: String,
    source: &'static str,
    preview_url: String,
}

pub async fn run(args: DevArgs) -> anyhow::Result<()> {
    let dir = args
        .dir
        .canonicalize()
        .with_context(|| format!("Theme directory not found: {}", args.dir.display()))?;
    if !dir.join("templates").is_dir() {
        bail!(
            "{} has no templates/ directory (is it a theme directory?)",
            dir.display()
        );
    }
    let themes_dir = dir.parent().context("The theme directory has no parent")?;
    let theme_id = dir
        .file_name()
        .and_then(|n| n.to_str())
        .context("Invalid theme directory name")?;
    if !dir.join("theme.json").is_file() {
        tracing::warn!("{} has no theme.json (needed to install it)", dir.display());
    }

    let engine = ThemeEngine::new(themes_dir, theme_id)
        .with_template_reload(true)
        .with_locale(&args.locale);
    let (tx, rx) = watch::channel(0u64);
    tokio::spawn(watch_files(dir.clone(), tx));

    let addr: SocketAddr = format!("{}:{}", args.host, args.port)
        .parse()
        .with_context(|| format!("Invalid address {}:{}", args.host, args.port))?;
    let state = DevState {
        dir: dir.clone(),
        engine,
        version: rx,
        loopback: addr.ip().is_loopback(),
    };
    if !state.loopback {
        tracing::warn!("Not listening on loopback: editing files through the API is disabled");
    }
    let app = router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!(
        "Theme '{}' ({})",
        theme_id,
        dir.display().to_string().trim_start_matches(r"\\?\")
    );
    println!("Preview: http://{}/", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

fn router(state: DevState) -> Router {
    let assets = ServeDir::new(state.dir.join("assets"));
    let theme_id = state
        .dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string();
    Router::new()
        .route("/", get(index_page))
        .route("/preview/{name}", get(preview_page))
        .route("/__dev/version", get(version))
        .route("/__dev/api/theme", get(api_theme))
        .route("/__dev/api/samples/{name}", get(api_sample))
        .route("/__dev/api/check", get(api_check))
        .route("/__dev/api/files", get(api_files))
        .route(
            "/__dev/api/files/{*path}",
            get(api_read_file).put(api_write_file).delete(api_delete_file),
        )
        .route("/__dev/api/rename", post(api_rename_file))
        .route("/__dev/api/render", post(api_render))
        .route("/__dev/api/package/validate", get(api_package_validate))
        .route("/__dev/api/package/sign", post(api_package_sign))
        .route("/__dev/api/package/export", get(api_package_export))
        // 公開サイトと同じ URL（テンプレートが `/theme/assets/...` や `/themes/<id>/assets/...` で参照する）
        .nest_service("/theme/assets", assets.clone())
        .nest_service(&format!("/themes/{}/assets", theme_id), assets)
        .layer(middleware::from_fn_with_state(state.clone(), guard))
        .with_state(state)
}

/// `Host` がループバックの名前か（DNS リバインディングで、ほかのサイトが読み書きするのを防ぐ）
fn is_loopback_host(host: &str) -> bool {
    let name = if let Some(rest) = host.strip_prefix('[') {
        rest.split(']').next().unwrap_or("")
    } else {
        host.split(':').next().unwrap_or("")
    };
    matches!(name, "localhost" | "127.0.0.1" | "::1")
}

/// ループバックで待ち受けているときは `Host` を検査し、読み出し以外にはヘッダー `x-aegis-dev` を求める
async fn guard(State(state): State<DevState>, req: Request, next: Next) -> Response {
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
    if !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS)
        && !req.headers().contains_key(WRITE_HEADER)
    {
        return (
            StatusCode::FORBIDDEN,
            format!("Missing {WRITE_HEADER} header"),
        )
            .into_response();
    }
    next.run(req).await
}

// ---------------------------------------------------------------------------
// ファイルの監視
// ---------------------------------------------------------------------------

/// フォルダー内のファイルの更新時刻・大きさ・数。変わったら保存されたとみなす
fn fingerprint(dir: &Path) -> (u64, u64, SystemTime) {
    let mut count = 0u64;
    let mut size = 0u64;
    let mut latest = SystemTime::UNIX_EPOCH;
    let walker = walkdir::WalkDir::new(dir).into_iter().filter_entry(|e| {
        let name = e.file_name().to_string_lossy();
        !(name.starts_with('.') || name == "node_modules")
    });
    for entry in walker.flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        count += 1;
        if let Ok(meta) = entry.metadata() {
            size += meta.len();
            if let Ok(modified) = meta.modified() {
                latest = latest.max(modified);
            }
        }
    }
    (count, size, latest)
}

async fn watch_files(dir: PathBuf, tx: watch::Sender<u64>) {
    let scan = |dir: PathBuf| async move {
        tokio::task::spawn_blocking(move || fingerprint(&dir))
            .await
            .ok()
    };
    let mut last = scan(dir.clone()).await;
    loop {
        tokio::time::sleep(WATCH_INTERVAL).await;
        let current = scan(dir.clone()).await;
        if current != last {
            last = current;
            tx.send_modify(|v| *v += 1);
        }
        if tx.is_closed() {
            return;
        }
    }
}

#[derive(Deserialize)]
struct VersionQuery {
    since: Option<u64>,
}

/// `GET /__dev/version?since=N` — 版が N 以外になる（ファイルが保存される）まで待って、今の版を返す
async fn version(State(state): State<DevState>, Query(q): Query<VersionQuery>) -> Json<Value> {
    let mut rx = state.version.clone();
    if let Some(since) = q.since {
        // `changed()` は受信側ごとの「見た」状態に左右されるので、現在の版を直接比べて待つ
        let _ = tokio::time::timeout(LONG_POLL, rx.wait_for(|v| *v != since)).await;
    }
    let version = *rx.borrow();
    Json(json!({ "version": version }))
}

// ---------------------------------------------------------------------------
// サンプルのデータ
// ---------------------------------------------------------------------------

/// `templates/` 直下の `*.html`（名前順）
fn list_templates(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir.join("templates"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.ends_with(".html"))
        .collect();
    names.sort();
    names
}

/// テンプレートごとの既定のサンプル。実サイトの `PageRenderContext` に近い形にする
fn default_sample(template: &str) -> Value {
    let stem = template.trim_end_matches(".html");
    let content_type = if stem == "article" { "article" } else { "page" };
    let items: Vec<Value> = (1..=3)
        .map(|n| {
            json!({
                "id": format!("sample-{n}"),
                "content_type": "article",
                "title": format!("サンプル記事 {n}"),
                "slug": format!("sample-{n}"),
                "excerpt": "テーマの見た目を確かめるためのサンプルの抜粋です。長い文章がどう折り返されるかも見てください。",
                "author_id": "editor",
                "published_at": format!("2026-10-{:02}T09:00:00Z", 20 + n),
                "created_at": format!("2026-10-{:02}T09:00:00Z", 20 + n),
            })
        })
        .collect();
    let mut context = json!({
        "site_title": "Sample Site",
        "site_description": "テーマ開発用のサンプルサイトです。",
        "template": template,
        "content_type": content_type,
        "title": "サンプルのタイトル",
        "slug": "sample",
        "content_html": "<h2>見出し</h2><p>これは本文のサンプルです。<a href=\"#\">リンク</a>や<strong>強調</strong>の見た目を確かめられます。</p><ul><li>リスト 1</li><li>リスト 2</li></ul><blockquote>引用のサンプル</blockquote>",
        "author": "editor",
        "published_at": "2026-10-20T09:00:00Z",
        "updated_at": "2026-10-25T18:30:00Z",
    });
    if stem == "notices" {
        // お知らせバナー: 閉じられるもの・リンクの文字を決めたもの・決めていないもの
        context["fields"] = json!({
            "notices": [
                { "id": "n1", "message": "今夜 0 時から 2 時まで、メンテナンスのため一部の機能をお使いいただけません。",
                  "link_url": "/status", "link_label": "", "style": "warning", "dismissible": true, "rev": "r1" },
                { "id": "n2", "message": "秋のセールを開催中です。", "link_url": "/sale", "link_label": "セールを見る",
                  "style": "info", "dismissible": true, "rev": "r2" },
                { "id": "n3", "message": "ご注文が完了しました。", "link_url": "", "link_label": "",
                  "style": "success", "dismissible": false, "rev": "r3" },
            ],
            "preview": true,
        });
    }
    if stem == "consent" {
        // 同意バナー: 最初から表示する（`preview`）。設定の既定と、既定の名前の区分・名前を決めた区分
        context["fields"] = json!({
            "consent": {
                "layout": "banner", "position": "bottom", "backdrop": false, "policy_url": "/privacy",
                "reopen_button": true, "delay_ms": 0, "cookie_days": 180,
            },
            "message": "",
            "categories": [
                { "id": "analytics", "label": "", "description": "" },
                { "id": "marketing", "label": "", "description": "" },
                { "id": "video", "label": "埋め込みの動画", "description": "YouTube などの動画を表示します。" },
            ],
            "preview": true,
        });
    }
    if stem == "index" {
        context["items"] = Value::Array(items);
        context["pagination"] = json!({
            "page": 2, "per_page": 3, "total_items": 9, "total_pages": 3,
            "prev_url": "/?page=1", "next_url": "/?page=3",
        });
    }
    context
}

/// 既定のデータに `overlay`（`samples/<name>.json`）の最上位のキーを重ねて、描画用の値にする
fn build_context(template: &str, overlay: Value) -> Result<PageRenderContext, String> {
    let mut merged =
        serde_json::to_value(PageRenderContext::default()).map_err(|e| e.to_string())?;
    for layer in [default_sample(template), overlay] {
        if let (Some(base), Value::Object(layer)) = (merged.as_object_mut(), layer) {
            base.extend(layer);
        }
    }
    serde_json::from_value(merged).map_err(|e| e.to_string())
}

/// 描画できるサンプルの一覧と、読めなかった `samples/*.json` のエラー。
/// 既定は `templates/` の `base.html` 以外すべて。`samples/<name>.json` があれば上書きし、
/// 無い名前のものは追加する（`template` を省くと `<name>.html`）。保存のたびに読み直す
fn load_samples(dir: &Path) -> (Vec<Sample>, Vec<String>) {
    load_samples_with(dir, &HashMap::new())
}

/// `load_samples` で、`overrides`（名前 → JSON の文字列）を `samples/<名前>.json` の代わりに使う
/// （保存していない編集中の内容でプレビューするため）
fn load_samples_with(
    dir: &Path,
    overrides: &HashMap<String, String>,
) -> (Vec<Sample>, Vec<String>) {
    let mut errors = Vec::new();
    let mut samples: Vec<Sample> = list_templates(dir)
        .into_iter()
        .filter(|t| t != "base.html")
        .filter_map(|template| {
            let name = template.trim_end_matches(".html").to_string();
            let context = build_context(&template, Value::Null).ok()?;
            Some(Sample {
                name,
                source: "builtin",
                context,
            })
        })
        .collect();

    let mut files: Vec<PathBuf> = std::fs::read_dir(dir.join("samples"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "json"))
        .collect();
    // 新しく作って、まだ保存していないサンプルも対象にする
    for name in overrides.keys() {
        let path = dir.join("samples").join(format!("{name}.json"));
        if !files.contains(&path) {
            files.push(path);
        }
    }
    files.sort();
    for path in files {
        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let text = match overrides.get(name) {
            Some(text) => Ok(text.clone()),
            None => std::fs::read_to_string(&path),
        };
        let loaded = text
            .map_err(|e| e.to_string())
            .and_then(|text| serde_json::from_str::<Value>(&text).map_err(|e| e.to_string()))
            .and_then(|overlay| {
                let template = overlay
                    .get("template")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("{name}.html"));
                build_context(&template, overlay)
            });
        match loaded {
            Ok(context) => {
                let sample = Sample {
                    name: name.to_string(),
                    source: "file",
                    context,
                };
                match samples.iter_mut().find(|s| s.name == name) {
                    Some(existing) => *existing = sample,
                    None => samples.push(sample),
                }
            }
            Err(e) => errors.push(format!("samples/{name}.json: {e}")),
        }
    }
    (samples, errors)
}

// ---------------------------------------------------------------------------
// ページ
// ---------------------------------------------------------------------------

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// 断片のテンプレート（`</body>` の前に差し込む同意バナー）は、プレビューのために文書で包み、組み込みの CSS を付ける
/// （テーマが自分の CSS で作るなら、`<style>` を足しても上書きできる）。ほかのテンプレートはそのまま
fn as_document(template: &str, html: String) -> String {
    if template == "notices.html" {
        // お知らせバナーはページの上に出る。閉じるボタンは、スクリプトが動くサイトでだけ出る
        return format!(
            "<!doctype html><html><head><meta charset=\"utf-8\"><style>{css}</style></head><body style=\"margin:0;font-family:system-ui\">{html}<p style=\"opacity:.55;margin:1rem\">(the page below the notices)</p></body></html>",
            css = crate::notices::style(),
            html = html
        );
    }
    if template != "consent.html" {
        return html;
    }
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><style>{css}</style></head><body style=\"min-height:100vh;margin:0;font-family:system-ui\"><p style=\"opacity:.55;margin:1rem\">(the page behind the banner)</p>{html}</body></html>",
        css = crate::consent::style(),
        html = html
    )
}

fn with_reload(mut html: String) -> String {
    match html.rfind("</body>") {
        Some(at) => html.insert_str(at, RELOAD_SCRIPT),
        None => html.push_str(RELOAD_SCRIPT),
    }
    html
}

fn error_page(title: &str, detail: &str) -> Html<String> {
    Html(with_reload(format!(
        "<!doctype html><meta charset=\"utf-8\"><title>{t}</title><body style=\"font-family:system-ui;margin:2rem\"><h1 style=\"color:#b00020\">{t}</h1><pre style=\"white-space:pre-wrap;background:#fdecea;padding:1rem;border-radius:6px\">{d}</pre></body>",
        t = escape(title),
        d = escape(detail)
    )))
}

/// `GET /` — サンプルの一覧
async fn index_page(State(state): State<DevState>) -> Html<String> {
    let (samples, errors) = load_samples(&state.dir);
    let name = ThemeManifest::load_from_dir(&state.dir)
        .map(|m| m.name)
        .unwrap_or_else(|| "(no theme.json)".to_string());
    let mut body = format!(
        "<!doctype html><meta charset=\"utf-8\"><title>Theme dev: {n}</title><body style=\"font-family:system-ui;margin:2rem;max-width:40rem\"><h1>{n}</h1><p>Pick a page to preview. It reloads when you save a file.</p><ul>",
        n = escape(&name)
    );
    for s in &samples {
        body.push_str(&format!(
            "<li><a href=\"/preview/{n}\">{n}</a> <small>({t}, {src})</small></li>",
            n = escape(&s.name),
            t = escape(&s.context.template),
            src = s.source
        ));
    }
    body.push_str("</ul>");
    for e in &errors {
        body.push_str(&format!("<p style=\"color:#b00020\">{}</p>", escape(e)));
    }
    body.push_str("</body>");
    Html(with_reload(body))
}

/// `GET /preview/{name}` — サンプルをテーマのテンプレートで描画する。失敗したときもページに理由を出す
async fn preview_page(State(state): State<DevState>, UrlPath(name): UrlPath<String>) -> Response {
    let (samples, _) = load_samples(&state.dir);
    let Some(sample) = samples.into_iter().find(|s| s.name == name) else {
        return (
            StatusCode::NOT_FOUND,
            error_page("No such sample", &format!("'{name}' is not a sample")),
        )
            .into_response();
    };
    match state
        .engine
        .render_diagnosed(&sample.context.template, &sample.context)
    {
        Ok(html) => Html(with_reload(as_document(&sample.context.template, html))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            error_page(
                &format!(
                    "Template error in {}{}",
                    e.template.as_deref().unwrap_or(&sample.context.template),
                    e.line.map(|l| format!(":{l}")).unwrap_or_default()
                ),
                &e.message,
            ),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// JSON API（Theme Builder UI 用）
// ---------------------------------------------------------------------------

/// `GET /__dev/api/theme` — マニフェスト・テンプレート・サンプルの一覧
async fn api_theme(State(state): State<DevState>) -> Json<Value> {
    let (samples, errors) = load_samples(&state.dir);
    let samples: Vec<SampleInfo> = samples
        .into_iter()
        .map(|s| SampleInfo {
            preview_url: format!("/preview/{}", s.name),
            template: s.context.template,
            source: s.source,
            name: s.name,
        })
        .collect();
    Json(json!({
        "manifest": ThemeManifest::load_from_dir(&state.dir),
        "templates": list_templates(&state.dir),
        "samples": samples,
        "sample_errors": errors,
        "version": *state.version.borrow(),
    }))
}

/// `GET /__dev/api/samples/{name}` — 描画に使うデータそのもの
async fn api_sample(
    State(state): State<DevState>,
    UrlPath(name): UrlPath<String>,
) -> Result<Json<PageRenderContext>, StatusCode> {
    load_samples(&state.dir)
        .0
        .into_iter()
        .find(|s| s.name == name)
        .map(|s| Json(s.context))
        .ok_or(StatusCode::NOT_FOUND)
}

/// `GET /__dev/api/check` — すべてのサンプルを描画して、失敗したものを返す
async fn api_check(State(state): State<DevState>) -> Json<Value> {
    let (samples, load_errors) = load_samples(&state.dir);
    let results: Vec<Value> = samples
        .iter()
        .map(|s| {
            match state
                .engine
                .render_diagnosed(&s.context.template, &s.context)
            {
                Ok(_) => json!({ "name": s.name, "template": s.context.template, "ok": true }),
                Err(e) => json!({
                    "name": s.name,
                    "template": s.context.template,
                    "ok": false,
                    "error": e,
                }),
            }
        })
        .collect();
    let ok = load_errors.is_empty() && results.iter().all(|r| r["ok"] == true);
    Json(json!({ "ok": ok, "results": results, "sample_errors": load_errors }))
}

// ---------------------------------------------------------------------------
// ファイルの読み書きと、未保存の内容でのプレビュー
// ---------------------------------------------------------------------------

/// 1 つのファイルの大きさの上限
const MAX_FILE_BYTES: usize = 1024 * 1024;
/// 読み書きできる拡張子（テキストのみ）
const TEXT_EXTENSIONS: [&str; 8] = ["html", "css", "js", "mjs", "json", "svg", "txt", "md"];

type ApiError = (StatusCode, Json<Value>);

fn api_error(status: StatusCode, message: impl Into<String>) -> ApiError {
    (status, Json(json!({ "error": message.into() })))
}

/// API で扱えるファイルの相対パス（`theme.json`、または `templates/`・`samples/`・`assets/` の下の
/// テキストファイル）か。`..`・絶対パス・隠しファイルは含めない
fn is_editable_path(rel: &str) -> bool {
    let path = Path::new(rel);
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(part) => match part.to_str() {
                Some(part) if !part.starts_with('.') => parts.push(part),
                _ => return false,
            },
            _ => return false,
        }
    }
    let text = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| TEXT_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()));
    match parts.as_slice() {
        ["theme.json"] | ["signature.json"] => true,
        [top, _, ..] => matches!(*top, "templates" | "samples" | "assets") && text,
        _ => false,
    }
}

/// `rel` の実際のパス。いちばん近い既存のフォルダーを実体に直して、テーマのフォルダーの外
/// （シンボリックリンク経由など）に出ないことを確かめる
fn resolve_editable(dir: &Path, rel: &str) -> Result<PathBuf, ApiError> {
    if !is_editable_path(rel) {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            format!("'{rel}' is not an editable theme file"),
        ));
    }
    let target = dir.join(rel);
    let mut existing = target.as_path();
    while !existing.exists() {
        existing = existing
            .parent()
            .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "Invalid path"))?;
    }
    let real = existing
        .canonicalize()
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, e.to_string()))?;
    if !real.starts_with(dir) {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "The path leaves the theme directory",
        ));
    }
    Ok(target)
}

/// `GET /__dev/api/files` — 読み書きできるファイルの一覧
async fn api_files(State(state): State<DevState>) -> Json<Value> {
    let mut files: Vec<Value> = walkdir::WalkDir::new(&state.dir)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| {
            let rel = e
                .path()
                .strip_prefix(&state.dir)
                .ok()?
                .to_string_lossy()
                .replace('\\', "/");
            let kind = match rel.split('/').next()? {
                "theme.json" | "signature.json" => "manifest",
                "templates" => "template",
                "samples" => "sample",
                "assets" => "asset",
                _ => return None,
            };
            Some(json!({ "path": rel, "kind": kind, "editable": is_editable_path(&rel) }))
        })
        .collect();
    files.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    Json(json!({ "files": files, "writable": state.loopback }))
}

/// `GET /__dev/api/files/{*path}` — ファイルの中身
async fn api_read_file(
    State(state): State<DevState>,
    UrlPath(rel): UrlPath<String>,
) -> Result<Json<Value>, ApiError> {
    let path = resolve_editable(&state.dir, &rel)?;
    let bytes = std::fs::read(&path)
        .map_err(|e| api_error(StatusCode::NOT_FOUND, format!("{rel}: {e}")))?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(api_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "The file is too large",
        ));
    }
    let content = String::from_utf8(bytes).map_err(|_| {
        api_error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "The file is not UTF-8 text",
        )
    })?;
    Ok(Json(json!({ "path": rel, "content": content })))
}

#[derive(Deserialize)]
struct WriteFile {
    content: String,
}

/// `PUT /__dev/api/files/{*path}` — ファイルを保存する（新しいファイルも作れる）
async fn api_write_file(
    State(state): State<DevState>,
    UrlPath(rel): UrlPath<String>,
    Json(body): Json<WriteFile>,
) -> Result<StatusCode, ApiError> {
    if !state.loopback {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            "Editing is only available when listening on a loopback address",
        ));
    }
    if body.content.len() > MAX_FILE_BYTES {
        return Err(api_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "The file is too large",
        ));
    }
    let path = resolve_editable(&state.dir, &rel)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }
    // 作ったフォルダーも含めて、もう一度外に出ていないか確かめる
    resolve_editable(&state.dir, &rel)?;
    std::fs::write(&path, body.content)
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /__dev/api/files/{*path}` — ファイルを削除する
async fn api_delete_file(
    State(state): State<DevState>,
    UrlPath(rel): UrlPath<String>,
) -> Result<StatusCode, ApiError> {
    if !state.loopback {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            "Editing is only available when listening on a loopback address",
        ));
    }
    if rel == "theme.json" {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "theme.json cannot be deleted",
        ));
    }
    let path = resolve_editable(&state.dir, &rel)?;
    if !path.is_file() {
        return Err(api_error(
            StatusCode::NOT_FOUND,
            format!("'{rel}' does not exist"),
        ));
    }
    std::fs::remove_file(&path)
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct RenameRequest {
    from: String,
    to: String,
}

/// `POST /__dev/api/rename` — ファイルを改名・移動する
async fn api_rename_file(
    State(state): State<DevState>,
    Json(body): Json<RenameRequest>,
) -> Result<StatusCode, ApiError> {
    if !state.loopback {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            "Editing is only available when listening on a loopback address",
        ));
    }
    if body.from == "theme.json"
        || body.to == "theme.json"
        || body.from == "signature.json"
        || body.to == "signature.json"
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "theme.json cannot be renamed",
        ));
    }
    let from_path = resolve_editable(&state.dir, &body.from)?;
    if !from_path.is_file() {
        return Err(api_error(
            StatusCode::NOT_FOUND,
            format!("'{}' does not exist", body.from),
        ));
    }
    let to_path = resolve_editable(&state.dir, &body.to)?;
    if to_path.exists() {
        return Err(api_error(
            StatusCode::CONFLICT,
            format!("'{}' already exists", body.to),
        ));
    }
    if let Some(parent) = to_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }
    // 作ったフォルダーも含めて、もう一度外に出ていないか確かめる
    resolve_editable(&state.dir, &body.to)?;
    std::fs::rename(&from_path, &to_path)
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct RenderRequest {
    /// 描画するサンプルの名前
    sample: String,
    /// 特定のブロックのみを描画する場合のブロック名（省略時はページ全体）
    #[serde(default)]
    block: Option<String>,
    /// 保存していないテンプレート（`templates/` からの名前 → 中身。例: `article.html`）
    #[serde(default)]
    templates: HashMap<String, String>,
    /// 保存していないサンプル（名前 → `samples/<名前>.json` の中身）
    #[serde(default)]
    samples: HashMap<String, String>,
}

fn wrap_block_preview(block_html: &str, locale: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="{locale}">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <link rel="stylesheet" href="/theme/assets/style.css">
  <style>
    body {{
      margin: 0;
      padding: 1.5rem;
      background: var(--bg, #ffffff);
      color: var(--text, #111827);
      font-family: system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    }}
  </style>
</head>
<body>
{block_html}
</body>
</html>"#
    )
}

fn annotate_template_source(source: &str, template_name: &str) -> String {
    let mut out = String::with_capacity(source.len() + 512);
    for (line_idx, line) in source.lines().enumerate() {
        let line_num = line_idx + 1;
        let mut cursor = line;
        while let Some(pos) = cursor.find('<') {
            out.push_str(&cursor[..pos]);
            let rest = &cursor[pos + 1..];
            if rest.starts_with('/') || rest.starts_with('!') || rest.starts_with('?') {
                out.push('<');
                cursor = rest;
                continue;
            }
            let tag_len = rest
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
                .unwrap_or(rest.len());
            let tag_name = &rest[..tag_len];
            let is_ui = matches!(
                tag_name.to_ascii_lowercase().as_str(),
                "body"
                    | "header"
                    | "footer"
                    | "nav"
                    | "main"
                    | "section"
                    | "article"
                    | "aside"
                    | "div"
                    | "p"
                    | "h1"
                    | "h2"
                    | "h3"
                    | "h4"
                    | "h5"
                    | "h6"
                    | "ul"
                    | "ol"
                    | "li"
                    | "a"
                    | "button"
                    | "form"
                    | "input"
                    | "textarea"
                    | "select"
                    | "option"
                    | "label"
                    | "span"
                    | "img"
                    | "table"
                    | "thead"
                    | "tbody"
                    | "tr"
                    | "td"
                    | "th"
                    | "figure"
                    | "figcaption"
                    | "time"
                    | "small"
                    | "blockquote"
                    | "details"
                    | "summary"
            );
            if is_ui {
                out.push('<');
                out.push_str(tag_name);
                out.push_str(&format!(
                    r#" data-aegis-source="templates/{template_name}:{line_num}""#
                ));
                cursor = &rest[tag_len..];
            } else {
                out.push('<');
                cursor = rest;
            }
        }
        out.push_str(cursor);
        out.push('\n');
    }
    if !source.ends_with('\n') && out.ends_with('\n') {
        out.pop();
    }
    out
}

/// `POST /__dev/api/render` — 保存していない編集中の内容を反映して描画する。何も書き込まない
async fn api_render(State(state): State<DevState>, Json(req): Json<RenderRequest>) -> Json<Value> {
    let (samples, sample_errors) = load_samples_with(&state.dir, &req.samples);
    let Some(sample) = samples.into_iter().find(|s| s.name == req.sample) else {
        return Json(json!({
            "ok": false,
            "error": {
                "message": format!("'{}' is not a sample", req.sample),
                "template": null,
                "line": null,
            },
            "sample_errors": sample_errors,
        }));
    };
    // 共有のエンジンには触らず、上書きを入れた別のエンジンで描画する
    let engine = ThemeEngine::new(state.engine.themes_dir(), &state.engine.active_theme())
        .with_locale(&state.engine.locale())
        .with_template_reload(true);

    let templates_dir = state.dir.join("templates");
    if let Ok(entries) = std::fs::read_dir(&templates_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("html") {
                if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
                    if !req.templates.contains_key(file_name) {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            let annotated = annotate_template_source(&content, file_name);
                            engine.register_template(file_name, &annotated);
                        }
                    }
                }
            }
        }
    }

    for (name, content) in &req.templates {
        let annotated = annotate_template_source(content, name);
        engine.register_template(name, &annotated);
    }

    let blocks = engine.template_blocks(&sample.context.template);

    if let Some(ref block_name) = req.block {
        match engine.render_block_diagnosed(&sample.context.template, block_name, &sample.context) {
            Ok(block_html) => {
                let wrapped = wrap_block_preview(&block_html, &state.engine.locale());
                Json(json!({
                    "ok": true,
                    "html": wrapped,
                    "sample_errors": sample_errors,
                    "blocks": blocks,
                }))
            }
            Err(e) => Json(json!({
                "ok": false,
                "error": e,
                "sample_errors": sample_errors,
                "blocks": blocks,
            })),
        }
    } else {
        match engine.render_diagnosed(&sample.context.template, &sample.context) {
            Ok(html) => Json(json!({
                "ok": true,
                "html": as_document(&sample.context.template, html),
                "sample_errors": sample_errors,
                "blocks": blocks,
            })),
            Err(e) => Json(json!({
                "ok": false,
                "error": e,
                "sample_errors": sample_errors,
                "blocks": blocks,
            })),
        }
    }
}

#[derive(Serialize)]
struct PackageValidationResult {
    ok: bool,
    manifest: Option<ThemeManifest>,
    signed: bool,
    public_key: Option<String>,
    errors: Vec<String>,
}

/// `GET /__dev/api/package/validate` — テーマパッケージとしての適合性を検証する
async fn api_package_validate(State(state): State<DevState>) -> Json<PackageValidationResult> {
    let mut errors = Vec::new();
    let manifest_result = state
        .engine
        .validate_theme_package_with_policy(&state.dir, None, false);

    let (manifest, signature_key) = match manifest_result {
        Ok(m) => {
            let sig_file = state.dir.join("signature.json");
            let pk = if sig_file.exists() {
                match aegis_core::verify_package_signature(&state.dir, None, false) {
                    Ok(k) => k,
                    Err(e) => {
                        errors.push(format!("Signature verification error: {}", e));
                        None
                    }
                }
            } else {
                None
            };
            (Some(m), pk)
        }
        Err(e) => {
            errors.push(e.to_string());
            (None, None)
        }
    };

    let ok = errors.is_empty();
    let signed = signature_key.is_some();

    Json(PackageValidationResult {
        ok,
        manifest,
        signed,
        public_key: signature_key,
        errors,
    })
}

#[derive(Deserialize)]
struct PackageSignRequest {
    #[serde(default)]
    private_key: Option<String>,
    #[serde(default)]
    generate_key: bool,
}

#[derive(Serialize)]
struct PackageSignResponse {
    ok: bool,
    public_key: String,
    generated_private_key: Option<String>,
    signature: String,
}

/// `POST /__dev/api/package/sign` — テーマディレクトリを署名し、signature.json を保存する
async fn api_package_sign(
    State(state): State<DevState>,
    Json(req): Json<PackageSignRequest>,
) -> std::result::Result<Json<PackageSignResponse>, (StatusCode, Json<Value>)> {
    let (priv_hex, generated_priv) =
        if req.generate_key || req.private_key.as_deref().unwrap_or("").trim().is_empty() {
            let (priv_k, _pub_k) = aegis_core::generate_ed25519_keypair().map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": format!("Failed to generate keypair: {}", e) })),
                )
            })?;
            (priv_k.clone(), Some(priv_k))
        } else {
            let priv_k = req.private_key.unwrap().trim().to_string();
            (priv_k, None)
        };

    let sig = aegis_core::sign_package_dir(&state.dir, &priv_hex).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Failed to sign package: {}", e) })),
        )
    })?;

    // change will be detected by watcher loop

    Ok(Json(PackageSignResponse {
        ok: true,
        public_key: sig.public_key,
        generated_private_key: generated_priv,
        signature: sig.signature,
    }))
}

/// `GET /__dev/api/package/export` — テーマディレクトリを ZIP アーカイブとしてエクスポートする
async fn api_package_export(
    State(state): State<DevState>,
) -> std::result::Result<Response, (StatusCode, String)> {
    let manifest = ThemeManifest::load_from_dir(&state.dir)
        .unwrap_or_else(|| ThemeManifest::fallback_from_id(&state.engine.active_theme()));
    let zip_bytes = aegis_core::create_theme_zip(&state.dir).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create ZIP: {}", e),
        )
    })?;

    let filename = format!("{}-{}.zip", manifest.id, manifest.version);
    let disposition = format!(r#"attachment; filename="{}""#, filename);

    let mut response = Response::new(axum::body::Body::from(zip_bytes));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/zip"),
    );
    if let Ok(disp_val) = header::HeaderValue::from_str(&disposition) {
        response
            .headers_mut()
            .insert(header::CONTENT_DISPOSITION, disp_val);
    }

    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use tower::ServiceExt;

    fn theme_dir() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let theme = tmp.path().join("mytheme");
        std::fs::create_dir_all(theme.join("templates")).unwrap();
        std::fs::create_dir_all(theme.join("assets")).unwrap();
        std::fs::write(
            theme.join("theme.json"),
            r#"{"id":"mytheme","name":"My Theme"}"#,
        )
        .unwrap();
        std::fs::write(
            theme.join("templates/base.html"),
            "<html lang=\"{{ locale }}\"><body>{% block content %}{% endblock %}</body></html>",
        )
        .unwrap();
        std::fs::write(
            theme.join("templates/index.html"),
            r#"{% extends "base.html" %}{% block content %}{{ site_title }}:{% for i in items %}[{{ i.title }}]{% endfor %}{% endblock %}"#,
        )
        .unwrap();
        std::fs::write(
            theme.join("templates/article.html"),
            r#"{% extends "base.html" %}{% block content %}<h1>{{ title }}</h1>{{ content_html | safe }}{% endblock %}"#,
        )
        .unwrap();
        std::fs::write(theme.join("assets/app.css"), "body{}").unwrap();
        tmp
    }

    fn app(tmp: &tempfile::TempDir) -> (Router, watch::Sender<u64>) {
        let dir = tmp.path().join("mytheme").canonicalize().unwrap();
        let engine = ThemeEngine::new(dir.parent().unwrap(), "mytheme").with_template_reload(true);
        let (tx, rx) = watch::channel(0u64);
        let state = DevState {
            dir,
            engine,
            version: rx,
            loopback: true,
        };
        (router(state), tx)
    }

    async fn get_text(app: &Router, uri: &str) -> (StatusCode, String) {
        let res = app
            .clone()
            .oneshot(
                Request::get(uri)
                    .header("host", "localhost:8090")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = res.status();
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    #[tokio::test]
    async fn test_preview_respects_locale_setting() {
        let tmp = theme_dir();
        let dir = tmp.path().join("mytheme").canonicalize().unwrap();
        let engine = ThemeEngine::new(dir.parent().unwrap(), "mytheme")
            .with_template_reload(true)
            .with_locale("ja");
        let (_tx, rx) = watch::channel(0u64);
        assert_eq!(engine.locale(), "ja");
        let state = DevState {
            dir,
            engine,
            version: rx,
            loopback: true,
        };
        let app = router(state);
        let (status, html) = get_text(&app, "/preview/index").await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("<html lang=\"ja\">"), "{html}");
        assert!(html.contains("Sample Site:[サンプル記事 1]"), "{html}");
    }

    /// The cookie banner is a fragment (it goes before `</body>` on the site): the preview shows it
    /// right away, on a page of its own with the built-in styles
    #[tokio::test]
    async fn test_the_consent_banner_template_is_previewed_as_a_visible_document() {
        let tmp = theme_dir();
        std::fs::write(
            tmp.path().join("mytheme/templates/consent.html"),
            r#"<aside data-aegis-consent-root{% if not fields.preview %} hidden{% endif %} class="mine-{{ fields.consent.layout }}">{% for c in fields.categories %}[{{ c.id }}]{% endfor %}</aside>"#,
        )
        .unwrap();
        let (app, _tx) = app(&tmp);
        let (status, html) = get_text(&app, "/preview/consent").await;
        assert_eq!(status, StatusCode::OK, "{html}");
        assert!(html.starts_with("<!doctype html>"), "{html}");
        assert!(
            html.contains(".aegis-consent"),
            "the built-in styles are included: {html}"
        );
        assert!(
            html.contains(r#"<aside data-aegis-consent-root class="mine-banner">[analytics][marketing][video]</aside>"#),
            "{html}"
        );
        assert!(html.contains("/__dev/version"), "{html}");
    }

    #[tokio::test]
    async fn test_preview_renders_default_samples_and_injects_the_reload_script() {
        let tmp = theme_dir();
        let (app, _tx) = app(&tmp);

        let (status, html) = get_text(&app, "/preview/index").await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("Sample Site:[サンプル記事 1]"), "{html}");
        assert!(html.contains("/__dev/version"), "{html}");

        let (_, html) = get_text(&app, "/preview/article").await;
        assert!(
            html.contains("<h1>サンプルのタイトル</h1><h2>見出し</h2>"),
            "{html}"
        );
    }

    #[tokio::test]
    async fn test_sample_files_override_defaults_and_add_pages() {
        let tmp = theme_dir();
        let samples = tmp.path().join("mytheme/samples");
        std::fs::create_dir_all(&samples).unwrap();
        std::fs::write(samples.join("index.json"), r#"{"site_title":"Custom"}"#).unwrap();
        std::fs::write(
            samples.join("long-article.json"),
            r#"{"template":"article.html","title":"とても長いタイトル"}"#,
        )
        .unwrap();
        let (app, _tx) = app(&tmp);

        let (_, html) = get_text(&app, "/preview/index").await;
        assert!(html.contains("Custom:[サンプル記事 1]"), "{html}");
        let (_, html) = get_text(&app, "/preview/long-article").await;
        assert!(html.contains("<h1>とても長いタイトル</h1>"), "{html}");
    }

    #[tokio::test]
    async fn test_template_errors_are_shown_on_the_page_and_reported_by_check() {
        let tmp = theme_dir();
        std::fs::write(
            tmp.path().join("mytheme/templates/article.html"),
            "{% if %}broken",
        )
        .unwrap();
        let samples = tmp.path().join("mytheme/samples");
        std::fs::create_dir_all(&samples).unwrap();
        std::fs::write(samples.join("bad.json"), "{ not json").unwrap();
        let (app, _tx) = app(&tmp);

        let (status, html) = get_text(&app, "/preview/article").await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(html.contains("Template error in article.html"), "{html}");
        assert!(
            html.contains("/__dev/version"),
            "reloads once fixed: {html}"
        );

        let (_, body) = get_text(&app, "/__dev/api/check").await;
        let check: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(check["ok"], false);
        let failed: Vec<&Value> = check["results"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["ok"] == false)
            .collect();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0]["name"], "article");
        assert_eq!(check["sample_errors"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_edited_templates_are_picked_up_without_a_restart() {
        let tmp = theme_dir();
        let (app, _tx) = app(&tmp);
        let (_, before) = get_text(&app, "/preview/article").await;
        assert!(before.contains("<h1>"), "{before}");

        std::fs::write(
            tmp.path().join("mytheme/templates/article.html"),
            r#"{% extends "base.html" %}{% block content %}<h2>{{ title }}</h2>{% endblock %}"#,
        )
        .unwrap();
        let (_, after) = get_text(&app, "/preview/article").await;
        assert!(after.contains("<h2>サンプルのタイトル</h2>"), "{after}");
    }

    #[tokio::test]
    async fn test_theme_api_and_assets() {
        let tmp = theme_dir();
        let (app, _tx) = app(&tmp);

        let (_, body) = get_text(&app, "/__dev/api/theme").await;
        let theme: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(theme["manifest"]["name"], "My Theme");
        assert_eq!(
            theme["templates"],
            json!(["article.html", "base.html", "index.html"])
        );
        let names: Vec<&str> = theme["samples"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["article", "index"]);

        let (status, css) = get_text(&app, "/theme/assets/app.css").await;
        assert_eq!((status, css.as_str()), (StatusCode::OK, "body{}"));
        let (status, _) = get_text(&app, "/themes/mytheme/assets/app.css").await;
        assert_eq!(status, StatusCode::OK);
        let (status, _) = get_text(&app, "/preview/missing").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_version_returns_when_the_files_change() {
        let tmp = theme_dir();
        let (app, tx) = app(&tmp);

        // 古い版を持っているページには、待たずに今の版を返す
        tx.send_modify(|v| *v += 1);
        let (_, body) = get_text(&app, "/__dev/version?since=0").await;
        assert_eq!(serde_json::from_str::<Value>(&body).unwrap()["version"], 1);

        // 今の版を持っているページは、保存されるまで待つ
        let waiting = {
            let app = app.clone();
            tokio::spawn(async move { get_text(&app, "/__dev/version?since=1").await })
        };
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!waiting.is_finished());
        tx.send_modify(|v| *v += 1);
        let (_, body) = waiting.await.unwrap();
        assert_eq!(serde_json::from_str::<Value>(&body).unwrap()["version"], 2);
    }

    #[test]
    fn test_fingerprint_changes_on_save_and_ignores_hidden_files() {
        let tmp = theme_dir();
        let dir = tmp.path().join("mytheme");
        let before = fingerprint(&dir);
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::write(dir.join(".git/HEAD"), "x").unwrap();
        assert_eq!(fingerprint(&dir), before);
        std::fs::write(dir.join("assets/app.css"), "body{color:red}").unwrap();
        assert_ne!(fingerprint(&dir), before);
    }

    /// ヘッダー（`Host` は既定でループバック）つきでリクエストを送る
    async fn send(
        app: &Router,
        method: &str,
        uri: &str,
        headers: &[(&str, &str)],
        body: Option<Value>,
    ) -> (StatusCode, String) {
        let mut req = Request::builder().method(method).uri(uri);
        if !headers.iter().any(|(k, _)| *k == "host") {
            req = req.header("host", "localhost:8090");
        }
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let req = match body {
            Some(body) => req
                .header("content-type", "application/json")
                .body(Body::from(body.to_string())),
            None => req.body(Body::empty()),
        }
        .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    const DEV: (&str, &str) = ("x-aegis-dev", "1");

    #[tokio::test]
    async fn test_files_can_be_listed_read_and_saved() {
        let tmp = theme_dir();
        let (app, _tx) = app(&tmp);

        let (_, body) = send(&app, "GET", "/__dev/api/files", &[], None).await;
        let list: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(list["writable"], true);
        let paths: Vec<&str> = list["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["path"].as_str().unwrap())
            .collect();
        assert_eq!(
            paths,
            [
                "assets/app.css",
                "templates/article.html",
                "templates/base.html",
                "templates/index.html",
                "theme.json"
            ]
        );

        let (status, body) = send(&app, "GET", "/__dev/api/files/assets/app.css", &[], None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            serde_json::from_str::<Value>(&body).unwrap()["content"],
            "body{}"
        );

        // 新しいフォルダー・ファイルも作れる
        let (status, _) = send(
            &app,
            "PUT",
            "/__dev/api/files/samples/new.json",
            &[DEV],
            Some(json!({ "content": "{}" })),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let saved = std::fs::read_to_string(tmp.path().join("mytheme/samples/new.json")).unwrap();
        assert_eq!(saved, "{}");
    }

    #[tokio::test]
    async fn test_files_can_be_deleted_and_renamed() {
        let tmp = theme_dir();
        let (app, _tx) = app(&tmp);

        // 新規ファイル作成
        let (status, _) = send(
            &app,
            "PUT",
            "/__dev/api/files/templates/extra.html",
            &[DEV],
            Some(json!({ "content": "<div>Extra</div>" })),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert!(tmp.path().join("mytheme/templates/extra.html").is_file());

        // 改名 (rename)
        let (status, _) = send(
            &app,
            "POST",
            "/__dev/api/rename",
            &[DEV],
            Some(json!({ "from": "templates/extra.html", "to": "templates/renamed.html" })),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert!(!tmp.path().join("mytheme/templates/extra.html").exists());
        assert!(tmp.path().join("mytheme/templates/renamed.html").is_file());

        // 既存への改名衝突 (conflict)
        let (status, _) = send(
            &app,
            "POST",
            "/__dev/api/rename",
            &[DEV],
            Some(json!({ "from": "templates/renamed.html", "to": "templates/index.html" })),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);

        // 削除 (delete)
        let (status, _) = send(
            &app,
            "DELETE",
            "/__dev/api/files/templates/renamed.html",
            &[DEV],
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert!(!tmp.path().join("mytheme/templates/renamed.html").exists());

        // 存在しないファイルの削除
        let (status, _) = send(
            &app,
            "DELETE",
            "/__dev/api/files/templates/renamed.html",
            &[DEV],
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        // theme.json の削除・改名は禁止
        let (status, _) = send(&app, "DELETE", "/__dev/api/files/theme.json", &[DEV], None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, _) = send(
            &app,
            "POST",
            "/__dev/api/rename",
            &[DEV],
            Some(json!({ "from": "theme.json", "to": "theme-renamed.json" })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_writes_are_guarded() {
        let tmp = theme_dir();
        let (app, _tx) = app(&tmp);
        let body = Some(json!({ "content": "x" }));
        let before = std::fs::read_to_string(tmp.path().join("mytheme/theme.json")).unwrap();

        // ヘッダーが無い（ほかのサイトのフォームなど）
        let (status, _) = send(
            &app,
            "PUT",
            "/__dev/api/files/theme.json",
            &[],
            body.clone(),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        // Host がループバックでない（DNS リバインディング）
        let (status, _) = send(
            &app,
            "GET",
            "/__dev/api/files",
            &[("host", "evil.example:8090")],
            None,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        // テーマの外・対象外のファイル
        for path in [
            "templates/../../secret.html",
            "templates/%2e%2e/theme.json",
            "config/default.toml",
            "templates/.hidden.html",
            "templates/logo.png",
            "other.json",
        ] {
            let (status, _) = send(
                &app,
                "PUT",
                &format!("/__dev/api/files/{path}"),
                &[DEV],
                body.clone(),
            )
            .await;
            assert!(
                status == StatusCode::BAD_REQUEST || status == StatusCode::NOT_FOUND,
                "{path}: {status}"
            );
        }
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("mytheme/theme.json")).unwrap(),
            before
        );
        assert!(!tmp.path().join("secret.html").exists());
    }

    #[tokio::test]
    async fn test_writes_are_refused_when_not_listening_on_loopback() {
        let tmp = theme_dir();
        let dir = tmp.path().join("mytheme").canonicalize().unwrap();
        let engine = ThemeEngine::new(dir.parent().unwrap(), "mytheme");
        let (_tx, rx) = watch::channel(0u64);
        let app = router(DevState {
            dir,
            engine,
            version: rx,
            loopback: false,
        });
        let (status, _) = send(
            &app,
            "PUT",
            "/__dev/api/files/theme.json",
            &[DEV, ("host", "192.168.0.5:8090")],
            Some(json!({ "content": "{}" })),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (_, body) = send(
            &app,
            "GET",
            "/__dev/api/files",
            &[("host", "192.168.0.5:8090")],
            None,
        )
        .await;
        assert_eq!(
            serde_json::from_str::<Value>(&body).unwrap()["writable"],
            false
        );
    }

    #[tokio::test]
    async fn test_render_uses_unsaved_content_and_reports_the_line() {
        let tmp = theme_dir();
        let (app, _tx) = app(&tmp);
        let render = |body: Value| {
            let app = app.clone();
            async move {
                let (_, text) = send(&app, "POST", "/__dev/api/render", &[DEV], Some(body)).await;
                serde_json::from_str::<Value>(&text).unwrap()
            }
        };

        let res = render(json!({
            "sample": "article",
            "templates": { "article.html": "<i>{{ title }}</i>" },
            "samples": { "article": "{\"title\":\"下書き\"}" },
        }))
        .await;
        assert_eq!(res["ok"], true);
        assert_eq!(res["html"], "<i>下書き</i>");

        // 保存していない新しいサンプル
        let res = render(json!({
            "sample": "draft",
            "templates": { "draft.html": "{{ title }}" },
            "samples": { "draft": "{\"title\":\"new\"}" },
        }))
        .await;
        assert_eq!(res["html"], "new");

        let res = render(json!({
            "sample": "article",
            "templates": { "article.html": "line1\n{% if %}" },
        }))
        .await;
        assert_eq!(res["ok"], false);
        assert_eq!(res["error"]["template"], "article.html");
        assert_eq!(res["error"]["line"], 2);

        let res = render(json!({ "sample": "nope" })).await;
        assert_eq!(res["ok"], false);

        // ディスクには何も書かれず、共有のエンジンの描画も変わらない
        let (_, html) = get_text(&app, "/preview/article").await;
        assert!(html.contains("<h1>サンプルのタイトル</h1>"), "{html}");
    }

    #[tokio::test]
    async fn test_render_with_block_and_source_annotations() {
        let tmp = theme_dir();
        let (app, _) = app(&tmp);

        let (status, text) = send(
            &app,
            "POST",
            "/__dev/api/render",
            &[DEV],
            Some(json!({
                "sample": "index",
                "templates": {},
                "samples": {},
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let body: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(body["ok"], true);
        let html = body["html"].as_str().unwrap();
        assert!(html.contains("data-aegis-source="));
        let blocks = body["blocks"].as_array().unwrap();
        assert!(blocks.iter().any(|b| b["name"] == "content"));

        let (status_block, text_block) = send(
            &app,
            "POST",
            "/__dev/api/render",
            &[DEV],
            Some(json!({
                "sample": "index",
                "block": "content",
                "templates": {},
                "samples": {},
            })),
        )
        .await;
        assert_eq!(status_block, StatusCode::OK);
        let body_block: Value = serde_json::from_str(&text_block).unwrap();
        assert_eq!(body_block["ok"], true);
        let block_html = body_block["html"].as_str().unwrap();
        assert!(block_html.contains("<!DOCTYPE html>"));
        assert!(block_html.contains("サンプル記事 1"));
    }

    #[tokio::test]
    async fn test_package_validate_sign_and_export() {
        let tmp = theme_dir();
        let (app, _) = app(&tmp);

        // 1. 初期状態の検証 (unsigned)
        let (status, text) = send(&app, "GET", "/__dev/api/package/validate", &[], None).await;
        assert_eq!(status, StatusCode::OK);
        let val: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(val["ok"], true);
        assert_eq!(val["signed"], false);
        assert!(val["public_key"].is_null());

        // 2. 自動鍵生成で署名
        let (status, text) = send(
            &app,
            "POST",
            "/__dev/api/package/sign",
            &[DEV],
            Some(json!({ "generate_key": true })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let sign_res: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(sign_res["ok"], true);
        let pub_key = sign_res["public_key"].as_str().unwrap();
        assert_eq!(pub_key.len(), 64);
        assert!(sign_res["generated_private_key"].is_string());

        // 3. 署名後の検証 (signed)
        let (status, text) = send(&app, "GET", "/__dev/api/package/validate", &[], None).await;
        assert_eq!(status, StatusCode::OK);
        let val: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(val["ok"], true);
        assert_eq!(val["signed"], true);
        assert_eq!(val["public_key"], pub_key);

        // 4. ZIP アーカイブのエクスポート
        let req = Request::builder()
            .method("GET")
            .uri("/__dev/api/package/export")
            .header("host", "localhost:8090")
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(
            res.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/zip"
        );
        let disp = res
            .headers()
            .get(header::CONTENT_DISPOSITION)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(disp.contains("mytheme-1.0.0.zip") || disp.contains(".zip"));
        let bytes = to_bytes(res.into_body(), 10 * 1024 * 1024).await.unwrap();
        assert!(!bytes.is_empty());

        let reader = std::io::Cursor::new(bytes);
        let mut zip = zip::ZipArchive::new(reader).unwrap();
        let entry_names: Vec<String> = (0..zip.len())
            .map(|i| zip.by_index(i).unwrap().name().unwrap().to_string())
            .collect();
        assert!(entry_names.contains(&"theme.json".to_string()));
        assert!(entry_names.contains(&"signature.json".to_string()));
        assert!(entry_names.contains(&"templates/base.html".to_string()));
    }
}
