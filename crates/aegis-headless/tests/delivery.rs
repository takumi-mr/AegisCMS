//! 配信 API の検証（実際の SQLite を通す）。最優先は「公開していないものが、どの経路でも返らない」こと。

use std::collections::HashMap;

use aegis_db::{
    ContentPublicationChange, ContentRecord, ContentVersionRecord, DatabaseAdapter, SqliteAdapter,
};
use aegis_headless::{get, list, HeadlessConfig, HeadlessError, ListParams};
use aegis_schema::{SchemaDefinition, SchemaLoader};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};

const ARTICLE: &str = r#"
content_type: article
label: Article
fields:
  - name: excerpt
    type: string
    nullable: true
  - name: internal_note
    type: string
    nullable: true
    public: false
  - name: custom_meta
    type: jsonb
    nullable: true
"#;

const CATEGORY: &str = "content_type: category\nlabel: Category\n";

fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-11-05T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

fn record(
    content_type: &str,
    id: &str,
    day: u32,
    status: &str,
    category: Option<&str>,
    unpublish_at: Option<&str>,
) -> ContentRecord {
    let mut custom_fields = HashMap::new();
    if let Some(category) = category {
        custom_fields.insert("category_id".to_string(), json!(category));
    }
    if content_type == "article" {
        custom_fields.insert("excerpt".to_string(), json!(format!("Excerpt {id}")));
        custom_fields.insert("internal_note".to_string(), json!("SECRET-NOTE"));
        custom_fields.insert("custom_meta".to_string(), json!({ "seo": { "k": id } }));
    }
    ContentRecord {
        id: id.to_string(),
        content_type: content_type.to_string(),
        title: format!("Title {id}"),
        slug: format!("slug-{id}"),
        status: status.to_string(),
        blocks_json: json!([{ "name": "core/paragraph", "text": format!("Body of {id}") }])
            .to_string(),
        author_id: Some("author-internal-id".to_string()),
        published_at: (status == "published").then(|| format!("2026-10-{day:02}T00:00:00Z")),
        unpublish_at: unpublish_at.map(str::to_string),
        created_at: format!("2026-10-{day:02}T00:00:00Z"),
        updated_at: format!("2026-10-{day:02}T00:00:00Z"),
        custom_fields,
    }
}

struct Site {
    adapter: SqliteAdapter,
    schemas: HashMap<String, SchemaDefinition>,
    config: HeadlessConfig,
}

impl Site {
    async fn new() -> Self {
        let adapter = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
        adapter.init().await.unwrap();
        let schemas: HashMap<String, SchemaDefinition> = [ARTICLE, CATEGORY]
            .into_iter()
            .map(|yaml| {
                let schema = SchemaLoader::load_from_str(yaml).unwrap();
                (schema.content_type.clone(), schema)
            })
            .collect();
        adapter.run_migrations(&schemas).await.unwrap();
        Self {
            adapter,
            schemas,
            config: HeadlessConfig::default(),
        }
    }

    async fn put(&self, record: ContentRecord) {
        let schema = &self.schemas[&record.content_type];
        self.adapter.insert_content(schema, &record).await.unwrap();
    }

    async fn list(&self, params: ListParams) -> Result<Value, HeadlessError> {
        let delivered = list(
            &self.adapter,
            &self.config,
            &self.schemas,
            "article",
            &params,
            now(),
        )
        .await?;
        Ok(serde_json::to_value(delivered.body).unwrap())
    }

    async fn get(&self, content_type: &str, slug: &str) -> Result<Value, HeadlessError> {
        Ok(get(
            &self.adapter,
            &self.config,
            &self.schemas,
            content_type,
            slug,
            now(),
        )
        .await?
        .body)
    }
}

fn ids(page: &Value) -> Vec<&str> {
    page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap())
        .collect()
}

/// 公開中・下書き・予約中・公開期間を過ぎた公開、を並べたサイト
async fn mixed_site() -> Site {
    let site = Site::new().await;
    site.put(record("category", "c-live", 1, "published", None, None))
        .await;
    site.put(record("category", "c-draft", 2, "draft", None, None))
        .await;
    site.put(record(
        "article",
        "a1",
        1,
        "published",
        Some("c-live"),
        None,
    ))
    .await;
    site.put(record(
        "article",
        "a2",
        2,
        "published",
        Some("c-draft"),
        None,
    ))
    .await;
    site.put(record("article", "a3", 3, "draft", None, None))
        .await;
    site.put(record(
        "article",
        "a4",
        4,
        "published",
        None,
        Some("2026-11-05T11:59:00Z"),
    ))
    .await;
    site.put(record("article", "a5", 5, "scheduled", None, None))
        .await;
    site.put(record(
        "article",
        "a6",
        6,
        "published",
        None,
        Some("2099-01-01T00:00:00Z"),
    ))
    .await;
    site
}

#[tokio::test]
async fn only_live_published_content_is_listed() {
    let site = mixed_site().await;
    let page = site.list(ListParams::default()).await.unwrap();
    // 下書き(a3)・予約中(a5)・公開期間を過ぎた(a4)は返らない。期間内の公開(a6)は返る
    assert_eq!(ids(&page), vec!["a6", "a2", "a1"]);
}

#[tokio::test]
async fn single_items_that_are_not_live_are_not_found() {
    let site = mixed_site().await;
    assert!(site.get("article", "slug-a1").await.is_ok());
    assert!(site.get("article", "a1").await.is_ok(), "by id too");
    for hidden in ["slug-a3", "slug-a4", "slug-a5", "missing"] {
        assert!(
            matches!(
                site.get("article", hidden).await,
                Err(HeadlessError::NotFound)
            ),
            "{hidden}"
        );
    }
}

#[tokio::test]
async fn types_outside_the_allow_list_do_not_exist() {
    let site = mixed_site().await;
    // category は公開する型に入っていない（既定は article と page）
    assert!(matches!(
        site.get("category", "slug-c-live").await,
        Err(HeadlessError::NotFound)
    ));
    assert!(matches!(
        list(
            &site.adapter,
            &site.config,
            &site.schemas,
            "category",
            &ListParams::default(),
            now()
        )
        .await,
        Err(HeadlessError::NotFound)
    ));
    assert!(matches!(
        site.get("nonexistent", "x").await,
        Err(HeadlessError::NotFound)
    ));
}

#[tokio::test]
async fn internal_fields_never_appear() {
    let site = mixed_site().await;
    let single = site.get("article", "slug-a1").await.unwrap();
    let page = site.list(ListParams::default()).await.unwrap();
    for body in [single.to_string(), page.to_string()] {
        for secret in [
            "SECRET-NOTE",
            "internal_note",
            "author-internal-id",
            "author_id",
            "unpublish_at",
            "\"status\"",
            "comments_open",
        ] {
            assert!(!body.contains(secret), "{secret} leaked in {body}");
        }
    }
    // 公開してよいものは返る
    assert_eq!(single["fields"]["excerpt"], "Excerpt a1");
    assert_eq!(
        single["fields"]["custom_meta"],
        json!({ "seo": { "k": "a1" } })
    );
    assert_eq!(single["excerpt"], "Excerpt a1");
    assert_eq!(single["blocks"][0]["text"], "Body of a1");
    // 一覧の要素は本文を含めない
    assert!(page["items"][0].get("blocks").is_none());
}

#[tokio::test]
async fn relations_show_only_published_targets() {
    let site = mixed_site().await;
    let live = site.get("article", "slug-a1").await.unwrap();
    assert_eq!(live["relations"]["category_id"]["title"], "Title c-live");
    assert_eq!(live["relations"]["category_id"]["slug"], "slug-c-live");

    // 下書きのカテゴリを指す記事: 参照先の名前は一切出ない
    let to_draft = site.get("article", "slug-a2").await.unwrap();
    assert!(to_draft.get("relations").is_none());
    let page = site.list(ListParams::default()).await.unwrap();
    assert!(!page.to_string().contains("Title c-draft"));
    assert!(!page.to_string().contains("slug-c-draft"));
}

#[tokio::test]
async fn pages_walk_every_item_once_with_a_cursor() {
    let site = Site::new().await;
    for day in 1..=7 {
        site.put(record(
            "article",
            &format!("p{day}"),
            day,
            "published",
            None,
            None,
        ))
        .await;
    }
    let mut seen = Vec::new();
    let mut cursor: Option<String> = None;
    let mut pages = 0;
    loop {
        let page = site
            .list(ListParams {
                limit: Some("3".into()),
                cursor: cursor.clone(),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(page["total"], 7, "the total does not depend on the page");
        seen.extend(ids(&page).into_iter().map(str::to_string));
        pages += 1;
        match page["next_cursor"].as_str() {
            Some(next) => cursor = Some(next.to_string()),
            None => break,
        }
    }
    assert_eq!(pages, 3);
    assert_eq!(seen, vec!["p7", "p6", "p5", "p4", "p3", "p2", "p1"]);
}

#[tokio::test]
async fn a_hidden_row_does_not_break_paging() {
    // 公開期間を過ぎた行が 1 ページの端にあっても、続きのカーソルは途切れない
    let site = Site::new().await;
    site.put(record("article", "p1", 1, "published", None, None))
        .await;
    site.put(record(
        "article",
        "p2",
        2,
        "published",
        None,
        Some("2026-11-05T11:00:00Z"),
    ))
    .await;
    site.put(record("article", "p3", 3, "published", None, None))
        .await;
    let first = site
        .list(ListParams {
            limit: Some("2".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(ids(&first), vec!["p3"]);
    let cursor = first["next_cursor"]
        .as_str()
        .expect("there is more")
        .to_string();
    let second = site
        .list(ListParams {
            limit: Some("2".into()),
            cursor: Some(cursor),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(ids(&second), vec!["p1"]);
    assert!(second["next_cursor"].is_null());
}

#[tokio::test]
async fn category_filter_and_bad_input() {
    let site = mixed_site().await;
    let in_live = site
        .list(ListParams {
            category: Some("c-live".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(ids(&in_live), vec!["a1"]);
    let none = site
        .list(ListParams {
            category: Some("none".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(ids(&none), vec!["a6"]);
    let unknown_tag = site
        .list(ListParams {
            tag: Some("no-such-tag".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(ids(&unknown_tag).is_empty());
    assert_eq!(unknown_tag["total"], 0);

    for params in [
        ListParams {
            cursor: Some("not-a-cursor".into()),
            ..Default::default()
        },
        ListParams {
            limit: Some("0".into()),
            ..Default::default()
        },
    ] {
        assert!(matches!(
            site.list(params).await,
            Err(HeadlessError::BadRequest(_))
        ));
    }
}

async fn etag_of(site: &Site) -> String {
    get(
        &site.adapter,
        &site.config,
        &site.schemas,
        "article",
        "slug-a1",
        now(),
    )
    .await
    .unwrap()
    .etag
}

#[tokio::test]
async fn etag_changes_with_the_content() {
    let site = mixed_site().await;
    let first = etag_of(&site).await;
    assert_eq!(first, etag_of(&site).await, "stable while nothing changes");
    assert!(first.starts_with("W/\""));

    // 編集中の内容だけを保存しても、公開版は変わらない（ETag もそのまま）
    let mut draft_edit = record("article", "a1", 1, "published", Some("c-live"), None);
    draft_edit.title = "Edited but not published".to_string();
    site.adapter
        .update_content(&site.schemas["article"], &draft_edit)
        .await
        .unwrap();
    assert_eq!(first, etag_of(&site).await);
    let unchanged = site.get("article", "slug-a1").await.unwrap();
    assert_eq!(unchanged["title"], "Title a1");

    // 公開し直すと、公開版が変わって ETag も変わる
    let mut changed = draft_edit.clone();
    changed.updated_at = "2026-10-20T00:00:00Z".to_string();
    let version = ContentVersionRecord {
        content_type: changed.content_type.clone(),
        content_id: changed.id.clone(),
        revision: 0,
        snapshot_json: serde_json::to_string(&changed).unwrap(),
        created_at: changed.updated_at.clone(),
        created_by: None,
        change_note: None,
    };
    site.adapter
        .update_content_with_versions(
            &site.schemas["article"],
            &changed,
            &[version],
            ContentPublicationChange::PublishVersion(0),
        )
        .await
        .unwrap();
    assert_ne!(first, etag_of(&site).await);
    assert_eq!(
        site.get("article", "slug-a1").await.unwrap()["title"],
        "Edited but not published"
    );
}
