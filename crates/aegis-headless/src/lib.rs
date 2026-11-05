//! ヘッドレス配信 API: 公開済みのコンテンツを、スキーマが決める形の JSON で返す読み取り専用の層。
//! 設計は `docs/headless.md`。
//!
//! - DB は [`PublishedSource`]（公開版の取得だけ）を通して読む。編集用のメソッドは型として見えない
//! - HTTP のルート・認証なしの公開・レート制限・CORS・キャッシュのヘッダーはサーバー側にあり、
//!   ここは引数の検証、公開済みの判定、スキーマに従う整形、カーソル、`ETag` だけを扱う

pub mod error;
pub mod params;
pub mod service;
pub mod source;
pub mod view;

pub use error::HeadlessError;
pub use params::{CategoryFilter, Cursor, ListParams, ListQuery, DEFAULT_PAGE_SIZE};
pub mod openapi;
pub use openapi::public_openapi;
pub use service::{etag, get, list, Delivered, HeadlessConfig, ListResponse};
pub use source::PublishedSource;
pub use view::project;
