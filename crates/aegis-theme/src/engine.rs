use aegis_core::{AegisError, PageRenderContext, Result};
use minijinja::Environment;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use crate::builtins::*;

#[derive(Clone)]
pub struct ThemeEngine {
    active_theme: String,
    theme_templates_dir: Option<PathBuf>,
    dynamic_templates: Arc<RwLock<HashMap<String, String>>>,
    base_path: String,
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
            active_theme: active,
            theme_templates_dir,
            dynamic_templates: Arc::new(RwLock::new(HashMap::new())),
            base_path: String::new(),
        }
    }

    /// サイトを公開するパス（`[server].base_path` の正規化済みの値、例: `/cms`）。
    /// テンプレートからはグローバル変数 `base_path` として `{{ base_path }}/articles/...` のように使う
    pub fn with_base_path(mut self, base_path: &str) -> Self {
        self.base_path = base_path.to_string();
        self
    }

    pub fn active_theme(&self) -> &str {
        &self.active_theme
    }

    /// プラグインや実行時設定から動的テンプレートを登録・上書きする
    pub fn register_template(&self, name: &str, content: &str) {
        let mut map = self.dynamic_templates.write().unwrap();
        map.insert(name.to_string(), content.to_string());
    }

    /// 登録済みの動的テンプレートを削除する
    pub fn unregister_template(&self, name: &str) {
        let mut map = self.dynamic_templates.write().unwrap();
        map.remove(name);
    }

    /// テンプレートと PageRenderContext から HTML を描画する
    pub fn render(&self, template_name: &str, ctx: &PageRenderContext) -> Result<String> {
        let mut env = Environment::new();
        // `[server].base_path` の正規化済みの値（URL で安全な文字のみ）なので自動エスケープ（`/` → `&#x2f;`）しない
        env.add_global(
            "base_path",
            minijinja::Value::from_safe_string(self.base_path.clone()),
        );

        env.add_filter("date_short", |val: &str| -> String {
            if val.len() >= 10 {
                val[..10].to_string()
            } else {
                val.to_string()
            }
        });

        env.add_filter(
            "truncate",
            |val: &str,
             limit: Option<usize>,
             _killwords: Option<bool>,
             end: Option<&str>|
             -> String {
                let lim = limit.unwrap_or(255);
                let suffix = end.unwrap_or("...");
                if val.chars().count() > lim {
                    let mut s: String = val.chars().take(lim).collect();
                    s.push_str(suffix);
                    s
                } else {
                    val.to_string()
                }
            },
        );

        let dynamic_clone = self.dynamic_templates.clone();

        let fs_dir = self.theme_templates_dir.clone();

        env.set_loader(
            move |name: &str| -> std::result::Result<Option<String>, minijinja::Error> {
                // 1. 動的オーバーライド (プラグイン・インメモリ登録)
                {
                    let map = dynamic_clone.read().unwrap();
                    if let Some(content) = map.get(name) {
                        return Ok(Some(content.clone()));
                    }
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_core::ContentSummary;

    #[test]
    fn test_render_builtin_base_and_index() {
        let engine = ThemeEngine::new(Path::new("non_existent_dir"), "default");
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
    fn test_render_article_template() {
        let engine = ThemeEngine::new(Path::new("non_existent_dir"), "default");
        let ctx = PageRenderContext {
            site_title: "My Tech Site".to_string(),
            site_description: "".to_string(),
            template: "article.html".to_string(),
            content_type: "article".to_string(),
            title: "Zero-Trust WASM Plugins".to_string(),
            slug: "zero-trust-wasm-plugins".to_string(),
            content_html: "<p>WASM sandbox is fast and secure.</p>".to_string(),
            author: Some("Takumi".to_string()),
            published_at: Some("2026-10-22".to_string()),
            updated_at: None,
            fields: HashMap::new(),
            relations: HashMap::new(),
            items: vec![],
            extra_css: "".to_string(),
            extra_head: "".to_string(),
            theme_vars: HashMap::new(),
        };

        let html = engine.render("article.html", &ctx).expect("Render failed");
        assert!(html.contains("Zero-Trust WASM Plugins"));
        assert!(html.contains("<p>WASM sandbox is fast and secure.</p>"));
        assert!(html.contains("Takumi"));
    }

    #[test]
    fn test_dynamic_template_override() {
        let engine = ThemeEngine::new(Path::new("non_existent_dir"), "default");

        // プラグインによるテンプレートの動的オーバーライド
        engine.register_template("article.html", r#"<div class="custom-theme"><h1>{{ title }}</h1><div class="body">{{ content_html | safe }}</div></div>"#);

        let ctx = PageRenderContext {
            site_title: "Theme Override Site".to_string(),
            site_description: "".to_string(),
            template: "article.html".to_string(),
            content_type: "article".to_string(),
            title: "Overridden Title".to_string(),
            slug: "overridden-title".to_string(),
            content_html: "<p>Custom content</p>".to_string(),
            author: None,
            published_at: None,
            updated_at: None,
            fields: HashMap::new(),
            relations: HashMap::new(),
            items: vec![],
            extra_css: "".to_string(),
            extra_head: "".to_string(),
            theme_vars: HashMap::new(),
        };

        let html = engine.render("article.html", &ctx).expect("Render failed");
        assert!(html.contains(r#"<div class="custom-theme">"#));
        assert!(html.contains("Overridden Title"));
        assert!(!html.contains("<!DOCTYPE html>")); // オーバーライドされたカスタムテンプレートにはbase.htmlがないため
    }
}
