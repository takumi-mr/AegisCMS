//! `[theme].locale`: 組み込みのテンプレートがどれも、選んだ言語で描画されること

use std::collections::HashMap;
use std::path::Path;

use aegis_core::{CommentSection, ContentSummary, PageRenderContext, Pagination, PublicComment};
use aegis_theme::{messages, ThemeEngine};
use serde_json::{json, Value};

fn object(value: Value) -> HashMap<String, Value> {
    value.as_object().unwrap().clone().into_iter().collect()
}

fn item(slug: &str, author: Option<&str>) -> ContentSummary {
    ContentSummary {
        id: slug.to_string(),
        content_type: "article".to_string(),
        title: format!("Title of {}", slug),
        slug: slug.to_string(),
        excerpt: "An excerpt".to_string(),
        author_id: author.map(str::to_string),
        published_at: Some("2026-10-22T09:30:00Z".to_string()),
        created_at: "2026-10-22T09:30:00Z".to_string(),
        ..Default::default()
    }
}

fn pagination() -> Option<Pagination> {
    Some(Pagination {
        page: 2,
        per_page: 10,
        total_items: 25,
        total_pages: 3,
        prev_url: Some("/?page=1".to_string()),
        next_url: Some("/?page=3".to_string()),
    })
}

fn comments(open: bool) -> Option<CommentSection> {
    Some(CommentSection {
        content_id: "c1".to_string(),
        open,
        count: 2,
        items: vec![PublicComment {
            id: "k1".to_string(),
            author_name: "Alice".to_string(),
            body_html: "<p>Nice</p>".to_string(),
            created_at: "2026-10-22T09:30:00Z".to_string(),
            staff: true,
            ..Default::default()
        }],
        post_url: "/api/v1/contents/article/a/comments".to_string(),
        max_depth: 2,
        max_length: 2000,
    })
}

/// 分岐（記事なし・コメント欄が閉じている・通知など）ごとに 1 つずつ、全テンプレートを覆う描画の入力
fn contexts() -> Vec<PageRenderContext> {
    let theme_vars: HashMap<String, String> = [
        ("products_url", "/products"),
        ("cart_url", "/cart"),
        ("cart_script", "/shop/cart.js"),
        ("shop_api", "/api/v1/shop"),
        ("currency", "jpy"),
        ("shop_enabled", "true"),
        ("forum_url", "/forum"),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    let base = |template: &str| PageRenderContext {
        site_title: "Example".to_string(),
        template: template.to_string(),
        title: "A title".to_string(),
        slug: "a-slug".to_string(),
        content_type: "article".to_string(),
        theme_vars: theme_vars.clone(),
        ..Default::default()
    };
    let thread = json!({
        "id": "t1", "title": "A thread", "author_name": "Bob", "post_count": 3, "is_pinned": true,
        "is_locked": true, "category_slug": "general", "category_name": "General",
        "last_post_at": "2026-10-22T09:30:00Z", "created_at": "2026-10-22T09:30:00Z"
    });
    let category = json!({
        "slug": "general", "name": "General", "description": "Talk", "thread_count": 4, "post_count": 9
    });
    let post = json!({
        "id": "p1", "post_number": 1, "author_name": "Bob", "author_initial": "B",
        "is_author_staff": true, "created_at": "2026-10-22T09:30:00Z", "body_html": "<p>Hi</p>",
        "reply_to_number": 1
    });

    let mut out = Vec::new();
    // 記事一覧: 記事あり（作者あり・なし）/ 記事なし
    out.push(PageRenderContext {
        items: vec![item("a", Some("alice")), item("b", None)],
        pagination: pagination(),
        ..base("index.html")
    });
    out.push(base("index.html"));
    out.push(PageRenderContext {
        fields: object(json!({ "description": "About it" })),
        pagination: pagination(),
        ..base("category.html")
    });
    out.push(base("category.html"));
    // タグのページ・一覧
    out.push(PageRenderContext {
        fields: object(json!({ "description": "About it" })),
        items: vec![ContentSummary {
            fields: object(json!({ "url": "/articles/a" })),
            ..item("a", None)
        }],
        pagination: pagination(),
        ..base("tag.html")
    });
    out.push(base("tag.html"));
    out.push(PageRenderContext {
        items: vec![ContentSummary {
            title: "Rust".to_string(),
            fields: object(
                json!({ "url": "/tags/rust", "count": 3, "description": "The language" }),
            ),
            ..item("rust", None)
        }],
        ..base("tags.html")
    });
    out.push(base("tags.html"));
    out.push(PageRenderContext {
        fields: object(json!({ "query": "rust" })),
        items: vec![ContentSummary {
            fields: object(json!({ "url": "/articles/a", "snippet_html": "<mark>rust</mark>" })),
            ..item("a", None)
        }],
        pagination: pagination(),
        ..base("search.html")
    });
    out.push(PageRenderContext {
        fields: object(json!({ "query": "rust" })),
        pagination: pagination(),
        ..base("search.html")
    });
    out.push(PageRenderContext {
        author: None,
        published_at: Some("2026-10-22T09:30:00Z".to_string()),
        comments: comments(true),
        ..base("article.html")
    });
    out.push(PageRenderContext {
        comments: comments(false),
        ..base("page.html")
    });
    out.push(PageRenderContext {
        comments: Some(CommentSection {
            count: 0,
            items: Vec::new(),
            ..comments(true).unwrap()
        }),
        ..base("page.html")
    });
    // タグの付いた記事
    out.push(PageRenderContext {
        fields: object(
            json!({ "tags": [{ "name": "Rust", "slug": "rust", "url": "/tags/rust" }] }),
        ),
        ..base("article.html")
    });
    out.push(base("404.html"));
    // ショップ
    let product = |stock: Value| {
        object(json!({
            "id": "p1", "price": 1200, "sku": "SKU-1", "stock": stock, "excerpt": "Tasty",
            "buy_url": "https://example.com/buy"
        }))
    };
    out.push(PageRenderContext {
        items: vec![ContentSummary {
            fields: product(json!(0)),
            ..item("beans", None)
        }],
        pagination: pagination(),
        ..base("products.html")
    });
    out.push(base("products.html"));
    out.push(PageRenderContext {
        fields: product(json!(3)),
        comments: comments(true),
        ..base("product.html")
    });
    out.push(PageRenderContext {
        fields: product(json!(0)),
        ..base("product.html")
    });
    // ショップを使わず、外部の購入ページへ案内する商品
    let mut external = PageRenderContext {
        fields: product(json!(3)),
        ..base("product.html")
    };
    external
        .theme_vars
        .insert("shop_enabled".to_string(), "false".to_string());
    out.push(external);
    out.push(base("cart.html"));
    out.push(base("order.html"));
    // 掲示板
    out.push(PageRenderContext {
        fields: object(json!({ "categories": [category], "recent_threads": [thread] })),
        ..base("forum_index.html")
    });
    out.push(PageRenderContext {
        fields: object(json!({ "categories": [] , "recent_threads": [] })),
        ..base("forum_index.html")
    });
    out.push(PageRenderContext {
        fields: object(json!({
            "category": category, "threads": [thread], "pending_notice": true
        })),
        pagination: pagination(),
        ..base("forum_category.html")
    });
    out.push(PageRenderContext {
        fields: object(json!({ "category": category, "threads": [] })),
        ..base("forum_category.html")
    });
    for locked in [false, true] {
        let mut locked_thread = thread.clone();
        locked_thread["is_locked"] = json!(locked);
        out.push(PageRenderContext {
            fields: object(json!({
                "category": category, "thread": locked_thread, "posts": [post],
                "pending_notice": true, "reported_notice": true
            })),
            pagination: pagination(),
            ..base("forum_thread.html")
        });
    }
    out
}

/// `<script>` / `<style>` の中身を除いたマークアップ（スクリプトの識別子や CSS を、文言として数えない）
fn markup(html: &str) -> String {
    let mut out = html.to_string();
    for tag in ["script", "style"] {
        while let Some(start) = out.find(&format!("<{}", tag)) {
            let Some(end) = out[start..].find(&format!("</{}>", tag)) else {
                break;
            };
            out.replace_range(start..start + end + tag.len() + 3, "");
        }
    }
    out
}

fn engine(locale: &str) -> ThemeEngine {
    // 同梱テーマのテンプレートではなく、組み込みのテンプレートを描画する
    ThemeEngine::new(Path::new("non_existent_dir"), "default").with_locale(locale)
}

/// 文言に差し込み位置・マークアップが無く、2 つの言語で違うもの（出力の中の有無で言語を判定できる）
fn plain_texts() -> Vec<(&'static str, &'static str, &'static str)> {
    messages::all()
        .filter(|(key, en, ja)| {
            !key.starts_with("js.")
                && !key.starts_with("maintenance.")
                && en != ja
                && !en.contains(['{', '<'])
        })
        .collect()
}

#[test]
fn test_japanese_pages_have_no_english_built_in_text() {
    let (en_engine, ja_engine) = (engine("en"), engine("ja"));
    let mut used = std::collections::HashSet::new();
    for ctx in contexts() {
        let en = markup(&en_engine.render(&ctx.template, &ctx).unwrap());
        let ja = markup(&ja_engine.render(&ctx.template, &ctx).unwrap());
        assert!(en.contains(r#"<html lang="en">"#), "{}", ctx.template);
        assert!(ja.contains(r#"<html lang="ja">"#), "{}", ctx.template);
        for (key, en_text, ja_text) in plain_texts() {
            if en.contains(en_text) {
                used.insert(key);
                assert!(
                    !ja.contains(en_text),
                    "{}: \"{}\" ({}) is still English in the ja page",
                    ctx.template,
                    en_text,
                    key
                );
                assert!(
                    ja.contains(ja_text),
                    "{}: \"{}\" ({}) is missing in the ja page",
                    ctx.template,
                    ja_text,
                    key
                );
            }
        }
    }
    // どの文言も、どれかのテンプレートの描画に現れる（使われていない文言を残さない）
    for (key, _, _) in plain_texts() {
        assert!(
            used.contains(key),
            "{} is not used by any built-in template",
            key
        );
    }
}

#[test]
fn test_placeholders_and_plurals_in_pages() {
    let en_engine = engine("en");
    let ja_engine = engine("ja");
    let ctx = |n: u64| PageRenderContext {
        comments: Some(CommentSection {
            count: n,
            ..comments(true).unwrap()
        }),
        ..contexts()
            .into_iter()
            .find(|c| c.template == "article.html")
            .unwrap()
    };
    let en = en_engine.render("article.html", &ctx(1)).unwrap();
    assert!(en.contains("<h2>1 comment</h2>"), "{}", en);
    assert!(en.contains("Published on 2026-10-22"), "{}", en);
    let en = en_engine.render("article.html", &ctx(2)).unwrap();
    assert!(en.contains("<h2>2 comments</h2>"));
    let ja = ja_engine.render("article.html", &ctx(2)).unwrap();
    assert!(ja.contains("<h2>2 件のコメント</h2>"), "{}", ja);
    assert!(ja.contains("2026-10-22 に公開"), "{}", ja);
    // コメントの投稿スクリプトが使う文言は、フォームの属性で渡す（スクリプトに言語を書かない）
    assert!(ja.contains(r#"data-msg-sending="送信中…""#), "{}", ja);
    assert!(!ja.contains("Sending…"), "{}", ja);

    // 値は HTML エスケープされる
    let mut thread_ctx = contexts()
        .into_iter()
        .find(|c| c.template == "forum_thread.html")
        .unwrap();
    thread_ctx.fields.get_mut("thread").unwrap()["author_name"] =
        json!("<script>alert(1)</script>");
    let html = ja_engine.render("forum_thread.html", &thread_ctx).unwrap();
    assert!(
        html.contains("<strong>&lt;script&gt;alert(1)&lt;/script&gt;</strong> が開始"),
        "{}",
        html
    );
    assert!(!html.contains("<script>alert(1)"));
}

#[test]
fn test_unknown_locale_falls_back_to_english_and_keys_resolve() {
    let ctx = contexts().remove(0);
    let html = engine("fr").render(&ctx.template, &ctx).unwrap();
    assert!(html.contains(r#"<html lang="en">"#) && html.contains("Read story"));
}

#[test]
fn test_every_key_used_by_a_template_exists() {
    let source = include_str!("../src/builtins.rs");
    let mut found = 0;
    for part in source.split("t(\"").skip(1) {
        let key = part.split('"').next().unwrap();
        // `t("...")` 以外の `...t("` にも当たるので、キーの形のものだけを見る
        if !key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c == '.' || c == '_')
        {
            continue;
        }
        found += 1;
        let exists = messages::lookup("en", key).is_some()
            || messages::lookup("en", &format!("{}.one", key)).is_some();
        assert!(exists, "t(\"{}\") has no message", key);
    }
    assert!(found > 100, "found only {} keys", found);
}
