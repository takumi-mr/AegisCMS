# official-charts

[English](README.md) | **日本語**

本文に置けるグラフブロック（`official-charts/chart`）を追加する公式プラグインです。JavaScript や外部 CDN を使わず、サーバー（Wasm）側でアクセシブルな `<svg>` とスクリーンリーダー向けのデータ表を生成します。Strict CSP（`style=""` 属性やインラインスクリプトの禁止）のもとでもそのまま動作します。

## 対応するグラフの種類

| `type`           | 説明                                                 | 複数系列    |
| ---------------- | ---------------------------------------------------- | ----------- |
| `bar`            | 縦棒グラフ（既定）。各系列の棒を並べて比較します     | ✅          |
| `stacked-bar`    | 積み上げ棒グラフ。合計と内訳を同時に見せます         | ✅          |
| `horizontal-bar` | 横棒グラフ。項目名が長い場合やランキング向きです     | ✅          |
| `line`           | 折れ線グラフ（単一系列では薄い面塗りが付きます）     | ✅          |
| `donut`          | ドーナツグラフ。構成比と合計を中央・凡例に表示します | —（先頭値） |

## 入力欄 (`fields`)

| 属性     | 型       | 必須 | 説明                                                                                                                      |
| -------- | -------- | ---- | ------------------------------------------------------------------------------------------------------------------------- |
| `type`   | `select` |      | グラフの種類（`bar` / `stacked-bar` / `horizontal-bar` / `line` / `donut`）。既定は `bar`                                 |
| `title`  | `string` |      | グラフのタイトル（`<figcaption>` および SVG の `aria-label` に使用）                                                      |
| `unit`   | `string` |      | 数値の後ろに付ける単位（例: `%`、`ms`、`req/s`）                                                                          |
| `series` | `string` |      | 系列名（1 系列のときは空欄、複数系列のときは `"2025年, 2026年"` のようにカンマ区切りで 6 つまで。API では文字列配列も可） |
| `max`    | `number` |      | 軸の上限値（省略時はデータから切りの良い目盛りを自動計算）                                                                |
| `items`  | `list`   | ✅   | データ行（最大 50 件）。各項目は `label`（項目名）と `values`（値・カンマ区切り、数値や配列・`value` キーも可）           |

## 記述例（Markdown / API）

### 単一系列（横棒・ドーナツ）

```markdown
::: official-charts/chart {"type": "horizontal-bar", "title": "データベース別スループット", "unit": "req/s", "items": [{"label": "SQLite (WAL)", "values": "2730"}, {"label": "PostgreSQL", "values": "490"}, {"label": "MySQL", "values": "490"}]}

::: official-charts/chart {"type": "donut", "title": "流入元", "unit": "%", "items": [{"label": "検索", "values": "58"}, {"label": "SNS", "values": "27"}, {"label": "直接", "values": "15"}]}
```

### 複数系列（縦棒・積み上げ棒・折れ線）

```markdown
::: official-charts/chart {"type": "bar", "title": "月別アクセス数", "unit": "k", "series": "2025年, 2026年", "items": [{"label": "1月", "values": "120, 150"}, {"label": "2月", "values": "140, 185"}, {"label": "3月", "values": "135, 210"}]}

::: official-charts/chart {"type": "line", "title": "レイテンシ推移", "unit": "ms", "series": "p50, p95", "items": [{"label": "v0.8", "values": "18, 42"}, {"label": "v0.9", "values": "15, 34"}, {"label": "v0.10", "values": "12, 26"}]}
```

## 使い方・テーマによる色変更

設定ファイルの `[plugins].enabled` に `"official-charts"` を足すか、Admin Studio の **Settings › Plugins** で有効にします。

```toml
[plugins]
enabled = ["official-charts"]
```

- 見た目の CSS は `:where()` で詳細度 0 にしており、1 つ目の系列色はテーマの `--accent`（未定義時は `#6366f1`）を使います。
- 2〜6 番目の系列色や背景・文字色は、テーマの CSS で `.oc-chart` の CSS 変数（`--oc-c0`〜`--oc-c5`）を上書きして調整できます。文字や目盛り線には `currentColor` を使うため、明るいテーマでも暗いテーマでも読めます。
- 必須の入力欄が空のブロックや有効な数値が 1 つも無いブロックは表示されません。

## ビルドとテスト

```bash
task plugin:build:charts   # plugin.wasm を作り直す（要 rustup target add wasm32-unknown-unknown）
cargo test -p aegis-plugin-charts
```
