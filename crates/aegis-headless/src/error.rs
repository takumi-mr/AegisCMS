/// 配信 API の失敗。メッセージは英語のみ（API のエラーは多言語対応しない）
#[derive(Debug, thiserror::Error)]
pub enum HeadlessError {
    /// 公開していない型、存在しない・公開していない・公開期間を過ぎたコンテンツ
    #[error("Not found")]
    NotFound,
    /// 引数の形式が正しくない
    #[error("{0}")]
    BadRequest(String),
    /// DB の失敗。中身は応答に出さない（サーバーのログに残す）
    #[error("Internal error")]
    Source(String),
}

impl From<aegis_core::AegisError> for HeadlessError {
    fn from(error: aegis_core::AegisError) -> Self {
        HeadlessError::Source(error.to_string())
    }
}
