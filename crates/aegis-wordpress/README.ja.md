# aegis-wordpress

[English](README.md) | **日本語**

WordPress からの移行のうち、ファイルを読む部分を担います。WordPress の書き出しファイル（WXR）を読み取り、投稿の HTML を AegisCMS のブロックに変換します。

## 概要

このクレートは DB にも REST API にも触れません。ファイルを Rust の値に直すだけです。`aegis-server import wordpress`（`crates/aegis-server/src/import_wordpress.rs`）がこれを使い、結果を起動中のサーバーに送ります。使い方の案内は [docs/wordpress-migration.ja.md](../../docs/wordpress-migration.ja.md) を参照してください。

## モジュール

- **`wxr`**: 「ツール > エクスポート」で作る XML を読み取ります。
    - `parse_wxr(xml)` が `WxrExport`（サイト情報・著者・カテゴリ・タグ・その他の分類・投稿／固定ページ／添付ファイルとそのコメント）を返します。
    - `channel` の直下を 1 件ずつ読むので、大きな書き出しでもファイル全体を木にしません。
    - 読み取るだけです。AegisCMS の型への割り当て（投稿 → 記事など）は呼び出し側が行います。
- **`html`**: 変換で使う、小さな HTML 断片のパーサーとシリアライザーです。
- **`blocks`**: `convert(content, &Context)` が、投稿の HTML・Gutenberg のコメント・ショートコードを `BlockNode`（`aegis-ast`）に変換します。
    - 段落・見出し・リスト・引用・コード・表・画像・区切り線・折りたたみは、対応するブロックにします。段落内の書式は、テキストブロックで使える Markdown の一部にします。
    - 変換すると意味が変わるもの（段落内の改行・`sup`・入れ子のリスト・結合したセルのある表）は、元の HTML を `core/html` ブロックに残します（表示時に無害化されます）。
    - 埋め込みは URL だけの段落（リンクカード）にします。
    - 変換できなかったもの・確認が要るものは `Note`（`shortcode`・`html-kept`・`dropped`・`embed`・`gallery`）として返し、取り込みの報告に使います。

## 使い方

```rust
use aegis_wordpress::{convert, parse_wxr, Context};

let export = parse_wxr(&std::fs::read_to_string("export.xml")?)?;
for item in &export.items {
    let conversion = convert(&item.content, &Context::default());
    // conversion.blocks: Vec<BlockNode>, conversion.notes: Vec<Note>
}
```

`Context { attachments: Some(&map) }`（添付ファイル ID → URL）を渡すと、`[gallery ids="1,2"]` を画像にできます。
