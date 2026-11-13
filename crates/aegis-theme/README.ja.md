# aegis-theme

[English](README.md) | **日本語**

MiniJinja を基盤とする AegisCMS のテンプレートエンジンおよびテーマシステム。

## 概要

`aegis-theme` は、公開サイトの HTML レンダリングを担当するテンプレートエンジンクレートです。Python の Jinja2 / Django テンプレート構文と高い互換性を持つ `minijinja` を採用し、高速かつ安全な SSR (Server-Side Rendering) を実現します。

テーマ制作者は `themes/<theme_name>/` ディレクトリにテンプレートファイル（`.html`, `.jinja`）や静的アセットを配置し、`theme.toml` でテーマ設定や多言語対応、カスタムブロックのレイアウトを定義します。

## 主な責務・提供機能

- **ThemeEngine**:
  - テンプレートの探索・コンパイル・キャッシュ・描画。
  - レンダリング時の診断・警告収集 (`RenderDiagnostic`)。
  - 開発モード時のテンプレート自動リロード。
- **豊富なビルトイン関数・フィルタ (`builtins`)**:
  - **フォーマット**: 日付フォーマット (`date_format`), 通貨表示 (`format_money`), 数値フォーマット。
  - **HTML・テキスト操作**: エスケープ、抜粋生成 (`truncate_words`), スラッグ化 (`slugify`), Markdown パース。
  - **URL 生成**: 投稿パーマリンク (`post_url`), カテゴリ・タグ一覧 URL, メディア公開 URL。
  - **多言語対応**: `t("key")` によるロケール別メッセージカタログ参照。
- **テーママニフェスト (`manifest`)**:
  - `theme.toml` のパースと検証。テーマ名、バージョン、作者、対応テンプレート階層（単一投稿、固定ページ、一覧、404 等）、カスタムオプションの定義。
- **多言語メッセージ (`messages`)**:
  - 組み込みの多言語システム文言（ページネーション、投稿日、コメント欄、カート等）の en / ja カタログ提供。

## モジュール構成

| モジュール | 説明 |
| :--- | :--- |
| `engine` | テンプレートの読み込みとレンダリング実行 (`ThemeEngine`) |
| `builtins` | Jinja2 互換カスタムフィルタ・グローバル関数の登録 |
| `manifest` | テーマ設定ファイル `theme.toml` の定義と検証 |
| `messages` | システム標準の多言語メッセージカタログ (en, ja) |
| `config` | サイトテーマ設定 (`ThemeConfig`, `FrontPage`) |
