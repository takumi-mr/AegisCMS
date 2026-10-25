//! コメント（と、今後の商品レビュー）で共有する型: 審査の状態、`check-comment` フックの入出力、
//! テーマのテンプレートに渡すコメント欄

use serde::{Deserialize, Serialize};

/// 投稿の審査の状態。コメントと商品レビューで同じ状態・遷移を使う
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModerationStatus {
    /// 承認待ち（公開ページに出さない）
    Pending,
    /// 承認済み（公開ページに出る）
    Approved,
    Spam,
    /// ゴミ箱（完全に削除する前の状態）
    Trash,
}

impl ModerationStatus {
    pub const ALL: [ModerationStatus; 4] = [Self::Pending, Self::Approved, Self::Spam, Self::Trash];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Spam => "spam",
            Self::Trash => "trash",
        }
    }
}

impl std::str::FromStr for ModerationStatus {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|status| status.as_str() == s)
            .ok_or_else(|| format!("Unknown moderation status: {}", s))
    }
}

impl std::fmt::Display for ModerationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `check-comment` フックに渡す、公開サイトから投稿されたコメント（保存する前）
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CommentSubmission {
    pub content_type: String,
    pub content_id: String,
    pub content_title: String,
    pub content_slug: String,
    /// 返信先のコメント
    #[serde(default)]
    pub parent_id: Option<String>,
    pub author_name: String,
    pub author_email: String,
    /// 本文（プレーンテキスト）
    pub body: String,
    #[serde(default)]
    pub ip: Option<String>,
    #[serde(default)]
    pub user_agent: Option<String>,
    /// ホストの判定（`approved` / `pending`）。プラグインはこれより厳しくだけできる
    pub status: String,
}

/// `check-comment` の結果。`Allow` はホストの判定のまま、`Hold` は承認待ちに、`Spam` はスパムにする。
/// 投稿そのものを断るときはフックがエラー（`{"error": "..."}`）を返す
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "verdict", rename_all = "lowercase")]
pub enum CommentVerdict {
    #[default]
    Allow,
    Hold {
        #[serde(default)]
        reason: Option<String>,
    },
    Spam {
        #[serde(default)]
        reason: Option<String>,
    },
}

/// テーマのテンプレートに渡すコメント欄（`comments`）。コメントを受け付ける種類の公開ページにだけ付く
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CommentSection {
    /// コンテンツの ID（投稿のときに送る）
    pub content_id: String,
    /// 新しいコメントを受け付けているか（公開から `[comments].close_after_days` を過ぎると閉じる）
    pub open: bool,
    /// 承認済みのコメント数
    pub count: u64,
    /// 承認済みのコメント（スレッドの順。返信は親の直後に `depth` を 1 つ増やして並ぶ）
    pub items: Vec<PublicComment>,
    /// 投稿先（`POST`、ベースパス込み）
    pub post_url: String,
    /// 返信できる深さ（0 は返信なし）
    pub max_depth: u32,
    /// 本文の最大文字数
    pub max_length: usize,
}

/// 公開ページに出すコメント（メールアドレス・IP アドレスは含めない）
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PublicComment {
    pub id: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    pub author_name: String,
    /// 本文の HTML（エスケープ済み。段落と改行だけ）
    pub body_html: String,
    pub created_at: String,
    /// スレッドの深さ（トップレベルは 0）
    pub depth: u32,
    /// Admin Studio のユーザー（サイトの運営者）が書いた返信
    #[serde(default)]
    pub staff: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_round_trip() {
        for status in ModerationStatus::ALL {
            assert_eq!(status.as_str().parse::<ModerationStatus>(), Ok(status));
        }
        assert!("deleted".parse::<ModerationStatus>().is_err());
    }

    #[test]
    fn test_verdict_json() {
        let parse = |s: &str| serde_json::from_str::<CommentVerdict>(s).unwrap();
        assert_eq!(parse(r#"{"verdict":"allow"}"#), CommentVerdict::Allow);
        assert_eq!(
            parse(r#"{"verdict":"spam","reason":"known spammer"}"#),
            CommentVerdict::Spam {
                reason: Some("known spammer".to_string())
            }
        );
        assert_eq!(
            parse(r#"{"verdict":"hold"}"#),
            CommentVerdict::Hold { reason: None }
        );
    }
}
