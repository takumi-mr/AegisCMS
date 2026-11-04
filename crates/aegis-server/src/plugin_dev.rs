//! `aegis-server plugin dev` の再エクスポート。
//! 実装は独立クレート `aegis-plugin-dev` に分離されており、
//! 初回ビルドの軽量化や単体実行（`aegis-plugin-dev <dir>`）に対応しています。

pub use aegis_plugin_dev::*;
