use super::*;

const NOW: &str = "2026-10-31T00:00:00.000000Z";

fn reserved() -> Vec<String> {
    vec!["/api".into(), "/uploads".into(), "/admin".into()]
}

fn input(from: &str, to: &str) -> RuleInput {
    RuleInput {
        from: from.to_string(),
        to: to.to_string(),
        status: None,
    }
}

fn add(
    table: &mut RedirectTable,
    from: &str,
    to: &str,
) -> std::result::Result<RedirectRule, RedirectError> {
    let id = format!("id-{}", table.rules().len() + 1);
    table.add(&input(from, to), &reserved(), id, NOW)
}

#[test]
fn test_canonical_path_decodes_and_trims() {
    assert_eq!(canonical_path("/articles/old/"), "/articles/old");
    assert_eq!(canonical_path("/"), "/");
    assert_eq!(canonical_path("//"), "/");
    assert_eq!(canonical_path("/%E8%A8%98%E4%BA%8B"), "/記事");
    assert_eq!(canonical_path("/a%2"), "/a%2");
    // UTF-8 にならない列はそのまま
    assert_eq!(canonical_path("/%FF"), "/%FF");
}

#[test]
fn test_normalize_from() {
    assert_eq!(
        normalize_from(" /old-page/ ", &reserved()).unwrap(),
        "/old-page"
    );
    assert_eq!(
        normalize_from("/%E8%A8%98%E4%BA%8B", &reserved()).unwrap(),
        "/記事"
    );
    for bad in [
        "",
        "old",
        "//evil.example",
        "/",
        "/a?b=1",
        "/a#frag",
        "/a b",
        "/a/../b",
        "/a/./b",
        "/api",
        "/api/v1/contents",
        "/uploads/x.png",
        "/admin",
        "/admin/assets/a.js",
    ] {
        assert!(
            matches!(
                normalize_from(bad, &reserved()),
                Err(RedirectError::Invalid { field: "from", .. })
            ),
            "{:?}",
            bad
        );
    }
    // 先頭が似ているだけのパスは予約ではない
    assert!(normalize_from("/apiary", &reserved()).is_ok());
    assert!(normalize_from("/administrators", &reserved()).is_ok());
    assert!(normalize_from(&format!("/{}", "a".repeat(600)), &reserved()).is_err());
}

#[test]
fn test_validate_to() {
    for ok in [
        "/articles/new",
        "/articles/new?ref=old#top",
        "https://example.com/new",
        "http://example.com",
    ] {
        assert_eq!(validate_to(ok).unwrap(), ok);
    }
    for bad in [
        "",
        "new",
        "//evil.example/x",
        "/a\\b",
        "javascript:alert(1)",
        "ftp://example.com",
        "https://",
        "https:///path",
        "https://user@example.com/",
        "/a b",
        "/a\nb",
    ] {
        assert!(
            matches!(
                validate_to(bad),
                Err(RedirectError::Invalid { field: "to", .. })
            ),
            "{:?}",
            bad
        );
    }
}

#[test]
fn test_status_is_301_or_302() {
    let mut table = RedirectTable::default();
    let mut temporary = input("/a", "/b");
    temporary.status = Some(302);
    assert_eq!(
        table
            .add(&temporary, &reserved(), "1".into(), NOW)
            .unwrap()
            .status,
        302
    );
    assert_eq!(add(&mut table, "/c", "/d").unwrap().status, 301);
    let mut bad = input("/e", "/f");
    bad.status = Some(307);
    assert!(matches!(
        table.add(&bad, &reserved(), "3".into(), NOW),
        Err(RedirectError::Invalid {
            field: "status",
            ..
        })
    ));
}

#[test]
fn test_resolve_matches_decoded_paths_without_trailing_slash() {
    let mut table = RedirectTable::default();
    add(&mut table, "/old", "/new").unwrap();
    add(&mut table, "/記事", "/articles/x").unwrap();
    assert_eq!(table.resolve("/old").unwrap().to, "/new");
    assert_eq!(table.resolve("/old/").unwrap().to, "/new");
    assert_eq!(
        table.resolve("/%E8%A8%98%E4%BA%8B").unwrap().to,
        "/articles/x"
    );
    assert!(table.resolve("/older").is_none());
    assert!(table.resolve("/").is_none());
}

#[test]
fn test_duplicates_and_loops_are_refused() {
    let mut table = RedirectTable::default();
    add(&mut table, "/a", "/b").unwrap();
    assert_eq!(add(&mut table, "/a/", "/c"), Err(RedirectError::Duplicate));
    // 自分自身・たどると戻る
    assert_eq!(add(&mut table, "/x", "/x"), Err(RedirectError::Loop));
    assert_eq!(add(&mut table, "/x", "/x?ref=1"), Err(RedirectError::Loop));
    assert_eq!(add(&mut table, "/b", "/a"), Err(RedirectError::Loop));
    add(&mut table, "/b", "/c").unwrap();
    assert_eq!(add(&mut table, "/c", "/a"), Err(RedirectError::Loop));
    // 外部の URL は戻ってこない
    add(&mut table, "/c", "https://example.com/a").unwrap();
}

#[test]
fn test_update_changes_the_rule_and_checks_again() {
    let mut table = RedirectTable::default();
    let a = add(&mut table, "/a", "/b").unwrap();
    add(&mut table, "/c", "/d").unwrap();
    let mut change = input("/a", "/e");
    change.status = Some(302);
    let updated = table.update(&a.id, &change, &reserved(), "later").unwrap();
    assert_eq!((updated.to.as_str(), updated.status), ("/e", 302));
    assert_eq!(updated.updated_at, "later");
    assert_eq!(updated.created_at, NOW);
    // 別の規則の from にはできない。自分の from のままなら重複ではない
    assert_eq!(
        table.update(&a.id, &input("/c", "/e"), &reserved(), NOW),
        Err(RedirectError::Duplicate)
    );
    // 自分の規則は無いものとしてループを調べる（/a → /b を /a → /a にはできないが、/a → /c は可）
    assert_eq!(
        table.update(&a.id, &input("/a", "/a"), &reserved(), NOW),
        Err(RedirectError::Loop)
    );
    assert_eq!(
        table.update("missing", &input("/z", "/y"), &reserved(), NOW),
        Err(RedirectError::NotFound)
    );
}

#[test]
fn test_remove() {
    let mut table = RedirectTable::default();
    let a = add(&mut table, "/a", "/b").unwrap();
    assert_eq!(table.remove(&a.id).unwrap().from, "/a");
    assert!(table.remove(&a.id).is_none());
    assert!(table.rules().is_empty());
}

#[test]
fn test_auto_rules_do_not_chain_and_clear_live_urls() {
    let mut table = RedirectTable::default();
    // /one → /two（自動）。その後 /two を /three に変えた: /one も /three へ付け替える
    table.add_auto("/one", "/two", "1".into(), NOW).unwrap();
    table
        .add_auto("/two", "/three", "2".into(), "later")
        .unwrap();
    let rules: Vec<(&str, &str, bool)> = table
        .rules()
        .iter()
        .map(|r| (r.from.as_str(), r.to.as_str(), r.auto))
        .collect();
    assert_eq!(rules, [("/one", "/three", true), ("/two", "/three", true)]);
    assert_eq!(table.resolve("/one").unwrap().updated_at, "later");

    // 元の slug に戻した（/three → /one）: 今は生きている /one を from にする規則は消え、/two は /one へ
    table.add_auto("/three", "/one", "3".into(), NOW).unwrap();
    let rules: Vec<(&str, &str)> = table
        .rules()
        .iter()
        .map(|r| (r.from.as_str(), r.to.as_str()))
        .collect();
    assert_eq!(rules, [("/two", "/one"), ("/three", "/one")]);

    // 同じ URL・ルートには作らない
    assert!(table.add_auto("/same", "/same/", "4".into(), NOW).is_none());
    assert!(table.add_auto("/", "/x", "5".into(), NOW).is_none());
}

#[test]
fn test_auto_rule_replaces_a_manual_rule_for_the_same_path() {
    let mut table = RedirectTable::default();
    add(&mut table, "/old", "https://example.com/elsewhere").unwrap();
    table.add_auto("/old", "/new", "2".into(), NOW).unwrap();
    assert_eq!(table.rules().len(), 1);
    assert_eq!(table.resolve("/old").unwrap().to, "/new");
}

#[test]
fn test_the_limit_drops_the_oldest_auto_rule_but_never_a_manual_one() {
    let rules = (0..MAX_RULES)
        .map(|i| RedirectRule {
            id: format!("m{}", i),
            from: format!("/m{}", i),
            to: "/x".to_string(),
            status: 301,
            auto: false,
            created_at: NOW.to_string(),
            updated_at: NOW.to_string(),
        })
        .collect();
    let mut table = RedirectTable { rules };
    assert_eq!(add(&mut table, "/extra", "/y"), Err(RedirectError::TooMany));
    // 手の規則だけで満杯: 自動の規則は足さない
    assert!(table.add_auto("/p", "/q", "a".into(), NOW).is_none());
    assert_eq!(table.rules().len(), MAX_RULES);

    // 自動の規則が混ざっていれば、いちばん古いものを消して入れる
    let mut rules = table.rules().to_vec();
    rules[10].auto = true;
    rules[10].created_at = "2020-01-01T00:00:00.000000Z".to_string();
    rules[20].auto = true;
    let mut table = RedirectTable { rules };
    table.add_auto("/p", "/q", "a".into(), NOW).unwrap();
    assert_eq!(table.rules().len(), MAX_RULES);
    assert!(table.resolve("/m10").is_none());
    assert!(table.resolve("/m20").is_some());
    assert!(table.resolve("/p").is_some());
}

#[test]
fn test_encode_location() {
    assert_eq!(
        encode_location("/articles/new?a=1&b=2#top"),
        "/articles/new?a=1&b=2#top"
    );
    assert_eq!(encode_location("/記事"), "/%E8%A8%98%E4%BA%8B");
    assert_eq!(encode_location("/a%20b"), "/a%20b");
    assert_eq!(encode_location("/a\"b<c>"), "/a%22b%3Cc%3E");
}

#[test]
fn test_public_paths_by_content_type() {
    let config = crate::config::AegisConfig::default();
    assert_eq!(
        public_paths(&config, "article", "hello"),
        ["/articles/hello", "/hello"]
    );
    assert_eq!(public_paths(&config, "page", "about"), ["/about"]);
    assert_eq!(
        public_paths(&config, &config.commerce.product_type, "mug"),
        ["/products/mug"]
    );
    assert!(public_paths(&config, "category", "news").is_empty());
}

#[test]
fn test_reserved_prefixes_include_the_admin_path() {
    let mut config = crate::config::AegisConfig::default();
    config.server.admin_path = "/studio".to_string();
    let reserved = reserved_prefixes(&config);
    assert!(reserved.contains(&"/api".to_string()));
    assert!(reserved.contains(&"/studio".to_string()));
}
