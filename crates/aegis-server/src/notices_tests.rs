use chrono::TimeZone;

use super::*;

const NOW: &str = "2026-11-01T00:00:00.000000Z";

fn input(message: &str) -> NoticeInput {
    NoticeInput {
        message: message.to_string(),
        link_url: String::new(),
        link_label: String::new(),
        style: "info".to_string(),
        start_at: None,
        end_at: None,
        dismissible: true,
        enabled: true,
    }
}

fn at(day: u32, hour: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 12, day, hour, 0, 0).unwrap()
}

fn add(list: &mut NoticeList, message: &str) -> Notice {
    let id = format!("id-{}", list.items().len() + 1);
    list.add(&input(message), id, NOW).unwrap()
}

#[test]
fn test_inputs_are_checked() {
    let invalid = |change: fn(&mut NoticeInput)| {
        let mut i = input("Hello");
        change(&mut i);
        match NoticeList::default().add(&i, "x".into(), NOW) {
            Err(NoticeError::Invalid { field, .. }) => field,
            other => panic!("expected an invalid input, got {:?}", other),
        }
    };
    assert_eq!(invalid(|i| i.message = "  ".into()), "message");
    assert_eq!(invalid(|i| i.message = "x".repeat(501)), "message");
    assert_eq!(invalid(|i| i.message = "two\nlines".into()), "message");
    assert_eq!(
        invalid(|i| i.link_url = "javascript:alert(1)".into()),
        "link_url"
    );
    assert_eq!(
        invalid(|i| i.link_url = "//evil.example".into()),
        "link_url"
    );
    assert_eq!(invalid(|i| i.link_url = "/a b".into()), "link_url");
    assert_eq!(invalid(|i| i.link_url = "example.com".into()), "link_url");
    assert_eq!(invalid(|i| i.link_label = "x".repeat(61)), "link_label");
    assert_eq!(invalid(|i| i.style = "rainbow".into()), "style");
    assert_eq!(
        invalid(|i| i.start_at = Some("tomorrow".into())),
        "start_at"
    );
    assert_eq!(invalid(|i| i.end_at = Some("2026-13-45".into())), "end_at");
    // the end has to come after the start
    assert_eq!(
        invalid(|i| {
            i.start_at = Some("2026-12-02T00:00:00Z".into());
            i.end_at = Some("2026-12-01T00:00:00Z".into());
        }),
        "end_at"
    );
    assert_eq!(
        invalid(|i| {
            i.start_at = Some("2026-12-01T00:00:00Z".into());
            i.end_at = Some("2026-12-01T00:00:00Z".into());
        }),
        "end_at"
    );
    // fine
    let mut ok = input("  Sale starts today!  ");
    ok.link_url = "https://example.com/sale".into();
    ok.link_label = " See the sale ".into();
    ok.style = "success".into();
    let notice = NoticeList::default().add(&ok, "x".into(), NOW).unwrap();
    assert_eq!(notice.message, "Sale starts today!");
    assert_eq!(notice.link_label, "See the sale");
}

#[test]
fn test_times_are_stored_in_utc() {
    let mut i = input("Hello");
    i.start_at = Some("2026-12-01T09:00:00+09:00".into());
    i.end_at = Some(" 2026-12-02T00:00:00Z ".into());
    let notice = NoticeList::default().add(&i, "x".into(), NOW).unwrap();
    assert_eq!(
        notice.start_at.as_deref(),
        Some("2026-12-01T00:00:00.000000Z")
    );
    assert_eq!(
        notice.end_at.as_deref(),
        Some("2026-12-02T00:00:00.000000Z")
    );
    // empty strings mean "no limit"
    i.start_at = Some(String::new());
    i.end_at = None;
    let open = NoticeList::default().add(&i, "x".into(), NOW).unwrap();
    assert_eq!((open.start_at, open.end_at), (None, None));
}

#[test]
fn test_status_follows_the_period_and_the_switch() {
    let mut list = NoticeList::default();
    let mut i = input("Hello");
    i.start_at = Some("2026-12-10T00:00:00Z".into());
    i.end_at = Some("2026-12-20T00:00:00Z".into());
    let notice = list.add(&i, "a".into(), NOW).unwrap();
    assert_eq!(notice.status(at(9, 23)), NoticeStatus::Scheduled);
    // the start is included, the end is not
    assert_eq!(notice.status(at(10, 0)), NoticeStatus::Active);
    assert_eq!(notice.status(at(19, 23)), NoticeStatus::Active);
    assert_eq!(notice.status(at(20, 0)), NoticeStatus::Expired);
    i.enabled = false;
    let off = list.update("a", &i, NOW).unwrap();
    assert_eq!(off.status(at(15, 0)), NoticeStatus::Disabled);
    // no period: always
    let always = add(&mut list, "Always");
    assert_eq!(always.status(at(1, 0)), NoticeStatus::Active);
}

#[test]
fn test_only_active_notices_are_shown_in_order_and_at_most_three() {
    let mut list = NoticeList::default();
    let first = add(&mut list, "first");
    let mut future = input("future");
    future.start_at = Some("2026-12-30T00:00:00Z".into());
    list.add(&future, "future".into(), NOW).unwrap();
    let mut off = input("off");
    off.enabled = false;
    list.add(&off, "off".into(), NOW).unwrap();
    add(&mut list, "second");
    add(&mut list, "third");
    add(&mut list, "fourth");
    let shown: Vec<&str> = list
        .active(at(1, 0))
        .iter()
        .map(|n| n.message.as_str())
        .collect();
    assert_eq!(shown, ["first", "second", "third"]);
    assert_eq!(first.message, "first");
    // when the first goes, the fourth takes its place
    list.remove(&first.id);
    let shown: Vec<&str> = list
        .active(at(1, 0))
        .iter()
        .map(|n| n.message.as_str())
        .collect();
    assert_eq!(shown, ["second", "third", "fourth"]);
    // and the scheduled one appears at its time
    let later: Vec<&str> = list
        .active(at(30, 0))
        .iter()
        .map(|n| n.message.as_str())
        .collect();
    assert_eq!(later, ["future", "second", "third"]);
}

#[test]
fn test_updating_changes_the_version_and_removing_works() {
    let mut list = NoticeList::default();
    let original = add(&mut list, "Hello");
    let updated = list
        .update(
            &original.id,
            &input("Hello again"),
            "2026-11-02T00:00:00.000000Z",
        )
        .unwrap();
    assert_eq!(updated.message, "Hello again");
    assert_eq!(updated.created_at, NOW);
    // a new version: visitors who closed it see it again
    assert_ne!(updated.rev(), original.rev());
    assert_eq!(
        list.update("missing", &input("x"), NOW),
        Err(NoticeError::NotFound)
    );
    // a bad update changes nothing
    assert!(list.update(&original.id, &input(""), NOW).is_err());
    assert_eq!(list.items()[0].message, "Hello again");
    assert!(list.remove(&original.id).is_some());
    assert!(list.remove(&original.id).is_none());
    assert!(list.items().is_empty());
}

#[test]
fn test_the_number_of_notices_is_limited() {
    let mut list = NoticeList::default();
    for i in 0..MAX_NOTICES {
        add(&mut list, &format!("n{}", i));
    }
    assert_eq!(
        list.add(&input("one more"), "x".into(), NOW),
        Err(NoticeError::TooMany)
    );
    assert_eq!(list.items().len(), MAX_NOTICES);
}

#[test]
fn test_the_head_tags_are_only_added_while_a_notice_is_shown() {
    let mut config = crate::config::AegisConfig::default();
    assert_eq!(head_html(&config, false), "");
    config.server.base_path = "/cms".to_string();
    let head = head_html(&config, true);
    let version = env!("CARGO_PKG_VERSION");
    assert!(head.contains(&format!("href=\"/cms/_aegis/notices.css?v={}\"", version)));
    assert!(head.contains(&format!(
        "src=\"/cms/_aegis/notices.js?v={}\" defer",
        version
    )));
}

#[test]
fn test_the_script_and_the_built_in_template_use_the_same_names() {
    let script = script();
    for name in [
        "data-aegis-notice",
        "data-aegis-notice-rev",
        "data-aegis-notice-close",
        "aegis_notices",
        "localStorage",
    ] {
        assert!(script.contains(name), "notices.js does not use {}", name);
    }
    assert!(style().contains(".aegis-notice"));
    // Every style the API accepts has a look
    for style_name in STYLES {
        assert!(
            style().contains(&format!("aegis-notice-{}", style_name)) || *style_name == "info",
            "no CSS for {}",
            style_name
        );
    }
}
