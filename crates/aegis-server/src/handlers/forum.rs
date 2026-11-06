//! 掲示板（フォーラム）のハンドラー。
//! 公開画面（テーマのテンプレート `forum_index.html` / `forum_category.html` / `forum_thread.html`）
//! および JSON API（`/api/v1/forum/...`）を提供する。

use axum::{
    extract::{Form, Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

use super::public::{render_not_found, themed_response};
use crate::audit::ClientIp;
use crate::auth::{extract_session_id, AdminSession, EditorSession};
use crate::render;
use crate::state::AppState;
use aegis_cache::CacheTags;
use aegis_captcha::CaptchaError;
use aegis_comments::PolicyOverrides;
use aegis_core::{ModerationStatus, PageRenderContext, Pagination};
use aegis_db::{PostListQuery, ThreadListQuery};
use aegis_forum::{
    Author, CreateCategoryRequest, CreatePostRequest, CreateReportRequest, CreateThreadRequest,
    ForumError, ForumService, UpdateCategoryRequest, Viewer,
};

type ApiError = (StatusCode, Json<serde_json::Value>);

fn api_error(status: StatusCode, message: impl Into<String>) -> ApiError {
    (status, Json(json!({ "error": message.into() })))
}

fn forum_error_to_api(e: ForumError) -> ApiError {
    let status = match &e {
        ForumError::Invalid(_) => StatusCode::BAD_REQUEST,
        ForumError::NotFound(_) => StatusCode::NOT_FOUND,
        ForumError::Forbidden(_) => StatusCode::FORBIDDEN,
        ForumError::Locked => StatusCode::FORBIDDEN,
        ForumError::RateLimited => StatusCode::TOO_MANY_REQUESTS,
        ForumError::Disabled => StatusCode::SERVICE_UNAVAILABLE,
        ForumError::Internal(msg) => {
            tracing::error!("Forum internal error: {}", msg);
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error");
        }
    };
    api_error(status, e.to_string())
}

fn get_forum(state: &AppState) -> Result<Arc<ForumService>, ApiError> {
    state.forum.clone().ok_or_else(|| {
        api_error(
            StatusCode::NOT_FOUND,
            "Forum is not enabled ([forum].enabled)",
        )
    })
}

/// テンプレートの `theme_vars` に渡すフォーラムの情報
fn forum_vars(state: &AppState) -> HashMap<String, String> {
    let base_path = state
        .config
        .server
        .normalized_base_path()
        .unwrap_or_default();
    HashMap::from([
        (
            "forum_enabled".to_string(),
            state.forum.is_some().to_string(),
        ),
        ("forum_url".to_string(), format!("{}/forum", base_path)),
        (
            "forum_api".to_string(),
            format!("{}/api/v1/forum", base_path),
        ),
        // 会員でログインして投稿するためのリンク（会員アカウントを使うときだけ。ページはキャッシュされるので、
        // ログインしているかどうかには依存しない）
        (
            "forum_members".to_string(),
            state.members.is_some().to_string(),
        ),
        (
            "forum_login_url".to_string(),
            format!("{}/account/login", base_path),
        ),
        (
            "forum_members_only".to_string(),
            (state.members.is_some()
                && state.config.forum.posting == aegis_forum::ForumPosting::Members)
                .to_string(),
        ),
    ])
}

/// フォーラムのページを描画する（`on-render-page` フックを適用）
async fn render_forum_page(state: &AppState, mut ctx: PageRenderContext) -> Response {
    let site = render::site_page_context(state);
    ctx.site_title = site.site_title;
    ctx.site_description = site.site_description;
    ctx.extra_css = site.extra_css;
    if ctx.extra_head.is_empty() {
        ctx.extra_head = site.extra_head;
    }
    ctx.theme_vars.extend(forum_vars(state));
    {
        let supervisor = state.supervisor.read().await;
        if let Ok(hooked) = supervisor.apply_render_page(ctx.clone()).await {
            ctx = hooked;
        }
    }
    themed_response(
        state
            .theme
            .render(&ctx.template, &ctx)
            .map_err(|e| e.to_string()),
    )
}

/// プレーンテキスト本文を安全に HTML 化（エスケープ + 改行保持）
fn body_to_html(body: &str) -> String {
    body.split("\n\n")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| {
            let lines: Vec<String> = p.lines().map(escape_html).collect();
            format!("<p>{}</p>", lines.join("<br>"))
        })
        .collect()
}

fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// 画面に出す投稿者名。退会した会員の投稿は名前が空なので、`[theme].locale` の言語の文言にする
fn shown_name(state: &AppState, name: String) -> String {
    if name.is_empty() {
        aegis_theme::messages::translate(&state.config.theme.locale, "forum.former_user", &[])
    } else {
        name
    }
}

fn author_initial(name: &str) -> String {
    name.chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string())
}

// =========================================================================
// ビューモデル
// =========================================================================

#[derive(Debug, Default, Deserialize)]
pub struct ForumPageParams {
    pub page: Option<u64>,
    pub submitted: Option<String>,
    pub reported: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CategoryView {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub thread_count: u64,
    pub post_count: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ThreadView {
    pub id: String,
    pub category_slug: String,
    pub category_name: String,
    pub title: String,
    pub author_name: String,
    pub post_count: u64,
    pub created_at: String,
    pub last_post_at: String,
    pub is_pinned: bool,
    pub is_locked: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PostView {
    pub id: String,
    pub post_number: u64,
    pub author_name: String,
    pub author_initial: String,
    /// 投稿者（会員）のアバター画像の公開 URL。無ければ頭文字の丸を出す
    #[serde(default)]
    pub avatar_url: Option<String>,
    pub is_author_staff: bool,
    pub body_html: String,
    pub created_at: String,
    pub reply_to_number: Option<u64>,
}

// =========================================================================
// 公開 HTML 画面
// =========================================================================

/// `GET /forum` — フォーラムトップ（カテゴリ一覧 & 最近アクティブなスレッド）
pub async fn render_forum_index(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    let forum = match &state.forum {
        Some(f) => f.clone(),
        None => return render_not_found(&state, "Forum is currently disabled.").await,
    };
    if !state.system().forum_enabled {
        return render_not_found(&state, "Forum is currently disabled.").await;
    }

    // 見る人に応じたカテゴリ。会員・スタッフ限定のカテゴリが入るときは、その人専用のページになるので
    // 共有のキャッシュに載せない。ログインしていなければ、公開のカテゴリだけ（今までどおりキャッシュする）
    let viewer = resolve_viewer(&state, &headers).await;
    let categories_res = forum.list_categories(viewer).await;
    let categories = match categories_res {
        Ok(cats) => cats,
        Err(_) => return render_not_found(&state, "Unable to load categories.").await,
    };

    let mut cat_views = Vec::new();
    let mut recent_thread_views = Vec::new();

    for cat in &categories {
        let q = ThreadListQuery {
            category_id: Some(cat.id.clone()),
            status: Some("approved".to_string()),
            before_last_post_at: None,
            before_id: None,
            limit: 5,
        };
        let threads = forum.list_threads(&q).await.unwrap_or_default();
        let thread_count = threads.len() as u64;
        let post_count: u64 = threads.iter().map(|t| t.post_count).sum();

        cat_views.push(CategoryView {
            id: cat.id.clone(),
            slug: cat.slug.clone(),
            name: cat.name.clone(),
            description: cat.description.clone(),
            thread_count,
            post_count,
        });

        for t in threads {
            recent_thread_views.push(ThreadView {
                id: t.id,
                category_slug: cat.slug.clone(),
                category_name: cat.name.clone(),
                title: t.title,
                author_name: shown_name(&state, t.author_name),
                post_count: t.post_count,
                created_at: t.created_at,
                last_post_at: t.last_post_at,
                is_pinned: t.is_pinned,
                is_locked: t.is_locked,
            });
        }
    }

    recent_thread_views.sort_by(|a, b| b.last_post_at.cmp(&a.last_post_at));
    recent_thread_views.truncate(10);

    let mut ctx = PageRenderContext {
        template: "forum_index.html".to_string(),
        content_type: "forum".to_string(),
        title: "Community Forum".to_string(),
        slug: "forum".to_string(),
        content_html: String::new(),
        ..render::site_page_context(&state)
    };

    if let Ok(v) = serde_json::to_value(&cat_views) {
        ctx.fields.insert("categories".to_string(), v);
    }
    if let Ok(v) = serde_json::to_value(&recent_thread_views) {
        ctx.fields.insert("recent_threads".to_string(), v);
    }

    let mut tags = CacheTags::new();
    tags.add_url("/forum");
    tags.add_tag("site-global");
    tags.add_dependency("collection", "forum");
    let resp = render_forum_page(&state, ctx).await;
    if categories.iter().any(|c| c.visibility != "public") {
        return no_store(resp);
    }
    crate::cdn::apply_cache_tags(&state, resp, &tags)
}

/// `GET /forum/:category_slug` — カテゴリ別スレッド一覧
pub async fn render_forum_category(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(category_slug): Path<String>,
    Query(params): Query<ForumPageParams>,
) -> Response {
    let forum = match &state.forum {
        Some(f) => f.clone(),
        None => return render_not_found(&state, "Forum is currently disabled.").await,
    };
    if !state.system().forum_enabled {
        return render_not_found(&state, "Forum is currently disabled.").await;
    }

    let cat = match forum.get_category_by_slug(&category_slug).await {
        Ok(c) => c,
        Err(_) => return render_not_found(&state, "Category not found.").await,
    };
    // 会員・スタッフ限定のカテゴリは、見られる人にだけ見せる（無いものとして扱う）
    let personal = cat.visibility != "public";
    if personal {
        let viewer = resolve_viewer(&state, &headers).await;
        if forum.check_category_visible(viewer, &cat).is_err() {
            return render_not_found(&state, "Category not found.").await;
        }
    }

    let per_page = forum.config().default_page_size.clamp(1, 100);
    let page = params.page.unwrap_or(1).max(1);

    let q = ThreadListQuery {
        category_id: Some(cat.id.clone()),
        status: Some("approved".to_string()),
        before_last_post_at: None,
        before_id: None,
        limit: per_page * page,
    };
    let threads_raw = forum.list_threads(&q).await.unwrap_or_default();

    let total = threads_raw.len() as u64;
    let offset = ((page - 1) * per_page) as usize;
    let paged_threads: Vec<_> = threads_raw
        .into_iter()
        .skip(offset)
        .take(per_page as usize)
        .map(|t| ThreadView {
            id: t.id,
            category_slug: cat.slug.clone(),
            category_name: cat.name.clone(),
            title: t.title,
            author_name: shown_name(&state, t.author_name),
            post_count: t.post_count,
            created_at: t.created_at,
            last_post_at: t.last_post_at,
            is_pinned: t.is_pinned,
            is_locked: t.is_locked,
        })
        .collect();

    let total_pages = total.div_ceil(per_page).max(1);
    let base_path = state
        .config
        .server
        .normalized_base_path()
        .unwrap_or_default();

    let pagination = if total_pages > 1 {
        Some(Pagination {
            page,
            per_page,
            total_items: total,
            total_pages,
            prev_url: (page > 1)
                .then(|| format!("{}/forum/{}?page={}", base_path, cat.slug, page - 1)),
            next_url: (page < total_pages)
                .then(|| format!("{}/forum/{}?page={}", base_path, cat.slug, page + 1)),
        })
    } else {
        None
    };

    let cat_view = CategoryView {
        id: cat.id.clone(),
        slug: cat.slug.clone(),
        name: cat.name.clone(),
        description: cat.description.clone(),
        thread_count: total,
        post_count: paged_threads.iter().map(|t| t.post_count).sum(),
    };

    let mut ctx = PageRenderContext {
        template: "forum_category.html".to_string(),
        content_type: "forum_category".to_string(),
        title: cat.name.clone(),
        slug: format!("forum/{}", cat.slug),
        content_html: String::new(),
        pagination,
        ..render::site_page_context(&state)
    };

    if let Ok(v) = serde_json::to_value(&cat_view) {
        ctx.fields.insert("category".to_string(), v);
    }
    if let Ok(v) = serde_json::to_value(&paged_threads) {
        ctx.fields.insert("threads".to_string(), v);
    }
    insert_captcha_html(&state, &mut ctx);
    if params.submitted.as_deref() == Some("pending") {
        ctx.fields.insert("pending_notice".to_string(), json!(true));
    }

    let mut tags = CacheTags::new();
    tags.add_url(&format!("/forum/{}", cat.slug));
    tags.add_tag("site-global");
    tags.add_dependency("collection", "forum");
    tags.add_dependency("forum-category", &cat.slug);
    let resp = render_forum_page(&state, ctx).await;
    if personal {
        return no_store(resp);
    }
    crate::cdn::apply_cache_tags(&state, resp, &tags)
}

/// `GET /forum/:category_slug/:thread_id` — スレッド詳細 & レス一覧
pub async fn render_forum_thread(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((category_slug, thread_id)): Path<(String, String)>,
    Query(params): Query<ForumPageParams>,
) -> Response {
    let forum = match &state.forum {
        Some(f) => f.clone(),
        None => return render_not_found(&state, "Forum is currently disabled.").await,
    };
    if !state.system().forum_enabled {
        return render_not_found(&state, "Forum is currently disabled.").await;
    }

    let cat = match forum.get_category_by_slug(&category_slug).await {
        Ok(c) => c,
        Err(_) => return render_not_found(&state, "Category not found.").await,
    };
    let personal = cat.visibility != "public";
    if personal {
        let viewer = resolve_viewer(&state, &headers).await;
        if forum.check_category_visible(viewer, &cat).is_err() {
            return render_not_found(&state, "Category not found.").await;
        }
    }

    let thread = match forum.get_thread(&thread_id).await {
        Ok(t) if t.status == "approved" => t,
        _ => return render_not_found(&state, "Thread not found.").await,
    };

    if thread.category_id != cat.id {
        return render_not_found(&state, "Thread not found in this category.").await;
    }

    let per_page = forum.config().default_page_size.clamp(1, 100);
    let page = params.page.unwrap_or(1).max(1);

    let q = PostListQuery {
        thread_id: thread_id.clone(),
        after_post_number: 0,
        status: Some("approved".to_string()),
        limit: per_page * page,
    };
    let posts_raw = forum.list_posts(&q).await.unwrap_or_default();

    let total = posts_raw.len() as u64;
    let offset = ((page - 1) * per_page) as usize;
    let page_posts: Vec<_> = posts_raw
        .into_iter()
        .skip(offset)
        .take(per_page as usize)
        .collect();
    // 会員のアバターは、表示のたびに今の値を引く（画像を変えれば過去の投稿にも反映される）
    let avatars = crate::handlers::members::avatar_urls(
        &state,
        page_posts
            .iter()
            .filter(|p| p.author_kind.as_deref() == Some("member"))
            .filter_map(|p| p.author_id.clone())
            .collect(),
    )
    .await;
    let paged_posts: Vec<_> = page_posts
        .into_iter()
        .map(|p| PostView {
            avatar_url: if p.author_kind.as_deref() == Some("member") {
                p.author_id
                    .as_deref()
                    .and_then(|id| avatars.get(id).cloned())
            } else {
                None
            },
            id: p.id,
            post_number: p.post_number,
            author_name: shown_name(&state, p.author_name.clone()),
            author_initial: author_initial(&shown_name(&state, p.author_name.clone())),
            is_author_staff: p.author_kind.as_deref() == Some("staff"),
            body_html: body_to_html(&p.body),
            created_at: p.created_at,
            reply_to_number: p.reply_to_number,
        })
        .collect();

    let total_pages = total.div_ceil(per_page).max(1);
    let base_path = state
        .config
        .server
        .normalized_base_path()
        .unwrap_or_default();

    let pagination = if total_pages > 1 {
        Some(Pagination {
            page,
            per_page,
            total_items: total,
            total_pages,
            prev_url: (page > 1).then(|| {
                format!(
                    "{}/forum/{}/{}?page={}",
                    base_path,
                    cat.slug,
                    thread.id,
                    page - 1
                )
            }),
            next_url: (page < total_pages).then(|| {
                format!(
                    "{}/forum/{}/{}?page={}",
                    base_path,
                    cat.slug,
                    thread.id,
                    page + 1
                )
            }),
        })
    } else {
        None
    };

    let thread_view = ThreadView {
        id: thread.id.clone(),
        category_slug: cat.slug.clone(),
        category_name: cat.name.clone(),
        title: thread.title.clone(),
        author_name: shown_name(&state, thread.author_name.clone()),
        post_count: thread.post_count,
        created_at: thread.created_at.clone(),
        last_post_at: thread.last_post_at.clone(),
        is_pinned: thread.is_pinned,
        is_locked: thread.is_locked,
    };

    let cat_view = CategoryView {
        id: cat.id.clone(),
        slug: cat.slug.clone(),
        name: cat.name.clone(),
        description: cat.description.clone(),
        thread_count: 0,
        post_count: 0,
    };

    let mut ctx = PageRenderContext {
        template: "forum_thread.html".to_string(),
        content_type: "forum_thread".to_string(),
        title: thread.title.clone(),
        slug: format!("forum/{}/{}", cat.slug, thread.id),
        content_html: String::new(),
        pagination,
        ..render::site_page_context(&state)
    };

    if let Ok(v) = serde_json::to_value(&cat_view) {
        ctx.fields.insert("category".to_string(), v);
    }
    if let Ok(v) = serde_json::to_value(&thread_view) {
        ctx.fields.insert("thread".to_string(), v);
    }
    if let Ok(v) = serde_json::to_value(&paged_posts) {
        ctx.fields.insert("posts".to_string(), v);
    }
    insert_captcha_html(&state, &mut ctx);
    if params.submitted.as_deref() == Some("pending") {
        ctx.fields.insert("pending_notice".to_string(), json!(true));
    }
    if params.reported.as_deref() == Some("1") {
        ctx.fields
            .insert("reported_notice".to_string(), json!(true));
    }

    let mut tags = CacheTags::new();
    tags.add_url(&format!("/forum/{}/{}", cat.slug, thread.id));
    tags.add_tag("site-global");
    tags.add_dependency("collection", "forum");
    tags.add_dependency("forum-category", &cat.slug);
    tags.add_dependency("forum-thread", &thread.id);
    let resp = render_forum_page(&state, ctx).await;
    if personal {
        return no_store(resp);
    }
    crate::cdn::apply_cache_tags(&state, resp, &tags)
}

// =========================================================================
// HTML フォーム送信（Web UI からの直接 POST）
// =========================================================================

#[derive(Debug, Deserialize)]
pub struct CreateThreadForm {
    pub title: String,
    #[serde(default)]
    pub author_name: String,
    pub body: String,
    /// 上以外の欄。CAPTCHA のトークン（提供元ごとに欄の名前が違う）はここから取る
    #[serde(flatten)]
    pub extra: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct CreatePostForm {
    #[serde(default)]
    pub author_name: String,
    pub body: String,
    /// 返信先のレス番号。`flatten` と数値は併用できないので文字列で受けて読む
    #[serde(default)]
    pub reply_to_number: String,
    /// 上以外の欄。CAPTCHA のトークンはここから取る
    #[serde(flatten)]
    pub extra: HashMap<String, String>,
}

/// 掲示板の CAPTCHA。ログインしている投稿者（スタッフ・会員）は対象外。
/// 通らなければ、投稿者に見せる英語の文言を返す
async fn check_forum_captcha(
    state: &AppState,
    author: &Option<Author>,
    token: Option<&str>,
    ip: Option<&str>,
) -> Result<(), CaptchaError> {
    match &state.captcha.forum {
        Some(captcha) if author.is_none() => captcha.verify(token, ip).await,
        _ => Ok(()),
    }
}

/// フォームから CAPTCHA のトークンを取る
fn form_captcha_token<'a>(state: &AppState, extra: &'a HashMap<String, String>) -> Option<&'a str> {
    let captcha = state.captcha.forum.as_ref()?;
    extra.get(captcha.response_field()).map(String::as_str)
}

/// CAPTCHA を通らなかった API の応答
fn captcha_api_error(failure: CaptchaError) -> ApiError {
    let (status, code) = match failure {
        CaptchaError::Failed => (StatusCode::BAD_REQUEST, "captcha_failed"),
        CaptchaError::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "captcha_unavailable"),
    };
    (
        status,
        Json(json!({ "error": failure.to_string(), "code": code })),
    )
}

/// 投稿フォームに置く CAPTCHA のウィジェットを `fields.captcha_html` に入れる（掲示板にかけているときだけ）。
/// ページはキャッシュされるので、ログインしている人にも出る（検証はログインしていない投稿者だけ）
fn insert_captcha_html(state: &AppState, ctx: &mut PageRenderContext) {
    if let Some(captcha) = &state.captcha.forum {
        ctx.fields
            .insert("captcha_html".to_string(), json!(captcha.widget_html()));
    }
}

/// CAPTCHA を通らなかったフォームの応答（`back` は戻り先のパス）
fn captcha_form_error(failure: CaptchaError, base_path: &str, back: &str) -> Response {
    let status = match failure {
        CaptchaError::Failed => StatusCode::BAD_REQUEST,
        CaptchaError::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    };
    (
        status,
        Html(format!(
            "<h1>Error</h1><p>{}</p><p><a href=\"{}/{}\">Go back</a></p>",
            failure, base_path, back
        )),
    )
        .into_response()
}

/// JSON の API の本文。CAPTCHA のトークン（`captcha_token`）を足して受ける
#[derive(Debug, Deserialize)]
pub struct WithCaptcha<T> {
    #[serde(flatten)]
    pub request: T,
    #[serde(default)]
    pub captcha_token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateReportForm {
    #[serde(default = "default_post_target")]
    pub target_type: String,
    pub target_id: String,
    pub reason: String,
}

fn default_post_target() -> String {
    "post".to_string()
}

/// `POST /forum/:category_slug/threads` — フォームからの新規スレッド作成
pub async fn handle_create_thread_form(
    State(state): State<Arc<AppState>>,
    Path(category_slug): Path<String>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Form(form): Form<CreateThreadForm>,
) -> Response {
    let forum = match &state.forum {
        Some(f) => f.clone(),
        None => return render_not_found(&state, "Forum is currently disabled.").await,
    };
    if !state.system().forum_enabled {
        return render_not_found(&state, "Forum is currently disabled.").await;
    }

    let cat = match forum.get_category_by_slug(&category_slug).await {
        Ok(c) => c,
        Err(_) => return render_not_found(&state, "Category not found.").await,
    };

    let base_path = state
        .config
        .server
        .normalized_base_path()
        .unwrap_or_default();

    // 他のサイトからの送信を断る（会員の Cookie で認証するため）
    let poster = match resolve_poster(&state, &headers, form.author_name, true).await {
        Ok(poster) => poster,
        Err(failure) => return poster_form_error(failure, &base_path),
    };
    let mut author_name = poster.name;
    let author = poster.author;
    if author_name.trim().is_empty() {
        author_name = "Anonymous".to_string();
    }
    if let Err(failure) = check_forum_captcha(
        &state,
        &author,
        form_captcha_token(&state, &form.extra),
        client_ip.as_deref(),
    )
    .await
    {
        return captcha_form_error(failure, &base_path, &format!("forum/{}", category_slug));
    }

    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let req = CreateThreadRequest {
        category_id: cat.id,
        title: form.title,
        body: form.body,
        author_name,
        author,
        ip: client_ip,
        user_agent,
    };

    match forum.create_thread(req).await {
        Ok((thread, _)) => {
            if thread.status == "approved" {
                let mut tags = CacheTags::new();
                tags.add_dependency("collection", "forum");
                tags.add_dependency("forum-category", &category_slug);
                crate::cdn::purge_tags(&state, &tags).await;
            }
            let redirect_url = if thread.status == "approved" {
                format!("{}/forum/{}/{}", base_path, category_slug, thread.id)
            } else {
                format!("{}/forum/{}?submitted=pending", base_path, category_slug)
            };
            Redirect::to(&redirect_url).into_response()
        }
        Err(e) => {
            let msg = format!("Failed to create thread: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Html(format!(
                    "<h1>Error</h1><p>{}</p><p><a href=\"{}/forum/{}\">Go back</a></p>",
                    msg, base_path, category_slug
                )),
            )
                .into_response()
        }
    }
}

/// `POST /forum/:category_slug/:thread_id/posts` — フォームからのレス投稿
pub async fn handle_create_post_form(
    State(state): State<Arc<AppState>>,
    Path((category_slug, thread_id)): Path<(String, String)>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Form(form): Form<CreatePostForm>,
) -> Response {
    let forum = match &state.forum {
        Some(f) => f.clone(),
        None => return render_not_found(&state, "Forum is currently disabled.").await,
    };
    if !state.system().forum_enabled {
        return render_not_found(&state, "Forum is currently disabled.").await;
    }

    let base_path = state
        .config
        .server
        .normalized_base_path()
        .unwrap_or_default();

    let poster = match resolve_poster(&state, &headers, form.author_name, true).await {
        Ok(poster) => poster,
        Err(failure) => return poster_form_error(failure, &base_path),
    };
    let mut author_name = poster.name;
    let author = poster.author;
    if author_name.trim().is_empty() {
        author_name = "Anonymous".to_string();
    }
    if let Err(failure) = check_forum_captcha(
        &state,
        &author,
        form_captcha_token(&state, &form.extra),
        client_ip.as_deref(),
    )
    .await
    {
        return captcha_form_error(
            failure,
            &base_path,
            &format!("forum/{}/{}", category_slug, thread_id),
        );
    }

    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let req = CreatePostRequest {
        thread_id: thread_id.clone(),
        body: form.body,
        reply_to_post_id: None,
        reply_to_number: form.reply_to_number.trim().parse().ok(),
        author_name,
        author,
        ip: client_ip,
        user_agent,
    };

    match forum.create_post(req).await {
        Ok(post) => {
            if post.status == "approved" {
                let mut tags = CacheTags::new();
                tags.add_dependency("collection", "forum");
                tags.add_dependency("forum-category", &category_slug);
                tags.add_dependency("forum-thread", &thread_id);
                crate::cdn::purge_tags(&state, &tags).await;
            }
            let redirect_url = if post.status == "approved" {
                format!(
                    "{}/forum/{}/{}#post-{}",
                    base_path, category_slug, thread_id, post.post_number
                )
            } else {
                format!(
                    "{}/forum/{}/{}?submitted=pending",
                    base_path, category_slug, thread_id
                )
            };
            Redirect::to(&redirect_url).into_response()
        }
        Err(e) => {
            let msg = format!("Failed to post reply: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Html(format!(
                    "<h1>Error</h1><p>{}</p><p><a href=\"{}/forum/{}/{}\">Go back</a></p>",
                    msg, base_path, category_slug, thread_id
                )),
            )
                .into_response()
        }
    }
}

/// `POST /forum/:category_slug/:thread_id/report` — フォームからの通報送信
pub async fn handle_create_report_form(
    State(state): State<Arc<AppState>>,
    Path((category_slug, thread_id)): Path<(String, String)>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Form(form): Form<CreateReportForm>,
) -> Response {
    let forum = match &state.forum {
        Some(f) => f.clone(),
        None => return render_not_found(&state, "Forum is currently disabled.").await,
    };
    if !state.system().forum_enabled {
        return render_not_found(&state, "Forum is currently disabled.").await;
    }

    let base_path = state
        .config
        .server
        .normalized_base_path()
        .unwrap_or_default();

    let reporter_id = resolve_reporter_id(&state, &headers).await;
    let req = CreateReportRequest {
        target_type: form.target_type,
        target_id: form.target_id,
        reason: form.reason,
        reporter_id,
        reporter_ip: client_ip,
    };

    match forum.report(req).await {
        Ok(_) => {
            let redirect_url = format!(
                "{}/forum/{}/{}?reported=1",
                base_path, category_slug, thread_id
            );
            Redirect::to(&redirect_url).into_response()
        }
        Err(e) => {
            let msg = format!("Failed to submit report: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Html(format!(
                    "<h1>Error</h1><p>{}</p><p><a href=\"{}/forum/{}/{}\">Go back</a></p>",
                    msg, base_path, category_slug, thread_id
                )),
            )
                .into_response()
        }
    }
}

/// 投稿者を決められなかった理由
enum PosterError {
    /// 会員に公開名がまだ無い（掲示板に出す名前は、本名ではなく公開名）
    NicknameRequired,
    /// フォームが他のサイトから送られた（会員の Cookie で認証しているときは断る）
    CrossSite,
}

/// 投稿する人。ログインしていなければ `author` は `None`（匿名）
struct Poster {
    author: Option<Author>,
    name: String,
}

/// CMS のアカウントでログインしているか（そのユーザー ID）
async fn staff_user_id(state: &AppState, headers: &HeaderMap) -> Option<String> {
    let session_id = extract_session_id(headers)?;
    let session = state.sessions.get_session(&session_id).await.ok()??;
    Some(session.user_id)
}

/// 会員アカウントでログインしているか
async fn member_session(
    state: &AppState,
    headers: &HeaderMap,
) -> Option<aegis_members::MemberSessionData> {
    state.members.as_ref()?;
    crate::handlers::members::resolve_member_session(state, headers)
        .await
        .ok()
}

/// 見る人の立場: CMS のアカウント → スタッフ、会員 → 会員、どちらでもなければ匿名
async fn resolve_viewer(state: &AppState, headers: &HeaderMap) -> Viewer {
    if staff_user_id(state, headers).await.is_some() {
        return Viewer::Staff;
    }
    if member_session(state, headers).await.is_some() {
        return Viewer::Member;
    }
    Viewer::Anonymous
}

/// 投稿する人を決める。CMS のアカウント（スタッフ。名前は指定できる）、会員（名前は公開名。
/// 指定は無視する）、どちらでもなければ匿名（指定の名前）。
/// `form` が真（HTML のフォーム）で、会員が Cookie で認証しているときは、他のサイトから送られた
/// リクエストを断る（`Origin` / `Referer` を確かめる）
async fn resolve_poster(
    state: &AppState,
    headers: &HeaderMap,
    specified_name: String,
    form: bool,
) -> Result<Poster, PosterError> {
    if let Some(user_id) = staff_user_id(state, headers).await {
        let name = if specified_name.trim().is_empty() {
            if let Ok(Some(user)) = state.db.get_user_by_username(&user_id).await {
                user.display_name.unwrap_or(user_id.clone())
            } else {
                user_id.clone()
            }
        } else {
            specified_name
        };
        return Ok(Poster {
            author: Some(Author::staff(user_id)),
            name,
        });
    }

    if let Some(session) = member_session(state, headers).await {
        let cookie_only = !headers.contains_key(header::AUTHORIZATION);
        if form
            && cookie_only
            && !crate::handlers::members::request_comes_from_this_site(state, headers)
        {
            return Err(PosterError::CrossSite);
        }
        let profile = match state.members.as_ref() {
            Some(members) => members.get_profile(&session.member_id).await.ok(),
            None => None,
        };
        let nickname = profile
            .as_ref()
            .and_then(|p| p.nickname.as_deref())
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(str::to_string);
        let Some(name) = nickname else {
            return Err(PosterError::NicknameRequired);
        };
        return Ok(Poster {
            author: Some(Author::member(session.member_id)),
            name,
        });
    }

    Ok(Poster {
        author: None,
        name: specified_name,
    })
}

/// 通報した人の ID（スタッフはユーザー ID、会員は `member:` + ID、匿名は `None`）
async fn resolve_reporter_id(state: &AppState, headers: &HeaderMap) -> Option<String> {
    if let Some(user_id) = staff_user_id(state, headers).await {
        return Some(user_id);
    }
    member_session(state, headers)
        .await
        .map(|session| format!("member:{}", session.member_id))
}

/// 投稿者を決められなかった API の応答
fn poster_api_error(failure: PosterError) -> ApiError {
    match failure {
        PosterError::NicknameRequired => (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "Set a public name in your account before posting",
                "code": "nickname_required",
            })),
        ),
        PosterError::CrossSite => (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Cross-site requests are not allowed", "code": "cross_site" })),
        ),
    }
}

/// 投稿者を決められなかったフォームの応答（英語。戻るリンクつき）
fn poster_form_error(failure: PosterError, base_path: &str) -> Response {
    let (status, message) = match failure {
        PosterError::NicknameRequired => (
            StatusCode::BAD_REQUEST,
            format!(
                "Set a public name in <a href=\"{}/account\">your account</a> before posting.",
                base_path
            ),
        ),
        PosterError::CrossSite => (
            StatusCode::FORBIDDEN,
            "This request did not come from this site.".to_string(),
        ),
    };
    (
        status,
        Html(format!(
            "<h1>Error</h1><p>{}</p><p><a href=\"{}/forum\">Go back</a></p>",
            message, base_path
        )),
    )
        .into_response()
}

/// その人専用の応答（共有のキャッシュに載せない）
fn no_store(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("private, no-store"),
    );
    response
}

async fn is_editor_authenticated(state: &AppState, headers: &HeaderMap) -> bool {
    if let Some(session_id) = extract_session_id(headers) {
        if let Ok(Some(session)) = state.sessions.get_session(&session_id).await {
            return matches!(session.role.as_str(), "admin" | "editor");
        }
    }
    false
}

// =========================================================================
// REST API (`/api/v1/forum/...`)
// =========================================================================

/// `GET /api/v1/forum/categories`
pub async fn api_list_categories(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let forum = get_forum(&state)?;
    let viewer = resolve_viewer(&state, &headers).await;
    let categories = forum
        .list_categories(viewer)
        .await
        .map_err(forum_error_to_api)?;
    Ok(Json(json!({ "categories": categories })))
}

/// `POST /api/v1/forum/categories` (Admin認可)
pub async fn api_create_category(
    State(state): State<Arc<AppState>>,
    admin: AdminSession,
    Json(req): Json<CreateCategoryRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let forum = get_forum(&state)?;
    let record = forum
        .create_category(req, Some(&admin.0.user_id))
        .await
        .map_err(forum_error_to_api)?;
    let mut tags = CacheTags::new();
    tags.add_dependency("collection", "forum");
    crate::cdn::purge_tags(&state, &tags).await;
    Ok((StatusCode::CREATED, Json(json!({ "category": record }))))
}

/// `GET /api/v1/forum/categories/:slug`
pub async fn api_get_category(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let forum = get_forum(&state)?;
    let cat = forum
        .get_category_by_slug(&slug)
        .await
        .map_err(forum_error_to_api)?;
    let viewer = resolve_viewer(&state, &headers).await;
    forum
        .check_category_visible(viewer, &cat)
        .map_err(forum_error_to_api)?;
    Ok(Json(json!({ "category": cat })))
}

/// `PUT /api/v1/forum/categories/:id` (Admin認可)
pub async fn api_update_category(
    State(state): State<Arc<AppState>>,
    admin: AdminSession,
    Path(id): Path<String>,
    Json(req): Json<UpdateCategoryRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let forum = get_forum(&state)?;
    let updated = forum
        .update_category(&id, req, Some(&admin.0.user_id))
        .await
        .map_err(forum_error_to_api)?;
    let mut tags = CacheTags::new();
    tags.add_dependency("collection", "forum");
    tags.add_dependency("forum-category", &updated.slug);
    crate::cdn::purge_tags(&state, &tags).await;
    Ok(Json(json!({ "category": updated })))
}

/// `DELETE /api/v1/forum/categories/:id` (Admin認可)
pub async fn api_delete_category(
    State(state): State<Arc<AppState>>,
    admin: AdminSession,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let forum = get_forum(&state)?;
    forum
        .delete_category(&id, Some(&admin.0.user_id))
        .await
        .map_err(forum_error_to_api)?;
    let mut tags = CacheTags::new();
    tags.add_dependency("collection", "forum");
    crate::cdn::purge_tags(&state, &tags).await;
    Ok(Json(json!({ "success": true })))
}

#[derive(Debug, Deserialize)]
pub struct ThreadListApiQuery {
    pub category_slug: Option<String>,
    pub status: Option<String>,
    pub cursor_last_post_at: Option<String>,
    pub cursor_id: Option<String>,
    pub limit: Option<u64>,
}

fn resolve_status_filter(requested: Option<&str>, is_editor: bool) -> Option<String> {
    if !is_editor {
        return Some("approved".to_string());
    }
    match requested.map(str::trim) {
        None | Some("") | Some("approved") => Some("approved".to_string()),
        Some("all") => None,
        Some(other) => Some(other.to_string()),
    }
}

/// `GET /api/v1/forum/threads` — 全カテゴリまたは指定カテゴリのスレッド一覧
pub async fn api_list_all_threads(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<ThreadListApiQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let forum = get_forum(&state)?;
    let category_id = if let Some(slug) = &query.category_slug {
        if !slug.trim().is_empty() {
            let cat = forum
                .get_category_by_slug(slug)
                .await
                .map_err(forum_error_to_api)?;
            Some(cat.id)
        } else {
            None
        }
    } else {
        None
    };

    let limit = query
        .limit
        .unwrap_or(forum.config().default_page_size)
        .clamp(1, 100);
    let is_editor = is_editor_authenticated(&state, &headers).await;
    let status = resolve_status_filter(query.status.as_deref(), is_editor);

    let q = ThreadListQuery {
        category_id,
        status,
        before_last_post_at: query.cursor_last_post_at,
        before_id: query.cursor_id,
        limit,
    };

    let threads = forum.list_threads(&q).await.map_err(forum_error_to_api)?;
    Ok(Json(json!({ "threads": threads, "limit": limit })))
}

/// `GET /api/v1/forum/categories/:slug/threads`
pub async fn api_list_threads(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Query(query): Query<ThreadListApiQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let forum = get_forum(&state)?;
    let cat = forum
        .get_category_by_slug(&slug)
        .await
        .map_err(forum_error_to_api)?;
    let viewer = resolve_viewer(&state, &headers).await;
    forum
        .check_category_visible(viewer, &cat)
        .map_err(forum_error_to_api)?;

    let limit = query
        .limit
        .unwrap_or(forum.config().default_page_size)
        .clamp(1, 100);
    let is_editor = is_editor_authenticated(&state, &headers).await;
    let status = resolve_status_filter(query.status.as_deref(), is_editor);

    let q = ThreadListQuery {
        category_id: Some(cat.id),
        status,
        before_last_post_at: query.cursor_last_post_at,
        before_id: query.cursor_id,
        limit,
    };

    let threads = forum.list_threads(&q).await.map_err(forum_error_to_api)?;
    Ok(Json(json!({ "threads": threads, "limit": limit })))
}

/// `POST /api/v1/forum/threads`
pub async fn api_create_thread(
    State(state): State<Arc<AppState>>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(WithCaptcha {
        request: mut req,
        captcha_token,
    }): Json<WithCaptcha<CreateThreadRequest>>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    if !state.system().forum_enabled {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Forum is temporarily disabled" })),
        ));
    }
    let forum = get_forum(&state)?;
    let poster = resolve_poster(&state, &headers, req.author_name, false)
        .await
        .map_err(poster_api_error)?;
    check_forum_captcha(
        &state,
        &poster.author,
        captcha_token.as_deref(),
        client_ip.as_deref(),
    )
    .await
    .map_err(captcha_api_error)?;
    req.author = poster.author;
    req.author_name = poster.name;
    req.ip = client_ip;
    req.user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let (thread, post) = forum.create_thread(req).await.map_err(forum_error_to_api)?;
    if thread.status == "approved" {
        let mut tags = CacheTags::new();
        tags.add_dependency("collection", "forum");
        crate::cdn::purge_tags(&state, &tags).await;
    }
    Ok((
        StatusCode::CREATED,
        Json(json!({ "thread": thread, "initial_post": post })),
    ))
}

/// スレッドのカテゴリを見られる人か（会員・スタッフ限定のカテゴリのスレッドは、無いものとして扱う）
async fn check_thread_visible(
    state: &AppState,
    forum: &ForumService,
    headers: &HeaderMap,
    category_id: &str,
) -> Result<(), ApiError> {
    let category = forum
        .get_category(category_id)
        .await
        .map_err(forum_error_to_api)?;
    if category.visibility == "public" {
        return Ok(());
    }
    let viewer = resolve_viewer(state, headers).await;
    forum
        .check_category_visible(viewer, &category)
        .map_err(|_| api_error(StatusCode::NOT_FOUND, "Thread not found"))
}

/// `GET /api/v1/forum/threads/:id`
pub async fn api_get_thread(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let forum = get_forum(&state)?;
    let thread = forum.get_thread(&id).await.map_err(forum_error_to_api)?;
    if thread.status != "approved" && !is_editor_authenticated(&state, &headers).await {
        return Err(api_error(StatusCode::NOT_FOUND, "Thread not found"));
    }
    check_thread_visible(&state, &forum, &headers, &thread.category_id).await?;
    Ok(Json(json!({ "thread": thread })))
}

#[derive(Debug, Deserialize)]
pub struct PostListApiQuery {
    pub cursor_post_number: Option<u64>,
    pub status: Option<String>,
    pub limit: Option<u64>,
}

/// `GET /api/v1/forum/threads/:id/posts`
pub async fn api_list_posts(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<PostListApiQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let forum = get_forum(&state)?;
    let thread = forum.get_thread(&id).await.map_err(forum_error_to_api)?;
    check_thread_visible(&state, &forum, &headers, &thread.category_id).await?;
    let limit = query
        .limit
        .unwrap_or(forum.config().default_page_size)
        .clamp(1, 100);
    let is_editor = is_editor_authenticated(&state, &headers).await;
    let status = resolve_status_filter(query.status.as_deref(), is_editor);
    let q = PostListQuery {
        thread_id: id,
        after_post_number: query.cursor_post_number.unwrap_or(0),
        status,
        limit,
    };
    let mut posts = forum.list_posts(&q).await.map_err(forum_error_to_api)?;
    if !is_editor {
        // 投稿者の IP・User-Agent は、運営者だけに見せる
        for post in &mut posts {
            post.ip = None;
            post.user_agent = None;
        }
    }
    Ok(Json(json!({ "posts": posts, "limit": limit })))
}

/// `POST /api/v1/forum/posts`
pub async fn api_create_post(
    State(state): State<Arc<AppState>>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(WithCaptcha {
        request: mut req,
        captcha_token,
    }): Json<WithCaptcha<CreatePostRequest>>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    if !state.system().forum_enabled {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Forum is temporarily disabled" })),
        ));
    }
    let forum = get_forum(&state)?;
    let poster = resolve_poster(&state, &headers, req.author_name, false)
        .await
        .map_err(poster_api_error)?;
    check_forum_captcha(
        &state,
        &poster.author,
        captcha_token.as_deref(),
        client_ip.as_deref(),
    )
    .await
    .map_err(captcha_api_error)?;
    req.author = poster.author;
    req.author_name = poster.name;
    req.ip = client_ip;
    req.user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let post = forum.create_post(req).await.map_err(forum_error_to_api)?;
    if post.status == "approved" {
        let mut tags = CacheTags::new();
        tags.add_dependency("collection", "forum");
        tags.add_dependency("forum-thread", &post.thread_id);
        crate::cdn::purge_tags(&state, &tags).await;
    }
    Ok((StatusCode::CREATED, Json(json!({ "post": post }))))
}

#[derive(Debug, Deserialize)]
pub struct PinThreadReq {
    pub pinned: bool,
}

/// `POST /api/v1/forum/threads/:id/pin` (Editor認可)
pub async fn api_pin_thread(
    State(state): State<Arc<AppState>>,
    session: EditorSession,
    Path(id): Path<String>,
    Json(body): Json<PinThreadReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let forum = get_forum(&state)?;
    forum
        .set_thread_pinned(&id, body.pinned, Some(&session.0.user_id))
        .await
        .map_err(forum_error_to_api)?;
    let mut tags = CacheTags::new();
    tags.add_dependency("collection", "forum");
    tags.add_dependency("forum-thread", &id);
    crate::cdn::purge_tags(&state, &tags).await;
    Ok(Json(json!({ "success": true, "pinned": body.pinned })))
}

#[derive(Debug, Deserialize)]
pub struct LockThreadReq {
    pub locked: bool,
}

/// `POST /api/v1/forum/threads/:id/lock` (Editor認可)
pub async fn api_lock_thread(
    State(state): State<Arc<AppState>>,
    session: EditorSession,
    Path(id): Path<String>,
    Json(body): Json<LockThreadReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let forum = get_forum(&state)?;
    forum
        .set_thread_locked(&id, body.locked, Some(&session.0.user_id))
        .await
        .map_err(forum_error_to_api)?;
    let mut tags = CacheTags::new();
    tags.add_dependency("collection", "forum");
    tags.add_dependency("forum-thread", &id);
    crate::cdn::purge_tags(&state, &tags).await;
    Ok(Json(json!({ "success": true, "locked": body.locked })))
}

#[derive(Debug, Deserialize)]
pub struct ModerateStatusReq {
    pub status: ModerationStatus,
}

/// `PATCH /api/v1/forum/threads/:id/status` (Editor認可)
pub async fn api_moderate_thread(
    State(state): State<Arc<AppState>>,
    session: EditorSession,
    Path(id): Path<String>,
    Json(body): Json<ModerateStatusReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let forum = get_forum(&state)?;
    let thread = forum
        .moderate_thread(&id, body.status, Some(&session.0.user_id))
        .await
        .map_err(forum_error_to_api)?;
    let mut tags = CacheTags::new();
    tags.add_dependency("collection", "forum");
    tags.add_dependency("forum-thread", &id);
    crate::cdn::purge_tags(&state, &tags).await;
    Ok(Json(json!({ "thread": thread })))
}

/// `PATCH /api/v1/forum/posts/:id/status` (Editor認可)
pub async fn api_moderate_post(
    State(state): State<Arc<AppState>>,
    session: EditorSession,
    Path(id): Path<String>,
    Json(body): Json<ModerateStatusReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let forum = get_forum(&state)?;
    let post = forum
        .moderate_post(&id, body.status, Some(&session.0.user_id))
        .await
        .map_err(forum_error_to_api)?;
    let mut tags = CacheTags::new();
    tags.add_dependency("collection", "forum");
    tags.add_dependency("forum-thread", &post.thread_id);
    crate::cdn::purge_tags(&state, &tags).await;
    Ok(Json(json!({ "post": post })))
}

/// `DELETE /api/v1/forum/threads/:id` (Editor認可)
pub async fn api_delete_thread(
    State(state): State<Arc<AppState>>,
    session: EditorSession,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let forum = get_forum(&state)?;
    forum
        .delete_thread(&id, Some(&session.0.user_id))
        .await
        .map_err(forum_error_to_api)?;
    let mut tags = CacheTags::new();
    tags.add_dependency("collection", "forum");
    tags.add_dependency("forum-thread", &id);
    crate::cdn::purge_tags(&state, &tags).await;
    Ok(Json(json!({ "success": true })))
}

/// `DELETE /api/v1/forum/posts/:id` (Editor認可)
pub async fn api_delete_post(
    State(state): State<Arc<AppState>>,
    session: EditorSession,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let forum = get_forum(&state)?;
    forum
        .delete_post(&id, Some(&session.0.user_id))
        .await
        .map_err(forum_error_to_api)?;
    let mut tags = CacheTags::new();
    tags.add_dependency("collection", "forum");
    crate::cdn::purge_tags(&state, &tags).await;
    Ok(Json(json!({ "success": true })))
}

/// `GET /api/v1/forum/settings` (Editor認可)
pub async fn api_get_forum_settings(
    State(state): State<Arc<AppState>>,
    _session: EditorSession,
) -> Result<Json<serde_json::Value>, ApiError> {
    let forum = get_forum(&state)?;
    let settings = forum.settings().await.map_err(forum_error_to_api)?;
    Ok(Json(serde_json::to_value(settings).unwrap_or_default()))
}

/// `PUT /api/v1/forum/settings` (Admin認可)
pub async fn api_update_forum_settings(
    State(state): State<Arc<AppState>>,
    admin: AdminSession,
    Json(overrides): Json<PolicyOverrides>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let forum = get_forum(&state)?;
    let settings = forum
        .update_settings(Some(&admin.0.user_id), overrides)
        .await
        .map_err(forum_error_to_api)?;
    Ok(Json(serde_json::to_value(settings).unwrap_or_default()))
}

#[derive(Debug, Deserialize)]
pub struct ReportReqBody {
    pub target_type: String,
    pub target_id: String,
    pub reason: String,
}

/// `POST /api/v1/forum/reports` (公開)
pub async fn api_create_report(
    State(state): State<Arc<AppState>>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(body): Json<ReportReqBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let forum = get_forum(&state)?;
    let reporter_id = resolve_reporter_id(&state, &headers).await;
    let report = forum
        .report(CreateReportRequest {
            target_type: body.target_type,
            target_id: body.target_id,
            reason: body.reason,
            reporter_id,
            reporter_ip: client_ip,
        })
        .await
        .map_err(forum_error_to_api)?;
    Ok((StatusCode::CREATED, Json(json!({ "report": report }))))
}

#[derive(Debug, Deserialize)]
pub struct ReportListApiQuery {
    pub status: Option<String>,
    pub limit: Option<u64>,
}

/// `GET /api/v1/forum/reports` (Editor認可)
pub async fn api_list_reports(
    State(state): State<Arc<AppState>>,
    _session: EditorSession,
    Query(query): Query<ReportListApiQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let forum = get_forum(&state)?;
    let status = query
        .status
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "all");
    let reports = forum
        .list_reports(status, query.limit.unwrap_or(100))
        .await
        .map_err(forum_error_to_api)?;
    Ok(Json(json!({ "reports": reports })))
}

#[derive(Debug, Deserialize)]
pub struct ResolveReportReq {
    pub status: String,
}

/// `PATCH /api/v1/forum/reports/:id` (Editor認可)
pub async fn api_resolve_report(
    State(state): State<Arc<AppState>>,
    session: EditorSession,
    Path(id): Path<String>,
    Json(body): Json<ResolveReportReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let forum = get_forum(&state)?;
    let report = forum
        .resolve_report(&id, &body.status, Some(&session.0.user_id))
        .await
        .map_err(forum_error_to_api)?;
    Ok(Json(json!({ "report": report })))
}
