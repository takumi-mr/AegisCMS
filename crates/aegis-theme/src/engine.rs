use aegis_core::{AegisError, PageRenderContext, Result};
use minijinja::Environment;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use crate::builtins::*;
use crate::manifest::{ThemeInfo, ThemeManifest};

struct ThemeEngineInner {
    active_theme: String,
    theme_templates_dir: Option<PathBuf>,
    dynamic_templates: HashMap<String, String>,
    base_path: String,
    /// テンプレートをリクエストごとに読み直す（`[theme].reload_templates`、テーマ開発用）
    reload_templates: bool,
    /// コンパイル済みテンプレートを保持する環境。テーマ・テンプレート・ベースパスの変更で破棄する
    environment: Option<Arc<Environment<'static>>>,
}

#[derive(Clone)]
pub struct ThemeEngine {
    themes_dir: PathBuf,
    inner: Arc<RwLock<ThemeEngineInner>>,
}

impl ThemeEngine {
    pub fn new(themes_dir: &Path, active_theme: &str) -> Self {
        let active = active_theme.to_string();
        let theme_dir = themes_dir.join(&active).join("templates");
        let theme_templates_dir = if theme_dir.exists() {
            Some(theme_dir)
        } else {
            None
        };

        Self {
            themes_dir: themes_dir.to_path_buf(),
            inner: Arc::new(RwLock::new(ThemeEngineInner {
                active_theme: active,
                theme_templates_dir,
                dynamic_templates: HashMap::new(),
                base_path: String::new(),
                reload_templates: false,
                environment: None,
            })),
        }
    }

    /// サイトを公開するパス（`[server].base_path` の正規化済みの値、例: `/cms`）。
    /// テンプレートからはグローバル変数 `base_path` として `{{ base_path }}/articles/...` のように使う
    pub fn with_base_path(self, base_path: &str) -> Self {
        {
            let mut inner = self.inner.write().unwrap();
            inner.base_path = base_path.to_string();
            inner.environment = None;
        }
        self
    }

    /// `true` でテンプレートを描画のたびに読み直す（`[theme].reload_templates`）。
    /// 既定ではコンパイル済みのテンプレートを再利用し、テーマの切り替えやテンプレートの登録時だけ読み直す
    pub fn with_template_reload(self, reload: bool) -> Self {
        {
            let mut inner = self.inner.write().unwrap();
            inner.reload_templates = reload;
            inner.environment = None;
        }
        self
    }

    /// 現在アクティブなテーマ識別子を取得
    pub fn active_theme(&self) -> String {
        self.inner.read().unwrap().active_theme.clone()
    }

    /// アクティブなテーマを動的に切り替える
    pub fn set_active_theme(&self, theme_id: &str) -> Result<()> {
        let target_id = theme_id.trim();
        if target_id.is_empty() {
            return Err(AegisError::Theme("Theme ID cannot be empty".to_string()));
        }

        // テーマディレクトリまたは組み込みデフォルトテーマの存在確認
        let target_dir = self.themes_dir.join(target_id);
        if !target_dir.exists() && target_id != "default" {
            return Err(AegisError::Theme(format!(
                "Theme '{}' not found in themes directory: {}",
                target_id,
                self.themes_dir.display()
            )));
        }

        let templates_dir = target_dir.join("templates");
        let theme_templates_dir = if templates_dir.exists() {
            Some(templates_dir)
        } else {
            None
        };

        let mut inner = self.inner.write().unwrap();
        inner.active_theme = target_id.to_string();
        inner.theme_templates_dir = theme_templates_dir;
        inner.environment = None;

        tracing::info!("AegisCMS active theme switched to '{}'", target_id);
        Ok(())
    }

    /// インストールされているテーマ一覧を取得
    pub fn list_installed_themes(&self) -> Vec<ThemeInfo> {
        let active = self.active_theme();
        let mut themes = Vec::new();
        let mut found_ids = std::collections::HashSet::new();

        // 1. themes_dir 配下のディレクトリを走査
        if self.themes_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&self.themes_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        if let Some(folder_name) = path.file_name().and_then(|s| s.to_str()) {
                            // 隠しフォルダは無視
                            if folder_name.starts_with('.') {
                                continue;
                            }

                            let manifest = ThemeManifest::load_from_dir(&path)
                                .unwrap_or_else(|| ThemeManifest::fallback_from_id(folder_name));

                            let has_templates = path.join("templates").exists();
                            let has_assets = path.join("assets").exists();

                            // プレビュー画像ファイルの存在確認
                            let preview_url = self
                                .find_preview_image(&path, &manifest)
                                .map(|_| format!("/themes/{}/preview", manifest.id));

                            found_ids.insert(manifest.id.clone());
                            themes.push(ThemeInfo {
                                is_active: manifest.id == active,
                                manifest,
                                has_templates,
                                has_assets,
                                preview_url,
                            });
                        }
                    }
                }
            }
        }

        // 2. 組み込みの default テーマがファイルシステムに存在しない場合でも必ず一覧に含める
        if !found_ids.contains("default") {
            let default_manifest = ThemeManifest {
                id: "default".to_string(),
                name: "Aegis Dark Core (Built-in)".to_string(),
                version: "1.0.0".to_string(),
                description: "Built-in default theme with sleek dark styling and indigo gradients."
                    .to_string(),
                author: crate::manifest::ThemeAuthor::Simple("AegisCMS Core".to_string()),
                license: Some("MIT".to_string()),
                preview_image: None,
                tags: vec![
                    "dark".to_string(),
                    "builtin".to_string(),
                    "minimal".to_string(),
                ],
                homepage: Some("https://github.com/takumi-mr/AegisCMS".to_string()),
                repository: None,
                min_aegis_version: None,
            };

            themes.push(ThemeInfo {
                is_active: active == "default",
                manifest: default_manifest,
                has_templates: false,
                has_assets: false,
                preview_url: None,
            });
        }

        // アクティブなテーマを先頭、それ以外は ID 昇順でソート
        themes.sort_by(|a, b| {
            if a.is_active {
                std::cmp::Ordering::Less
            } else if b.is_active {
                std::cmp::Ordering::Greater
            } else {
                a.manifest.name.cmp(&b.manifest.name)
            }
        });

        themes
    }

    /// 特定テーマの情報を取得
    pub fn get_theme_info(&self, theme_id: &str) -> Option<ThemeInfo> {
        self.list_installed_themes()
            .into_iter()
            .find(|t| t.manifest.id == theme_id)
    }

    /// 現在アクティブなテーマのアセットディレクトリパス
    pub fn active_assets_dir(&self) -> Option<PathBuf> {
        let active = self.active_theme();
        self.theme_assets_dir(&active)
    }

    /// 指定されたテーマのアセットディレクトリパス
    pub fn theme_assets_dir(&self, theme_id: &str) -> Option<PathBuf> {
        let dir = self.themes_dir.join(theme_id).join("assets");
        if dir.exists() {
            Some(dir)
        } else {
            None
        }
    }

    /// テーマのプレビュー画像パスを取得
    pub fn theme_preview_file(&self, theme_id: &str) -> Option<PathBuf> {
        let theme_dir = self.themes_dir.join(theme_id);
        if !theme_dir.exists() {
            return None;
        }

        let manifest = ThemeManifest::load_from_dir(&theme_dir);
        if let Some(m) = manifest {
            self.find_preview_image(&theme_dir, &m)
        } else {
            self.find_preview_image(&theme_dir, &ThemeManifest::fallback_from_id(theme_id))
        }
    }

    fn find_preview_image(&self, theme_dir: &Path, manifest: &ThemeManifest) -> Option<PathBuf> {
        // 1. マニフェストで指定された画像
        if let Some(ref img) = manifest.preview_image {
            let p = theme_dir.join(img);
            if p.exists() {
                return Some(p);
            }
        }

        // 2. よくあるプレビュー画像ファイル名の探索
        for candidate in ["preview.svg", "preview.png", "preview.webp", "preview.jpg"] {
            let p = theme_dir.join(candidate);
            if p.exists() {
                return Some(p);
            }
        }

        None
    }

    /// 将来のマーケットプレイスからの導入を見据えたテーマパッケージの検証
    pub fn validate_theme_package(&self, theme_dir: &Path) -> Result<ThemeManifest> {
        if !theme_dir.is_dir() {
            return Err(AegisError::Theme(format!(
                "Path is not a directory: {}",
                theme_dir.display()
            )));
        }

        let manifest_path = theme_dir.join("theme.json");
        if !manifest_path.exists() {
            return Err(AegisError::Theme(
                "Missing required 'theme.json' manifest in theme package".to_string(),
            ));
        }

        let manifest = ThemeManifest::load_from_dir(theme_dir).ok_or_else(|| {
            AegisError::Theme("Failed to parse valid 'theme.json' manifest".to_string())
        })?;

        let templates_dir = theme_dir.join("templates");
        if !templates_dir.exists() {
            return Err(AegisError::Theme(
                "Theme package must contain a 'templates/' directory".to_string(),
            ));
        }

        let base_tpl = templates_dir.join("base.html");
        if !base_tpl.exists() {
            return Err(AegisError::Theme(
                "Theme package templates must include 'base.html'".to_string(),
            ));
        }

        Ok(manifest)
    }

    /// プラグインや実行時設定から動的テンプレートを登録・上書きする
    pub fn register_template(&self, name: &str, content: &str) {
        let mut inner = self.inner.write().unwrap();
        inner
            .dynamic_templates
            .insert(name.to_string(), content.to_string());
        inner.environment = None;
    }

    /// 登録済みの動的テンプレートを削除する
    pub fn unregister_template(&self, name: &str) {
        let mut inner = self.inner.write().unwrap();
        inner.dynamic_templates.remove(name);
        inner.environment = None;
    }

    /// テンプレートと PageRenderContext から HTML を描画する
    pub fn render(&self, template_name: &str, ctx: &PageRenderContext) -> Result<String> {
        let env = self.environment();

        let template = env.get_template(template_name).map_err(|e| {
            AegisError::Theme(format!("Template error for '{}': {}", template_name, e))
        })?;

        let value = minijinja::Value::from_serialize(ctx);
        let rendered = template.render(value).map_err(|e| {
            AegisError::Theme(format!(
                "Failed to render template '{}': {}",
                template_name, e
            ))
        })?;

        Ok(rendered)
    }

    /// 描画に使う環境（キャッシュがあれば再利用し、無ければ作ってキャッシュする）
    fn environment(&self) -> Arc<Environment<'static>> {
        if let Some(env) = &self.inner.read().unwrap().environment {
            return env.clone();
        }
        let mut inner = self.inner.write().unwrap();
        if let Some(env) = &inner.environment {
            return env.clone();
        }
        let env = Arc::new(Self::build_environment(
            inner.dynamic_templates.clone(),
            inner.theme_templates_dir.clone(),
            inner.base_path.clone(),
        ));
        if !inner.reload_templates {
            inner.environment = Some(env.clone());
        }
        env
    }

    /// フィルター・グローバル変数・テンプレートの探索順（動的登録 → テーマ → 組み込み）を設定した環境。
    /// 読み込んだテンプレートは環境の中でコンパイル済みのままキャッシュされる
    fn build_environment(
        dynamic_templates: HashMap<String, String>,
        fs_dir: Option<PathBuf>,
        base_path: String,
    ) -> Environment<'static> {
        let mut env = Environment::new();

        env.add_filter("date_short", |val: minijinja::Value| -> String {
            if let Some(s) = val.as_str() {
                if s.len() >= 10 {
                    s[..10].to_string()
                } else {
                    s.to_string()
                }
            } else {
                String::new()
            }
        });

        env.add_filter(
            "truncate",
            |val: minijinja::Value,
             limit: Option<usize>,
             _killwords: Option<bool>,
             end: Option<&str>|
             -> String {
                let s = val.as_str().unwrap_or("");
                let lim = limit.unwrap_or(255);
                let suffix = end.unwrap_or("...");
                if s.chars().count() > lim {
                    let mut res: String = s.chars().take(lim).collect();
                    res.push_str(suffix);
                    res
                } else {
                    s.to_string()
                }
            },
        );

        // `[server].base_path` の正規化済みの値（URL で安全な文字のみ）なので自動エスケープ（`/` → `&#x2f;`）しない
        env.add_global("base_path", minijinja::Value::from_safe_string(base_path));

        env.set_loader(
            move |name: &str| -> std::result::Result<Option<String>, minijinja::Error> {
                // 1. 動的オーバーライド (プラグイン・インメモリ登録)
                if let Some(content) = dynamic_templates.get(name) {
                    return Ok(Some(content.clone()));
                }

                // 2. ファイルシステムからの読み込み
                if let Some(ref dir) = fs_dir {
                    let file_path = dir.join(name);
                    if file_path.exists() {
                        match std::fs::read_to_string(&file_path) {
                            Ok(content) => return Ok(Some(content)),
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to read theme template {}: {}",
                                    file_path.display(),
                                    e
                                );
                            }
                        }
                    }
                }

                // 3. 組み込みデフォルトテンプレート
                match name {
                    "base.html" => Ok(Some(BASE_TEMPLATE.to_string())),
                    "index.html" => Ok(Some(INDEX_TEMPLATE.to_string())),
                    "article.html" => Ok(Some(ARTICLE_TEMPLATE.to_string())),
                    "page.html" => Ok(Some(PAGE_TEMPLATE.to_string())),
                    "404.html" => Ok(Some(NOT_FOUND_TEMPLATE.to_string())),
                    _ => Ok(None),
                }
            },
        );

        env
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_core::ContentSummary;

    #[test]
    fn test_render_builtin_base_and_index() {
        let engine = ThemeEngine::new(Path::new("non_existent_dir"), "default");
        assert_eq!(engine.active_theme(), "default");

        let ctx = PageRenderContext {
            site_title: "Test Aegis Site".to_string(),
            site_description: "A fast secure blog".to_string(),
            template: "index.html".to_string(),
            content_type: "index".to_string(),
            title: "".to_string(),
            slug: "".to_string(),
            content_html: "".to_string(),
            author: None,
            published_at: None,
            updated_at: None,
            fields: HashMap::new(),
            relations: HashMap::new(),
            items: vec![ContentSummary {
                id: "1".to_string(),
                content_type: "article".to_string(),
                title: "First Post".to_string(),
                slug: "first-post".to_string(),
                excerpt: "This is a summary".to_string(),
                author_id: Some("takumi".to_string()),
                published_at: Some("2026-10-22".to_string()),
                created_at: "2026-10-22".to_string(),
                fields: HashMap::new(),
                relations: HashMap::new(),
            }],
            extra_css: "".to_string(),
            extra_head: "".to_string(),
            theme_vars: HashMap::new(),
            pagination: None,
        };

        let html = engine.render("index.html", &ctx).expect("Render failed");
        assert!(html.contains("Test Aegis Site"));
        assert!(html.contains("First Post"));
        assert!(html.contains("/articles/first-post"));
        assert!(html.contains("Powered by <strong>AegisCMS</strong>"));
    }

    #[test]
    fn test_render_links_under_base_path() {
        let engine =
            ThemeEngine::new(Path::new("non_existent_dir"), "default").with_base_path("/cms");
        let ctx = PageRenderContext {
            site_title: "Sub-path Site".to_string(),
            template: "index.html".to_string(),
            items: vec![ContentSummary {
                title: "First Post".to_string(),
                slug: "first-post".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };

        let html = engine.render("index.html", &ctx).expect("Render failed");
        assert!(html.contains(r#"href="/cms/articles/first-post""#));
        assert!(html.contains(r#"href="/cms/admin/""#));
        assert!(html.contains(r#"href="/cms/""#));
        assert!(!html.contains(r#"href="/articles/"#));
    }

    #[test]
    fn test_render_builtin_article_with_none_author() {
        let engine = ThemeEngine::new(Path::new("non_existent_dir"), "default");
        let ctx = PageRenderContext {
            site_title: "Test Aegis Site".to_string(),
            site_description: "A fast secure blog".to_string(),
            template: "article.html".to_string(),
            content_type: "article".to_string(),
            title: "Test Story Without Author".to_string(),
            slug: "test-story".to_string(),
            content_html: "<p>Hello world body</p>".to_string(),
            author: None,
            published_at: None,
            updated_at: None,
            fields: HashMap::new(),
            relations: HashMap::new(),
            items: Vec::new(),
            extra_css: "".to_string(),
            extra_head: "".to_string(),
            theme_vars: HashMap::new(),
            pagination: None,
        };

        let html = engine
            .render("article.html", &ctx)
            .expect("Article render must succeed even if author and published_at are None");
        assert!(html.contains("Test Story Without Author"));
        assert!(html.contains("Hello world body"));
        assert!(html.contains("Aegis Author"));
    }

    #[test]
    fn test_dynamic_theme_switching() {
        let temp_base = std::env::temp_dir().join(format!(
            "aegis_theme_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let themes_dir = temp_base.as_path();
        let _ = std::fs::create_dir_all(themes_dir);

        // テスト用のカスタムテーマディレクトリを作成
        let custom_theme_dir = themes_dir.join("cyber-punk");
        let tpl_dir = custom_theme_dir.join("templates");
        std::fs::create_dir_all(&tpl_dir).unwrap();

        // theme.json を配置
        let manifest = r#"{
            "id": "cyber-punk",
            "name": "Cyberpunk Neon",
            "version": "1.0.0",
            "description": "Neon vibes",
            "tags": ["neon", "cyber"]
        }"#;
        std::fs::write(custom_theme_dir.join("theme.json"), manifest).unwrap();

        // カスタム index.html を配置
        let custom_index =
            r#"<!DOCTYPE html><html><body><h1>NEON: {{ site_title }}</h1></body></html>"#;
        std::fs::write(tpl_dir.join("index.html"), custom_index).unwrap();

        let engine = ThemeEngine::new(themes_dir, "default");
        assert_eq!(engine.active_theme(), "default");

        // 一覧取得
        let list = engine.list_installed_themes();
        assert!(list.iter().any(|t| t.manifest.id == "cyber-punk"));
        assert!(list.iter().any(|t| t.manifest.id == "default"));

        // 切り替え実行
        engine
            .set_active_theme("cyber-punk")
            .expect("Should switch to cyber-punk");
        assert_eq!(engine.active_theme(), "cyber-punk");

        let ctx = PageRenderContext {
            site_title: "Matrix Online".to_string(),
            site_description: "".to_string(),
            template: "index.html".to_string(),
            content_type: "index".to_string(),
            title: "".to_string(),
            slug: "".to_string(),
            content_html: "".to_string(),
            author: None,
            published_at: None,
            updated_at: None,
            fields: HashMap::new(),
            relations: HashMap::new(),
            items: vec![],
            extra_css: "".to_string(),
            extra_head: "".to_string(),
            theme_vars: HashMap::new(),
            pagination: None,
        };

        let html = engine.render("index.html", &ctx).unwrap();
        assert!(html.contains("<h1>NEON: Matrix Online</h1>"));

        // 存在しないテーマへの切り替えはエラー
        assert!(engine.set_active_theme("non-existent-theme").is_err());
    }

    #[test]
    fn test_validate_theme_package() {
        let temp_base = std::env::temp_dir().join(format!(
            "aegis_theme_pkg_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let theme_path = temp_base.join("my-cool-theme");
        std::fs::create_dir_all(&theme_path).unwrap();

        let engine = ThemeEngine::new(&temp_base, "default");

        // 1. theme.json がない場合はエラー
        assert!(engine.validate_theme_package(&theme_path).is_err());

        // 2. theme.json を作成
        std::fs::write(
            theme_path.join("theme.json"),
            r#"{"id":"my-cool-theme","name":"Cool Theme"}"#,
        )
        .unwrap();
        // templates がないのでエラー
        assert!(engine.validate_theme_package(&theme_path).is_err());

        // 3. templates/base.html を作成
        let tpl_dir = theme_path.join("templates");
        std::fs::create_dir_all(&tpl_dir).unwrap();
        std::fs::write(
            tpl_dir.join("base.html"),
            "<html><body>{% block content %}{% endblock %}</body></html>",
        )
        .unwrap();

        // 成功
        let manifest = engine.validate_theme_package(&theme_path).unwrap();
        assert_eq!(manifest.id, "my-cool-theme");
    }
}
