//! WordPress からの移行のうち、ファイルを読んで変換するところ（DB や API には触れない）。
//!
//! - [`wxr`]: WordPress の書き出しファイル（WXR）の読み取り
//! - [`html`]: 本文の HTML を読む小さなパーサー
//! - [`blocks`]: 本文（HTML・Gutenberg・ショートコード）を AegisCMS のブロックに直す

pub mod blocks;
pub mod html;
pub mod wxr;

pub use blocks::{convert, Context, Conversion, Note};
pub use wxr::{
    parse_wxr, WpAuthor, WpComment, WpItem, WpSite, WpTerm, WpTermRef, WxrError, WxrExport,
    WxrSummary,
};
