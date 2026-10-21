use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{routing::get, Router};
use clap::{Parser, Subcommand};
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod db;
mod handlers;
mod state;

use aegis_core::AegisConfig;
use aegis_runtime::PluginSupervisor;
use aegis_schema::SchemaLoader;
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
    let mut config = AegisConfig::load_or_default(&cli.config);

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

            let pool = db::init_db(&config.database.url, config.database.max_connections).await?;
            db::run_migrations(&pool, &schemas).await?;
            tracing::info!("Schema migrations completed successfully.");
        }
        Commands::Run { port, host } => {
            if let Some(p) = port {
                config.server.port = p;
            }
            if let Some(h) = host {
                config.server.host = h;
            }

            tracing::info!("Starting AegisCMS core engine...");
            tracing::info!("Database URL: {}", config.database.url);

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
            let pool = db::init_db(&config.database.url, config.database.max_connections).await?;
            db::run_migrations(&pool, &schemas).await?;

            // 3. Wasm プラグインスーパーバイザー初期化
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

            let app_state = Arc::new(AppState::new(config.clone(), pool, schemas, supervisor));

            // 4. CORS & Router 設定
            let cors = CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any);

            let mut app = Router::new()
                .route("/api/v1/health", get(handlers::health_check))
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
                .layer(cors)
                .layer(TraceLayer::new_for_http())
                .with_state(app_state);

            if config.paths.admin_dist_dir.exists() {
                let index_file = config.paths.admin_dist_dir.join("index.html");
                let serve_service = ServeDir::new(&config.paths.admin_dist_dir)
                    .not_found_service(ServeFile::new(index_file));
                app = app.fallback_service(serve_service);
            }

            let addr: SocketAddr =
                format!("{}:{}", config.server.host, config.server.port).parse()?;
            tracing::info!("AegisCMS API Gateway listening on http://{}", addr);

            let listener = tokio::net::TcpListener::bind(addr).await?;
            axum::serve(listener, app).await?;
        }
    }

    Ok(())
}
