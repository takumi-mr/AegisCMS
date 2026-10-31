use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[cfg(feature = "analytics")]
mod analytics;
mod app;
mod audit;
mod auth;
#[cfg(feature = "backup")]
mod backup;
mod cdn;
mod cluster;
mod config;
mod consent;
mod deploy;
mod handlers;
mod headers;
mod inject;
mod notify_plugins;
mod plugin_host;
mod plugin_registry;
mod redirects;
mod related;
mod render;
mod scheduler;
#[cfg(feature = "search")]
mod search;
mod seo;
mod site;
mod state;
mod system;
#[cfg(test)]
mod test_support;
mod theme_dev;

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
    /// Develop a theme: preview it with sample data and reload on save
    Theme {
        #[command(subcommand)]
        action: ThemeCommand,
    },
    /// Serve several sites over HTTPS with Caddy (deploy/README.md)
    Deploy {
        #[command(subcommand)]
        action: DeployCommand,
    },
    /// Export database records and uploaded storage objects into a verified ZIP backup archive
    #[cfg(feature = "backup")]
    Dump(backup::DumpArgs),
    /// Restore database records and uploaded storage objects from a backup archive
    #[cfg(feature = "backup")]
    Restore(backup::RestoreArgs),
}

#[derive(Subcommand, Debug)]
enum ThemeCommand {
    /// Serve a theme directory with sample data (no database needed) and reload the browser on save
    Dev(theme_dev::DevArgs),
}

#[derive(Subcommand, Debug)]
enum DeployCommand {
    /// Generate compose.yaml, Caddyfile and sites/<name>/aegis.toml from <dir>/sites.toml
    Generate {
        /// Directory with sites.toml (the files are written there)
        #[arg(long, default_value = ".")]
        dir: PathBuf,
        /// Where the built-in themes/ and plugins/ are, to check the names in sites.toml
        /// (/app in the AegisCMS image)
        #[arg(long, default_value = "/app")]
        builtin: PathBuf,
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
    /// Disable two-factor authentication (2FA) for a user
    Disable2fa { username: String },
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
    notifier: Option<Arc<dyn aegis_core::Notifier>>,
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

/// 公開ページに出す Admin Studio へのリンク。出さないなら空文字
fn public_admin_url(config: &AegisConfig, base_path: &str, admin_path: &str) -> String {
    let show = config
        .theme
        .show_admin_link
        .unwrap_or(admin_path == config::DEFAULT_ADMIN_PATH);
    if show {
        format!("{}{}/", base_path, admin_path)
    } else {
        String::new()
    }
}

/// Admin Studio の URL（`[auth.password_reset].admin_url`、`[auth.registration].admin_url`、無ければ `[commerce].public_url` + `[server].admin_path`）。
/// リンクを偽装させないよう（Host ヘッダーインジェクション防止）、リクエストの Host からは作らず設定値のみを使用する
fn resolve_admin_url(config: &AegisConfig) -> Option<String> {
    let reset_url = config.auth.password_reset.admin_url.trim();
    if !reset_url.is_empty() {
        return Some(reset_url.to_string());
    }
    let configured = config.auth.registration.admin_url.trim();
    if !configured.is_empty() {
        return Some(configured.to_string());
    }
    let public_url = config.commerce.public_url.trim().trim_end_matches('/');
    if !public_url.is_empty() {
        return Some(format!("{}{}/", public_url, config.server.admin_path()));
    }
    None
}

/// 確認リンクの開き先（`[auth.registration].admin_url`、無ければ `[commerce].public_url` + `[server].admin_path`）。
fn registration_admin_url(config: &AegisConfig) -> anyhow::Result<String> {
    resolve_admin_url(config).ok_or_else(|| {
        anyhow::anyhow!(
            "[auth.registration] needs the Admin Studio URL for the confirmation link: set [auth.registration].admin_url (e.g. \"https://cms.example.com/admin/\")"
        )
    })
}

/// 通知のテンプレートに渡すサイトの情報（サイト名と `[commerce].public_url`）。サイト名は起動時の値
/// （Admin Studio で変えた名前は再起動後に反映される）
fn notification_site(config: &AegisConfig, site_name: &str) -> aegis_notify::SiteInfo {
    let url = config
        .commerce
        .public_url
        .trim()
        .trim_end_matches('/')
        .to_string();
    aegis_notify::SiteInfo {
        name: site_name.to_string(),
        admin_url: if url.is_empty() {
            String::new()
        } else {
            format!("{}{}/", url, config.server.admin_path())
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
                let enabled = cli_enabled_plugins(config).await;
                let plugin_settings =
                    Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
                load_cached_plugin_settings(&enabled, &cache, &plugin_settings).await;
                load_plugins(config, &enabled, cache, plugin_settings)?.0
            } else {
                PluginSupervisor::new()
            };
            let supervisor = Arc::new(tokio::sync::RwLock::new(supervisor));
            notify_plugins::validate_plugin_channels(&config.notifications, &supervisor).await?;
            let service = aegis_notify::NotificationService::from_config_with_plugins(
                &config.notifications,
                notification_site(config, &config.theme.site_title),
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

async fn run_plugin_command(config: &AegisConfig, action: PluginCommand) -> anyhow::Result<()> {
    match action {
        PluginCommand::List => {
            config.plugins.validate()?;
            config.theme.validate()?;
            let enabled = cli_enabled_plugins(config).await;
            let plugins = aegis_runtime::WasmPluginLoader::list(
                &config.paths.plugins_dir,
                &enabled,
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
        UserCommand::Disable2fa { username } => {
            aegis_auth::users::disable_2fa(db, &username).await?;
            if let Some(sessions) = &sessions {
                let _ = sessions.revoke_user_sessions(&username).await;
            }
            println!("Disabled two-factor authentication for '{}'", username);
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
            if config.forum.enabled {
                if let Some(ref forum_url) = config.forum.database_url {
                    let trimmed = forum_url.trim();
                    if !trimmed.is_empty() {
                        let forum_db_cfg = aegis_db::DatabaseConfig {
                            url: trimmed.to_string(),
                            ..config.database.clone()
                        };
                        let forum_db = aegis_db::create_database_adapter(&forum_db_cfg).await?;
                        forum_db.init().await?;
                        tracing::info!("Forum dedicated database migration completed: {}", trimmed);
                    }
                }
            }
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
        Commands::Plugin { action } => run_plugin_command(&config, action).await?,
        Commands::Notify { action } => run_notify_command(&config, action).await?,
        Commands::Theme {
            action: ThemeCommand::Dev(args),
        } => theme_dev::run(args).await?,
        Commands::Deploy {
            action: DeployCommand::Generate { dir, builtin },
        } => deploy::run_generate(&dir, &builtin)?,
        #[cfg(feature = "backup")]
        Commands::Dump(args) => backup::run_dump(&config, args).await?,
        #[cfg(feature = "backup")]
        Commands::Restore(args) => backup::run_restore(&config, args).await?,
        Commands::Run { port, host } => {
            if let Some(p) = port {
                config.server.port = p;
            }
            if let Some(h) = host {
                config.server.host = h;
            }

            let base_path = config.server.normalized_base_path()?;
            let admin_path = config.server.normalized_admin_path()?;
            if config.storage.local.public_url_prefix.trim_end_matches('/') == admin_path {
                anyhow::bail!(
                    "[server].admin_path {:?} is the same as [storage.local].public_url_prefix: choose another name",
                    admin_path
                );
            }
            // SSO ログイン後の戻り先が既定（/admin/）のままなら、Admin Studio のパスに合わせる
            let admin_index = format!("{}/", admin_path);
            for redirect in [
                &mut config.auth.oidc.post_login_redirect,
                &mut config.auth.saml.post_login_redirect,
            ] {
                if redirect.as_str() == "/admin/" {
                    *redirect = admin_index.clone();
                }
            }
            config.plugins.validate()?;
            config.theme.validate()?;
            config.commerce.validate()?;
            config.comments.validate()?;
            config.tags.validate()?;
            if config.consent.enabled {
                config.consent.validate()?;
            }
            config.forum.validate()?;
            config.notifications.validate()?;
            config.cdn.validate()?;
            config.auth.registration.validate()?;
            #[cfg(feature = "analytics")]
            config.analytics.validate().map_err(anyhow::Error::msg)?;
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
            if config.cdn.enabled {
                tracing::info!(
                    "CDN Cache-Tag purge enabled (provider: {:?}, emit_headers: {})",
                    config.cdn.provider,
                    config.cdn.emit_headers
                );
            }

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
            // 4.1 サーバー間の変更の知らせ（[cache] と同じバックエンド）。プラグインの一覧を読む前に受け取り始める。
            // メモリ（1 台構成）なら、同じ DB で別のサーバーが動いていないかを確かめる
            let bus = aegis_cache::create_event_bus(&config.cache).await?;
            let bus_events = bus.subscribe();
            let single_server = if bus.reaches_other_servers() {
                None
            } else {
                Some(
                    cluster::SingleServerGuard::acquire(
                        db_adapter.clone(),
                        cluster::HEARTBEAT_INTERVAL,
                    )
                    .await?,
                )
            };

            // 5. Wasm プラグインスーパーバイザー初期化（[plugins].enabled のプラグインをその順に読み込む）
            let enabled_plugins =
                plugin_registry::resolve_enabled(db_adapter.as_ref(), &config.plugins.enabled)
                    .await?;
            let plugin_settings =
                Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
            load_cached_plugin_settings(&enabled_plugins, &cache, &plugin_settings).await;
            let (supervisor, plugin_loader) = load_plugins(
                &config,
                &enabled_plugins,
                cache.clone(),
                plugin_settings.clone(),
            )?;

            // 5.5 テーマエンジンの初期化 & プラグイン登録テンプレートの同期
            let theme_engine = Arc::new(
                aegis_theme::ThemeEngine::new(&config.paths.themes_dir, &config.theme.active)
                    .with_base_path(&base_path)
                    .with_admin_url(&public_admin_url(&config, &base_path, &admin_path))
                    .with_template_reload(config.theme.reload_templates)
                    .with_powered_by(config.theme.show_powered_by)
                    .with_locale(&config.theme.locale),
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

            let mut app_state = AppState::new(
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
            app_state.plugin_settings = plugin_settings;
            app_state.bus = bus;
            app_state.plugin_registry = Arc::new(plugin_registry::PluginRegistry::new(
                plugin_loader,
                enabled_plugins,
            ));
            *app_state
                .site
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                site::SiteOverrides::load(app_state.db.as_ref()).await;
            *app_state
                .seo
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                seo::SeoOverrides::load(app_state.db.as_ref()).await;
            *app_state
                .headers
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                headers::HeaderOverrides::load(app_state.db.as_ref()).await;
            *app_state.redirects.write().await =
                redirects::RedirectTable::load(app_state.db.as_ref()).await;
            *app_state
                .system
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                system::SystemOverrides::load(app_state.db.as_ref()).await;
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
                let mut notifier = aegis_notify::NotificationService::from_config_with_plugins(
                    &config.notifications,
                    notification_site(&config, &app_state.site().title),
                    Some(app_state.audit.clone()),
                    notify_plugins::plugin_channel_factory(app_state.supervisor.clone()),
                )?;
                // 送信待ちを DB に保存する（ワーカーは状態を作り終えてから起動する）
                if config.notifications.queue.persistent {
                    notifier = notifier.with_queue(app_state.db.clone());
                }
                tracing::info!(
                    "Notifications enabled (channels: {:?}, queue: {})",
                    notifier.channel_names(),
                    if notifier.is_persistent() {
                        "database"
                    } else {
                        "memory"
                    }
                );
                if let Some(sink) = &audit_notifications {
                    sink.attach(Arc::new(notifier.clone()));
                    tracing::info!(
                        "Audit notifications enabled ({} rule(s))",
                        config.notifications.audit.rules.len()
                    );
                }
                let app_state = app_state.with_notifications(notifier);
                // Admin Studio で編集したテンプレート
                handlers::notifications::reload_templates(&app_state).await;
                app_state
            } else {
                app_state
            };

            let admin_url_opt = resolve_admin_url(&config);
            let app_state = if let Some(ref url) = admin_url_opt {
                app_state.with_admin_url(url.clone())
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

            // 6.6.2 パスワード再設定（[auth.password_reset]）
            let app_state = if config.auth.password_reset.enabled {
                if let (Some(notifier), Some(admin_url)) =
                    (app_state.notifier.clone(), admin_url_opt.clone())
                {
                    tracing::info!("Password reset enabled (links open {})", admin_url);
                    let service = aegis_auth::password_reset::PasswordResetService::new(
                        app_state.db.clone(),
                        app_state.cache.clone(),
                        config.auth.password_reset.clone(),
                    );
                    app_state.with_password_reset(state::PasswordReset {
                        service,
                        notifier,
                        admin_url,
                    })
                } else {
                    app_state
                }
            } else {
                app_state
            };

            // 6.6.3 ユーザー招待（[auth.invitation]）
            let app_state = if config.auth.invitation.enabled {
                if let (Some(notifier), Some(admin_url)) =
                    (app_state.notifier.clone(), admin_url_opt.clone())
                {
                    tracing::info!("User invitation enabled (links open {})", admin_url);
                    let service = aegis_auth::invitation::InvitationService::new(
                        app_state.db.clone(),
                        app_state.cache.clone(),
                        config.auth.invitation.clone(),
                    );
                    app_state.with_invitation(state::Invitation {
                        service,
                        notifier,
                        admin_url,
                    })
                } else {
                    app_state
                }
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

                let customers = aegis_commerce::CustomerService::new(
                    app_state.db.clone(),
                    app_state.cache.clone(),
                    app_state.audit.clone(),
                );
                let reviews = aegis_commerce::ReviewService::new(
                    app_state.db.clone(),
                    app_state.audit.clone(),
                    app_state.notifier.clone(),
                    config.comments.moderation.clone(),
                );

                app_state
                    .with_commerce(commerce)
                    .with_customers(customers)
                    .with_reviews(reviews)
            } else {
                app_state
            };
            // 6.8 コメント（[comments]）。通知と Admin Studio の URL を使うのでその後に設定する
            let app_state = if config.comments.enabled {
                let comments = aegis_comments::CommentService::new(
                    config.comments.clone(),
                    app_state.comment_deps(),
                )?;
                comments.check_content_types(&*app_state.schemas.read().await)?;
                if app_state.notifier.is_none() {
                    tracing::warn!(
                        "Comments are enabled without [notifications]: authors and staff are not told about new comments"
                    );
                }
                tracing::info!(
                    "Comments enabled (types: {:?}, approval: {}, auto-approve known authors: {})",
                    config.comments.content_types,
                    config.comments.moderation.require_approval,
                    config.comments.moderation.auto_approve_known
                );
                app_state.with_comments(comments)
            } else {
                app_state
            };

            // 6.8b タグ（[tags]）
            let app_state = if config.tags.enabled {
                tracing::info!(
                    "Tags enabled (up to {} per content)",
                    config.tags.max_per_content
                );
                let tags = aegis_tags::TagService::new(app_state.db.clone(), config.tags.clone());
                app_state.with_tags(tags)
            } else {
                app_state
            };

            // 6.9 掲示板（[forum]）
            let app_state = if config.forum.enabled {
                let forum_db = if let Some(ref forum_url) = config.forum.database_url {
                    let trimmed = forum_url.trim();
                    if !trimmed.is_empty() {
                        let forum_db_cfg = aegis_db::DatabaseConfig {
                            url: trimmed.to_string(),
                            ..config.database.clone()
                        };
                        let dedicated = aegis_db::create_database_adapter(&forum_db_cfg).await?;
                        dedicated.init().await?;
                        tracing::info!("Forum enabled with dedicated database (URL: {})", trimmed);
                        dedicated
                    } else {
                        app_state.db.clone()
                    }
                } else {
                    app_state.db.clone()
                };

                let forum_deps = app_state.forum_deps(forum_db);
                let forum_service =
                    aegis_forum::ForumService::new(config.forum.clone(), forum_deps)?;
                tracing::info!(
                    "Forum enabled (rate limit: {}s, anonymous: {}, auto-lock: {})",
                    config.forum.rate_limit_secs,
                    config.forum.allow_anonymous,
                    config.forum.auto_lock_post_count
                );
                app_state.with_forum(forum_service)
            } else {
                app_state
            };
            // 6.9.1 問い合わせフォーム（[contact]）。通知と Admin Studio の URL を使うのでその後に設定する
            let app_state = if config.contact.enabled {
                let contact = aegis_contact::ContactService::new(
                    config.contact.clone(),
                    app_state.contact_deps(),
                )?;
                if app_state.notifier.is_none() {
                    tracing::warn!(
                        "Contact forms are enabled without [notifications]: messages are only in Admin Studio's inbox"
                    );
                }
                tracing::info!(
                    "Contact forms enabled ({:?})",
                    config
                        .contact
                        .forms
                        .iter()
                        .map(|f| f.name.as_str())
                        .collect::<Vec<_>>()
                );
                app_state.with_contact(contact)
            } else {
                app_state
            };
            // 6.9.2 ニュースレター（[newsletter]）。確認メールと配信は通知で送るので、通知の後に設定する
            let app_state = if config.newsletter.enabled {
                let newsletter = aegis_newsletter::NewsletterService::new(
                    config.newsletter.clone(),
                    app_state.newsletter_deps(),
                )?;
                if app_state.notifier.is_none() {
                    tracing::warn!(
                        "The newsletter is enabled without [notifications]: confirmation emails and issues cannot be sent"
                    );
                }
                tracing::info!("Newsletter enabled");
                app_state.with_newsletter(newsletter)
            } else {
                app_state
            };
            // 6.10 サイト内検索（[search]）。索引は起動後に裏で作り直す
            #[cfg(feature = "search")]
            let app_state = if config.search.enabled {
                let schemas = app_state.schemas.read().await;
                for content_type in &config.search.content_types {
                    if !schemas.contains_key(content_type) {
                        tracing::warn!(
                            "[search].content_types has '{}', but no such schema is loaded",
                            content_type
                        );
                    }
                }
                drop(schemas);
                let search = search::SiteSearch::from_config(&config.search)?;
                tracing::info!(
                    "Site search enabled (types: {:?}, index: {})",
                    search.content_types(),
                    if config.search.index_dir.trim().is_empty() {
                        "memory"
                    } else {
                        config.search.index_dir.trim()
                    }
                );
                let mut app_state = app_state;
                app_state.search = Some(Arc::new(search));
                app_state
            } else {
                app_state
            };
            // 6.11 アクセス解析（[analytics]）
            #[cfg(feature = "analytics")]
            let app_state = if config.analytics.enabled {
                tracing::info!(
                    "Analytics enabled (cookie-less beacon, retention: {} days)",
                    config.analytics.retention_days
                );
                let mut app_state = app_state;
                app_state.analytics = Some(Arc::new(aegis_analytics::AnalyticsService::new(
                    config.analytics.clone(),
                    app_state.db.clone(),
                    app_state.cache.clone(),
                )));
                app_state
            } else {
                app_state
            };
            let app_state = Arc::new(app_state);
            if let Some(commerce) = &app_state.commerce {
                commerce.spawn_expiry_task();
            }
            // 通知の送信待ち（[notifications.queue].persistent）。前回の起動で送れなかったものも送る
            if let Some(notifications) = &app_state.notifications {
                notifications.spawn_worker();
            }
            if let Some(contact) = &app_state.contact {
                contact.spawn_retention_task();
            }
            if let Some(newsletter) = &app_state.newsletter {
                newsletter.spawn_cleanup_task();
            }
            // 予約公開（status: "scheduled"）
            scheduler::spawn_publish_task(app_state.clone());
            // ほかのサーバーでの切り替え・設定変更を反映する
            cluster::spawn_listener(app_state.clone(), bus_events);
            #[cfg(feature = "search")]
            search::spawn_initial_rebuild(app_state.clone());
            #[cfg(feature = "analytics")]
            if let Some(analytics) = &app_state.analytics {
                analytics.spawn_tasks();
            }
            #[cfg(feature = "analytics")]
            let analytics_on_shutdown = app_state.analytics.clone();

            // 7. Router 設定（[server].base_path 配下で公開）
            let app = app::with_base_path(app::router(&config, app_state), &base_path, &admin_path);

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
            .with_graceful_shutdown(shutdown_signal())
            .await?;
            // 溜めていたアクセス解析の集計を書き出す
            #[cfg(feature = "analytics")]
            if let Some(analytics) = analytics_on_shutdown {
                analytics.flush().await;
            }
            if let Some(guard) = single_server {
                guard.release().await;
            }
            tracing::info!("AegisCMS stopped");
        }
    }

    Ok(())
}

async fn load_cached_plugin_settings(
    enabled: &[String],
    cache: &Arc<dyn aegis_core::CacheBackend>,
    plugin_settings: &Arc<
        std::sync::RwLock<
            std::collections::HashMap<String, std::collections::HashMap<String, String>>,
        >,
    >,
) {
    for name in enabled {
        let key = format!("plugin_settings:{}", name);
        if let Ok(Some(json_str)) = cache.get(&key).await {
            if let Ok(settings) =
                serde_json::from_str::<std::collections::HashMap<String, String>>(&json_str)
            {
                if let Ok(mut guard) = plugin_settings.write() {
                    guard.insert(name.clone(), settings);
                }
            }
        }
    }
}

/// CLI で使う有効なプラグインの一覧（Admin Studio で保存した値 → `[plugins].enabled`）。
/// DB を読めなければ `[plugins].enabled` を使う
async fn cli_enabled_plugins(config: &AegisConfig) -> Vec<String> {
    let resolved = async {
        let db = aegis_db::create_database_adapter(&config.database).await?;
        plugin_registry::resolve_enabled(db.as_ref(), &config.plugins.enabled).await
    }
    .await;
    resolved.unwrap_or_else(|e| {
        tracing::warn!(
            "Could not read the plugin list saved from Admin Studio, using [plugins].enabled: {}",
            e
        );
        config.plugins.enabled.clone()
    })
}

/// Ctrl+C（Unix では SIGTERM も）を待つ
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::warn!("Could not listen for Ctrl+C: {}", e);
            std::future::pending::<()>().await;
        }
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(e) => {
                tracing::warn!("Could not listen for SIGTERM: {}", e);
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
    tracing::info!("Shutting down");
}

/// `enabled`（有効なプラグインの一覧）をその順に読み込む（`[runtime].enable_wasm_plugins = false` なら読み込まない）。
/// あとで Admin Studio から切り替えられるように、読み込みに使ったローダーも返す
fn load_plugins(
    config: &AegisConfig,
    enabled: &[String],
    cache: Arc<dyn aegis_core::CacheBackend>,
    plugin_settings: Arc<
        std::sync::RwLock<
            std::collections::HashMap<String, std::collections::HashMap<String, String>>,
        >,
    >,
) -> anyhow::Result<(PluginSupervisor, Option<plugin_registry::PluginLoader>)> {
    if !config.runtime.enable_wasm_plugins {
        if !enabled.is_empty() {
            tracing::warn!("[runtime].enable_wasm_plugins = false: enabled plugins are not loaded");
        }
        return Ok((PluginSupervisor::new(), None));
    }
    tracing::info!("Initializing Wasmtime runtime engine...");
    let engine = create_wasm_engine(&config.runtime)?;
    // KV は [cache] のバックエンド、HTTP は allowed_domains の確認済みのリクエストのみ
    let host_services: Arc<dyn aegis_runtime::HostServices> = Arc::new(
        plugin_host::ServerHostServices::new(cache, config.plugins.clone())?
            .with_dynamic_settings(plugin_settings),
    );
    let loader = plugin_registry::PluginLoader::new(
        engine,
        host_services.into(),
        config.paths.plugins_dir.clone(),
        config.runtime.default_resource_limits(),
    );
    let supervisor = loader.load(enabled)?;
    tracing::info!(
        "Loaded {} Wasm plugins from {:?}",
        supervisor.plugins().len(),
        config.paths.plugins_dir
    );
    Ok((supervisor, Some(loader)))
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
    fn test_public_admin_url_hides_a_custom_admin_path_by_default() {
        let url = |admin_path: &str, show: Option<bool>| {
            let mut config = AegisConfig::default();
            config.theme.show_admin_link = show;
            public_admin_url(&config, "/cms", admin_path)
        };
        assert_eq!(url("/admin", None), "/cms/admin/");
        assert_eq!(url("/backstage", None), "");
        assert_eq!(url("/backstage", Some(true)), "/cms/backstage/");
        assert_eq!(url("/admin", Some(false)), "");
    }

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

        // 2FA を有効化してから CLI で解除
        aegis_auth::users::enable_2fa(db.as_ref(), "dave", "JBSWY3DPEHPK3PXP", "[]")
            .await
            .unwrap();
        let user = db.get_user_by_username("dave").await.unwrap().unwrap();
        assert!(user.totp_enabled);

        run_user_command(
            &config,
            UserCommand::Disable2fa {
                username: "dave".to_string(),
            },
        )
        .await
        .unwrap();

        let user = db.get_user_by_username("dave").await.unwrap().unwrap();
        assert!(!user.totp_enabled);
        assert_eq!(user.totp_secret, None);

        let _ = std::fs::remove_file(&temp_db);
    }
    #[test]
    fn test_resolve_admin_url_precedence() {
        let mut config = AegisConfig::default();
        assert_eq!(resolve_admin_url(&config), None);

        config.commerce.public_url = "https://example.com".to_string();
        assert_eq!(
            resolve_admin_url(&config),
            Some("https://example.com/admin/".to_string())
        );

        config.auth.registration.admin_url = "https://reg.example.com/admin/".to_string();
        assert_eq!(
            resolve_admin_url(&config),
            Some("https://reg.example.com/admin/".to_string())
        );

        config.auth.password_reset.admin_url = "https://reset.example.com/admin/".to_string();
        assert_eq!(
            resolve_admin_url(&config),
            Some("https://reset.example.com/admin/".to_string())
        );
    }
}
