use std::collections::HashMap;
use std::path::Path;

use aegis_auth::users::{create_user, NewUser};
use aegis_db::{ContentRecord, DatabaseAdapter};
use aegis_schema::SchemaLoader;
use serde::{Deserialize, Serialize};

use crate::config::AegisConfig;
use crate::site::{validate_title, TITLE_SETTING};

/// Options for initializing AegisCMS initial data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitOptions {
    /// Site title (saved to `_aegis_settings` under `site.title`)
    pub site_title: String,
    /// Username for the initial administrator
    pub admin_username: String,
    /// Password for the initial administrator
    pub admin_password: String,
    /// Email for the initial administrator
    pub admin_email: Option<String>,
    /// Display name for the initial administrator
    pub admin_display_name: Option<String>,
    /// Whether to create sample content (e.g. welcome article, about page)
    pub create_sample_data: bool,
}

impl Default for InitOptions {
    fn default() -> Self {
        Self {
            site_title: "AegisCMS".to_string(),
            admin_username: "admin".to_string(),
            admin_password: String::new(),
            admin_email: None,
            admin_display_name: Some("Administrator".to_string()),
            create_sample_data: true,
        }
    }
}

/// Result of initialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitResult {
    pub admin_user_id: String,
    pub admin_username: String,
    pub site_title: String,
    pub sample_article_id: Option<String>,
    pub sample_page_id: Option<String>,
}

/// Ensure required directories exist before running migrations or server startup.
pub async fn ensure_directories(config: &AegisConfig) -> anyhow::Result<()> {
    // 1. Local upload directory
    if let Err(e) = tokio::fs::create_dir_all(&config.storage.local.upload_dir).await {
        tracing::warn!(
            "Failed to create upload directory {:?}: {}",
            config.storage.local.upload_dir,
            e
        );
    }

    // 2. SQLite parent directory if using SQLite file path
    let url = config.database.url.trim();
    if url.starts_with("sqlite://") || url.starts_with("sqlite:") {
        let path_str = url
            .trim_start_matches("sqlite://")
            .trim_start_matches("sqlite:");
        let path_str = path_str.split('?').next().unwrap_or(path_str);
        if path_str != ":memory:" && !path_str.is_empty() {
            let path = Path::new(path_str);
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    let _ = tokio::fs::create_dir_all(parent).await;
                }
            }
        }
    }

    Ok(())
}

/// Check if the system is already initialized (i.e. at least one user account exists).
pub async fn is_initialized(db: &dyn DatabaseAdapter) -> anyhow::Result<bool> {
    let users = db.list_users().await?;
    Ok(!users.is_empty())
}

/// Execute initial data setup:
/// 1. Verifies that the system has no existing user accounts (prevents re-initialization).
/// 2. Prepares directories and runs database schema migrations.
/// 3. Creates the first administrator account.
/// 4. Saves the site title.
/// 5. Seeds initial sample content if requested.
pub async fn run_init(config: &AegisConfig, options: InitOptions) -> anyhow::Result<InitResult> {
    // Basic input validation
    let username = options.admin_username.trim();
    if username.is_empty() {
        anyhow::bail!("Admin username cannot be empty");
    }
    if options.admin_password.len() < 8 {
        anyhow::bail!("Admin password must be at least 8 characters long");
    }
    let title = options.site_title.trim();
    if !title.is_empty() {
        validate_title(title).map_err(|e| anyhow::anyhow!("{}", e))?;
    }

    // Ensure required storage directories exist
    ensure_directories(config).await?;

    // Connect to database
    let db_adapter = aegis_db::create_database_adapter(&config.database).await?;

    // Load schemas
    let schemas_vec = SchemaLoader::load_dir(&config.paths.schemas_dir)?;
    let schemas: HashMap<String, _> = schemas_vec
        .into_iter()
        .map(|s| (s.content_type.clone(), s))
        .collect();

    // Run migrations first so tables (including _aegis_users) exist
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
            }
        }
    }

    // Check if user accounts already exist
    if is_initialized(db_adapter.as_ref()).await? {
        anyhow::bail!("System is already initialized: user accounts already exist.");
    }

    // Create the initial administrator user
    let new_user = NewUser {
        username: username.to_string(),
        password: options.admin_password,
        role: "admin".to_string(),
        email: options
            .admin_email
            .map(|e| e.trim().to_string())
            .filter(|e| !e.is_empty()),
        display_name: options
            .admin_display_name
            .map(|d| d.trim().to_string())
            .filter(|d| !d.is_empty()),
    };
    let admin_user = create_user(db_adapter.as_ref(), new_user).await?;

    // Save site title setting
    if !title.is_empty() {
        db_adapter.set_setting(TITLE_SETTING, title).await?;
    }

    // Seed sample content if requested
    let mut sample_article_id = None;
    let mut sample_page_id = None;

    if options.create_sample_data {
        let now = chrono::Utc::now().to_rfc3339();

        // 1. Sample Article (if 'article' schema exists)
        if let Some(article_schema) = schemas.get("article") {
            let article_id = uuid::Uuid::new_v4().to_string();
            let mut custom_fields = HashMap::new();
            custom_fields.insert(
                "excerpt".to_string(),
                serde_json::json!("Welcome to your new AegisCMS website!"),
            );

            let blocks = serde_json::json!([
                {
                    "type": "paragraph",
                    "content": "Welcome to AegisCMS! This is your first sample article automatically generated during installation. You can edit, unpublish, or delete this post at any time from the Admin Studio."
                },
                {
                    "type": "paragraph",
                    "content": "AegisCMS provides content type schemas, high-speed cached rendering, Wasm plugins, and flexible theme customization."
                }
            ]);

            let record = ContentRecord {
                id: article_id.clone(),
                content_type: "article".to_string(),
                title: "Welcome to AegisCMS".to_string(),
                slug: "welcome-to-aegiscms".to_string(),
                status: "published".to_string(),
                blocks_json: blocks.to_string(),
                author_id: Some(admin_user.id.clone()),
                published_at: Some(now.clone()),
                unpublish_at: None,
                created_at: now.clone(),
                updated_at: now.clone(),
                custom_fields,
            };

            if let Err(e) = db_adapter.insert_content(article_schema, &record).await {
                tracing::warn!("Failed to seed sample article: {}", e);
            } else {
                sample_article_id = Some(article_id);
            }
        }

        // 2. Sample Page (if 'page' schema exists)
        if let Some(page_schema) = schemas.get("page") {
            let page_id = uuid::Uuid::new_v4().to_string();
            let page_blocks = serde_json::json!([
                {
                    "type": "paragraph",
                    "content": "This is an example 'About' page. Unlike blog posts, pages are best suited for static, evergreen content like company overviews, contact details, or policy documents."
                }
            ]);

            let page_record = ContentRecord {
                id: page_id.clone(),
                content_type: "page".to_string(),
                title: "About Us".to_string(),
                slug: "about".to_string(),
                status: "published".to_string(),
                blocks_json: page_blocks.to_string(),
                author_id: Some(admin_user.id.clone()),
                published_at: Some(now.clone()),
                unpublish_at: None,
                created_at: now.clone(),
                updated_at: now,
                custom_fields: HashMap::new(),
            };

            if let Err(e) = db_adapter.insert_content(page_schema, &page_record).await {
                tracing::warn!("Failed to seed sample page: {}", e);
            } else {
                sample_page_id = Some(page_id);
            }
        }
    }

    Ok(InitResult {
        admin_user_id: admin_user.id,
        admin_username: admin_user.username,
        site_title: title.to_string(),
        sample_article_id,
        sample_page_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_init_options_default() {
        let opts = InitOptions::default();
        assert_eq!(opts.site_title, "AegisCMS");
        assert_eq!(opts.admin_username, "admin");
        assert!(opts.create_sample_data);
    }

    #[tokio::test]
    async fn test_run_init_validation() {
        let config = AegisConfig::default();
        let short_pwd = InitOptions {
            admin_password: "short".to_string(),
            ..InitOptions::default()
        };
        assert!(run_init(&config, short_pwd).await.is_err());

        let empty_user = InitOptions {
            admin_username: "   ".to_string(),
            admin_password: "password123".to_string(),
            ..InitOptions::default()
        };
        assert!(run_init(&config, empty_user).await.is_err());

        let long_title = InitOptions {
            site_title: "a".repeat(201),
            admin_password: "password123".to_string(),
            ..InitOptions::default()
        };
        assert!(run_init(&config, long_title).await.is_err());
    }

    #[tokio::test]
    async fn test_run_init_success_and_guard() {
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
        let page_schema = r#"
content_type: page
label: "Page"
fields: []
"#;
        tokio::fs::write(schemas_dir.join("article.yaml"), article_schema)
            .await
            .unwrap();
        tokio::fs::write(schemas_dir.join("page.yaml"), page_schema)
            .await
            .unwrap();

        let db_file = temp_dir.path().join("test.db");
        let mut config = AegisConfig::default();
        config.database.url = format!("sqlite://{}?mode=rwc", db_file.to_str().unwrap());
        config.paths.schemas_dir = schemas_dir;

        let opts = InitOptions {
            site_title: "My Awesome Blog".to_string(),
            admin_username: "sysadmin".to_string(),
            admin_password: "super_secret_password".to_string(),
            admin_email: Some("admin@example.com".to_string()),
            admin_display_name: Some("System Admin".to_string()),
            create_sample_data: true,
        };

        let result = run_init(&config, opts.clone()).await.unwrap();
        assert_eq!(result.admin_username, "sysadmin");
        assert_eq!(result.site_title, "My Awesome Blog");
        assert!(result.sample_article_id.is_some());
        assert!(result.sample_page_id.is_some());

        // Verify database contents
        let db = aegis_db::create_database_adapter(&config.database)
            .await
            .unwrap();
        let users = db.list_users().await.unwrap();
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].username, "sysadmin");
        assert_eq!(users[0].role, "admin");

        let settings = db.list_settings().await.unwrap();
        assert_eq!(
            settings.get(TITLE_SETTING).map(String::as_str),
            Some("My Awesome Blog")
        );

        let article_schema_def = aegis_schema::SchemaLoader::load_from_file(
            config.paths.schemas_dir.join("article.yaml"),
        )
        .unwrap();
        let article = db
            .get_published_content(&article_schema_def, "welcome-to-aegiscms")
            .await
            .unwrap();
        assert!(article.is_some());
        assert_eq!(article.unwrap().title, "Welcome to AegisCMS");

        // Verify guard: second run must fail because user accounts already exist
        let second_run = run_init(&config, opts).await;
        assert!(second_run.is_err());
        let err_msg = second_run.unwrap_err().to_string();
        assert!(err_msg.contains("already initialized"));
    }
}
