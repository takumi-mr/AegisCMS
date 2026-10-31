use super::*;
use aegis_cache::MemoryCacheBackend;
use aegis_db::SqliteAdapter;
use chrono::TimeZone;

const DESKTOP: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/120 Safari/537.36";
const IPHONE: &str =
    "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) Mobile/15E148 Safari/604.1";

async fn service(config: AnalyticsConfig) -> AnalyticsService {
    let db = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
    db.init().await.unwrap();
    AnalyticsService::new(config, Arc::new(db), Arc::new(MemoryCacheBackend::new()))
}

fn at(day: u32, hour: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, day, hour, 0, 0).unwrap()
}

fn view<'a>(path: &'a str, referrer: &'a str, ua: &'a str, ip: &'a str) -> PageView<'a> {
    view_with_query(path, "", referrer, ua, ip)
}

fn view_with_query<'a>(
    path: &'a str,
    query: &'a str,
    referrer: &'a str,
    ua: &'a str,
    ip: &'a str,
) -> PageView<'a> {
    PageView {
        path,
        query,
        referrer,
        host: "example.com",
        user_agent: ua,
        ip,
    }
}

fn total(value: &str, pageviews: u64, visitors: u64) -> AnalyticsTotal {
    AnalyticsTotal {
        value: value.to_string(),
        pageviews,
        visitors,
    }
}

#[tokio::test]
async fn test_counts_pageviews_and_daily_visitors() {
    let service = service(AnalyticsConfig::default()).await;
    // 訪問者 A: 検索から / に来て /about を見て、また / に戻る
    let a = ("203.0.113.1", DESKTOP);
    assert_eq!(
        service
            .track(
                view("/", "https://www.google.com/search?q=x", a.1, a.0),
                at(1, 1)
            )
            .await,
        TrackOutcome::Counted
    );
    service
        .track(view("/about", "https://example.com/", a.1, a.0), at(1, 2))
        .await;
    service
        .track(
            view("/?utm=1", "https://example.com/about", a.1, a.0),
            at(1, 3),
        )
        .await;
    // 訪問者 B: スマートフォンで直接 /about
    service
        .track(view("/about", "", IPHONE, "198.51.100.2"), at(1, 4))
        .await;
    // 翌日の A は別の訪問者として数える
    service.track(view("/", "", a.1, a.0), at(2, 1)).await;
    // クローラーは数えない
    assert_eq!(
        service
            .track(view("/", "", "Googlebot/2.1", "66.249.66.1"), at(1, 5))
            .await,
        TrackOutcome::Bot
    );
    assert_eq!(
        service
            .track(
                view("https://evil.example/", "", DESKTOP, "203.0.113.9"),
                at(1, 5)
            )
            .await,
        TrackOutcome::Excluded
    );

    let summary = service
        .summary("2026-10-01", "2026-10-03", 10)
        .await
        .unwrap();
    assert_eq!(summary.pageviews, 5);
    assert_eq!(summary.visitors, 3);
    assert_eq!(
        summary.daily,
        vec![
            DailyPoint {
                day: "2026-10-01".into(),
                pageviews: 4,
                visitors: 2
            },
            DailyPoint {
                day: "2026-10-02".into(),
                pageviews: 1,
                visitors: 1
            },
            DailyPoint {
                day: "2026-10-03".into(),
                pageviews: 0,
                visitors: 0
            },
        ]
    );
    assert_eq!(
        summary.top_pages,
        vec![total("/", 3, 2), total("/about", 2, 2)]
    );
    assert_eq!(
        summary.top_referrers,
        vec![total("", 2, 2), total("google.com", 1, 1)]
    );
    assert_eq!(
        summary.devices,
        vec![total("desktop", 4, 2), total("mobile", 1, 1)]
    );
}

#[tokio::test]
async fn test_excluded_paths_and_flush_accumulates() {
    let service = service(AnalyticsConfig {
        exclude_paths: vec!["/preview".to_string()],
        ..AnalyticsConfig::default()
    })
    .await;
    assert_eq!(
        service
            .track(view("/preview/x", "", DESKTOP, "203.0.113.1"), at(1, 1))
            .await,
        TrackOutcome::Excluded
    );
    service
        .track(view("/", "", DESKTOP, "203.0.113.1"), at(1, 1))
        .await;
    service.flush().await;
    service
        .track(view("/", "", DESKTOP, "203.0.113.1"), at(1, 2))
        .await;
    service.flush().await;
    let summary = service
        .summary("2026-10-01", "2026-10-01", 10)
        .await
        .unwrap();
    assert_eq!((summary.pageviews, summary.visitors), (2, 1));
}

#[tokio::test]
async fn test_summary_validates_the_range() {
    let service = service(AnalyticsConfig::default()).await;
    assert_eq!(
        service.summary("2026/10/01", "2026-10-02", 10).await,
        Err(SummaryError::InvalidDate)
    );
    assert_eq!(
        service.summary("2026-10-02", "2026-10-01", 10).await,
        Err(SummaryError::InvalidRange)
    );
    assert_eq!(
        service.summary("2025-01-01", "2026-10-01", 10).await,
        Err(SummaryError::RangeTooLong)
    );
}

#[tokio::test]
async fn test_retention_deletes_old_days() {
    let service = service(AnalyticsConfig {
        retention_days: 10,
        ..AnalyticsConfig::default()
    })
    .await;
    service
        .track(view("/", "", DESKTOP, "203.0.113.1"), at(1, 1))
        .await;
    service
        .track(view("/", "", DESKTOP, "203.0.113.1"), at(20, 1))
        .await;
    service.flush().await;
    service.apply_retention(at(25, 0)).await;
    let summary = service
        .summary("2026-10-01", "2026-10-31", 10)
        .await
        .unwrap();
    assert_eq!(summary.pageviews, 1);
}

#[test]
fn test_beacon_script_escapes_the_endpoint() {
    let script = beacon_script("/cms/api/v1/analytics/event");
    assert!(script.contains(r#"u="/cms/api/v1/analytics/event""#));
    assert!(script.contains("doNotTrack"));
    assert!(script.contains("globalPrivacyControl"));
    let hostile = beacon_script("</script><script>alert(1)");
    assert_eq!(hostile.matches("</script>").count(), 1);
}

#[tokio::test]
async fn test_referrer_visitors_are_counted_per_referrer() {
    let service = service(AnalyticsConfig::default()).await;
    let ip = "203.0.113.1";
    // 直接来て、後で検索から来て、もう一度検索から来た
    service.track(view("/", "", DESKTOP, ip), at(1, 1)).await;
    service
        .track(view("/a", "https://www.google.com/", DESKTOP, ip), at(1, 2))
        .await;
    service
        .track(view("/b", "https://google.com/", DESKTOP, ip), at(1, 3))
        .await;
    let summary = service
        .summary("2026-10-01", "2026-10-01", 10)
        .await
        .unwrap();
    assert_eq!(
        summary.top_referrers,
        vec![total("google.com", 2, 1), total("", 1, 1)]
    );
    assert_eq!(summary.visitors, 1);
}

#[tokio::test]
async fn test_utm_tracking_comparison_and_csv() {
    let service = service(AnalyticsConfig::default()).await;
    // 前期間: 2026-10-01 〜 2026-10-07 (7日間)
    // 訪問者 1 人、PV 2
    service
        .track(view("/", "", DESKTOP, "203.0.113.1"), at(5, 10))
        .await;
    service
        .track(view("/about", "", DESKTOP, "203.0.113.1"), at(5, 11))
        .await;

    // 現在期間: 2026-10-08 〜 2026-10-14 (7日間)
    // UTM 付きで訪問者 2 人、PV 4
    service
        .track(
            view_with_query(
                "/product",
                "utm_source=twitter&utm_medium=social&utm_campaign=sale2026",
                "https://t.co/",
                DESKTOP,
                "203.0.113.2",
            ),
            at(10, 10),
        )
        .await;
    service
        .track(
            view_with_query(
                "/product",
                "utm_source=twitter&utm_campaign=sale2026",
                "",
                DESKTOP,
                "203.0.113.2",
            ),
            at(10, 11),
        )
        .await;
    service
        .track(
            view_with_query("/blog", "utm_source=newsletter", "", IPHONE, "203.0.113.3"),
            at(11, 10),
        )
        .await;
    service
        .track(view("/", "", IPHONE, "203.0.113.3"), at(11, 12))
        .await;

    let summary = service
        .summary("2026-10-08", "2026-10-14", 10)
        .await
        .unwrap();

    assert_eq!(summary.pageviews, 4);
    assert_eq!(summary.visitors, 2);

    // UTM パラメータ集計確認
    assert_eq!(
        summary.top_utm_sources,
        vec![total("twitter", 2, 1), total("newsletter", 1, 1)]
    );
    assert_eq!(summary.top_utm_campaigns, vec![total("sale2026", 2, 1)]);
    assert_eq!(summary.top_utm_mediums, vec![total("social", 1, 1)]);

    // 前期間比較確認 (前期間は 2026-10-01 〜 2026-10-07: PV 2, 訪問者 1)
    let prev = summary.previous.as_ref().expect("previous comparison");
    assert_eq!(prev.from, "2026-10-01");
    assert_eq!(prev.to, "2026-10-07");
    assert_eq!(prev.pageviews, 2);
    assert_eq!(prev.visitors, 1);
    // 変化率: PV 2 -> 4 (+100.0%), Visitors 1 -> 2 (+100.0%)
    assert_eq!(prev.pageviews_change_pct, Some(100.0));
    assert_eq!(prev.visitors_change_pct, Some(100.0));

    // CSV エクスポート確認
    let csv = export_summary_csv(&summary);
    assert!(csv.starts_with('\u{FEFF}'));
    assert!(csv.contains("Total Pageviews,4"));
    assert!(csv.contains("Previous Pageviews,2"));
    assert!(csv.contains("Pageviews Change,100%"));
    assert!(csv.contains("twitter,2,1"));
    assert!(csv.contains("sale2026,2,1"));
}
