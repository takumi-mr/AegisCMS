use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    extract::DefaultBodyLimit,
    routing::{get, post},
    Router,
};
use clap::{Parser, Subcommand};
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod audit;
mod auth;
mod cache;
mod db;
mod handlers;
mod state;
mod storage;

use aegis_core::{AegisConfig, StorageBackendKind};
use aegis_runtime::PluginSupervisor;
use aegis_schema::{OpenApiGenerator, SchemaLoader, TypeScriptGenerator};
use state::AppState;

#[derive(Parser, Debug)]
#[command(
    name = "aegis",
    about = "Next-Generation Secure CMS Core Server",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[arg(
        short,
        long,
        env = "AEGIS_CONFIG",
        default_value = "config/default.toml"
    )]
    config: PathBuf,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start the AegisCMS API Gateway server
    Run {
        #[arg(short, long, env = "PORT")]
        port: Option<u16>,

        #[arg(short = 'H', long, env = "HOST")]
        host: Option<String>,
    },
    /// Run schema migrations against the configured database
    Migrate,
    /// Generate TypeScript type definitions or OpenAPI specifications from schemas
    Codegen {
        /// Target format: "typescript" (or "ts"), "openapi" (or "json")
        #[arg(short, long, default_value = "typescript")]
        target: String,

        /// Output file path (defaults to stdout if omitted)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,aegis_server=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();
    let mut config = AegisConfig::load_or_default(&cli.config)?;

    match cli.command.unwrap_or(Commands::Run {
        port: None,
        host: None,
    }) {
        Commands::Migrate => {
            tracing::info!("Loading schemas from {:?}", config.paths.schemas_dir);
            let schemas_vec = SchemaLoader::load_dir(&config.paths.schemas_dir)?;
            let schemas: HashMap<String, _> = schemas_vec
                .into_iter()
                .map(|s| (s.content_type.clone(), s))
                .collect();

            let db_adapter = db::create_database_adapter(&config.database).await?;
            db_adapter.run_migrations(&schemas).await?;
            tracing::info!("Schema migrations completed successfully.");
        }
        Commands::Codegen { target, output } => {
            tracing::info!("Loading schemas from {:?}", config.paths.schemas_dir);
            let schemas_vec = SchemaLoader::load_dir(&config.paths.schemas_dir)?;
            let content = match target.to_lowercase().as_str() {
                "typescript" | "ts" => TypeScriptGenerator::generate(&schemas_vec),
                "openapi" | "json" => OpenApiGenerator::generate_json(&schemas_vec),
                unknown => {
                    anyhow::bail!(
                        "Unknown codegen target '{}'. Supported: 'typescript' ('ts'), 'openapi' ('json')",
                        unknown
                    );
                }
            };

            if let Some(out_path) = output {
                if let Some(parent) = out_path.parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }
                tokio::fs::write(&out_path, &content).await?;
                tracing::info!("Successfully generated {:?} ({})", out_path, target);
            } else {
                print!("{}", content);
            }
        }
        Commands::Run { port, host } => {
            if let Some(p) = port {
                config.server.port = p;
            }
            if let Some(h) = host {
                config.server.host = h;
            }

            tracing::info!("Starting AegisCMS core engine...");
            tracing::info!(
                "Database backend: {:?} (URL: {})",
                config.database.resolved_backend(),
                config.database.url
            );
            tracing::info!("Storage backend: {:?}", config.storage.backend);
            tracing::info!("Cache backend: {:?}", config.cache.backend);

            // 1. スキーマ読み込み
            let schemas_vec = SchemaLoader::load_dir(&config.paths.schemas_dir)?;
            let schemas: HashMap<String, _> = schemas_vec
                .into_iter()
                .map(|s| (s.content_type.clone(), s))
                .collect();
            tracing::info!(
                "Loaded {} schemas from {:?}",
                schemas.len(),
                config.paths.schemas_dir
            );

            // 2. DB初期化とマイグレーション
            let db_adapter = db::create_database_adapter(&config.database).await?;
            db_adapter.run_migrations(&schemas).await?;

            // 3. ストレージバックエンド初期化
            let storage = storage::create_storage_backend(&config.storage)?;

            // 4. キャッシュ & セッションサービス初期化 (ステートレス対応)
            let cache = cache::create_cache_backend(&config.cache).await?;
            let sessions = Arc::new(auth::SessionService::new(
                cache.clone(),
                config.cache.default_ttl_secs,
            ));

            // 5. Wasm プラグインスーパーバイザー初期化
            let mut supervisor = PluginSupervisor::new();
            if config.runtime.enable_wasm_plugins {
                tracing::info!("Initializing Wasmtime runtime engine...");
                match aegis_runtime::WasmEngine::new() {
                    Ok(engine) => {
                        let kv_storage = Arc::new(std::sync::Mutex::new(HashMap::new()));
                        let loaded = aegis_runtime::WasmPluginLoader::load_all_plugins(
                            &config.paths.plugins_dir,
                            engine,
                            &mut supervisor,
                            kv_storage,
                        )?;
                        tracing::info!(
                            "Loaded {} Wasm plugins from {:?}",
                            loaded,
                            config.paths.plugins_dir
                        );
                    }
                    Err(e) => {
                        tracing::error!("Failed to initialize Wasmtime runtime: {}", e);
                    }
                }
            }

            // 5.5 テーマエンジンの初期化 & プラグイン登録テンプレートの同期
            let theme_engine = Arc::new(aegis_theme::ThemeEngine::new(
                &config.paths.themes_dir,
                &config.theme.active,
            ));
            let plugin_templates = supervisor.collect_plugin_templates();
            for (name, content) in plugin_templates {
                theme_engine.register_template(&name, &content);
            }

            // 6. 監査ログサービス初期化
            let audit = Arc::new(audit::AuditService::from_config(
                &config.audit,
                db_adapter.clone(),
            ));
            audit.spawn_retention_task();

            let app_state = Arc::new(AppState::new(
                config.clone(),
                db_adapter,
                storage,
                cache,
                sessions,
                schemas,
                supervisor,
                audit,
                theme_engine,
            ));

            // 7. CORS & Router 設定
            let cors = CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any);

            let mut app = Router::new()
                .route("/api/v1/health", get(handlers::health_check))
                .route("/api/v1/openapi.json", get(handlers::get_openapi_spec))
                .route("/api/v1/schemas", get(handlers::list_schemas))
                .route("/api/v1/schemas/{content_type}", get(handlers::get_schema))
                .route(
                    "/api/v1/contents/{content_type}",
                    get(handlers::list_content).post(handlers::create_content),
                )
                .route(
                    "/api/v1/contents/{content_type}/{slug}",
                    get(handlers::get_content),
                )
                .route(
                    "/api/v1/storage/upload",
                    // axum の既定 (2MB) ではなく設定値をアップロードの上限にする
                    post(handlers::upload_file)
                        .layer(DefaultBodyLimit::max(config.storage.max_upload_bytes())),
                )
                .route("/api/v1/storage/objects", get(handlers::list_objects))
                .route(
                    "/api/v1/auth/session",
                    post(handlers::create_session)
                        .get(handlers::get_current_session)
                        .delete(handlers::delete_session),
                )
                .route("/api/v1/audit-logs", get(handlers::list_audit_logs))
                .route("/metrics", get(handlers::get_metrics))
                // --- 公開フロントエンド（テンプレートレンダリング） ---
                .route("/", get(handlers::render_index))
                .route("/articles", get(handlers::render_index))
                .route("/articles/{slug}", get(handlers::render_article))
                .route("/{slug}", get(handlers::render_page_or_slug));

            // ローカルストレージの場合、/uploads で静的ファイル配信
            if config.storage.backend == StorageBackendKind::Local {
                let _ = std::fs::create_dir_all(&config.storage.local.upload_dir);
                let prefix = config.storage.local.public_url_prefix.clone();
                app = app.nest_service(&prefix, ServeDir::new(&config.storage.local.upload_dir));
            }

            // テーマアセット配信 (/theme/assets/*)
            let theme_assets_dir = config
                .paths
                .themes_dir
                .join(&config.theme.active)
                .join("assets");
            if theme_assets_dir.exists() {
                app = app.nest_service("/theme/assets", ServeDir::new(theme_assets_dir));
            }

            // Admin Studio SPA 配信 (/admin および /assets)
            if config.paths.admin_dist_dir.exists() {
                let index_file = config.paths.admin_dist_dir.join("index.html");
                let admin_assets_dir = config.paths.admin_dist_dir.join("assets");
                if admin_assets_dir.exists() {
                    app = app.nest_service("/assets", ServeDir::new(admin_assets_dir));
                }
                let admin_serve = ServeDir::new(&config.paths.admin_dist_dir)
                    .not_found_service(ServeFile::new(index_file));
                app = app.nest_service("/admin", admin_serve);
            }

            let app = app
                .layer(cors)
                .layer(TraceLayer::new_for_http())
                .with_state(app_state);

            let addr: SocketAddr =
                format!("{}:{}", config.server.host, config.server.port).parse()?;
            tracing::info!("AegisCMS API Gateway listening on http://{}", addr);

            let listener = tokio::net::TcpListener::bind(addr).await?;
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await?;
        }
    }

    Ok(())
}
