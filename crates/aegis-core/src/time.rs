//! 時刻の扱い（jiff）。
//!
//! 時刻は UTC の [`Timestamp`] で持ち、DB やログには RFC 3339 の文字列で書く。書式は用途ごとに
//! 3 つに固定する（かつて使っていた chrono の出力とバイト単位で同じ。DB に残っている値と
//! 文字列として比較されるため、書式を変えてはいけない）。
//!
//! | メソッド                | 例                                     | 用途                       |
//! | ----------------------- | -------------------------------------- | -------------------------- |
//! | [`to_rfc3339_secs`]     | `2026-03-05T09:00:00Z`                 | 秒単位でよい値             |
//! | [`to_rfc3339_micros`]   | `2026-03-05T09:00:00.123456Z`          | 並べ替えに使う値・監査ログ |
//! | [`to_rfc3339_auto`]     | `2026-03-05T09:00:00.123+00:00`        | 従来の `to_rfc3339()`      |
//!
//! [`to_rfc3339_secs`]: TimestampExt::to_rfc3339_secs
//! [`to_rfc3339_micros`]: TimestampExt::to_rfc3339_micros
//! [`to_rfc3339_auto`]: TimestampExt::to_rfc3339_auto
//!
//! 1970 年より前の時刻は扱わない（このシステムが書く時刻は現在以降のみ）。

pub use jiff::{SignedDuration, Timestamp};

/// `Timestamp` を RFC 3339 の文字列にする（書式は [モジュールの説明](self) の表）
pub trait TimestampExt {
    /// `2026-03-05T09:00:00Z`（秒未満は切り捨て）
    fn to_rfc3339_secs(&self) -> String;
    /// `2026-03-05T09:00:00.123456Z`（マイクロ秒、常に 6 桁。それより細かい分は切り捨て）
    fn to_rfc3339_micros(&self) -> String;
    /// `2026-03-05T09:00:00+00:00`。秒未満があるときだけ 3・6・9 桁のどれかで付ける
    fn to_rfc3339_auto(&self) -> String;
}

/// 秒まで（`%Y-%m-%dT%H:%M:%S`）
fn date_time_part(ts: &Timestamp) -> String {
    ts.strftime("%Y-%m-%dT%H:%M:%S").to_string()
}

/// 秒未満のナノ秒（0〜999_999_999）
fn subsec_nanos(ts: &Timestamp) -> u32 {
    ts.as_nanosecond().rem_euclid(1_000_000_000) as u32
}

impl TimestampExt for Timestamp {
    fn to_rfc3339_secs(&self) -> String {
        format!("{}Z", date_time_part(self))
    }

    fn to_rfc3339_micros(&self) -> String {
        format!(
            "{}.{:06}Z",
            date_time_part(self),
            subsec_nanos(self) / 1_000
        )
    }

    fn to_rfc3339_auto(&self) -> String {
        let nanos = subsec_nanos(self);
        let fraction = if nanos == 0 {
            String::new()
        } else if nanos.is_multiple_of(1_000_000) {
            format!(".{:03}", nanos / 1_000_000)
        } else if nanos.is_multiple_of(1_000) {
            format!(".{:06}", nanos / 1_000)
        } else {
            format!(".{nanos:09}")
        };
        format!("{}{fraction}+00:00", date_time_part(self))
    }
}

/// RFC 3339 の文字列を `Timestamp` にする。オフセットは UTC に直す
pub fn parse_rfc3339(value: &str) -> Result<Timestamp, jiff::Error> {
    value.parse()
}

/// `n` 日分の長さ（夏時間などは無く、1 日 = 24 時間）
pub fn days(n: i64) -> SignedDuration {
    SignedDuration::from_hours(n.saturating_mul(24))
}

/// `later` から `earlier` を引いた秒数（切り捨て。`later` が前なら負）
pub fn seconds_between(later: Timestamp, earlier: Timestamp) -> i64 {
    later.duration_since(earlier).as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(text: &str) -> Timestamp {
        text.parse().unwrap()
    }

    #[test]
    fn test_formats_match_the_old_chrono_output() {
        let whole = ts("2026-03-05T09:00:00Z");
        assert_eq!(whole.to_rfc3339_secs(), "2026-03-05T09:00:00Z");
        assert_eq!(whole.to_rfc3339_micros(), "2026-03-05T09:00:00.000000Z");
        assert_eq!(whole.to_rfc3339_auto(), "2026-03-05T09:00:00+00:00");

        let millis = ts("2026-03-05T09:00:00.120Z");
        assert_eq!(millis.to_rfc3339_secs(), "2026-03-05T09:00:00Z");
        assert_eq!(millis.to_rfc3339_micros(), "2026-03-05T09:00:00.120000Z");
        assert_eq!(millis.to_rfc3339_auto(), "2026-03-05T09:00:00.120+00:00");

        let micros = ts("2026-03-05T09:00:00.123456Z");
        assert_eq!(micros.to_rfc3339_micros(), "2026-03-05T09:00:00.123456Z");
        assert_eq!(micros.to_rfc3339_auto(), "2026-03-05T09:00:00.123456+00:00");

        let nanos = ts("2026-03-05T09:00:00.123456789Z");
        assert_eq!(nanos.to_rfc3339_micros(), "2026-03-05T09:00:00.123456Z");
        assert_eq!(
            nanos.to_rfc3339_auto(),
            "2026-03-05T09:00:00.123456789+00:00"
        );
    }

    #[test]
    fn test_parse_converts_offsets_and_rejects_garbage() {
        assert_eq!(
            parse_rfc3339("2026-03-05T18:00:00+09:00")
                .unwrap()
                .to_rfc3339_secs(),
            "2026-03-05T09:00:00Z"
        );
        assert!(parse_rfc3339("yesterday").is_err());
        assert!(parse_rfc3339("2026-03-05").is_err());
        assert!(parse_rfc3339("").is_err());
    }

    #[test]
    fn test_days_and_seconds_between() {
        let start = ts("2026-03-05T09:00:00Z");
        assert_eq!((start + days(1)).to_rfc3339_secs(), "2026-03-06T09:00:00Z");
        assert_eq!((start - days(5)).to_rfc3339_secs(), "2026-02-28T09:00:00Z");
        assert_eq!(seconds_between(start + days(1), start), 86_400);
        assert_eq!(seconds_between(start, start + days(1)), -86_400);
    }
}
