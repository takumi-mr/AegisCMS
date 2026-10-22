use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod app;
mod audit;
mod auth;
mod cache;
mod db;
mod handlers;
mod render;
mod state;
mod storage;
#[cfg(test)]
mod test_support;

use aegis_core::AegisConfig;
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
    /// Manage local user accounts (login with `POST /api/v1/auth/session`)
    User {
        #[command(subcommand)]
        action: UserCommand,
    },
    /// Inspect Wasm plugins installed in [paths].plugins_dir (enable them with [plugins].enabled)
    Plugin {
        #[command(subcommand)]
        action: PluginCommand,
    },
}

#[derive(Subcommand, Debug)]
enum PluginCommand {
    /// List installed and enabled plugins
    List,
}

#[derive(Subcommand, Debug)]
enum UserCommand {
    /// Create a user. The password is read from AEGIS_USER_PASSWORD, a terminal prompt or stdin
    Add {
        username: String,
        /// admin, editor or viewer
        #[arg(short, long, default_value = "editor")]
        role: String,
        #[arg(long)]
        email: Option<String>,
        #[arg(long)]
        display_name: Option<String>,
    },
    /// Change a user's password (read like `user add`)
    Passwd { username: String },
    /// Change a user's role (applies from the next login)
    Role { username: String, role: String },
    /// Disable a user so it can no longer log in (existing sessions stay valid until they expire)
    Disable { username: String },
    /// Re-enable a disabled user
    Enable { username: String },
    /// List users
    List,
    /// Link an existing user to an OpenID Connect account ([auth.oidc].issuer_url + the IdP's `sub`)
    LinkOidc {
        username: String,
        /// The `sub` claim of the user at the IdP
        sub: String,
    },
}

fn run_plugin_command(config: &AegisConfig, action: PluginCommand) -> anyhow::Result<()> {
    match action {
        PluginCommand::List => {
            config.plugins.validate()?;
            let plugins = aegis_runtime::WasmPluginLoader::list(
                &config.paths.plugins_dir,
                &config.plugins.enabled,
                &config.runtime.default_resource_limits(),
            )?;
            if plugins.is_empty() {
                println!(
                    "No plugins installed in {}",
                    config.paths.plugins_dir.display()
                );
                return Ok(());
            }
            println!(
                "{:<28} {:<10} {:<9} HOOKS / PROBLEM",
                "NAME", "VERSION", "STATUS"
            );
            for plugin in plugins {
                let version = plugin
                    .manifest
                    .as_ref()
                    .map(|m| m.version.as_str())
                    .unwrap_or("-");
                let status = if plugin.is_enabled {
                    "enabled"
                } else {
                    "disabled"
                };
                let detail = match (&plugin.problem, &plugin.manifest) {
                    (Some(problem), _) => format!("! {}", problem),
                    (None, Some(manifest)) => manifest.capabilities.hooks.join(", "),
                    (None, None) => String::new(),
                };
                println!(
                    "{:<28} {:<10} {:<9} {}",
                    plugin.name, version, status, detail
                );
            }
        }
    }
    Ok(())
}

/// 新しいパスワードを読み込む: 環境変数 `AEGIS_USER_PASSWORD` → 端末なら非表示入力（確認つき）→ 標準入力の 1 行
fn read_new_password() -> anyhow::Result<String> {
    use std::io::IsTerminal;

    if let Ok(password) = std::env::var("AEGIS_USER_PASSWORD") {
        return Ok(password);
    }
    if std::io::stdin().is_terminal() {
        let password = rpassword::prompt_password("Password: ")?;
        let confirm = rpassword::prompt_password("Confirm password: ")?;
        anyhow::ensure!(password == confirm, "Passwords do not match");
        return Ok(password);
    }
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(line.trim_end_matches(['\r', '\n']).to_string())
}

async fn run_user_command(config: &AegisConfig, action: UserCommand) -> anyhow::Result<()> {
    let db = db::create_database_adapter(&config.database).await?;
    let db = db.as_ref();
    match action {
        UserCommand::Add {
            username,
            role,
            email,
            display_name,
        } => {
            let password = read_new_password()?;
            let user = auth::users::create_user(
                db,
                auth::users::NewUser {
                    username,
                    password,
                    role,
                    email,
                    display_name,
                },
            )
            .await?;
            println!("Created user '{}' (role: {})", user.username, user.role);
        }
        UserCommand::Passwd { username } => {
            let password = read_new_password()?;
            auth::users::set_password(db, &username, &password).await?;
            println!("Updated the password of '{}'", username);
        }
        UserCommand::Role { username, role } => {
            auth::users::set_role(db, &username, &role).await?;
            println!("Changed the role of '{}' to {}", username, role);
        }
        UserCommand::Disable { username } => {
            auth::users::set_disabled(db, &username, true).await?;
            println!("Disabled '{}'", username);
        }
        UserCommand::Enable { username } => {
            auth::users::set_disabled(db, &username, false).await?;
            println!("Enabled '{}'", username);
        }
        UserCommand::LinkOidc { username, sub } => {
            anyhow::ensure!(
                !config.auth.oidc.issuer_url.trim().is_empty(),
                "[auth.oidc].issuer_url is not set"
            );
            let subject = auth::oidc_subject(&config.auth.oidc.issuer_url, &sub);
            auth::users::link_identity(db, &username, auth::OIDC_PROVIDER, &subject).await?;
            println!("Linked '{}' to OIDC subject {}", username, subject);
        }
        UserCommand::List => {
            let users = db.list_users().await?;
            if users.is_empty() {
                println!("No users. Create one with: aegis user add <username> --role admin");
            }
            for user in users {
                println!(
                    "{:<24} {:<8} {:<9} {}",
                    user.username,
                    user.role,
                    if user.disabled { "disabled" } else { "active" },
                    user.email.unwrap_or_default()
                );
            }
        }
    }
    Ok(())
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
        Commands::User { action } => run_user_command(&config, action).await?,
        Commands::Plugin { action } => run_plugin_command(&config, action)?,
        Commands::Run { port, host } => {
            if let Some(p) = port {
                config.server.port = p;
            }
            if let Some(h) = host {
                config.server.host = h;
            }

            let base_path = config.server.normalized_base_path()?;
            config.plugins.validate()?;

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
            if db_adapter.list_users().await?.is_empty() {
                tracing::warn!(
                    "No user accounts exist, so nobody can log in. Create one with: aegis user add <username> --role admin"
                );
            }
            let auth_provider = Arc::new(auth::LocalAuthProvider::new(db_adapter.clone()));

            // 3. ストレージバックエンド初期化
            let storage = storage::create_storage_backend(&config.storage, &base_path)?;

            // 4. キャッシュ & セッションサービス初期化 (ステートレス対応)
            let cache = cache::create_cache_backend(&config.cache).await?;
            let sessions = Arc::new(auth::SessionService::new(
                cache.clone(),
                config.cache.default_ttl_secs,
            ));
            let login_limiter = Arc::new(auth::LoginRateLimiter::new(
                cache.clone(),
                config.auth.login_rate_limit.clone(),
            ));

            // 5. Wasm プラグインスーパーバイザー初期化（[plugins].enabled のプラグインをその順に読み込む）
            let mut supervisor = PluginSupervisor::new();
            if !config.runtime.enable_wasm_plugins {
                if !config.plugins.enabled.is_empty() {
                    tracing::warn!(
                        "[runtime].enable_wasm_plugins = false: plugins in [plugins].enabled are not loaded"
                    );
                }
            } else {
                tracing::info!("Initializing Wasmtime runtime engine...");
                let engine = aegis_runtime::WasmEngine::new()?;
                let kv_storage = Arc::new(std::sync::Mutex::new(HashMap::new()));
                let loaded = aegis_runtime::WasmPluginLoader::load_enabled(
                    &config.paths.plugins_dir,
                    &config.plugins.enabled,
                    engine,
                    &mut supervisor,
                    kv_storage,
                    &config.runtime.default_resource_limits(),
                )?;
                tracing::info!(
                    "Loaded {} Wasm plugins from {:?}",
                    loaded,
                    config.paths.plugins_dir
                );
            }

            // 5.5 テーマエンジンの初期化 & プラグイン登録テンプレートの同期
            let theme_engine = Arc::new(
                aegis_theme::ThemeEngine::new(&config.paths.themes_dir, &config.theme.active)
                    .with_base_path(&base_path),
            );
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

            // 6.5 外部 IdP ログイン（[auth.oidc]）
            let oidc = if config.auth.oidc.enabled {
                Some(build_oidc_login(
                    &config.auth.oidc,
                    &base_path,
                    cache.clone(),
                    db_adapter.clone(),
                )?)
            } else {
                None
            };

            let app_state = AppState::new(
                config.clone(),
                db_adapter,
                storage,
                cache,
                sessions,
                auth_provider,
                login_limiter,
                schemas,
                supervisor,
                audit,
                theme_engine,
            );
            let app_state = Arc::new(match oidc {
                Some(oidc) => app_state.with_oidc(oidc),
                None => app_state,
            });

            // 7. Router 設定（[server].base_path 配下で公開）
            let app = app::with_base_path(app::router(&config, app_state), &base_path);

            let addr: SocketAddr =
                format!("{}:{}", config.server.host, config.server.port).parse()?;
            tracing::info!(
                "AegisCMS API Gateway listening on http://{}{}/",
                addr,
                base_path
            );

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

/// `[auth.oidc]` を検証し、OpenID Connect ログインを組み立てる（IdP への接続は初回ログイン時）
#[cfg(feature = "oidc")]
fn build_oidc_login(
    config: &aegis_core::OidcConfig,
    base_path: &str,
    cache: Arc<dyn aegis_core::CacheBackend>,
    db: Arc<dyn db::DatabaseAdapter>,
) -> anyhow::Result<state::OidcLogin> {
    let provider = auth::oidc::OidcAuthProvider::new(config.clone(), cache, db)?;
    tracing::info!(
        "OIDC login enabled (issuer: {}, client_id: {})",
        config.issuer_url,
        config.client_id
    );
    Ok(state::OidcLogin {
        provider: Arc::new(provider),
        display_name: config.display_name.clone(),
        post_login_redirect: app::under_base_path(base_path, &config.post_login_redirect),
    })
}

#[cfg(not(feature = "oidc"))]
fn build_oidc_login(
    _config: &aegis_core::OidcConfig,
    _base_path: &str,
    _cache: Arc<dyn aegis_core::CacheBackend>,
    _db: Arc<dyn db::DatabaseAdapter>,
) -> anyhow::Result<state::OidcLogin> {
    anyhow::bail!("[auth.oidc] is enabled, but this build of aegis-server has no 'oidc' feature")
}
