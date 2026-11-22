//! `aegis-server import wordpress` を、実際に待ち受けるサーバーに向けて通しで動かすテスト。
//!
//! 単体テストが押さえる変換・計画・報告とは別に、ログイン → 著者 → 分類 → メディア → 記事 →
//! コメント → 旧 URL のリダイレクトの順につながっていること、同じファイルをもう一度流しても
//! 何も増えないことを確かめる。

use std::net::SocketAddr;
use std::sync::Arc;

use aegis_auth::users::{create_user, NewUser};
use aegis_comments::CommentService;
use aegis_runtime::PluginSupervisor;
use aegis_schema::SchemaLoader;
use reqwest::{redirect::Policy, Client as Http, StatusCode};
use serde_json::Value;

use super::{run, WordpressArgs};
use crate::config::AegisConfig;

const ADMIN: &str = "importer";
const PASSWORD: &str = "correct-horse-battery";

/// 1x1 の PNG
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xff, 0xff, 0x3f,
    0x00, 0x05, 0xfe, 0x02, 0xfe, 0xdc, 0xcc, 0x59, 0xe7, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e,
    0x44, 0xae, 0x42, 0x60, 0x82,
];

/// 取り込み先のサーバー（実際のポートで待ち受ける）
struct Site {
    url: String,
    http: Http,
    admin: String,
}

#[allow(clippy::field_reassign_with_default)]
async fn site() -> Site {
    let schemas_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schemas: Vec<_> = SchemaLoader::load_dir(&schemas_dir)
        .unwrap()
        .into_iter()
        .filter(|s| matches!(s.content_type.as_str(), "article" | "page" | "category"))
        .collect();

    let mut config = AegisConfig::default();
    config.comments.enabled = true;
    config.comments.moderation.require_approval = false;
    config.tags.enabled = true;
    let state = crate::test_support::test_app_state(config, PluginSupervisor::new(), schemas).await;
    let tags = aegis_tags::TagService::new(state.db.clone(), state.config.tags.clone());
    let comments =
        CommentService::new(state.config.comments.clone(), state.comment_deps()).unwrap();
    let state = Arc::new(state.with_tags(tags).with_comments(comments));
    create_user(
        state.db.as_ref(),
        NewUser {
            username: ADMIN.to_string(),
            password: PASSWORD.to_string(),
            role: "admin".to_string(),
            email: None,
            display_name: None,
        },
    )
    .await
    .unwrap();
    let admin = state
        .sessions
        .create_session(ADMIN, "admin", Default::default(), None)
        .await
        .unwrap()
        .id;

    let app = crate::app::router(&state.config, state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap()
    });
    Site {
        url,
        http: Http::builder().redirect(Policy::none()).build().unwrap(),
        admin,
    }
}

impl Site {
    async fn get(&self, path: &str) -> (StatusCode, Value) {
        let response = self
            .http
            .get(format!("{}{path}", self.url))
            .bearer_auth(&self.admin)
            .send()
            .await
            .unwrap();
        let status = response.status();
        let text = response.text().await.unwrap();
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }
}

/// 元のサイトの代わりに、添付ファイルを配る（`/wp-content/uploads/2024/05/photo.png`）
async fn old_site() -> String {
    let app = axum::Router::new().route(
        "/wp-content/uploads/2024/05/photo.png",
        axum::routing::get(|| async { ([("content-type", "image/png")], PNG) }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    url
}

fn export(old: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<rss version="2.0"
	xmlns:excerpt="http://wordpress.org/export/1.2/excerpt/"
	xmlns:content="http://purl.org/rss/1.0/modules/content/"
	xmlns:dc="http://purl.org/dc/elements/1.1/"
	xmlns:wp="http://wordpress.org/export/1.2/">
<channel>
	<title>Old Blog</title>
	<link>{old}</link>
	<wp:wxr_version>1.2</wp:wxr_version>
	<wp:base_site_url>{old}</wp:base_site_url>
	<wp:author><wp:author_id>1</wp:author_id><wp:author_login><![CDATA[hanako]]></wp:author_login><wp:author_email><![CDATA[hanako@example.com]]></wp:author_email><wp:author_display_name><![CDATA[Hanako]]></wp:author_display_name></wp:author>
	<wp:category><wp:term_id>3</wp:term_id><wp:category_nicename><![CDATA[tech]]></wp:category_nicename><wp:category_parent><![CDATA[]]></wp:category_parent><wp:cat_name><![CDATA[Tech]]></wp:cat_name></wp:category>
	<wp:tag><wp:term_id>5</wp:term_id><wp:tag_slug><![CDATA[rust]]></wp:tag_slug><wp:tag_name><![CDATA[Rust]]></wp:tag_name></wp:tag>
	<item>
		<title>Hello</title>
		<link>{old}/2024/05/hello/</link>
		<dc:creator><![CDATA[hanako]]></dc:creator>
		<content:encoded><![CDATA[<!-- wp:paragraph -->
<p>Hello <strong>world</strong>.</p>
<!-- /wp:paragraph -->
<!-- wp:image -->
<figure class="wp-block-image"><img src="{old}/wp-content/uploads/2024/05/photo.png" alt="A photo"/></figure>
<!-- /wp:image -->]]></content:encoded>
		<excerpt:encoded><![CDATA[A short note]]></excerpt:encoded>
		<wp:post_id>10</wp:post_id>
		<wp:post_date_gmt><![CDATA[2024-05-01 10:00:00]]></wp:post_date_gmt>
		<wp:post_name><![CDATA[hello]]></wp:post_name>
		<wp:status><![CDATA[publish]]></wp:status>
		<wp:post_type><![CDATA[post]]></wp:post_type>
		<category domain="category" nicename="tech"><![CDATA[Tech]]></category>
		<category domain="post_tag" nicename="rust"><![CDATA[Rust]]></category>
		<wp:comment>
			<wp:comment_id>1</wp:comment_id>
			<wp:comment_author><![CDATA[Taro]]></wp:comment_author>
			<wp:comment_author_email><![CDATA[taro@example.com]]></wp:comment_author_email>
			<wp:comment_date_gmt><![CDATA[2024-05-01 12:00:00]]></wp:comment_date_gmt>
			<wp:comment_content><![CDATA[Nice post!]]></wp:comment_content>
			<wp:comment_approved><![CDATA[1]]></wp:comment_approved>
			<wp:comment_type><![CDATA[comment]]></wp:comment_type>
			<wp:comment_parent>0</wp:comment_parent>
		</wp:comment>
		<wp:comment>
			<wp:comment_id>2</wp:comment_id>
			<wp:comment_author><![CDATA[Hanako]]></wp:comment_author>
			<wp:comment_date_gmt><![CDATA[2024-05-01 13:00:00]]></wp:comment_date_gmt>
			<wp:comment_content><![CDATA[Thanks!]]></wp:comment_content>
			<wp:comment_approved><![CDATA[1]]></wp:comment_approved>
			<wp:comment_type><![CDATA[comment]]></wp:comment_type>
			<wp:comment_parent>1</wp:comment_parent>
		</wp:comment>
	</item>
	<item>
		<title>About</title>
		<link>{old}/about/</link>
		<dc:creator><![CDATA[hanako]]></dc:creator>
		<content:encoded><![CDATA[About us.]]></content:encoded>
		<wp:post_id>20</wp:post_id>
		<wp:post_date_gmt><![CDATA[2024-04-01 09:00:00]]></wp:post_date_gmt>
		<wp:post_name><![CDATA[about]]></wp:post_name>
		<wp:status><![CDATA[draft]]></wp:status>
		<wp:post_type><![CDATA[page]]></wp:post_type>
	</item>
	<item>
		<title>photo</title>
		<link>{old}/hello/photo/</link>
		<wp:post_id>11</wp:post_id>
		<wp:post_date_gmt><![CDATA[2024-05-01 09:00:00]]></wp:post_date_gmt>
		<wp:post_name><![CDATA[photo]]></wp:post_name>
		<wp:status><![CDATA[inherit]]></wp:status>
		<wp:post_parent>10</wp:post_parent>
		<wp:post_type><![CDATA[attachment]]></wp:post_type>
		<wp:attachment_url><![CDATA[{old}/wp-content/uploads/2024/05/photo.png]]></wp:attachment_url>
	</item>
</channel>
</rss>
"#
    )
}

fn args(site: &Site, file: std::path::PathBuf, report: std::path::PathBuf) -> WordpressArgs {
    WordpressArgs {
        file,
        url: site.url.clone(),
        username: Some(ADMIN.to_string()),
        password: Some(PASSWORD.to_string()),
        dry_run: false,
        report,
        skip_media: false,
        skip_authors: false,
    }
}

/// 一覧の応答の配列（キーは API ごとに違う: `contents`・`comments`・`items`）
fn items(body: &Value) -> Vec<Value> {
    body.as_object()
        .and_then(|map| map.values().find_map(|v| v.as_array()))
        .unwrap_or_else(|| panic!("no list in {body}"))
        .clone()
}

#[tokio::test]
async fn test_import_runs_through_every_phase_and_is_safe_to_repeat() {
    let site = site().await;
    let old = old_site().await;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("export.xml");
    std::fs::write(&file, export(&old)).unwrap();
    let report = dir.path().join("report.md");

    run(args(&site, file.clone(), report.clone()))
        .await
        .expect("the import succeeds");
    // 失敗があれば `run` が Err を返す。報告の表に、作ったものが並ぶ
    let text = std::fs::read_to_string(&report).unwrap();
    assert!(text.contains("| article | 1 | 0 | 0 | 0 |"), "{text}");
    assert!(text.contains("| comment | 2 | 0 | 0 | 0 |"), "{text}");

    // 記事: 元の公開日、本文のブロック、画像の URL の付け替え、カテゴリ・抜粋
    let (status, article) = site.get("/api/v1/contents/article/hello").await;
    assert_eq!(status, StatusCode::OK, "{article}");
    assert_eq!(article["title"], "Hello");
    assert_eq!(article["status"], "published");
    assert!(
        article["published_at"]
            .as_str()
            .unwrap()
            .starts_with("2024-05-01T10:00:00"),
        "{article}"
    );
    assert_eq!(article["fields"]["excerpt"], "A short note", "{article}");
    let blocks = article["blocks"].to_string();
    assert!(blocks.contains("world"), "{blocks}");
    assert!(
        !blocks.contains(&old),
        "the old host is still in the body: {blocks}"
    );
    assert!(blocks.contains("wordpress/2024/05/photo.png"), "{blocks}");
    let category = &article["fields"]["category_id"];
    assert!(category.is_string(), "{article}");

    // 固定ページは下書きのまま
    let (status, page) = site.get("/api/v1/contents/page/about").await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(page["status"], "draft");

    // 分類・メディア・著者
    let (status, cat) = site.get("/api/v1/contents/category/tech").await;
    assert_eq!(status, StatusCode::OK, "{cat}");
    assert_eq!(cat["id"], *category);
    let (_, tags) = site.get("/api/v1/contents/article/hello/tags").await;
    assert!(tags.to_string().contains("rust"), "{tags}");
    let (_, users) = site.get("/api/v1/users").await;
    let hanako = users["users"]
        .as_array()
        .unwrap()
        .clone()
        .into_iter()
        .find(|u| u["username"] == "hanako")
        .unwrap_or_else(|| panic!("no hanako in {users}"));
    assert_eq!(hanako["role"], "contributor");
    assert_eq!(hanako["disabled"], true, "{hanako}");

    // コメント（返信のつながりを保つ）
    let (status, comments) = site.get("/api/v1/contents/article/hello/comments").await;
    assert_eq!(status, StatusCode::OK, "{comments}");
    let list = items(&comments);
    assert_eq!(list.len(), 2, "{comments}");

    // 旧 URL → 新しい URL の恒久リダイレクト
    let old_path = site
        .http
        .get(format!("{}/2024/05/hello/", site.url))
        .send()
        .await
        .unwrap();
    assert_eq!(old_path.status(), StatusCode::MOVED_PERMANENTLY);
    assert!(old_path.headers()["location"]
        .to_str()
        .unwrap()
        .ends_with("/articles/hello"));

    // もう一度流しても、増えない
    let second = dir.path().join("report-2.md");
    run(args(&site, file, second.clone()))
        .await
        .expect("the second run succeeds");
    // 2 回目は、すべて「すでにある」と数える
    let text = std::fs::read_to_string(&second).unwrap();
    assert!(text.contains("| article | 0 | 1 | 0 | 0 |"), "{text}");
    assert!(text.contains("| category | 0 | 1 | 0 | 0 |"), "{text}");
    let (_, list) = site.get("/api/v1/contents/article").await;
    assert_eq!(items(&list).len(), 1, "{list}");
    let (_, comments) = site.get("/api/v1/contents/article/hello/comments").await;
    assert_eq!(items(&comments).len(), 2, "{comments}");
}

#[tokio::test]
async fn test_import_stops_with_a_clear_message_for_a_wrong_password() {
    let site = site().await;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("export.xml");
    std::fs::write(&file, export("http://127.0.0.1:9")).unwrap();
    let mut wrong = args(&site, file, dir.path().join("report.md"));
    wrong.password = Some("not-the-password".to_string());

    let error = run(wrong).await.unwrap_err().to_string();
    assert!(error.contains("Login failed"), "{error}");
}
