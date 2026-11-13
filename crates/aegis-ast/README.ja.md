# aegis-ast

[English](README.md) | **日本語**

ブロックコンテンツの抽象構文木 (AST) データ構造、決定論的 HTML レンダラー、および Cloudflare `lol-html` によるストリーミング HTML リライター。

## 概要

AegisCMS では、コンテンツを生の HTML 文字列として直接データベースに保存・編集するのではなく、構造化されたブロックツリー (AST) として厳密に管理します。

`aegis-ast` は以下のパイプラインを担当します：
1. **Block AST 表現**: Gutenberg ライクなブロックデータ構造の表現と操作。
2. **決定論的 HTML レンダリング**: AST を安全かつ一貫したセマンティック HTML へ変換。
3. **セキュリティとストリーミングリライト**: Cloudflare の `lol-html` を用いて、外部リンクへの `rel="noopener noreferrer"` 自動付与や XSS 対策タグのサニタイズを低レイテンシ・ストリーミング処理で実行。

## 主な責務・提供機能

- **BlockNode**:
  - `core/paragraph`, `core/heading`, `core/image`, `core/table` などの標準ブロックおよびカスタムブロックを表現するツリー構造。
  - 属性 (`attributes`), インラインマークアップテキスト (`text`), 子ノード (`children`) の保持。
- **HtmlRenderer**:
  - AST から標準 HTML への高速・決定論的レンダリング。
  - インラインマークアップ（太字、斜体、リンク、コード）のパースとセーフエスケープ。
  - レンダリング結果からのリンク抽出 (`extract_links`)。
- **StreamingHtmlRewriter (`lol-html`)**:
  - レンダリング済み HTML や外部 HTML に対し、メモリ効率の高いゼロコピー・ストリーミング書き換えを実行。
  - 外部ドメインリンクへの `rel="noopener noreferrer"` 自動付与。
  - `target="_blank"` のセキュア化。
- **AstPipeline / AstTransformer**:
  - 見出しへの自動 ID 付与 (`HeadingIdTransformer`) や、プラグインによる AST 変換パイプラインの構築。

## モジュール構成

| モジュール | 説明 |
| :--- | :--- |
| `node` | ブロック AST の基本構造体 `BlockNode` の定義 |
| `inline` | インライン HTML マークアップのパースとプレーンテキスト抽出 (`plain_text`, `render_inline`) |
| `renderer` | AST から HTML への決定論的レンダラー (`HtmlRenderer`) |
| `rewriter` | `lol-html` を使用したストリーミング HTML 書き換え (`StreamingHtmlRewriter`) |
| `transform` | AST 走査および変換パイプライン (`AstPipeline`, `AstTransformer`, `HeadingIdTransformer`) |
| `config` | セキュリティおよび HTML 変換設定 (`SecurityConfig`) |

## 使用例

```rust
use aegis_ast::{BlockNode, HtmlRenderer, SecurityConfig, StreamingHtmlRewriter};

// 1. AST の構築
let mut heading = BlockNode::heading(2, "Hello AegisCMS");
heading.attributes.insert("id".into(), "hello".into());

let paragraph = BlockNode::paragraph("Safe and secure CMS.");
let blocks = vec![heading, paragraph];

// 2. HTML へのレンダリング
let renderer = HtmlRenderer::new();
let html = renderer.render(&blocks);

// 3. ストリーミングリライターによるセキュリティ属性の付与
let config = SecurityConfig::default();
let rewriter = StreamingHtmlRewriter::new(&config);
let secured_html = rewriter.rewrite(&html).expect("rewrite failed");
```
