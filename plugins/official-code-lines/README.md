# official-code-lines

コードブロックに**行番号**と**ファイル名の見出し**を足す公式プラグインです（[`on-render-page` フック](../../docs/plugin_hooks.md)）。

```text
 ┌ src/main.rs ─────────────┐   ← ファイル名（コードブロックの filename）
 │ 1  fn main() {           │
 │ 2      println!("hi");   │   ← 行番号（コピーしても入らない）
 │ 3  }                     │
 └──────────────────────────┘
```

- **行番号**: スクリプトが `<pre>` に番号の列を足します（コードの文字は変えないので、選択やコピーに番号は入りません）。行数が `min_lines` 未満のブロックと Mermaid の図には付けません。番号を付けたブロックは折り返さず、横にスクロールします（折り返すと番号がずれるため）。
- **ファイル名**: コードブロックの `filename` を、ブロックの上に出します。Admin Studio のコードブロックの「ファイル名」の欄か、Markdown の ` ```rust title="src/main.rs" ` で付けます。本体が `<pre data-filename="...">` として出力するので、このプラグインが無くても、テーマの CSS から `attr(data-filename)` で使えます。
- 本文に `<pre` が無いページには何も足しません。スクリプトが動かなくても、コードはそのまま読めます。
- コードの色付け・コピーボタン（`official-code-enhancer`）と一緒に使えます。

## 使い方

設定ファイルの `[plugins].enabled` に `"official-code-lines"` を足すか、Admin Studio の **Settings › Plugins** で有効にします。

```toml
[plugins]
enabled = ["official-code-lines"]
```

テーマが `extra_head` と `extra_css` を出力している必要があります（組み込みのテンプレートは出力します）。

## 設定

Admin Studio の **Settings › Plugins** の「設定」か、設定ファイルの `[plugins.settings.official-code-lines]` で変えます。省略した項目・不正な値は既定値になります。

| キー | 既定値 | 説明 |
| --- | --- | --- |
| `content_types` | `article,page` | 動かすコンテンツの種類（カンマ区切り） |
| `selector` | `.prose` | 本文の範囲（CSS セレクター。カンマ区切りで複数）。この中の `<pre>` だけを対象にする |
| `line_numbers` | `true` | 行番号を付ける |
| `show_filename` | `true` | ファイル名の見出しを出す |
| `min_lines` | `2` | 行番号を付ける最小の行数（1〜1000） |
| `number_color` | テーマの `--text-muted` | 行番号の色。`#hex`・`rgb()`・`hsl()`・`var(--名前)`・色名 |

```toml
[plugins.settings.official-code-lines]
min_lines = 5
number_color = "#8b949e"
```

## 見た目を変える

番号を付けたブロックは `pre.ocl-numbered`（番号の列は `.ocl-rows`）、見出しは `<div class="ocl-filename">` です。設定で足りないところは、テーマの CSS で上書きしてください。

## ビルド

```bash
task plugin:build:content   # plugin.wasm を作り直す（要 rustup target add wasm32-unknown-unknown）
cargo test -p aegis-plugin-code-lines
```
