use aegis_core::{AegisError, PageRenderContext, Result};
use minijinja::Environment;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use crate::builtins::*;
use crate::manifest::{ThemeInfo, ThemeManifest};

/// 描画の失敗の詳細（テーマ開発用）。どのテンプレートの何行目かを含む
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RenderDiagnostic {
    /// 原因（minijinja のエラーの種類・詳細と、その元のエラー）
    pub message: String,
    /// エラーが起きたテンプレート（`extends`・`include` の先のこともある）
    pub template: Option<String>,
    /// 1 始まりの行番号
    pub line: Option<usize>,
}

impl RenderDiagnostic {
    fn from_error(err: &minijinja::Error) -> Self {
        let mut message = err.to_string();
        // `include`・`extends` 先のエラーは外側のエラーに包まれるので、一番内側の場所を採る
        let (mut template, mut line) = (err.name().map(str::to_string), err.line());
        let mut source = std::error::Error::source(err);
        while let Some(cause) = source {
            message.push_str(&format!(
                "
caused by: {cause}"
            ));
            if let Some(inner) = cause.downcast_ref::<minijinja::Error>() {
                if inner.line().is_some() {
                    template = inner.name().map(str::to_string);
                    line = inner.line();
                }
            }
            source = cause.source();
        }
        Self {
            message,
            template,
            line,
        }
    }
}

/// テンプレートのグローバル変数の元になる設定
struct Globals {
    base_path: String,
    admin_url: Option<String>,
    show_powered_by: bool,
    locale: String,
}

struct ThemeEngineInner {
    active_theme: String,
    theme_templates_dir: Option<PathBuf>,
    dynamic_templates: HashMap<String, String>,
    base_path: String,
    /// 公開ページから Admin Studio へのリンク（グローバル変数 `admin_url`。空ならリンクを出さない）。
    /// `None` なら `{base_path}/admin/`
    admin_url: Option<String>,
    /// フッターに「Powered by」の表記を出すか（`[theme].show_powered_by`）
    show_powered_by: bool,
    /// 組み込みテンプレートの言語（`[theme].locale`）
    locale: String,
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
                admin_url: None,
                show_powered_by: true,
                locale: "en".to_string(),
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

    /// 公開ページから Admin Studio へのリンクの URL（`base_path` 込み）。空文字なら出さない。
    /// テンプレートからはグローバル変数 `admin_url` として `{% if admin_url %}<a href="{{ admin_url }}">...{% endif %}` のように使う。
    /// 指定しなければ `{base_path}/admin/`
    pub fn with_admin_url(self, url: &str) -> Self {
        {
            let mut inner = self.inner.write().unwrap();
            inner.admin_url = Some(url.to_string());
            inner.environment = None;
        }
        self
    }

    /// フッターに「Powered by AegisCMS」を出すか（`[theme].show_powered_by`、既定は `true`）。
    /// テンプレートからはグローバル変数 `show_powered_by` として `{% if show_powered_by %}` のように使う
    pub fn with_powered_by(self, show: bool) -> Self {
        {
            let mut inner = self.inner.write().unwrap();
            inner.show_powered_by = show;
            inner.environment = None;
        }
        self
    }

    /// 組み込みテンプレートの言語（`[theme].locale`、`en` / `ja`。知らない値は `en`）。
    /// テンプレートからはグローバル変数 `locale`（`<html lang="{{ locale }}">`）と、組み込みの文言を返す
    /// 関数 `t("キー", n=件数, 名前=値)` として使う
    pub fn with_locale(self, locale: &str) -> Self {
        {
            let mut inner = self.inner.write().unwrap();
            inner.locale = if crate::messages::is_supported(locale) {
                locale.to_string()
            } else {
                "en".to_string()
            };
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

    /// テーマディレクトリのパスを取得
    pub fn themes_dir(&self) -> &Path {
        &self.themes_dir
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
        self.validate_theme_package_with_policy(theme_dir, None, false)
    }

    /// 署名検証およびポリシーを指定したテーマパッケージの検証
    pub fn validate_theme_package_with_policy(
        &self,
        theme_dir: &Path,
        trusted_keys: Option<&[String]>,
        require_signature: bool,
    ) -> Result<ThemeManifest> {
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

        if manifest.id.trim().is_empty() {
            return Err(AegisError::Theme(
                "Theme manifest id cannot be empty".to_string(),
            ));
        }

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

        // min_aegis_version の検証
        if let Some(ref min_ver) = manifest.min_aegis_version {
            aegis_core::check_min_aegis_version(min_ver)?;
        }

        // 署名検証
        aegis_core::verify_package_signature(theme_dir, trusted_keys, require_signature)?;

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

        let value = minijinja::Value::from(minijinja::value::Serde(ctx));
        let rendered = template.render(value).map_err(|e| {
            AegisError::Theme(format!(
                "Failed to render template '{}': {}",
                template_name, e
            ))
        })?;

        Ok(rendered)
    }

    /// `render` と同じ描画で、失敗したときにテンプレート名と行番号を返す（テーマ開発用）
    pub fn render_diagnosed(
        &self,
        template_name: &str,
        ctx: &PageRenderContext,
    ) -> std::result::Result<String, RenderDiagnostic> {
        let env = self.environment();
        let template = env
            .get_template(template_name)
            .map_err(|e| RenderDiagnostic::from_error(&e))?;
        let value = minijinja::Value::from(minijinja::value::Serde(ctx));
        template
            .render(value)
            .map_err(|e| RenderDiagnostic::from_error(&e))
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
            Globals {
                base_path: inner.base_path.clone(),
                admin_url: inner.admin_url.clone(),
                show_powered_by: inner.show_powered_by,
                locale: inner.locale.clone(),
            },
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
        globals: Globals,
    ) -> Environment<'static> {
        let Globals {
            base_path,
            admin_url,
            show_powered_by,
            locale,
        } = globals;
        let mut env = Environment::new();

        env.add_filter("date_short", |val: minijinja::Value| -> String {
            match val.as_str() {
                Some(s) => s.get(..10).unwrap_or(s).to_string(),
                None => String::new(),
            }
        });

        // RFC 3339 → `2026-10-25 23:41`（UTC のまま、分まで）
        env.add_filter("datetime_short", |val: minijinja::Value| -> String {
            match val.as_str() {
                Some(s) => s.get(..16).unwrap_or(s).replacen('T', " ", 1),
                None => String::new(),
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

        // 最小単位の整数の金額を通貨の表記にする（`{{ fields.price | money("jpy") }}` → `¥1,200`）
        env.add_filter(
            "money",
            |val: minijinja::Value, currency: Option<&str>| -> String {
                let currency = currency.unwrap_or("jpy");
                match i64::try_from(val.clone()) {
                    Ok(amount) => aegis_core::format_money(amount, currency),
                    // カスタムフィールドは文字列で返ることもある
                    Err(_) => match val.as_str().and_then(|s| s.trim().parse::<i64>().ok()) {
                        Some(amount) => aegis_core::format_money(amount, currency),
                        None => String::new(),
                    },
                }
            },
        );

        // `[server].base_path` の正規化済みの値（URL で安全な文字のみ）なので自動エスケープ（`/` → `&#x2f;`）しない
        // `base_path` と `[server].admin_path` の検証済みの値から作る URL（URL で安全な文字のみ）
        let admin_url = admin_url.unwrap_or_else(|| format!("{}/admin/", base_path));
        env.add_global("base_path", minijinja::Value::from_safe_string(base_path));
        env.add_global("admin_url", minijinja::Value::from_safe_string(admin_url));
        env.add_global("show_powered_by", show_powered_by);
        // 組み込みの文言。値は HTML エスケープして差し込み、結果は安全な文字列として返す
        // （文言は信頼できる静的な文字列で、`<strong>` のようなマークアップを含むものがある）
        env.add_global("locale", locale.clone());
        env.add_function(
            "t",
            move |key: &str, kwargs: minijinja::value::Kwargs| -> minijinja::Value {
                let mut params = Vec::new();
                for name in kwargs.args() {
                    if let Ok(value) = kwargs.get::<minijinja::Value>(name) {
                        params.push((name.to_string(), value.to_string()));
                    }
                }
                minijinja::Value::from_safe_string(crate::messages::translate(
                    &locale, key, &params,
                ))
            },
        );

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
                    "front.html" => Ok(Some(FRONT_TEMPLATE.to_string())),
                    "category.html" => Ok(Some(CATEGORY_TEMPLATE.to_string())),
                    "tag.html" => Ok(Some(TAG_TEMPLATE.to_string())),
                    "tags.html" => Ok(Some(TAGS_TEMPLATE.to_string())),
                    "search.html" => Ok(Some(SEARCH_TEMPLATE.to_string())),
                    "contact.html" => Ok(Some(CONTACT_TEMPLATE.to_string())),
                    "article.html" => Ok(Some(ARTICLE_TEMPLATE.to_string())),
                    "page.html" => Ok(Some(PAGE_TEMPLATE.to_string())),
                    "blank.html" => Ok(Some(BLANK_TEMPLATE.to_string())),
                    "404.html" => Ok(Some(NOT_FOUND_TEMPLATE.to_string())),
                    "shop_common.html" => Ok(Some(SHOP_COMMON_TEMPLATE.to_string())),
                    "products.html" => Ok(Some(PRODUCTS_TEMPLATE.to_string())),
                    "product.html" => Ok(Some(PRODUCT_TEMPLATE.to_string())),
                    "cart.html" => Ok(Some(CART_TEMPLATE.to_string())),
                    "order.html" => Ok(Some(ORDER_TEMPLATE.to_string())),
                    "comments.html" => Ok(Some(COMMENTS_TEMPLATE.to_string())),
                    "forum_common.html" => Ok(Some(FORUM_COMMON_TEMPLATE.to_string())),
                    "forum_index.html" => Ok(Some(FORUM_INDEX_TEMPLATE.to_string())),
                    "forum_category.html" => Ok(Some(FORUM_CATEGORY_TEMPLATE.to_string())),
                    "forum_thread.html" => Ok(Some(FORUM_THREAD_TEMPLATE.to_string())),
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
            comments: None,
        };

        let html = engine.render("index.html", &ctx).expect("Render failed");
        assert!(html.contains("Test Aegis Site"));
        assert!(html.contains("First Post"));
        assert!(html.contains("/articles/first-post"));
        assert!(html.contains("Powered by <strong>AegisCMS</strong>"));
    }

    #[test]
    fn test_powered_by_can_be_hidden_in_every_bundled_theme() {
        let ctx = PageRenderContext {
            site_title: "Quiet Site".to_string(),
            template: "index.html".to_string(),
            ..Default::default()
        };
        let themes_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../themes");
        let mut themes: Vec<String> = std::fs::read_dir(&themes_dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        // 組み込みのテンプレート（テーマのディレクトリが無い場合）
        themes.push("non-existent-theme".to_string());
        for theme in themes {
            let shown = ThemeEngine::new(&themes_dir, &theme)
                .render("index.html", &ctx)
                .unwrap();
            let hidden = ThemeEngine::new(&themes_dir, &theme)
                .with_powered_by(false)
                .render("index.html", &ctx)
                .unwrap();
            assert!(hidden.contains("Quiet Site"), "{}", theme);
            for mark in ["AegisCMS", "AEGIS-WASM"] {
                assert!(!hidden.contains(mark), "{}: {}", theme, mark);
            }
            assert_ne!(shown, hidden, "{} has no powered-by mark to hide", theme);
        }
    }

    #[test]
    fn test_admin_link_follows_admin_url_in_every_bundled_theme() {
        let ctx = PageRenderContext {
            site_title: "Quiet Site".to_string(),
            template: "index.html".to_string(),
            ..Default::default()
        };
        let themes_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../themes");
        let mut themes: Vec<String> = std::fs::read_dir(&themes_dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        themes.push("non-existent-theme".to_string());
        for theme in themes {
            let render = |engine: ThemeEngine| engine.render("index.html", &ctx).unwrap();
            // 既定は {base_path}/admin/
            let default = render(ThemeEngine::new(&themes_dir, &theme));
            assert!(default.contains(r#"href="/admin/""#), "{}", theme);
            // パスを変えたら、そのパスへ
            let custom = render(ThemeEngine::new(&themes_dir, &theme).with_admin_url("/cms/back/"));
            assert!(custom.contains(r#"href="/cms/back/""#), "{}", theme);
            assert!(!custom.contains(r#"href="/admin/""#), "{}", theme);
            // 空ならリンクごと出さない（パスも漏れない）
            let hidden = render(ThemeEngine::new(&themes_dir, &theme).with_admin_url(""));
            assert!(!hidden.contains("/admin"), "{}", theme);
            assert!(!hidden.contains("Admin Studio"), "{}", theme);
        }
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
            comments: None,
        };

        let html = engine
            .render("article.html", &ctx)
            .expect("Article render must succeed even if author and published_at are None");
        assert!(html.contains("Test Story Without Author"));
        assert!(html.contains("Hello world body"));
        assert!(html.contains("Aegis Author"));
    }

    #[test]
    fn test_render_builtin_blank_template() {
        let engine = ThemeEngine::new(Path::new("non_existent_dir"), "default");
        let ctx = PageRenderContext {
            site_title: "Landing Site".to_string(),
            site_description: "LP Description".to_string(),
            template: "blank.html".to_string(),
            content_type: "page".to_string(),
            title: "Special Campaign".to_string(),
            slug: "campaign".to_string(),
            content_html: "<section class=\"full-hero\"><h1>Hero Content</h1></section>"
                .to_string(),
            author: None,
            published_at: None,
            updated_at: None,
            fields: HashMap::new(),
            relations: HashMap::new(),
            items: Vec::new(),
            extra_css: ".full-hero { width: 100%; }".to_string(),
            extra_head: "<meta name=\"custom\" content=\"val\">".to_string(),
            theme_vars: HashMap::new(),
            pagination: None,
            comments: None,
        };

        let html = engine
            .render("blank.html", &ctx)
            .expect("Blank template render failed");
        assert!(html.contains("<title>Special Campaign | Landing Site</title>"));
        assert!(html.contains("<meta name=\"custom\" content=\"val\">"));
        assert!(html.contains(".full-hero { width: 100%; }"));
        assert!(html.contains("<section class=\"full-hero\"><h1>Hero Content</h1></section>"));
        // base.html 由来のヘッダー・フッター（例: "Admin Studio", "Powered by"）が含まれないことを検証
        assert!(!html.contains("site-header"));
        assert!(!html.contains("site-footer"));
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
            comments: None,
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

        // 4. min_aegis_version の検証
        std::fs::write(
            theme_path.join("theme.json"),
            r#"{"id":"my-cool-theme","name":"Cool Theme","min_aegis_version":"99.0.0"}"#,
        )
        .unwrap();
        let err = engine.validate_theme_package(&theme_path);
        assert!(err.is_err());
        assert!(err
            .unwrap_err()
            .to_string()
            .contains("Package requires Aegis version >="));

        // 5. 署名必須ポリシーの検証（trusted_keys 未設定、および signature.json 不在）
        std::fs::write(
            theme_path.join("theme.json"),
            r#"{"id":"my-cool-theme","name":"Cool Theme"}"#,
        )
        .unwrap();
        let err = engine.validate_theme_package_with_policy(&theme_path, None, true);
        assert!(err.is_err());
        assert!(err
            .unwrap_err()
            .to_string()
            .contains("no trusted public keys are configured"));

        let trusted =
            vec!["0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string()];
        let err = engine.validate_theme_package_with_policy(&theme_path, Some(&trusted), true);
        assert!(err.is_err());
        assert!(err
            .unwrap_err()
            .to_string()
            .contains("Package signature required by policy, but 'signature.json' was not found"));

        let _ = std::fs::remove_dir_all(&temp_base);
    }

    #[test]
    fn test_forum_thread_template_shows_short_dates_and_reply_anchor() {
        let engine = ThemeEngine::new(Path::new("non_existent_dir"), "default");
        let mut fields = HashMap::new();
        fields.insert(
            "category".to_string(),
            serde_json::json!({ "slug": "general", "name": "General" }),
        );
        fields.insert(
            "thread".to_string(),
            serde_json::json!({
                "id": "t1", "title": "Hello", "author_name": "Ann", "post_count": 2,
                "is_pinned": false, "is_locked": false,
                "created_at": "2026-10-25T09:05:07.123456Z",
            }),
        );
        fields.insert(
            "posts".to_string(),
            serde_json::json!([
                {
                    "id": "p1", "post_number": 1, "author_name": "Ann", "author_initial": "A",
                    "is_author_staff": false, "body_html": "<p>Hi</p>",
                    "created_at": "2026-10-25T09:05:07.123456Z", "reply_to_number": null,
                },
                {
                    "id": "p2", "post_number": 2, "author_name": "Bob", "author_initial": "B",
                    "is_author_staff": false, "body_html": "<p>Re</p>",
                    "created_at": "2026-10-26T10:30:00.000000Z", "reply_to_number": 1,
                },
            ]),
        );
        let ctx = PageRenderContext {
            site_title: "Forum".to_string(),
            site_description: String::new(),
            template: "forum_thread.html".to_string(),
            content_type: "forum".to_string(),
            title: "Hello".to_string(),
            slug: String::new(),
            content_html: String::new(),
            author: None,
            published_at: None,
            updated_at: None,
            fields,
            relations: HashMap::new(),
            items: Vec::new(),
            extra_css: String::new(),
            extra_head: String::new(),
            theme_vars: HashMap::new(),
            pagination: None,
            comments: None,
        };
        let html = engine.render("forum_thread.html", &ctx).unwrap();
        assert!(html.contains(">2026-10-25 09:05</time>"), "{html}");
        assert!(html.contains(">2026-10-26 10:30</time>"), "{html}");
        assert!(html.contains(">2026-10-25</time>"), "{html}");
        assert!(!html.contains(">2026-10-25T09:05:07"), "{html}");
        // only the reply shows `>>1`, linking to the post it answers
        assert_eq!(html.matches("&gt;&gt;1</a>").count(), 1, "{html}");
        assert!(html.contains("href=\"#post-1\">&gt;&gt;1</a>"), "{html}");
        // link buttons keep their own text colour
        assert!(html.contains(".aegis-forum a:not(.aegis-forum-btn) { color: inherit;"));
        assert!(!html.contains(".aegis-forum a { color: inherit"));
    }

    #[test]
    fn test_date_filters_are_char_boundary_safe() {
        let env = ThemeEngine::build_environment(
            HashMap::new(),
            None,
            Globals {
                base_path: String::new(),
                admin_url: None,
                show_powered_by: true,
                locale: "en".to_string(),
            },
        );
        let out = env
            .render_str(
                "{{ a | date_short }}|{{ a | datetime_short }}|{{ b | date_short }}|{{ b | datetime_short }}|{{ missing | date_short }}|{{ missing | datetime_short }}",
                minijinja::context! { a => "2026-10-25T09:05:07Z", b => "あいうえおかきくけこ" },
            )
            .unwrap();
        assert_eq!(
            out,
            "2026-10-25|2026-10-25 09:05|あいうえおかきくけこ|あいうえおかきくけこ||"
        );
    }

    #[test]
    fn test_render_diagnosed_reports_the_template_and_line() {
        let engine = ThemeEngine::new(Path::new("non_existent_dir"), "default");
        let ctx = PageRenderContext::default();

        engine.register_template(
            "syntax.html",
            "<p>ok</p>
<p>{% if %}</p>
",
        );
        let err = engine.render_diagnosed("syntax.html", &ctx).unwrap_err();
        assert_eq!(err.template.as_deref(), Some("syntax.html"));
        assert_eq!(err.line, Some(2));

        // 描画時のエラーは、`include` の先のテンプレートと行を指す
        engine.register_template(
            "part.html",
            "a
b
{{ 1 / 0 }}
",
        );
        engine.register_template("page.html", "{% include 'part.html' %}");
        let err = engine.render_diagnosed("page.html", &ctx).unwrap_err();
        assert_eq!(err.template.as_deref(), Some("part.html"));
        assert_eq!(err.line, Some(3));

        let err = engine.render_diagnosed("missing.html", &ctx).unwrap_err();
        assert!(err.message.contains("missing.html"), "{}", err.message);

        engine.register_template("ok.html", "fine");
        assert_eq!(engine.render_diagnosed("ok.html", &ctx).unwrap(), "fine");
    }
}
