//! `aegis-server theme dev` の再エクスポート。
//! 実装は独立クレート `aegis-theme-dev` に分離されており、
//! 初回ビルドの軽量化や単体実行（`aegis-theme-dev <dir>`）に対応しています。

pub use aegis_theme_dev::*;
