//! 一覧の引数（クエリ文字列）の検証と、ページ送りのカーソル。
//!
//! 入力はすべて文字列で受け取り、ここで検証する（形式の誤りは `BadRequest`）。
//! カーソルは作成日時と ID の組を符号化した不透明な文字列で、壊れた値や改ざんされた値は受け付けない。

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::DateTime;
use serde::{Deserialize, Serialize};

use crate::error::HeadlessError;

/// 1 ページの件数の既定
pub const DEFAULT_PAGE_SIZE: u32 = 20;
/// カテゴリ ID・タグの slug・カーソルの ID の長さの上限（文字数）
const MAX_VALUE_CHARS: usize = 100;
/// カーソル全体の長さの上限（文字数）
const MAX_CURSOR_CHARS: usize = 400;

/// 一覧のクエリ文字列（未検証）
#[derive(Debug, Clone, Default)]
pub struct ListParams {
    pub limit: Option<String>,
    pub cursor: Option<String>,
    /// カテゴリの ID。`none` は未分類
    pub category: Option<String>,
    /// タグの slug
    pub tag: Option<String>,
}

/// 検証済みの一覧の条件
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListQuery {
    pub limit: u32,
    pub cursor: Option<Cursor>,
    pub category: Option<CategoryFilter>,
    pub tag: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CategoryFilter {
    /// 未分類
    None,
    Id(String),
}

/// 前のページの最後の行の位置（作成日時と ID）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor {
    #[serde(rename = "c")]
    pub created_at: String,
    #[serde(rename = "i")]
    pub id: String,
}

impl Cursor {
    pub fn encode(&self) -> String {
        let json = serde_json::to_vec(self).expect("a cursor always serializes");
        URL_SAFE_NO_PAD.encode(json)
    }

    pub fn decode(raw: &str) -> Result<Self, HeadlessError> {
        let invalid = || HeadlessError::BadRequest("Invalid cursor".to_string());
        if raw.chars().count() > MAX_CURSOR_CHARS {
            return Err(invalid());
        }
        let bytes = URL_SAFE_NO_PAD.decode(raw).map_err(|_| invalid())?;
        let cursor: Cursor = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        let valid_time = DateTime::parse_from_rfc3339(&cursor.created_at).is_ok();
        if !valid_time || cursor.id.is_empty() || cursor.id.chars().count() > MAX_VALUE_CHARS {
            return Err(invalid());
        }
        Ok(cursor)
    }
}

fn bad_request(message: &str) -> HeadlessError {
    HeadlessError::BadRequest(message.to_string())
}

fn checked_value(raw: &str, name: &str) -> Result<String, HeadlessError> {
    let value = raw.trim();
    if value.is_empty()
        || value.chars().count() > MAX_VALUE_CHARS
        || value.chars().any(char::is_control)
    {
        return Err(bad_request(&format!("Invalid {name}")));
    }
    Ok(value.to_string())
}

impl ListParams {
    /// `max_page_size` を超える `limit` は上限に丸める。0・数値でない値は拒否する
    pub fn validate(&self, max_page_size: u32) -> Result<ListQuery, HeadlessError> {
        let max = max_page_size.max(1);
        let limit = match self.limit.as_deref() {
            None => DEFAULT_PAGE_SIZE.min(max),
            Some(raw) => {
                let n: u32 = raw
                    .trim()
                    .parse()
                    .map_err(|_| bad_request("Invalid limit"))?;
                if n == 0 {
                    return Err(bad_request("Invalid limit"));
                }
                n.min(max)
            }
        };
        let cursor = match self.cursor.as_deref() {
            None | Some("") => None,
            Some(raw) => Some(Cursor::decode(raw)?),
        };
        let category = match self.category.as_deref() {
            None | Some("") => None,
            Some("none") => Some(CategoryFilter::None),
            Some(raw) => Some(CategoryFilter::Id(checked_value(raw, "category")?)),
        };
        let tag = match self.tag.as_deref() {
            None | Some("") => None,
            Some(raw) => Some(checked_value(raw, "tag")?),
        };
        Ok(ListQuery {
            limit,
            cursor,
            category,
            tag,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(limit: Option<&str>, cursor: Option<&str>) -> ListParams {
        ListParams {
            limit: limit.map(str::to_string),
            cursor: cursor.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn limit_defaults_is_clamped_and_rejects_nonsense() {
        assert_eq!(params(None, None).validate(100).unwrap().limit, 20);
        assert_eq!(params(None, None).validate(10).unwrap().limit, 10);
        assert_eq!(params(Some("5"), None).validate(100).unwrap().limit, 5);
        assert_eq!(params(Some("500"), None).validate(100).unwrap().limit, 100);
        for bad in ["0", "-1", "abc", "1.5", ""] {
            assert!(
                matches!(
                    params(Some(bad), None).validate(100),
                    Err(HeadlessError::BadRequest(_))
                ),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn cursor_round_trips_and_rejects_tampering() {
        let cursor = Cursor {
            created_at: "2026-11-05T00:00:00Z".to_string(),
            id: "article-1".to_string(),
        };
        let encoded = cursor.encode();
        assert_eq!(Cursor::decode(&encoded).unwrap(), cursor);
        assert_eq!(
            params(None, Some(&encoded)).validate(100).unwrap().cursor,
            Some(cursor)
        );

        let not_json = URL_SAFE_NO_PAD.encode("hello");
        let bad_time = URL_SAFE_NO_PAD.encode(r#"{"c":"yesterday","i":"x"}"#);
        let empty_id = URL_SAFE_NO_PAD.encode(r#"{"c":"2026-11-05T00:00:00Z","i":""}"#);
        let too_long = "a".repeat(500);
        for bad in ["!!!", "abc", &not_json, &bad_time, &empty_id, &too_long] {
            assert!(
                matches!(Cursor::decode(bad), Err(HeadlessError::BadRequest(_))),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn category_and_tag_are_checked() {
        let query = ListParams {
            category: Some("none".into()),
            tag: Some("rust".into()),
            ..Default::default()
        }
        .validate(100)
        .unwrap();
        assert_eq!(query.category, Some(CategoryFilter::None));
        assert_eq!(query.tag.as_deref(), Some("rust"));

        let query = ListParams {
            category: Some("cat-1".into()),
            ..Default::default()
        }
        .validate(100)
        .unwrap();
        assert_eq!(query.category, Some(CategoryFilter::Id("cat-1".into())));

        let too_long = "x".repeat(101);
        for bad in ["a\nb", too_long.as_str(), "   "] {
            let result = ListParams {
                category: Some(bad.to_string()),
                ..Default::default()
            }
            .validate(100);
            assert!(result.is_err(), "{bad:?}");
        }
    }
}
