use super::*;

fn doc(content_type: &str, id: &str, title: &str, body: &str) -> SearchDocument {
    SearchDocument {
        content_type: content_type.into(),
        id: id.into(),
        slug: format!("slug-{}", id),
        title: title.into(),
        body: body.into(),
        url: format!("/{}s/slug-{}", content_type, id),
        published_at: Some("2026-10-01T00:00:00Z".into()),
    }
}

fn sample() -> SearchIndex {
    let index = SearchIndex::in_memory().unwrap();
    index
        .rebuild(&[
            doc(
                "article",
                "1",
                "東京都の天気",
                "今日の東京は晴れです。京都は雨。",
            ),
            doc(
                "article",
                "2",
                "京都の観光",
                "京都で寺を巡りました。Docker の話はありません。",
            ),
            doc(
                "page",
                "3",
                "AegisCMS のインストール",
                "Docker と Caddy で AegisCMS を動かします。",
            ),
            doc(
                "article",
                "4",
                "Release notes",
                "AegisCMSの使い方を説明します。",
            ),
        ])
        .unwrap();
    index
}

fn ids(results: &SearchResults) -> Vec<String> {
    results.hits.iter().map(|h| h.id.clone()).collect()
}

fn search(index: &SearchIndex, text: &str) -> SearchResults {
    index
        .search(&SearchQuery {
            text,
            limit: 10,
            ..Default::default()
        })
        .unwrap()
}

#[test]
fn test_japanese_phrase_matches_only_contiguous_text() {
    let index = sample();
    // 「東京都」は文書 1 のタイトルだけ（「京都」を含む文書 2 は一致しない）
    assert_eq!(ids(&search(&index, "東京都")), vec!["1"]);
    let kyoto = search(&index, "京都");
    assert_eq!(kyoto.total, 2);
    // タイトルに「京都」がある文書 2 が先
    assert_eq!(kyoto.hits[0].id, "2");
}

#[test]
fn test_single_cjk_character() {
    let index = sample();
    assert_eq!(ids(&search(&index, "寺")), vec!["2"]);
}

#[test]
fn test_terms_are_combined_with_and() {
    let index = sample();
    let mut found = ids(&search(&index, "docker aegiscms"));
    found.sort();
    assert_eq!(found, vec!["3"]);
    let mut found = ids(&search(&index, "Docker"));
    found.sort();
    assert_eq!(found, vec!["2", "3"]);
}

#[test]
fn test_mixed_script_phrase() {
    let index = sample();
    assert_eq!(ids(&search(&index, "AegisCMSの使い方")), vec!["4"]);
    assert_eq!(ids(&search(&index, "ＡＥＧＩＳＣＭＳ")).len(), 2);
}

#[test]
fn test_content_type_filter_and_paging() {
    let index = sample();
    let types = vec!["page".to_string()];
    let results = index
        .search(&SearchQuery {
            text: "aegiscms",
            content_types: &types,
            limit: 10,
            offset: 0,
        })
        .unwrap();
    assert_eq!(ids(&results), vec!["3"]);

    let page = index
        .search(&SearchQuery {
            text: "京都",
            limit: 1,
            offset: 1,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(page.total, 2);
    assert_eq!(page.hits.len(), 1);
}

#[test]
fn test_hit_fields_and_snippet() {
    let index = sample();
    let results = search(&index, "caddy");
    let hit = &results.hits[0];
    assert_eq!(hit.content_type, "page");
    assert_eq!(hit.slug, "slug-3");
    assert_eq!(hit.url, "/pages/slug-3");
    assert_eq!(hit.published_at.as_deref(), Some("2026-10-01T00:00:00Z"));
    assert_eq!(
        hit.snippet_html,
        "Docker と <mark>Caddy</mark> で AegisCMS を動かします。"
    );
}

#[test]
fn test_empty_and_symbol_only_queries_return_nothing() {
    let index = sample();
    assert_eq!(search(&index, "").total, 0);
    assert_eq!(search(&index, "  「」！？ ").total, 0);
}

#[test]
fn test_upsert_and_remove() {
    let index = sample();
    index
        .apply(&[
            SearchChange::Upsert(doc("article", "1", "大阪の天気", "曇り")),
            SearchChange::Remove {
                content_type: "page".into(),
                id: "3".into(),
            },
        ])
        .unwrap();
    assert_eq!(index.len(), 3);
    assert!(ids(&search(&index, "東京都")).is_empty());
    assert_eq!(ids(&search(&index, "大阪")), vec!["1"]);
    assert_eq!(ids(&search(&index, "caddy")), Vec::<String>::new());
}

#[test]
fn test_index_on_disk_survives_reopen() {
    let dir = tempfile::tempdir().unwrap();
    {
        let index = SearchIndex::open_in_dir(dir.path()).unwrap();
        index
            .rebuild(&[doc("article", "1", "永続化", "ディスクに残る")])
            .unwrap();
    }
    let index = SearchIndex::open_in_dir(dir.path()).unwrap();
    assert_eq!(index.len(), 1);
    assert_eq!(ids(&search(&index, "ディスク")), vec!["1"]);
}

#[test]
fn test_unreadable_index_dir_is_recreated() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("meta.json"), "not json").unwrap();
    let index = SearchIndex::open_in_dir(dir.path()).unwrap();
    assert!(index.is_empty());
}
