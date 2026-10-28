use crate::config::AegisConfig;
use crate::headers::HeaderOverrides;
use crate::plugin_registry::PluginRegistry;
use crate::seo::SeoOverrides;
use crate::site::{SiteInfo, SiteOverrides};
use aegis_audit::AuditService;
use aegis_auth::invitation::InvitationService;
use aegis_auth::password_reset::PasswordResetService;
use aegis_auth::registration::RegistrationService;
use aegis_auth::{LoginRateLimiter, SessionService};
use aegis_cache::{create_cdn_purger, CdnPurger, EventBus, MemoryEventBus, NoopCdnPurger};
use aegis_core::{AuthProvider, CacheBackend, Notifier, StorageBackend};
use aegis_db::DatabaseAdapter;
use aegis_runtime::PluginSupervisor;
use aegis_schema::SchemaDefinition;
use aegis_theme::ThemeEngine;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    pub config: AegisConfig,
    pub db: Arc<dyn DatabaseAdapter>,
    pub storage: Arc<dyn StorageBackend>,
    #[allow(dead_code)]
    pub cache: Arc<dyn CacheBackend>,
    /// CDN の Cache-Tag パージャー（`[cdn]`。無効時は `NoopCdnPurger`）
    pub cdn: Arc<dyn CdnPurger>,
    /// サーバー間の変更の知らせ（`[cache]` と同じバックエンド。メモリならこのプロセスの中だけ）
    pub bus: Arc<dyn EventBus>,
    pub sessions: Arc<SessionService>,
    /// ログイン（`POST /api/v1/auth/session`）で資格情報を検証するプロバイダ
    pub auth_provider: Arc<dyn AuthProvider>,
    /// ログイン失敗回数の制限（`[auth.login_rate_limit]`）
    pub login_limiter: Arc<LoginRateLimiter>,
    pub schemas: Arc<RwLock<HashMap<String, SchemaDefinition>>>,
    pub supervisor: Arc<RwLock<PluginSupervisor>>,
    /// 有効なプラグインの一覧と、Admin Studio からの切り替え（`supervisor` を差し替える）
    pub plugin_registry: Arc<PluginRegistry>,
    pub audit: Arc<AuditService>,
    pub theme: Arc<ThemeEngine>,
    /// 動的プラグイン設定（管理画面から変更された設定値のキャッシュ）
    pub plugin_settings: Arc<std::sync::RwLock<HashMap<String, HashMap<String, String>>>>,
    /// Admin Studio から変更したサイト名・favicon（`[theme]` より優先。DB の `_aegis_settings`）
    pub site: Arc<std::sync::RwLock<SiteOverrides>>,
    /// Admin Studio から変更した SEO & Bot 設定（DB の `_aegis_settings`）
    pub seo: Arc<std::sync::RwLock<SeoOverrides>>,
    /// Admin Studio から変更した共通ヘッダーおよび防御ヘッダー設定（DB の `_aegis_settings`）
    pub headers: Arc<std::sync::RwLock<HeaderOverrides>>,
    /// 外部 IdP ログイン（`[auth.oidc]`）。無効なら `None`
    pub oidc: Option<SsoLogin>,
    /// 外部 IdP ログイン（`[auth.saml]`）。無効なら `None`
    pub saml: Option<SamlLogin>,
    /// EC（`[commerce]`）。無効なら `None`
    pub commerce: Option<Arc<aegis_commerce::CommerceService>>,
    /// 顧客アカウント（EC 顧客・`_aegis_users` とは別管理）。無効なら `None`
    pub customers: Option<Arc<aegis_commerce::CustomerService>>,
    /// 商品レビュー。無効なら `None`
    pub reviews: Option<Arc<aegis_commerce::ReviewService>>,
    /// 読者のコメント（`[comments]`）。無効なら `None`
    pub comments: Option<Arc<aegis_comments::CommentService>>,
    /// 掲示板（`[forum]`）。無効なら `None`
    pub forum: Option<Arc<aegis_forum::ForumService>>,
    /// 通知（`[notifications]`）。無効なら `None`
    pub notifier: Option<Arc<dyn Notifier>>,
    /// 通知の管理（Admin Studio のテンプレート編集・テスト送信・送信待ち）。`notifier` と同じもの
    pub notifications: Option<aegis_notify::NotificationService>,
    /// Admin Studio からのアカウント登録（`[auth.registration]`）。無効なら `None`
    pub registration: Option<Arc<Registration>>,
    /// パスワード再設定（`[auth.password_reset]`）。無効なら `None`
    pub password_reset: Option<Arc<PasswordReset>>,
    /// 招待リンク（`[auth.invitation]`）。無効なら `None`
    pub invitation: Option<Arc<Invitation>>,
    /// Admin Studio の URL
    pub admin_url: Option<String>,
}

/// シングルサインオン（OpenID Connect / SAML）の設定済みプロバイダと、ログイン画面・リダイレクトに使う値
#[derive(Clone)]
pub struct SsoLogin {
    /// `initiate_login` で IdP への URL を作り、`authenticate` で IdP からの戻りを検証する
    pub provider: Arc<dyn AuthProvider>,
    /// ログイン画面のボタンに表示する名前
    pub display_name: String,
    /// ログイン後（成功・失敗とも）に戻る Admin Studio の URL
    pub post_login_redirect: String,
}

/// SAML ログイン
#[derive(Clone)]
pub struct SamlLogin {
    pub sso: SsoLogin,
    /// IdP に登録する SP メタデータ（`GET /api/v1/auth/saml/metadata`）
    pub metadata: String,
}

/// アカウント登録に使う部品
pub struct Registration {
    pub service: RegistrationService,
    /// 確認メールを送る通知
    pub notifier: Arc<dyn Notifier>,
    /// 確認リンクの開き先の Admin Studio の URL（`#verify-email=<トークン>` を付ける）
    pub admin_url: String,
}

/// パスワード再設定に使う部品
pub struct PasswordReset {
    pub service: PasswordResetService,
    pub notifier: Arc<dyn Notifier>,
    pub admin_url: String,
}

/// 招待リンクに使う部品
pub struct Invitation {
    pub service: InvitationService,
    pub notifier: Arc<dyn Notifier>,
    pub admin_url: String,
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: AegisConfig,
        db: Arc<dyn DatabaseAdapter>,
        storage: Arc<dyn StorageBackend>,
        cache: Arc<dyn CacheBackend>,
        sessions: Arc<SessionService>,
        auth_provider: Arc<dyn AuthProvider>,
        login_limiter: Arc<LoginRateLimiter>,
        schemas: HashMap<String, SchemaDefinition>,
        supervisor: PluginSupervisor,
        audit: Arc<AuditService>,
        theme: Arc<ThemeEngine>,
    ) -> Self {
        let plugin_registry = Arc::new(PluginRegistry::new(None, config.plugins.enabled.clone()));
        let cdn = create_cdn_purger(&config.cdn).unwrap_or_else(|_| Arc::new(NoopCdnPurger));
        Self {
            config,
            db,
            storage,
            cache,
            cdn,
            bus: Arc::new(MemoryEventBus::new()),
            sessions,
            auth_provider,
            login_limiter,
            schemas: Arc::new(RwLock::new(schemas)),
            supervisor: Arc::new(RwLock::new(supervisor)),
            plugin_registry,
            audit,
            theme,
            plugin_settings: Arc::new(std::sync::RwLock::new(HashMap::new())),
            site: Arc::new(std::sync::RwLock::new(SiteOverrides::default())),
            seo: Arc::new(std::sync::RwLock::new(SeoOverrides::default())),
            headers: Arc::new(std::sync::RwLock::new(HeaderOverrides::default())),
            oidc: None,
            saml: None,
            commerce: None,
            customers: None,
            reviews: None,
            comments: None,
            forum: None,
            notifier: None,
            notifications: None,
            registration: None,
            password_reset: None,
            invitation: None,
            admin_url: None,
        }
    }

    /// CDN パージャーを差し替える（起動時の初期化やテスト用）
    #[allow(dead_code)]
    pub fn with_cdn(mut self, cdn: Arc<dyn CdnPurger>) -> Self {
        self.cdn = cdn;
        self
    }

    /// 掲示板（`[forum]`）を有効にする
    pub fn with_forum(mut self, forum: aegis_forum::ForumService) -> Self {
        self.forum = Some(Arc::new(forum));
        self
    }

    /// 掲示板が使うサーバーの部品（`ForumService::new` に渡す）
    pub fn forum_deps(&self, db: Arc<dyn DatabaseAdapter>) -> aegis_forum::ForumDeps {
        aegis_forum::ForumDeps {
            db,
            cache: self.cache.clone(),
            audit: self.audit.clone(),
            notifier: self.notifier.clone(),
            site_url: self
                .config
                .commerce
                .public_url
                .trim()
                .trim_end_matches('/')
                .to_string(),
            admin_url: self.admin_url.clone().unwrap_or_default(),
        }
    }

    /// 公開サイトの名前と favicon（Admin Studio で変更した値 → `[theme]` の順）
    pub fn site(&self) -> SiteInfo {
        let overrides = self
            .site
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        SiteInfo::resolve(&self.config.theme, &overrides)
    }

    /// 公開サイトの SEO & Bot 設定
    pub fn seo(&self) -> SeoOverrides {
        self.seo
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// 共通ページヘッダーおよび防御ヘッダー設定
    pub fn headers(&self) -> HeaderOverrides {
        self.headers
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// EC（`[commerce]`）を有効にする
    pub fn with_commerce(mut self, commerce: aegis_commerce::CommerceService) -> Self {
        self.commerce = Some(Arc::new(commerce));
        self
    }

    /// 顧客アカウントサービスを有効にする
    pub fn with_customers(mut self, customers: aegis_commerce::CustomerService) -> Self {
        self.customers = Some(Arc::new(customers));
        self
    }

    /// レビューサービスを有効にする
    pub fn with_reviews(mut self, reviews: aegis_commerce::ReviewService) -> Self {
        self.reviews = Some(Arc::new(reviews));
        self
    }

    /// EC が使うサーバーの部品（`CommerceService::new` に渡す）
    pub fn commerce_deps(&self) -> aegis_commerce::CommerceDeps {
        aegis_commerce::CommerceDeps {
            db: self.db.clone(),
            cache: self.cache.clone(),
            schemas: self.schemas.clone(),
            supervisor: self.supervisor.clone(),
            audit: self.audit.clone(),
            notifier: self.notifier.clone(),
        }
    }

    /// コメント（`[comments]`）を有効にする
    pub fn with_comments(mut self, comments: aegis_comments::CommentService) -> Self {
        self.comments = Some(Arc::new(comments));
        self
    }

    /// コメントが使うサーバーの部品（`CommentService::new` に渡す）。通知と Admin Studio の URL を
    /// 設定した後に呼ぶ
    pub fn comment_deps(&self) -> aegis_comments::CommentDeps {
        let config = self.config.clone();
        aegis_comments::CommentDeps {
            db: self.db.clone(),
            cache: self.cache.clone(),
            schemas: self.schemas.clone(),
            supervisor: self.supervisor.clone(),
            audit: self.audit.clone(),
            notifier: self.notifier.clone(),
            links: aegis_comments::Links {
                content_url: Arc::new(move |content_type, slug| {
                    crate::site::content_url(&config, content_type, slug)
                }),
                site_url: self
                    .config
                    .commerce
                    .public_url
                    .trim()
                    .trim_end_matches('/')
                    .to_string(),
                admin_url: self.admin_url.clone().unwrap_or_default(),
            },
        }
    }

    /// 通知（`[notifications]`）を有効にする。EC より先に設定する（`commerce_deps` が使う）
    pub fn with_notifier(mut self, notifier: Arc<dyn Notifier>) -> Self {
        self.notifier = Some(notifier);
        self
    }

    /// 通知を有効にし、Admin Studio から管理できるようにする（`with_notifier` を兼ねる）
    pub fn with_notifications(self, service: aegis_notify::NotificationService) -> Self {
        let mut state = self.with_notifier(Arc::new(service.clone()));
        state.notifications = Some(service);
        state
    }

    /// アカウント登録（`[auth.registration]`）を有効にする
    pub fn with_registration(mut self, registration: Registration) -> Self {
        self.registration = Some(Arc::new(registration));
        self
    }

    /// パスワード再設定（`[auth.password_reset]`）を有効にする
    pub fn with_password_reset(mut self, reset: PasswordReset) -> Self {
        self.password_reset = Some(Arc::new(reset));
        self
    }

    /// 招待リンク（`[auth.invitation]`）を有効にする
    pub fn with_invitation(mut self, invitation: Invitation) -> Self {
        self.invitation = Some(Arc::new(invitation));
        self
    }

    /// Admin Studio の URL を設定する
    pub fn with_admin_url(mut self, url: impl Into<String>) -> Self {
        self.admin_url = Some(url.into());
        self
    }

    /// OpenID Connect ログインを有効にする
    pub fn with_oidc(mut self, oidc: SsoLogin) -> Self {
        self.oidc = Some(oidc);
        self
    }

    /// SAML ログインを有効にする
    pub fn with_saml(mut self, saml: SamlLogin) -> Self {
        self.saml = Some(saml);
        self
    }
}
