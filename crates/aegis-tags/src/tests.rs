use aegis_db::SqliteAdapter;

use super::*;

async fn service(max_per_content: usize) -> TagService {
    let db = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
    db.init().await.unwrap();
    TagService::new(
        Arc::new(db),
        TagsConfig {
            enabled: true,
            max_per_content,
        },
    )
}

fn input(name: &str, slug: Option<&str>) -> TagInput {
    TagInput {
        name: name.to_string(),
        slug: slug.map(str::to_string),
        description: None,
    }
}

#[test]
fn test_slugify_and_validation() {
    assert_eq!(slugify("Rust Lang"), "rust-lang");
    assert_eq!(slugify("  C++ / Go  "), "c-go");
    assert_eq!(slugify("日本語 Docker"), "docker");
    assert_eq!(slugify("日本語"), "");
    assert!(is_valid_slug("rust-lang"));
    assert!(is_valid_slug("v2"));
    for bad in ["", "-a", "a-", "a--b", "Rust", "a b", "日本語"] {
        assert!(!is_valid_slug(bad), "{:?}", bad);
    }
    assert!(!is_valid_slug(&"a".repeat(65)));
}

#[tokio::test]
async fn test_create_makes_slugs_and_rejects_duplicates() {
    let tags = service(20).await;
    let rust = tags.create(input("  Rust  Lang ", None)).await.unwrap();
    assert_eq!(rust.name, "Rust Lang");
    assert_eq!(rust.slug, "rust-lang");

    // 日本語だけの名前は、乱数つきの slug
    let japanese = tags.create(input("日本語", None)).await.unwrap();
    assert!(japanese.slug.starts_with("tag-"), "{}", japanese.slug);
    assert!(is_valid_slug(&japanese.slug));

    // 同じ名前（大文字小文字違い）と同じ slug は 409
    assert!(matches!(
        tags.create(input("rust lang", Some("other"))).await,
        Err(TagError::Conflict(_))
    ));
    assert!(matches!(
        tags.create(input("Another", Some("rust-lang"))).await,
        Err(TagError::Conflict(_))
    ));

    for (name, slug) in [("", None), ("x", Some("Bad Slug")), ("y\u{0007}", None)] {
        assert!(matches!(
            tags.create(input(name, slug)).await,
            Err(TagError::Invalid(_))
        ));
    }
    assert!(matches!(
        tags.create(input(&"n".repeat(51), None)).await,
        Err(TagError::Invalid(_))
    ));
}

#[tokio::test]
async fn test_update_keeps_the_slug_unless_given() {
    let tags = service(20).await;
    let a = tags.create(input("Alpha", None)).await.unwrap();
    let b = tags.create(input("Beta", None)).await.unwrap();

    let renamed = tags.update(&a.id, input("Alpha 2", None)).await.unwrap();
    assert_eq!(renamed.name, "Alpha 2");
    assert_eq!(renamed.slug, "alpha");

    let moved = tags
        .update(
            &a.id,
            TagInput {
                name: "Alpha 2".to_string(),
                slug: Some("first".to_string()),
                description: Some("  The first  ".to_string()),
            },
        )
        .await
        .unwrap();
    assert_eq!(moved.slug, "first");
    assert_eq!(moved.description.as_deref(), Some("The first"));

    // 自分の slug・名前のままなら衝突しない。他のタグの slug・名前は 409
    tags.update(&a.id, input("Alpha 2", Some("first")))
        .await
        .unwrap();
    assert!(matches!(
        tags.update(&b.id, input("beta", Some("first"))).await,
        Err(TagError::Conflict(_))
    ));
    assert!(matches!(
        tags.update(&b.id, input("ALPHA 2", None)).await,
        Err(TagError::Conflict(_))
    ));
    assert!(matches!(
        tags.update("missing", input("x", None)).await,
        Err(TagError::NotFound(_))
    ));
}

#[tokio::test]
async fn test_set_content_tags_validates_and_dedupes() {
    let tags = service(2).await;
    let a = tags.create(input("Alpha", None)).await.unwrap();
    let b = tags.create(input("Beta", None)).await.unwrap();
    let c = tags.create(input("Gamma", None)).await.unwrap();

    let set = tags
        .set_content_tags("article", "a1", &[b.id.clone(), a.id.clone(), b.id.clone()])
        .await
        .unwrap();
    assert_eq!(
        set.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
        vec!["Alpha", "Beta"]
    );

    // 上限（重複を除いて数える）と、存在しないタグ
    assert!(matches!(
        tags.set_content_tags("article", "a1", &[a.id.clone(), b.id.clone(), c.id.clone()])
            .await,
        Err(TagError::Invalid(_))
    ));
    assert!(matches!(
        tags.set_content_tags("article", "a1", &["missing".to_string()])
            .await,
        Err(TagError::Invalid(_))
    ));
    // 失敗しても、元のタグのまま
    assert_eq!(tags.content_tags("article", "a1").await.unwrap().len(), 2);

    // 別の種類のコンテンツにも付けられる。タグを消すと外れる
    tags.set_content_tags("page", "a1", &[a.id.clone()])
        .await
        .unwrap();
    tags.delete(&a.id).await.unwrap();
    assert_eq!(tags.content_tags("article", "a1").await.unwrap().len(), 1);
    assert!(tags.content_tags("page", "a1").await.unwrap().is_empty());

    tags.forget_content("article", "a1").await.unwrap();
    assert!(tags.content_tags("article", "a1").await.unwrap().is_empty());
    assert!(matches!(
        tags.delete(&a.id).await,
        Err(TagError::NotFound(_))
    ));
}

#[test]
fn test_config_validation() {
    assert!(TagsConfig::default().validate().is_ok());
    for bad in [0, 101] {
        let config = TagsConfig {
            max_per_content: bad,
            ..Default::default()
        };
        assert!(config.validate().is_err(), "{}", bad);
    }
}
