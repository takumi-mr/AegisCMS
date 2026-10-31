//! アクセス解析（`[analytics]`）: Cookie を使わない、個人を追跡しないページビューの集計。
//! HTTP 層（ビーコンの受け口・集計の API）はサーバー側にあり、ここは HTTP に依存しない
//!
//! - 公開ページに小さなビーコン（[`beacon_script`]）を入れ、表示されたページのパスと参照元を送ってもらう。
//!   ブラウザの Do Not Track・Global Privacy Control が有効なら送らない
//! - 訪問者は「その日の秘密の値（毎日作り直し、キャッシュにだけ置く）＋ IP アドレス＋ User-Agent」のハッシュで
//!   その日の中だけ見分ける。日をまたぐと同じ人とは分からない。IP アドレス・ハッシュは DB に保存しない
//! - DB（`_aegis_analytics`）には日ごとの集計（サイト全体・ページ・参照元のホスト・端末の種類ごとの
//!   ページビュー数と訪問者数）だけを残す。集計はメモリで溜め、一定の間隔でまとめて書く

pub mod agent;
pub mod config;

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aegis_core::CacheBackend;
use aegis_db::{AnalyticsCount, AnalyticsTotal, DatabaseAdapter};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub use config::AnalyticsConfig;

/// 集計の切り口
pub const DIMENSION_SITE: &str = "site";
pub const DIMENSION_PATH: &str = "path";
pub const DIMENSION_REFERRER: &str = "referrer";
pub const DIMENSION_DEVICE: &str = "device";
pub const DIMENSION_UTM_SOURCE: &str = "utm_source";
pub const DIMENSION_UTM_CAMPAIGN: &str = "utm_campaign";
pub const DIMENSION_UTM_MEDIUM: &str = "utm_medium";
pub const DIMENSION_SEARCH_QUERY: &str = "search_query";
pub const DIMENSION_SEARCH_EMPTY: &str = "search_empty";

/// 訪問者の見分けに使うキャッシュのキーを残す時間（日付が変わってからも少し残す）
const VISITOR_TTL_SECS: u64 = 2 * 24 * 60 * 60;
/// 集計の期間の上限（日数）
pub const MAX_RANGE_DAYS: i64 = 366;

/// 公開ページの `<head>` に入れるビーコン。`endpoint` は受け口の URL（`/api/v1/analytics/event` に
/// `[server].base_path` を付けたもの）
pub fn beacon_script(endpoint: &str) -> String {
    // `</script>` で閉じられないよう、JSON の文字列にしてから `<` を逃がす
    let endpoint = serde_json::to_string(endpoint)
        .unwrap_or_else(|_| "\"\"".to_string())
        .replace('<', "\\u003c");
    format!(
        "<script id=\"aegis-analytics\">(function(){{var n=navigator,w=window;\
         if(n.doNotTrack===\"1\"||w.doNotTrack===\"1\"||n.globalPrivacyControl||n.webdriver)return;\
         var d=JSON.stringify({{p:location.pathname,r:document.referrer}}),u={endpoint};\
         if(n.sendBeacon){{n.sendBeacon(u,new Blob([d],{{type:\"text/plain\"}}))}}\
         else{{fetch(u,{{method:\"POST\",body:d,keepalive:true}})}}}})();</script>"
    )
}

/// ビーコンで届いた 1 回のページ表示
#[derive(Debug, Clone, Default)]
pub struct PageView<'a> {
    /// 表示したページのパス（`location.pathname`）
    pub path: &'a str,
    /// ページのクエリ文字列（`location.search`）
    pub query: &'a str,
    /// `document.referrer`
    pub referrer: &'a str,
    /// リクエストの `Host`（自分のサイトからの移動を参照元から除くため）
    pub host: &'a str,
    pub user_agent: &'a str,
    /// 接続元の IP アドレス（ハッシュの材料にするだけで保存しない）
    pub ip: &'a str,
}

/// [`AnalyticsService::track`] の結果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackOutcome {
    Counted,
    /// クローラーなど
    Bot,
    /// 集計しないパス（`[analytics].exclude_paths`・読めないパス）
    Excluded,
}

type Key = (String, &'static str, String);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DailyPoint {
    pub day: String,
    pub pageviews: u64,
    pub visitors: u64,
}

/// 前期間との比較集計
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AnalyticsComparison {
    pub from: String,
    pub to: String,
    pub pageviews: u64,
    pub visitors: u64,
    pub pageviews_change_pct: Option<f64>,
    pub visitors_change_pct: Option<f64>,
}

/// 期間の集計
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AnalyticsSummary {
    pub from: String,
    pub to: String,
    pub pageviews: u64,
    /// 日ごとの訪問者数の合計（日をまたいだ同じ人は別に数える）
    pub visitors: u64,
    /// 期間のすべての日（記録の無い日は 0）
    pub daily: Vec<DailyPoint>,
    pub top_pages: Vec<AnalyticsTotal>,
    /// ほかのサイトからの入口のページ表示（`value` は参照元のホスト。空文字列なら直接の訪問・参照元なし）
    pub top_referrers: Vec<AnalyticsTotal>,
    pub devices: Vec<AnalyticsTotal>,
    pub top_utm_sources: Vec<AnalyticsTotal>,
    pub top_utm_campaigns: Vec<AnalyticsTotal>,
    pub top_utm_mediums: Vec<AnalyticsTotal>,
    pub top_search_queries: Vec<AnalyticsTotal>,
    pub top_search_empty: Vec<AnalyticsTotal>,
    pub previous: Option<AnalyticsComparison>,
}

pub struct AnalyticsService {
    config: AnalyticsConfig,
    db: Arc<dyn DatabaseAdapter>,
    cache: Arc<dyn CacheBackend>,
    pending: Mutex<HashMap<Key, (u64, u64)>>,
    /// その日の秘密の値（日付と組で持つ）
    salt: tokio::sync::Mutex<Option<(String, String)>>,
}

fn short_hash(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0u8]);
    }
    hex::encode(&hasher.finalize()[..12])
}

fn parse_day(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d").ok()
}

impl AnalyticsService {
    pub fn new(
        config: AnalyticsConfig,
        db: Arc<dyn DatabaseAdapter>,
        cache: Arc<dyn CacheBackend>,
    ) -> Self {
        Self {
            config,
            db,
            cache,
            pending: Mutex::new(HashMap::new()),
            salt: tokio::sync::Mutex::new(None),
        }
    }

    pub fn config(&self) -> &AnalyticsConfig {
        &self.config
    }

    /// その日の秘密の値。キャッシュ（Redis なら全サーバーで共有）に無ければ作る。
    /// 同時に作ろうとしたサーバーがあれば、先に作った方の値を使う
    async fn daily_salt(&self, day: &str) -> String {
        let mut cached = self.salt.lock().await;
        if let Some((d, salt)) = cached.as_ref() {
            if d == day {
                return salt.clone();
            }
        }
        let key = format!("analytics:salt:{}", day);
        let lock = format!("analytics:salt-lock:{}", day);
        let mut salt = None;
        for attempt in 0..10 {
            if let Ok(Some(value)) = self.cache.get(&key).await {
                salt = Some(value);
                break;
            }
            if attempt == 0
                && self
                    .cache
                    .increment(&lock, Some(VISITOR_TTL_SECS))
                    .await
                    .ok()
                    == Some(1)
            {
                let value = format!(
                    "{}{}",
                    uuid::Uuid::new_v4().simple(),
                    uuid::Uuid::new_v4().simple()
                );
                if let Err(e) = self.cache.set(&key, &value, Some(VISITOR_TTL_SECS)).await {
                    tracing::warn!("Could not store the analytics salt in the cache: {}", e);
                }
                salt = Some(value);
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        // キャッシュが使えないときは、このサーバーだけの値（訪問者数は多めに出ることがある）
        let salt = salt.unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string());
        *cached = Some((day.to_string(), salt.clone()));
        salt
    }

    /// その日の最初の 1 回なら true（キャッシュが使えなければ true）
    async fn first_time(&self, key: &str) -> bool {
        match self.cache.increment(key, Some(VISITOR_TTL_SECS)).await {
            Ok(n) => n == 1,
            Err(_) => true,
        }
    }

    /// ページ表示を 1 回数える（書き込みは [`flush`](Self::flush) でまとめて行う）
    pub async fn track(&self, view: PageView<'_>, now: DateTime<Utc>) -> TrackOutcome {
        if agent::is_bot(view.user_agent) {
            return TrackOutcome::Bot;
        }
        let Some(path) = agent::normalize_path(view.path) else {
            return TrackOutcome::Excluded;
        };
        if self.config.excludes(&path) {
            return TrackOutcome::Excluded;
        }
        let day = now.format("%Y-%m-%d").to_string();
        let salt = self.daily_salt(&day).await;
        let visitor = short_hash(&[&salt, view.ip, view.user_agent, view.host]);
        let new_visitor = self
            .first_time(&format!("analytics:v:{}:{}", day, visitor))
            .await;
        let new_on_path = self
            .first_time(&format!(
                "analytics:vp:{}:{}:{}",
                day,
                visitor,
                short_hash(&[&path])
            ))
            .await;
        let referrer = match agent::classify_referrer(view.referrer, view.host) {
            agent::Referrer::External(host) => Some(host),
            agent::Referrer::Direct => Some(String::new()),
            agent::Referrer::Internal => None,
        };
        let new_from_referrer = match &referrer {
            Some(host) => {
                self.first_time(&format!(
                    "analytics:vr:{}:{}:{}",
                    day,
                    visitor,
                    short_hash(&[host])
                ))
                .await
            }
            None => false,
        };
        let utm = agent::extract_utm(view.query);
        let new_from_utm_source = match &utm.source {
            Some(src) => {
                self.first_time(&format!(
                    "analytics:us:{}:{}:{}",
                    day,
                    visitor,
                    short_hash(&[src])
                ))
                .await
            }
            None => false,
        };
        let new_from_utm_campaign = match &utm.campaign {
            Some(camp) => {
                self.first_time(&format!(
                    "analytics:uc:{}:{}:{}",
                    day,
                    visitor,
                    short_hash(&[camp])
                ))
                .await
            }
            None => false,
        };
        let new_from_utm_medium = match &utm.medium {
            Some(med) => {
                self.first_time(&format!(
                    "analytics:um:{}:{}:{}",
                    day,
                    visitor,
                    short_hash(&[med])
                ))
                .await
            }
            None => false,
        };
        let device = agent::device(view.user_agent);

        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut add = |dimension: &'static str, value: String, visitor: bool| {
            let entry = pending.entry((day.clone(), dimension, value)).or_default();
            entry.0 += 1;
            entry.1 += u64::from(visitor);
        };
        add(DIMENSION_SITE, String::new(), new_visitor);
        add(DIMENSION_PATH, path, new_on_path);
        // 参照元はサイトの外から来たページ表示だけ（サイトの中での移動は数えない）。訪問者は参照元ごとにその日 1 回
        if let Some(host) = referrer {
            add(DIMENSION_REFERRER, host, new_from_referrer);
        }
        // 端末の種類は User-Agent で決まり、訪問者の見分けにも User-Agent を使うので、その日の最初の 1 回で数える
        add(DIMENSION_DEVICE, device.to_string(), new_visitor);
        if let Some(src) = utm.source {
            add(DIMENSION_UTM_SOURCE, src, new_from_utm_source);
        }
        if let Some(camp) = utm.campaign {
            add(DIMENSION_UTM_CAMPAIGN, camp, new_from_utm_campaign);
        }
        if let Some(med) = utm.medium {
            add(DIMENSION_UTM_MEDIUM, med, new_from_utm_medium);
        }
        TrackOutcome::Counted
    }

    /// 検索キーワードを 1 回数える（書き込みは [`flush`](Self::flush) でまとめて行う）。
    /// `hits` が 0 件の場合は `DIMENSION_SEARCH_EMPTY` にも加算する
    pub async fn track_search(
        &self,
        query: &str,
        hits: u64,
        ip: &str,
        user_agent: &str,
        host: &str,
        now: DateTime<Utc>,
    ) -> TrackOutcome {
        if agent::is_bot(user_agent) {
            return TrackOutcome::Bot;
        }
        let q = query.trim();
        if q.is_empty() {
            return TrackOutcome::Excluded;
        }
        let q_clipped: String = q
            .chars()
            .take(aegis_db::analytics::ANALYTICS_VALUE_MAX_CHARS)
            .collect();
        let day = now.format("%Y-%m-%d").to_string();
        let salt = self.daily_salt(&day).await;
        let visitor = short_hash(&[&salt, ip, user_agent, host]);
        let new_on_query = self
            .first_time(&format!(
                "analytics:vsq:{}:{}:{}",
                day,
                visitor,
                short_hash(&[&q_clipped])
            ))
            .await;

        let new_on_empty = if hits == 0 {
            self.first_time(&format!(
                "analytics:vse:{}:{}:{}",
                day,
                visitor,
                short_hash(&[&q_clipped])
            ))
            .await
        } else {
            false
        };

        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut add = |dimension: &'static str, value: String, visitor: bool| {
            let entry = pending.entry((day.clone(), dimension, value)).or_default();
            entry.0 += 1;
            entry.1 += u64::from(visitor);
        };

        add(DIMENSION_SEARCH_QUERY, q_clipped.clone(), new_on_query);
        if hits == 0 {
            add(DIMENSION_SEARCH_EMPTY, q_clipped, new_on_empty);
        }
        TrackOutcome::Counted
    }

    /// 溜まった集計を DB に書く。書けなければ戻して次の回に書く
    pub async fn flush(&self) {
        let counts: Vec<AnalyticsCount> = {
            let mut pending = self
                .pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            pending
                .drain()
                .map(
                    |((day, dimension, value), (pageviews, visitors))| AnalyticsCount {
                        day,
                        dimension: dimension.to_string(),
                        value,
                        pageviews,
                        visitors,
                    },
                )
                .collect()
        };
        if counts.is_empty() {
            return;
        }
        if let Err(e) = self.db.add_analytics_counts(&counts).await {
            tracing::warn!(
                "Could not save {} analytics counts (retrying later): {}",
                counts.len(),
                e
            );
            let mut pending = self
                .pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            for c in counts {
                let dimension = match c.dimension.as_str() {
                    DIMENSION_SITE => DIMENSION_SITE,
                    DIMENSION_PATH => DIMENSION_PATH,
                    DIMENSION_REFERRER => DIMENSION_REFERRER,
                    DIMENSION_UTM_SOURCE => DIMENSION_UTM_SOURCE,
                    DIMENSION_UTM_CAMPAIGN => DIMENSION_UTM_CAMPAIGN,
                    DIMENSION_UTM_MEDIUM => DIMENSION_UTM_MEDIUM,
                    DIMENSION_SEARCH_QUERY => DIMENSION_SEARCH_QUERY,
                    DIMENSION_SEARCH_EMPTY => DIMENSION_SEARCH_EMPTY,
                    _ => DIMENSION_DEVICE,
                };
                let entry = pending.entry((c.day, dimension, c.value)).or_default();
                entry.0 += c.pageviews;
                entry.1 += c.visitors;
            }
        }
    }

    /// 保存期間（`retention_days`）を過ぎた集計を消す
    pub async fn apply_retention(&self, now: DateTime<Utc>) {
        if self.config.retention_days == 0 {
            return;
        }
        let cutoff = (now - chrono::Duration::days(i64::from(self.config.retention_days)))
            .format("%Y-%m-%d")
            .to_string();
        match self.db.delete_analytics_before(&cutoff).await {
            Ok(0) => {}
            Ok(n) => tracing::info!("Deleted {} analytics rows older than {}", n, cutoff),
            Err(e) => tracing::warn!("Could not delete old analytics rows: {}", e),
        }
    }

    /// 定期的に書き出し、1 日に 1 回古い集計を消す。止まるときに溜まった分を失わないよう、
    /// サーバーの終了処理でも [`flush`](Self::flush) を呼ぶ
    pub fn spawn_tasks(self: &Arc<Self>) {
        let service = self.clone();
        tokio::spawn(async move {
            let interval = Duration::from_secs(service.config.flush_interval_secs.max(1));
            let mut last_retention: Option<String> = None;
            loop {
                tokio::time::sleep(interval).await;
                service.flush().await;
                let now = Utc::now();
                let today = now.format("%Y-%m-%d").to_string();
                if last_retention.as_deref() != Some(today.as_str()) {
                    service.apply_retention(now).await;
                    last_retention = Some(today);
                }
            }
        });
    }

    /// `from`〜`to`（`YYYY-MM-DD`、両端を含む）の集計。書き出し前の分も含める
    pub async fn summary(
        &self,
        from: &str,
        to: &str,
        limit: u64,
    ) -> Result<AnalyticsSummary, SummaryError> {
        let from_day = parse_day(from).ok_or(SummaryError::InvalidDate)?;
        let to_day = parse_day(to).ok_or(SummaryError::InvalidDate)?;
        if from_day > to_day {
            return Err(SummaryError::InvalidRange);
        }
        if (to_day - from_day).num_days() >= MAX_RANGE_DAYS {
            return Err(SummaryError::RangeTooLong);
        }
        self.flush().await;
        let (from, to) = (
            from_day.format("%Y-%m-%d").to_string(),
            to_day.format("%Y-%m-%d").to_string(),
        );
        let db = &self.db;
        let rows = db
            .list_analytics_daily(DIMENSION_SITE, &from, &to)
            .await
            .map_err(|e| SummaryError::Database(e.to_string()))?;
        let mut by_day: BTreeMap<String, DailyPoint> = BTreeMap::new();
        let mut day = from_day;
        while day <= to_day {
            let key = day.format("%Y-%m-%d").to_string();
            by_day.insert(
                key.clone(),
                DailyPoint {
                    day: key,
                    ..DailyPoint::default()
                },
            );
            match day.succ_opt() {
                Some(next) => day = next,
                None => break,
            }
        }
        for row in rows {
            if let Some(point) = by_day.get_mut(&row.day) {
                point.pageviews += row.pageviews;
                point.visitors += row.visitors;
            }
        }
        let daily: Vec<DailyPoint> = by_day.into_values().collect();
        let mut tops = Vec::with_capacity(8);
        for dimension in [
            DIMENSION_PATH,
            DIMENSION_REFERRER,
            DIMENSION_DEVICE,
            DIMENSION_UTM_SOURCE,
            DIMENSION_UTM_CAMPAIGN,
            DIMENSION_UTM_MEDIUM,
            DIMENSION_SEARCH_QUERY,
            DIMENSION_SEARCH_EMPTY,
        ] {
            tops.push(
                db.top_analytics_values(dimension, &from, &to, limit)
                    .await
                    .map_err(|e| SummaryError::Database(e.to_string()))?,
            );
        }
        let top_search_empty = tops.pop().unwrap_or_default();
        let top_search_queries = tops.pop().unwrap_or_default();
        let top_utm_mediums = tops.pop().unwrap_or_default();
        let top_utm_campaigns = tops.pop().unwrap_or_default();
        let top_utm_sources = tops.pop().unwrap_or_default();
        let devices = tops.pop().unwrap_or_default();
        let top_referrers = tops.pop().unwrap_or_default();
        let top_pages = tops.pop().unwrap_or_default();

        let pageviews: u64 = daily.iter().map(|d| d.pageviews).sum();
        let visitors: u64 = daily.iter().map(|d| d.visitors).sum();

        // 前期間（同じ日数分だけ遡った直前期間）の集計
        let num_days = (to_day - from_day).num_days() + 1;
        let prev_to_day = from_day - chrono::Duration::days(1);
        let prev_from_day = prev_to_day - chrono::Duration::days(num_days - 1);
        let prev_from = prev_from_day.format("%Y-%m-%d").to_string();
        let prev_to = prev_to_day.format("%Y-%m-%d").to_string();

        let prev_rows = db
            .list_analytics_daily(DIMENSION_SITE, &prev_from, &prev_to)
            .await
            .unwrap_or_default();
        let prev_pageviews: u64 = prev_rows.iter().map(|r| r.pageviews).sum();
        let prev_visitors: u64 = prev_rows.iter().map(|r| r.visitors).sum();

        let calc_pct = |cur: u64, prev: u64| -> Option<f64> {
            if prev == 0 {
                return None;
            }
            let pct = ((cur as f64 - prev as f64) / prev as f64) * 100.0;
            Some((pct * 10.0).round() / 10.0)
        };

        let previous = Some(AnalyticsComparison {
            from: prev_from,
            to: prev_to,
            pageviews: prev_pageviews,
            visitors: prev_visitors,
            pageviews_change_pct: calc_pct(pageviews, prev_pageviews),
            visitors_change_pct: calc_pct(visitors, prev_visitors),
        });

        Ok(AnalyticsSummary {
            from: from_day.format("%Y-%m-%d").to_string(),
            to: to_day.format("%Y-%m-%d").to_string(),
            pageviews,
            visitors,
            daily,
            top_pages,
            top_referrers,
            devices,
            top_utm_sources,
            top_utm_campaigns,
            top_utm_mediums,
            top_search_queries,
            top_search_empty,
            previous,
        })
    }
}

/// 集計データを UTF-8 BOM 付き CSV 文字列として書き出す
pub fn export_summary_csv(summary: &AnalyticsSummary) -> String {
    let mut csv = String::from("\u{FEFF}");
    csv.push_str("Summary Report\n");
    csv.push_str(&format!("Period,{} to {}\n", summary.from, summary.to));
    csv.push_str(&format!("Total Pageviews,{}\n", summary.pageviews));
    csv.push_str(&format!("Total Daily Visitors,{}\n", summary.visitors));

    if let Some(prev) = &summary.previous {
        csv.push_str(&format!("Previous Period,{} to {}\n", prev.from, prev.to));
        csv.push_str(&format!("Previous Pageviews,{}\n", prev.pageviews));
        csv.push_str(&format!("Previous Daily Visitors,{}\n", prev.visitors));
        let pv_change = prev
            .pageviews_change_pct
            .map(|p| format!("{p}%"))
            .unwrap_or_else(|| "N/A".to_string());
        let vis_change = prev
            .visitors_change_pct
            .map(|p| format!("{p}%"))
            .unwrap_or_else(|| "N/A".to_string());
        csv.push_str(&format!("Pageviews Change,{pv_change}\n"));
        csv.push_str(&format!("Visitors Change,{vis_change}\n"));
    }
    csv.push('\n');

    csv.push_str("Daily Trend\nDate,Pageviews,Visitors\n");
    for d in &summary.daily {
        csv.push_str(&format!("{},{},{}\n", d.day, d.pageviews, d.visitors));
    }
    csv.push('\n');

    fn write_section(csv: &mut String, title: &str, header: &str, items: &[AnalyticsTotal]) {
        csv.push_str(title);
        csv.push('\n');
        csv.push_str(header);
        csv.push_str(",Pageviews,Visitors\n");
        for item in items {
            let val = if item.value.is_empty() {
                "(direct)"
            } else {
                &item.value
            };
            let escaped = if val.contains(',') || val.contains('"') || val.contains('\n') {
                format!("\"{}\"", val.replace('"', "\"\""))
            } else {
                val.to_string()
            };
            csv.push_str(&format!(
                "{},{},{}\n",
                escaped, item.pageviews, item.visitors
            ));
        }
        csv.push('\n');
    }

    write_section(&mut csv, "Top Pages", "Path", &summary.top_pages);
    write_section(
        &mut csv,
        "Top Referrers",
        "Referrer",
        &summary.top_referrers,
    );
    write_section(&mut csv, "Devices", "Device", &summary.devices);
    write_section(&mut csv, "UTM Sources", "Source", &summary.top_utm_sources);
    write_section(
        &mut csv,
        "UTM Campaigns",
        "Campaign",
        &summary.top_utm_campaigns,
    );
    write_section(&mut csv, "UTM Mediums", "Medium", &summary.top_utm_mediums);
    write_section(
        &mut csv,
        "Top Search Queries",
        "Search Query",
        &summary.top_search_queries,
    );
    write_section(
        &mut csv,
        "Top No-Result Search Queries",
        "Search Query",
        &summary.top_search_empty,
    );

    csv
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SummaryError {
    InvalidDate,
    InvalidRange,
    RangeTooLong,
    Database(String),
}

impl std::fmt::Display for SummaryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidDate => write!(f, "Dates must be YYYY-MM-DD"),
            Self::InvalidRange => write!(f, "'from' must not be after 'to'"),
            Self::RangeTooLong => write!(f, "The range must be {} days or shorter", MAX_RANGE_DAYS),
            Self::Database(e) => write!(f, "Database error: {}", e),
        }
    }
}

impl std::error::Error for SummaryError {}

#[cfg(test)]
mod tests;
