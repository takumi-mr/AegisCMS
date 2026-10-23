use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod app;
mod audit;
mod auth;
mod config;
mod handlers;
mod notify_plugins;
mod plugin_host;
mod render;
mod scheduler;
mod state;
#[cfg(test)]
mod test_support;

use aegis_runtime::PluginSupervisor;
use aegis_schema::{OpenApiGenerator, SchemaLoader, TypeScriptGenerator};
use config::AegisConfig;
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
    /// Check the notification settings ([notifications])
    Notify {
        #[command(subcommand)]
        action: NotifyCommand,
    },
}

#[derive(Subcommand, Debug)]
enum NotifyCommand {
    /// Send a test notification through every configured channel and wait for the result
    Test {
        /// Email address to send it to
        #[arg(long, required_unless_present = "staff")]
        to: Option<String>,
        /// Also send it to the staff (staff_recipients, and plugin channels that notify staff)
        #[arg(long)]
        staff: bool,
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
    /// Change a user's role (existing sessions are revoked immediately)
    Role { username: String, role: String },
    /// Disable a user so it can no longer log in (existing sessions are revoked immediately)
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
    /// Link an existing user to a SAML account ([auth.saml].idp_entity_id + the IdP's NameID)
    LinkSaml {
        username: String,
        /// The NameID of the user at the IdP
        name_id: String,
    },
}

/// アカウント登録の確認メールを送る通知。通知が無効・確認メールを止めている設定では起動しない
fn registration_notifier(
    config: &AegisConfig,
    notifier: Option<Arc<aegis_notify::NotificationService>>,
) -> anyhow::Result<Arc<dyn aegis_core::Notifier>> {
    let Some(notifier) = notifier else {
        anyhow::bail!(
            "[auth.registration] sends confirmation emails: set [notifications].enabled = true"
        );
    };
    if !config.notifications.is_event_enabled("account.verify") {
        anyhow::bail!(
            "[auth.registration] needs the \"account.verify\" notification: remove it from [notifications].disabled_events"
        );
    }
    Ok(notifier)
}

/// 確認リンクの開き先（`[auth.registration].admin_url`、無ければ `[commerce].public_url` + `/admin/`）。
/// リンクを偽装させないよう、リクエストの Host からは作らない
fn registration_admin_url(config: &AegisConfig) -> anyhow::Result<String> {
    let configured = config.auth.registration.admin_url.trim();
    if !configured.is_empty() {
        return Ok(configured.to_string());
    }
    let public_url = config.commerce.public_url.trim().trim_end_matches('/');
    if !public_url.is_empty() {
        return Ok(format!("{}/admin/", public_url));
    }
    anyhow::bail!(
        "[auth.registration] needs the Admin Studio URL for the confirmation link: set [auth.registration].admin_url (e.g. \"https://cms.example.com/admin/\")"
    )
}

/// 通知のテンプレートに渡すサイトの情報（`[theme].site_title` と `[commerce].public_url`）
fn notification_site(config: &AegisConfig) -> aegis_notify::SiteInfo {
    let url = config
        .commerce
        .public_url
        .trim()
        .trim_end_matches('/')
        .to_string();
    aegis_notify::SiteInfo {
        name: config.theme.site_title.clone(),
        admin_url: if url.is_empty() {
            String::new()
        } else {
            format!("{}/admin/", url)
        },
        url,
    }
}

async fn run_notify_command(config: &AegisConfig, action: NotifyCommand) -> anyhow::Result<()> {
    match action {
        NotifyCommand::Test { to, staff } => {
            if !config.notifications.enabled {
                anyhow::bail!("Notifications are disabled: set [notifications].enabled = true");
            }
            // プラグインのチャネル（plugin:<名前>）も試せるようにプラグインを読み込む
            let uses_plugins = config
                .notifications
                .channels
                .iter()
                .any(|c| matches!(c, aegis_notify::ChannelKind::Plugin(_)));
            let supervisor = if uses_plugins {
                let cache = aegis_cache::create_cache_backend(&config.cache).await?;
                load_plugins(config, cache)?
            } else {
                PluginSupervisor::new()
            };
            let supervisor = Arc::new(tokio::sync::RwLock::new(supervisor));
            notify_plugins::validate_plugin_channels(&config.notifications, &supervisor).await?;
            let service = aegis_notify::NotificationService::from_config_with_plugins(
                &config.notifications,
                notification_site(config),
                None,
                notify_plugins::plugin_channel_factory(supervisor),
            )?;
            let event = aegis_core::NotificationEvent::new(
                "test",
                to.into_iter()
                    .map(|to| aegis_core::Recipient::person(to, ""))
                    .chain(staff.then_some(aegis_core::Recipient::Staff))
                    .collect(),
                serde_json::json!({}),
            );
            let deliveries = service.deliver(&event).await;
            if deliveries.is_empty() {
                anyhow::bail!(
                    "Nothing was sent: no channel accepts the recipient, or \"test\" is in [notifications].disabled_events"
                );
            }
            let mut failed = false;
            for delivery in deliveries {
                match delivery.result {
                    Ok(()) => println!("{}: sent \"{}\"", delivery.channel, delivery.subject),
                    Err(e) => {
                        failed = true;
                        println!(
                            "{}: failed after {} attempt(s): {}",
                            delivery.channel, delivery.attempts, e
                        );
                    }
                }
            }
            if failed {
                anyhow::bail!("Some notifications were not delivered");
            }
        }
    }
    Ok(())
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
    let db = aegis_db::create_database_adapter(&config.database).await?;
    let db = db.as_ref();
    let sessions = match aegis_cache::create_cache_backend(&config.cache).await {
        Ok(cache) => Some(aegis_auth::SessionService::new(
            cache,
            config.cache.default_ttl_secs,
        )),
        Err(e) => {
            tracing::debug!("Cache backend not available for CLI sessions: {}", e);
            None
        }
    };
    match action {
        UserCommand::Add {
            username,
            role,
            email,
            display_name,
        } => {
            let password = read_new_password()?;
            let user = aegis_auth::users::create_user(
                db,
                aegis_auth::users::NewUser {
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
            aegis_auth::users::set_password(db, &username, &password).await?;
            if let Some(sessions) = &sessions {
                let _ = sessions.revoke_user_sessions(&username).await;
            }
            println!("Updated the password of '{}'", username);
        }
        UserCommand::Role { username, role } => {
            aegis_auth::users::set_role(db, &username, &role).await?;
            if let Some(sessions) = &sessions {
                let _ = sessions.revoke_user_sessions(&username).await;
            }
            println!("Changed the role of '{}' to {}", username, role);
        }
        UserCommand::Disable { username } => {
            aegis_auth::users::set_disabled(db, &username, true).await?;
            if let Some(sessions) = &sessions {
                let _ = sessions.revoke_user_sessions(&username).await;
            }
            println!("Disabled '{}'", username);
        }
        UserCommand::Enable { username } => {
            aegis_auth::users::set_disabled(db, &username, false).await?;
            println!("Enabled '{}'", username);
        }
        UserCommand::LinkOidc { username, sub } => {
            anyhow::ensure!(
                !config.auth.oidc.issuer_url.trim().is_empty(),
                "[auth.oidc].issuer_url is not set"
            );
            let subject = aegis_auth::oidc_subject(&config.auth.oidc.issuer_url, &sub);
            aegis_auth::users::link_identity(db, &username, aegis_auth::OIDC_PROVIDER, &subject)
                .await?;
            println!("Linked '{}' to OIDC subject {}", username, subject);
        }
        UserCommand::LinkSaml { username, name_id } => {
            anyhow::ensure!(
                !config.auth.saml.idp_entity_id.trim().is_empty(),
                "[auth.saml].idp_entity_id is not set"
            );
            let subject = aegis_auth::saml_subject(&config.auth.saml.idp_entity_id, &name_id);
            aegis_auth::users::link_identity(db, &username, aegis_auth::SAML_PROVIDER, &subject)
                .await?;
            println!("Linked '{}' to SAML subject {}", username, subject);
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

            let db_adapter = aegis_db::create_database_adapter(&config.database).await?;
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
        Commands::Notify { action } => run_notify_command(&config, action).await?,
        Commands::Run { port, host } => {
            if let Some(p) = port {
                config.server.port = p;
            }
            if let Some(h) = host {
                config.server.host = h;
            }

            let base_path = config.server.normalized_base_path()?;
            config.plugins.validate()?;
            config.commerce.validate()?;
            config.notifications.validate()?;
            config.auth.registration.validate()?;
            if config.notifications.enabled
                && !config.notifications.audit.rules.is_empty()
                && !config.audit.enabled
            {
                anyhow::bail!(
                    "[[notifications.audit.rules]] notify audit events: set [audit].enabled = true"
                );
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
            let db_adapter = aegis_db::create_database_adapter(&config.database).await?;
            db_adapter.run_migrations(&schemas).await?;
            if db_adapter.list_users().await?.is_empty() {
                tracing::warn!(
                    "No user accounts exist, so nobody can log in. Create one with: aegis user add <username> --role admin"
                );
            }
            let auth_provider = Arc::new(aegis_auth::LocalAuthProvider::new(db_adapter.clone()));

            // 3. ストレージバックエンド初期化
            let storage = aegis_storage::create_storage_backend(
                &config.storage,
                // 配信のルートは public_url_prefix のまま（app::with_base_path がベースパスを取り除く）
                &app::under_base_path(&base_path, &config.storage.local.public_url_prefix),
            )?;

            // 4. キャッシュ & セッションサービス初期化 (ステートレス対応)
            let cache = aegis_cache::create_cache_backend(&config.cache).await?;
            let sessions = Arc::new(aegis_auth::SessionService::new(
                cache.clone(),
                config.cache.default_ttl_secs,
            ));
            let login_limiter = Arc::new(aegis_auth::LoginRateLimiter::new(
                cache.clone(),
                config.auth.login_rate_limit.clone(),
            ));

            // 5. Wasm プラグインスーパーバイザー初期化（[plugins].enabled のプラグインをその順に読み込む）
            let supervisor = load_plugins(&config, cache.clone())?;

            // 5.5 テーマエンジンの初期化 & プラグイン登録テンプレートの同期
            let theme_engine = Arc::new(
                aegis_theme::ThemeEngine::new(&config.paths.themes_dir, &config.theme.active)
                    .with_base_path(&base_path)
                    .with_template_reload(config.theme.reload_templates),
            );
            let plugin_templates = supervisor.collect_plugin_templates();
            for (name, content) in plugin_templates {
                theme_engine.register_template(&name, &content);
            }

            // 6. 監査ログサービス初期化
            let mut audit =
                aegis_audit::AuditService::from_config(&config.audit, db_adapter.clone());
            // 監査イベントの通知（[[notifications.audit.rules]]）。通知の送り先は 6.6 で渡す
            let audit_notifications = (config.notifications.enabled
                && !config.notifications.audit.rules.is_empty())
            .then(|| {
                let sink = Arc::new(aegis_notify::AuditNotificationSink::new(
                    config.notifications.audit.rules.clone(),
                ));
                audit.add_sink(sink.clone());
                sink
            });
            let audit = Arc::new(audit);
            audit.spawn_retention_task();

            // 6.5 外部 IdP ログイン（[auth.oidc] / [auth.saml]）
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
            let saml = if config.auth.saml.enabled {
                Some(build_saml_login(
                    &config.auth.saml,
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
            let app_state = match oidc {
                Some(oidc) => app_state.with_oidc(oidc),
                None => app_state,
            };
            let app_state = match saml {
                Some(saml) => app_state.with_saml(saml),
                None => app_state,
            };

            // 6.6 通知（[notifications]）。EC が注文の通知に使うので先に設定する
            let app_state = if config.notifications.enabled {
                notify_plugins::validate_plugin_channels(
                    &config.notifications,
                    &app_state.supervisor,
                )
                .await?;
                let notifier = aegis_notify::NotificationService::from_config_with_plugins(
                    &config.notifications,
                    notification_site(&config),
                    Some(app_state.audit.clone()),
                    notify_plugins::plugin_channel_factory(app_state.supervisor.clone()),
                )?;
                tracing::info!(
                    "Notifications enabled (channels: {:?})",
                    notifier.channel_names()
                );
                if let Some(sink) = &audit_notifications {
                    sink.attach(Arc::new(notifier.clone()));
                    tracing::info!(
                        "Audit notifications enabled ({} rule(s))",
                        config.notifications.audit.rules.len()
                    );
                }
                app_state.with_notifier(notifier)
            } else {
                app_state
            };

            // 6.6.1 アカウント登録（[auth.registration]）。確認メールを通知で送る
            let app_state = if config.auth.registration.enabled {
                let notifier = registration_notifier(&config, app_state.notifier.clone())?;
                let admin_url = registration_admin_url(&config)?;
                tracing::info!(
                    "Account registration enabled (role: {}, approval: {}, links open {})",
                    config.auth.registration.default_role,
                    config.auth.registration.require_approval,
                    admin_url
                );
                let service = aegis_auth::registration::RegistrationService::new(
                    app_state.db.clone(),
                    app_state.cache.clone(),
                    config.auth.registration.clone(),
                );
                app_state.with_registration(state::Registration {
                    service,
                    notifier,
                    admin_url,
                })
            } else {
                app_state
            };

            // 6.7 EC（[commerce]）
            let app_state = if config.commerce.enabled {
                let commerce = aegis_commerce::CommerceService::new(
                    config.commerce.clone(),
                    app_state.commerce_deps(),
                )?;
                commerce.check_product_schema(&*app_state.schemas.read().await)?;
                tracing::info!(
                    "Shop enabled (products: '{}', currency: {}, payment: {:?})",
                    config.commerce.product_type,
                    config.commerce.currency,
                    config.commerce.provider
                );
                app_state.with_commerce(commerce)
            } else {
                app_state
            };
            let app_state = Arc::new(app_state);
            if let Some(commerce) = &app_state.commerce {
                commerce.spawn_expiry_task();
            }
            // 予約公開（status: "scheduled"）
            scheduler::spawn_publish_task(app_state.clone());

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

/// `[plugins].enabled` のプラグインをその順に読み込む（`[runtime].enable_wasm_plugins = false` なら読み込まない）
fn load_plugins(
    config: &AegisConfig,
    cache: Arc<dyn aegis_core::CacheBackend>,
) -> anyhow::Result<PluginSupervisor> {
    let mut supervisor = PluginSupervisor::new();
    if !config.runtime.enable_wasm_plugins {
        if !config.plugins.enabled.is_empty() {
            tracing::warn!(
                "[runtime].enable_wasm_plugins = false: plugins in [plugins].enabled are not loaded"
            );
        }
        return Ok(supervisor);
    }
    tracing::info!("Initializing Wasmtime runtime engine...");
    let engine = create_wasm_engine(&config.runtime)?;
    // KV は [cache] のバックエンド、HTTP は allowed_domains の確認済みのリクエストのみ
    let host_services: Arc<dyn aegis_runtime::HostServices> = Arc::new(
        plugin_host::ServerHostServices::new(cache, config.plugins.clone())?,
    );
    let loaded = aegis_runtime::WasmPluginLoader::load_enabled(
        &config.paths.plugins_dir,
        &config.plugins.enabled,
        engine,
        &mut supervisor,
        host_services.into(),
        &config.runtime.default_resource_limits(),
    )?;
    tracing::info!(
        "Loaded {} Wasm plugins from {:?}",
        loaded,
        config.paths.plugins_dir
    );
    Ok(supervisor)
}

/// `[runtime].instance_pool_size` が 1 以上ならプーリングアロケータを使うエンジンを作る。
/// プールのアドレス空間を確保できない環境（仮想メモリの上限など）では、警告を出して通常のエンジンにする
fn create_wasm_engine(
    runtime: &aegis_runtime::RuntimeConfig,
) -> anyhow::Result<aegis_runtime::WasmEngine> {
    if runtime.instance_pool_size == 0 {
        return Ok(aegis_runtime::WasmEngine::new()?);
    }
    let pool = aegis_runtime::InstancePoolConfig {
        size: runtime.instance_pool_size,
        max_memory_mb: runtime.pool_max_memory_mb,
    };
    match aegis_runtime::WasmEngine::with_instance_pool(pool) {
        Ok(engine) => {
            tracing::info!(
                "Wasm instance pool: {} slots, {} MiB of memory per slot",
                pool.size,
                pool.max_memory_mb
            );
            Ok(engine)
        }
        Err(e) => {
            tracing::warn!(
                "Could not reserve the Wasm instance pool ({}); allocating instances on demand. \
                 Lower [runtime].instance_pool_size or set it to 0 to silence this warning",
                e
            );
            Ok(aegis_runtime::WasmEngine::new()?)
        }
    }
}

/// `[auth.oidc]` を検証し、OpenID Connect ログインを組み立てる（IdP への接続は初回ログイン時）
#[cfg(feature = "oidc")]
fn build_oidc_login(
    config: &aegis_auth::OidcConfig,
    base_path: &str,
    cache: Arc<dyn aegis_core::CacheBackend>,
    db: Arc<dyn aegis_db::DatabaseAdapter>,
) -> anyhow::Result<state::SsoLogin> {
    let provider = aegis_auth::oidc::OidcAuthProvider::new(config.clone(), cache, db)?;
    tracing::info!(
        "OIDC login enabled (issuer: {}, client_id: {})",
        config.issuer_url,
        config.client_id
    );
    Ok(state::SsoLogin {
        provider: Arc::new(provider),
        display_name: config.display_name.clone(),
        post_login_redirect: app::under_base_path(base_path, &config.post_login_redirect),
    })
}

#[cfg(not(feature = "oidc"))]
fn build_oidc_login(
    _config: &aegis_auth::OidcConfig,
    _base_path: &str,
    _cache: Arc<dyn aegis_core::CacheBackend>,
    _db: Arc<dyn aegis_db::DatabaseAdapter>,
) -> anyhow::Result<state::SsoLogin> {
    anyhow::bail!("[auth.oidc] is enabled, but this build of aegis-server has no 'oidc' feature")
}

/// `[auth.saml]` を検証し（IdP の証明書を含む）、SAML ログインを組み立てる
#[cfg(feature = "saml")]
fn build_saml_login(
    config: &aegis_auth::SamlConfig,
    base_path: &str,
    cache: Arc<dyn aegis_core::CacheBackend>,
    db: Arc<dyn aegis_db::DatabaseAdapter>,
) -> anyhow::Result<state::SamlLogin> {
    let provider = aegis_auth::saml::SamlAuthProvider::new(config.clone(), cache, db)?;
    tracing::info!(
        "SAML login enabled (IdP: {}, SP entity ID: {})",
        config.idp_entity_id,
        config.entity_id
    );
    Ok(state::SamlLogin {
        metadata: provider.sp_metadata(),
        sso: state::SsoLogin {
            provider: Arc::new(provider),
            display_name: config.display_name.clone(),
            post_login_redirect: app::under_base_path(base_path, &config.post_login_redirect),
        },
    })
}

#[cfg(not(feature = "saml"))]
fn build_saml_login(
    _config: &aegis_auth::SamlConfig,
    _base_path: &str,
    _cache: Arc<dyn aegis_core::CacheBackend>,
    _db: Arc<dyn aegis_db::DatabaseAdapter>,
) -> anyhow::Result<state::SamlLogin> {
    anyhow::bail!("[auth.saml] is enabled, but this build of aegis-server has no 'saml' feature")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registration_admin_url_never_comes_from_the_request() {
        let mut config = AegisConfig::default();
        assert!(registration_admin_url(&config).is_err());
        config.commerce.public_url = "https://shop.example.com/".to_string();
        assert_eq!(
            registration_admin_url(&config).unwrap(),
            "https://shop.example.com/admin/"
        );
        config.auth.registration.admin_url = " https://cms.example.com/studio/ ".to_string();
        assert_eq!(
            registration_admin_url(&config).unwrap(),
            "https://cms.example.com/studio/"
        );
    }

    #[test]
    fn test_registration_needs_the_confirmation_email() {
        let mut config = AegisConfig::default();
        assert!(registration_notifier(&config, None).is_err());

        config.notifications.enabled = true;
        config.notifications.channels = vec![aegis_notify::ChannelKind::Log];
        let notifier = Arc::new(
            aegis_notify::NotificationService::from_config(
                &config.notifications,
                aegis_notify::SiteInfo::default(),
                None,
            )
            .unwrap(),
        );
        assert!(registration_notifier(&config, Some(notifier.clone())).is_ok());
        config.notifications.disabled_events = vec!["account.verify".to_string()];
        assert!(registration_notifier(&config, Some(notifier)).is_err());
    }

    #[tokio::test]
    async fn test_cli_user_role_and_disable() {
        let mut config = AegisConfig::default();
        let temp_db =
            std::env::temp_dir().join(format!("test_cli_{}.db", uuid::Uuid::new_v4().simple()));
        config.database.url = format!("sqlite://{}?mode=rwc", temp_db.display());

        let db = aegis_db::create_database_adapter(&config.database)
            .await
            .unwrap();
        aegis_auth::users::create_user(
            db.as_ref(),
            aegis_auth::users::NewUser {
                username: "dave".to_string(),
                password: "password123".to_string(),
                role: "editor".to_string(),
                email: None,
                display_name: None,
            },
        )
        .await
        .unwrap();

        run_user_command(
            &config,
            UserCommand::Role {
                username: "dave".to_string(),
                role: "admin".to_string(),
            },
        )
        .await
        .unwrap();

        let user = db.get_user_by_username("dave").await.unwrap().unwrap();
        assert_eq!(user.role, "admin");

        run_user_command(
            &config,
            UserCommand::Disable {
                username: "dave".to_string(),
            },
        )
        .await
        .unwrap();

        let user = db.get_user_by_username("dave").await.unwrap().unwrap();
        assert!(user.disabled);

        let _ = std::fs::remove_file(&temp_db);
    }
}
