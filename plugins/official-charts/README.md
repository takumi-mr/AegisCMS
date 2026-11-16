# official-charts

**English** | [日本語](README.ja.md)

An official plugin that adds a chart block (`official-charts/chart`) you can place in the body. It generates accessible `<svg>` charts and a screen-reader data table on the server (in Wasm) without JavaScript or external CDNs, and works out of the box under Strict CSP (no `style=""` attributes and no inline scripts).

## Chart Types

| `type`           | Description                                                          | Multi-series    |
| ---------------- | -------------------------------------------------------------------- | --------------- |
| `bar`            | Vertical bar chart (default). Compares series side by side           | ✅              |
| `stacked-bar`    | Stacked bar chart. Shows total and breakdown together                | ✅              |
| `horizontal-bar` | Horizontal bar chart. Good for long labels or rankings               | ✅              |
| `line`           | Line chart (adds a subtle area fill when there is a single series)   | ✅              |
| `donut`          | Donut chart. Shows total in the center and percentages in the legend | — (first value) |

## Input Fields (`fields`)

| Attribute | Type     | Required | Description                                                                                                                 |
| --------- | -------- | -------- | --------------------------------------------------------------------------------------------------------------------------- |
| `type`    | `select` |          | Chart type (`bar`, `stacked-bar`, `horizontal-bar`, `line`, or `donut`). Default: `bar`                                     |
| `title`   | `string` |          | Chart title (used as `<figcaption>` and SVG `aria-label`)                                                                   |
| `unit`    | `string` |          | Unit appended to values (e.g. `%`, `ms`, `req/s`)                                                                           |
| `series`  | `string` |          | Series names. Leave empty for a single series, or comma-separated for up to 6 series (also accepts a string array)          |
| `max`     | `number` |          | Axis maximum (when omitted, calculated automatically with clean tick steps from the data)                                   |
| `items`   | `list`   | ✅       | Data rows (up to 50). Each item has `label` and `values` (number or comma-separated numbers; `value` / array also accepted) |

## Examples (Markdown / API)

### Single Series (Horizontal Bar & Donut)

```markdown
::: official-charts/chart {"type": "horizontal-bar", "title": "Throughput by Database", "unit": "req/s", "items": [{"label": "SQLite (WAL)", "values": "2730"}, {"label": "PostgreSQL", "values": "490"}, {"label": "MySQL", "values": "490"}]}

::: official-charts/chart {"type": "donut", "title": "Traffic Sources", "unit": "%", "items": [{"label": "Search", "values": "58"}, {"label": "Social", "values": "27"}, {"label": "Direct", "values": "15"}]}
```

### Multi-Series (Bar & Line)

```markdown
::: official-charts/chart {"type": "bar", "title": "Monthly Visits", "unit": "k", "series": "2025, 2026", "items": [{"label": "Jan", "values": "120, 150"}, {"label": "Feb", "values": "140, 185"}, {"label": "Mar", "values": "135, 210"}]}

::: official-charts/chart {"type": "line", "title": "Latency Trend", "unit": "ms", "series": "p50, p95", "items": [{"label": "v0.8", "values": "18, 42"}, {"label": "v0.9", "values": "15, 34"}, {"label": "v0.10", "values": "12, 26"}]}
```

## Usage & Styling

Add `"official-charts"` to `[plugins].enabled` in the config file, or enable it in Admin Studio's **Settings › Plugins**.

```toml
[plugins]
enabled = ["official-charts"]
```

- All built-in styles use zero-specificity CSS (`:where()`). The first series color uses the theme's `--accent` (`#6366f1` if absent).
- Override `--oc-c0` through `--oc-c5` on `.oc-chart` in your theme CSS to customize the palette. Text and grid lines use `currentColor`, so charts adapt to both light and dark themes.
- Blocks with empty items or no valid numeric values are not shown.

## Build & Test

```bash
task plugin:build:charts   # rebuild plugin.wasm (requires rustup target add wasm32-unknown-unknown)
cargo test -p aegis-plugin-charts
```
