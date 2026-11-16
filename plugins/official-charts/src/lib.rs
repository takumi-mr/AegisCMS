//! 公式プラグイン `official-charts`: 本文に置けるグラフブロック（`render-blocks`）。
//!
//! - `official-charts/chart`: `type`（`bar` / `stacked-bar` / `horizontal-bar` / `line` / `donut`）・
//!   `title`・`unit`・`series`（カンマ区切り、6 系列まで）・`max`・`items`（`label`・`values`）
//! - JavaScript を使わずにサーバー（Wasm）側でアクセシブルな `<svg>` とデータ表を組み立てる
//! - Strict CSP に準拠するため `style="..."` 属性を一切使わず、先頭ブロックの `<style>`（`:where()` で
//!   詳細度 0）と SVG の幾何属性・CSS 変数（`--accent`、`--oc-c0`〜`--oc-c5`）だけで描画する

use aegis_plugin_sdk::prelude::*;

#[derive(Default)]
pub struct ChartsPlugin;

/// データ行の上限
const MAX_ITEMS: usize = 50;

/// 系列数の上限（パレット数に合わせる）
const MAX_SERIES: usize = 6;

const STYLES: &str = "<style>\
:where(.oc-chart){margin:1.5rem 0;padding:1.25rem;border:1px solid rgba(127,127,127,.25);border-radius:12px;--oc-c0:var(--accent,#6366f1);--oc-c1:#0ea5e9;--oc-c2:#10b981;--oc-c3:#f59e0b;--oc-c4:#ec4899;--oc-c5:#8b5cf6}\
:where(.oc-title){font-weight:700;font-size:1.05em;margin:0 0 .75rem}\
:where(.oc-legend){list-style:none;display:flex;flex-wrap:wrap;gap:.5rem 1rem;margin:0 0 .75rem;padding:0;font-size:.85em}\
:where(.oc-legend li){display:inline-flex;align-items:center;gap:.4rem}\
:where(.oc-swatch){display:inline-block;width:.75rem;height:.75rem;border-radius:3px;flex:0 0 auto;background:var(--oc-color)}\
:where(.oc-svg){display:block;width:100%;height:auto;overflow:visible;font-family:inherit}\
:where(.oc-grid){stroke:currentColor;stroke-opacity:.14;stroke-width:1}\
:where(.oc-axis){stroke:currentColor;stroke-opacity:.35;stroke-width:1.2}\
:where(.oc-tick){fill:currentColor;fill-opacity:.7;font-size:11px}\
:where(.oc-axis-label){fill:currentColor;fill-opacity:.85;font-size:12px}\
:where(.oc-val){fill:currentColor;fill-opacity:.85;font-size:11px;font-variant-numeric:tabular-nums}\
.oc-s0{--oc-color:var(--oc-c0)}\
.oc-s1{--oc-color:var(--oc-c1)}\
.oc-s2{--oc-color:var(--oc-c2)}\
.oc-s3{--oc-color:var(--oc-c3)}\
.oc-s4{--oc-color:var(--oc-c4)}\
.oc-s5{--oc-color:var(--oc-c5)}\
:where(.oc-bar),:where(.oc-dot),:where(.oc-area){fill:var(--oc-color)}\
:where(.oc-line),:where(.oc-slice){stroke:var(--oc-color)}\
:where(.oc-donut-wrap){display:flex;flex-wrap:wrap;align-items:center;justify-content:center;gap:1.5rem}\
:where(.oc-donut-svg){width:200px;max-width:100%;height:auto;flex:0 0 auto}\
:where(.oc-donut-total){fill:currentColor;font-size:20px;font-weight:700}\
:where(.oc-donut-legend){flex-direction:column;margin:0}\
:where(.oc-donut-legend li){justify-content:space-between;gap:.75rem}\
:where(.oc-donut-pct){opacity:.7;font-variant-numeric:tabular-nums}\
:where(.oc-sr-only){position:absolute;width:1px;height:1px;padding:0;margin:-1px;overflow:hidden;clip:rect(0,0,0,0);white-space:nowrap;border:0}\
</style>";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChartType {
    Bar,
    StackedBar,
    HorizontalBar,
    Line,
    Donut,
}

impl ChartType {
    fn from_attr(value: &str) -> Self {
        match value {
            "stacked-bar" => Self::StackedBar,
            "horizontal-bar" => Self::HorizontalBar,
            "line" => Self::Line,
            "donut" => Self::Donut,
            _ => Self::Bar,
        }
    }

    fn class_suffix(self) -> &'static str {
        match self {
            Self::Bar => "bar",
            Self::StackedBar => "stacked-bar",
            Self::HorizontalBar => "horizontal-bar",
            Self::Line => "line",
            Self::Donut => "donut",
        }
    }

    fn default_aria_label(self) -> &'static str {
        match self {
            Self::Bar => "Bar chart",
            Self::StackedBar => "Stacked bar chart",
            Self::HorizontalBar => "Horizontal bar chart",
            Self::Line => "Line chart",
            Self::Donut => "Donut chart",
        }
    }
}

#[derive(Debug, Clone)]
struct DataRow {
    label: String,
    values: Vec<f64>,
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            _ => out.push(c),
        }
    }
    out
}

fn attr<'a>(block: &'a BlockNode, name: &str) -> &'a str {
    block
        .attributes
        .get(name)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or("")
}

fn number_attr(block: &BlockNode, name: &str) -> Option<f64> {
    let value = block.attributes.get(name)?;
    let n = match value {
        serde_json::Value::Number(n) => n.as_f64()?,
        serde_json::Value::String(s) => s.trim().parse().ok()?,
        _ => return None,
    };
    n.is_finite().then_some(n)
}

fn parse_series(block: &BlockNode) -> Vec<String> {
    let Some(val) = block.attributes.get("series") else {
        return Vec::new();
    };
    let names: Vec<String> = match val {
        serde_json::Value::String(s) => s
            .split(',')
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_string)
            .collect(),
        serde_json::Value::Array(arr) => arr
            .iter()
            .filter_map(|v| v.as_str())
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    };
    names.into_iter().take(MAX_SERIES).collect()
}

fn parse_number_value(val: &serde_json::Value) -> Option<f64> {
    let n = match val {
        serde_json::Value::Number(n) => n.as_f64()?,
        serde_json::Value::String(s) => s.trim().parse().ok()?,
        _ => return None,
    };
    (n.is_finite() && n >= 0.0).then_some(n)
}

fn parse_row_values(item: &serde_json::Map<String, serde_json::Value>) -> Vec<f64> {
    let Some(raw) = item.get("values").or_else(|| item.get("value")) else {
        return Vec::new();
    };
    let values: Vec<f64> = match raw {
        serde_json::Value::Number(n) => n
            .as_f64()
            .filter(|v| v.is_finite() && *v >= 0.0)
            .into_iter()
            .collect(),
        serde_json::Value::String(s) => s
            .split(',')
            .filter_map(|t| t.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v >= 0.0)
            .collect(),
        serde_json::Value::Array(arr) => arr.iter().filter_map(parse_number_value).collect(),
        _ => Vec::new(),
    };
    values.into_iter().take(MAX_SERIES).collect()
}

fn parse_items(block: &BlockNode) -> Vec<DataRow> {
    let Some(items) = block.attributes.get("items").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let obj = item.as_object()?;
            let label = obj
                .get("label")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .unwrap_or("");
            if label.is_empty() {
                return None;
            }
            let values = parse_row_values(obj);
            if values.is_empty() {
                return None;
            }
            Some(DataRow {
                label: label.to_string(),
                values,
            })
        })
        .take(MAX_ITEMS)
        .collect()
}

/// 4 分割した目盛りが切りの良い数になる軸の上限を求める
fn nice_axis_max(max_val: f64) -> f64 {
    if max_val <= 0.0 || !max_val.is_finite() {
        return 0.0;
    }
    let raw_step = max_val / 4.0;
    let mag = 10.0_f64.powf(raw_step.log10().floor());
    let norm = raw_step / mag;
    let nice_norm = if norm <= 1.0 {
        1.0
    } else if norm <= 2.0 {
        2.0
    } else if norm <= 2.5 {
        2.5
    } else if norm <= 5.0 {
        5.0
    } else {
        10.0
    };
    nice_norm * mag * 4.0
}

/// 数値を最大小数点以下 2 桁・不要な末尾 0 なしで整形する
fn format_num(v: f64) -> String {
    if !v.is_finite() {
        return "0".to_string();
    }
    let rounded = (v * 100.0).round() / 100.0;
    let rounded = if rounded == 0.0 { 0.0 } else { rounded };
    let s = format!("{rounded:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.to_string()
}

/// 値が 0 のときに単位を付けた文字列を組み立てる
fn format_val_unit(v: f64, unit: &str) -> String {
    if unit.is_empty() {
        format_num(v)
    } else {
        format!("{}{}", format_num(v), unit)
    }
}

/// SVG 軸ラベル向けの幅を意識した省略（全角 2・半角 1 の目安で `max_units` まで）
fn truncate_label(label: &str, max_units: usize) -> String {
    let mut used = 0usize;
    let mut out = String::new();
    for ch in label.chars() {
        let w = if ch.is_ascii() { 1 } else { 2 };
        if used + w > max_units {
            out.push('…');
            return out;
        }
        used += w;
        out.push(ch);
    }
    out
}

fn render_cartesian_legend(series_names: &[String]) -> String {
    let items: String = series_names
        .iter()
        .enumerate()
        .map(|(s, name)| {
            format!(
                r#"<li><span class="oc-swatch oc-s{s}" aria-hidden="true"></span><span>{}</span></li>"#,
                escape(name)
            )
        })
        .collect();
    format!(r#"<ul class="oc-legend">{items}</ul>"#)
}

fn render_sr_table(
    title: &str,
    unit: &str,
    series_names: &[String],
    rows: &[DataRow],
    is_donut: bool,
) -> String {
    let caption = if title.is_empty() {
        String::new()
    } else {
        format!("<caption>{}</caption>", escape(title))
    };
    let head_cols: String = if is_donut {
        "<th>Value</th><th>Share</th>".to_string()
    } else if series_names.is_empty() {
        "<th>Value</th>".to_string()
    } else {
        series_names
            .iter()
            .map(|n| format!("<th>{}</th>", escape(n)))
            .collect()
    };
    let total: f64 = if is_donut {
        rows.iter().map(|r| r.values[0]).sum()
    } else {
        0.0
    };
    let body_rows: String = rows
        .iter()
        .map(|row| {
            let cells: String = if is_donut {
                let v = row.values[0];
                let pct = if total > 0.0 {
                    (v / total) * 100.0
                } else {
                    0.0
                };
                format!(
                    "<td>{}</td><td>{pct:.1}%</td>",
                    escape(&format_val_unit(v, unit))
                )
            } else {
                row.values
                    .iter()
                    .map(|&v| format!("<td>{}</td>", escape(&format_val_unit(v, unit))))
                    .collect()
            };
            format!(
                r#"<tr><th scope="row">{}</th>{cells}</tr>"#,
                escape(&row.label)
            )
        })
        .collect();
    format!(
        r#"<table class="oc-sr-only">{caption}<thead><tr><th>Label</th>{head_cols}</tr></thead><tbody>{body_rows}</tbody></table>"#
    )
}

fn tooltip_text(label: &str, series_name: Option<&str>, value: f64, unit: &str) -> String {
    let val_str = format_val_unit(value, unit);
    match series_name {
        Some(series) => format!("{label} ({series}): {val_str}"),
        None => format!("{label}: {val_str}"),
    }
}

fn render_bar_svg(
    rows: &[DataRow],
    series_count: usize,
    series_names: &[String],
    stacked: bool,
    axis_max: f64,
    unit: &str,
    aria_label: &str,
) -> String {
    let left = 64.0_f64;
    let right = 620.0_f64;
    let top = 24.0_f64;
    let bottom = 264.0_f64;
    let plot_w = right - left;
    let plot_h = bottom - top;

    let mut svg = format!(
        r#"<svg class="oc-svg" viewBox="0 0 640 320" role="img" aria-label="{}">"#,
        escape(aria_label)
    );

    // 水平グリッドと Y 軸目盛り
    for i in 0..=4 {
        let ratio = i as f64 / 4.0;
        let y = bottom - plot_h * ratio;
        let class = if i == 0 { "oc-axis" } else { "oc-grid" };
        svg.push_str(&format!(
            r#"<line class="{class}" x1="{left:.1}" y1="{y:.1}" x2="{right:.1}" y2="{y:.1}"></line>"#
        ));
        let tick_val = axis_max * ratio;
        let tick_text = if i == 4 && !unit.is_empty() {
            format_val_unit(tick_val, unit)
        } else {
            format_num(tick_val)
        };
        svg.push_str(&format!(
            r#"<text class="oc-tick" x="56" y="{y:.1}" text-anchor="end" dominant-baseline="central">{}</text>"#,
            escape(&tick_text)
        ));
    }

    let slot_w = plot_w / rows.len() as f64;
    let max_label_units = ((slot_w / 7.0).floor() as usize).clamp(6, 16);

    for (idx, row) in rows.iter().enumerate() {
        let slot_x = left + slot_w * idx as f64;
        let slot_center = slot_x + slot_w / 2.0;

        if stacked {
            let bar_w = (slot_w * 0.6).clamp(4.0, 48.0);
            let bar_x = slot_center - bar_w / 2.0;
            let mut cur_y = bottom;
            for (s, &val) in row.values.iter().enumerate() {
                let h = ((val / axis_max) * plot_h).clamp(0.0, cur_y - top);
                if h > 0.0 {
                    let y = cur_y - h;
                    let tip = tooltip_text(
                        &row.label,
                        series_names.get(s).map(String::as_str),
                        val,
                        unit,
                    );
                    svg.push_str(&format!(
                        r#"<rect class="oc-bar oc-s{s}" x="{bar_x:.2}" y="{y:.2}" width="{bar_w:.2}" height="{h:.2}"><title>{}</title></rect>"#,
                        escape(&tip)
                    ));
                    cur_y = y;
                }
            }
        } else {
            let group_w = (slot_w * 0.72).clamp(4.0, 48.0 * series_count as f64);
            let gap = if series_count > 1 { 2.0 } else { 0.0 };
            let bar_w =
                ((group_w - gap * (series_count as f64 - 1.0)) / series_count as f64).max(2.0);
            let actual_group_w = bar_w * series_count as f64 + gap * (series_count as f64 - 1.0);
            let start_x = slot_center - actual_group_w / 2.0;

            for (s, &val) in row.values.iter().enumerate() {
                let h = ((val / axis_max) * plot_h).clamp(0.0, plot_h);
                let x = start_x + s as f64 * (bar_w + gap);
                let y = bottom - h;
                let tip = tooltip_text(
                    &row.label,
                    series_names.get(s).map(String::as_str),
                    val,
                    unit,
                );
                svg.push_str(&format!(
                    r#"<rect class="oc-bar oc-s{s}" x="{x:.2}" y="{y:.2}" width="{bar_w:.2}" height="{h:.2}" rx="3"><title>{}</title></rect>"#,
                    escape(&tip)
                ));
                if series_count == 1 && rows.len() <= 12 {
                    let val_y = (y - 6.0).max(top - 4.0);
                    svg.push_str(&format!(
                        r#"<text class="oc-val" x="{:.2}" y="{val_y:.2}" text-anchor="middle">{}</text>"#,
                        x + bar_w / 2.0,
                        escape(&format_num(val))
                    ));
                }
            }
        }

        let short_label = truncate_label(&row.label, max_label_units);
        svg.push_str(&format!(
            r#"<text class="oc-axis-label" x="{slot_center:.2}" y="286" text-anchor="middle">{}</text>"#,
            escape(&short_label)
        ));
    }

    svg.push_str("</svg>");
    svg
}

fn render_horizontal_bar_svg(
    rows: &[DataRow],
    series_count: usize,
    series_names: &[String],
    axis_max: f64,
    unit: &str,
    aria_label: &str,
) -> String {
    let left = 140.0_f64;
    let right = 560.0_f64;
    let plot_w = right - left;
    let top = 20.0_f64;
    let bottom_pad = 36.0_f64;

    let bar_h = 18.0_f64;
    let bar_gap = 4.0_f64;
    let row_pad = 14.0_f64;
    let group_h = bar_h * series_count as f64 + bar_gap * (series_count as f64 - 1.0);
    let row_stride = group_h + row_pad;
    let plot_bottom = top + row_stride * rows.len() as f64;
    let total_h = (plot_bottom + bottom_pad).ceil();

    let mut svg = format!(
        r#"<svg class="oc-svg" viewBox="0 0 640 {total_h:.0}" role="img" aria-label="{}">"#,
        escape(aria_label)
    );

    // 垂直グリッドと X 軸目盛り
    for i in 0..=4 {
        let ratio = i as f64 / 4.0;
        let x = left + plot_w * ratio;
        let class = if i == 0 { "oc-axis" } else { "oc-grid" };
        svg.push_str(&format!(
            r#"<line class="{class}" x1="{x:.1}" y1="{top:.1}" x2="{x:.1}" y2="{plot_bottom:.1}"></line>"#
        ));
        let tick_val = axis_max * ratio;
        let tick_text = if i == 4 && !unit.is_empty() {
            format_val_unit(tick_val, unit)
        } else {
            format_num(tick_val)
        };
        svg.push_str(&format!(
            r#"<text class="oc-tick" x="{x:.1}" y="{:.1}" text-anchor="middle">{}</text>"#,
            plot_bottom + 18.0,
            escape(&tick_text)
        ));
    }

    for (idx, row) in rows.iter().enumerate() {
        let row_top = top + idx as f64 * row_stride + row_pad / 2.0;
        let row_center = row_top + group_h / 2.0;
        let short_label = truncate_label(&row.label, 18);
        svg.push_str(&format!(
            r#"<text class="oc-axis-label" x="130" y="{row_center:.2}" text-anchor="end" dominant-baseline="central">{}</text>"#,
            escape(&short_label)
        ));

        for (s, &val) in row.values.iter().enumerate() {
            let y = row_top + s as f64 * (bar_h + bar_gap);
            let w = ((val / axis_max) * plot_w).clamp(0.0, plot_w);
            let tip = tooltip_text(
                &row.label,
                series_names.get(s).map(String::as_str),
                val,
                unit,
            );
            svg.push_str(&format!(
                r#"<rect class="oc-bar oc-s{s}" x="{left:.1}" y="{y:.2}" width="{w:.2}" height="{bar_h:.1}" rx="3"><title>{}</title></rect>"#,
                escape(&tip)
            ));
            let val_x = left + w + 6.0;
            let val_y = y + bar_h / 2.0;
            svg.push_str(&format!(
                r#"<text class="oc-val" x="{val_x:.2}" y="{val_y:.2}" dominant-baseline="central">{}</text>"#,
                escape(&format_val_unit(val, unit))
            ));
        }
    }

    svg.push_str("</svg>");
    svg
}

fn render_line_svg(
    rows: &[DataRow],
    series_count: usize,
    series_names: &[String],
    axis_max: f64,
    unit: &str,
    aria_label: &str,
) -> String {
    let left = 64.0_f64;
    let right = 620.0_f64;
    let top = 24.0_f64;
    let bottom = 264.0_f64;
    let plot_w = right - left;
    let plot_h = bottom - top;

    let mut svg = format!(
        r#"<svg class="oc-svg" viewBox="0 0 640 320" role="img" aria-label="{}">"#,
        escape(aria_label)
    );

    // 水平グリッドと Y 軸目盛り
    for i in 0..=4 {
        let ratio = i as f64 / 4.0;
        let y = bottom - plot_h * ratio;
        let class = if i == 0 { "oc-axis" } else { "oc-grid" };
        svg.push_str(&format!(
            r#"<line class="{class}" x1="{left:.1}" y1="{y:.1}" x2="{right:.1}" y2="{y:.1}"></line>"#
        ));
        let tick_val = axis_max * ratio;
        let tick_text = if i == 4 && !unit.is_empty() {
            format_val_unit(tick_val, unit)
        } else {
            format_num(tick_val)
        };
        svg.push_str(&format!(
            r#"<text class="oc-tick" x="56" y="{y:.1}" text-anchor="end" dominant-baseline="central">{}</text>"#,
            escape(&tick_text)
        ));
    }

    let slot_w = plot_w / rows.len() as f64;
    let max_label_units = ((slot_w / 7.0).floor() as usize).clamp(6, 16);
    let x_for = |idx: usize| left + slot_w * (idx as f64 + 0.5);

    // 単一系列かつ 2 点以上のときは面グラフの薄い塗りを先に描く
    if series_count == 1 && rows.len() >= 2 {
        let mut pts = Vec::with_capacity(rows.len() + 2);
        for (idx, row) in rows.iter().enumerate() {
            let x = x_for(idx);
            let y = bottom - ((row.values[0] / axis_max) * plot_h).clamp(0.0, plot_h);
            pts.push(format!("{x:.2},{y:.2}"));
        }
        pts.push(format!("{:.2},{bottom:.1}", x_for(rows.len() - 1)));
        pts.push(format!("{:.2},{bottom:.1}", x_for(0)));
        svg.push_str(&format!(
            r#"<polygon class="oc-area oc-s0" fill-opacity="0.14" points="{}"></polygon>"#,
            pts.join(" ")
        ));
    }

    for s in 0..series_count {
        let mut line_pts = Vec::with_capacity(rows.len());
        let mut dots = String::new();
        for (idx, row) in rows.iter().enumerate() {
            let val = row.values[s];
            let x = x_for(idx);
            let y = bottom - ((val / axis_max) * plot_h).clamp(0.0, plot_h);
            line_pts.push(format!("{x:.2},{y:.2}"));
            let tip = tooltip_text(
                &row.label,
                series_names.get(s).map(String::as_str),
                val,
                unit,
            );
            dots.push_str(&format!(
                r#"<circle class="oc-dot oc-s{s}" cx="{x:.2}" cy="{y:.2}" r="4"><title>{}</title></circle>"#,
                escape(&tip)
            ));
        }
        if rows.len() >= 2 {
            svg.push_str(&format!(
                r#"<polyline class="oc-line oc-s{s}" fill="none" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" points="{}"></polyline>"#,
                line_pts.join(" ")
            ));
        }
        svg.push_str(&dots);
    }

    for (idx, row) in rows.iter().enumerate() {
        let x = x_for(idx);
        let short_label = truncate_label(&row.label, max_label_units);
        svg.push_str(&format!(
            r#"<text class="oc-axis-label" x="{x:.2}" y="286" text-anchor="middle">{}</text>"#,
            escape(&short_label)
        ));
    }

    svg.push_str("</svg>");
    svg
}

fn render_donut_html(rows: &[DataRow], unit: &str, aria_label: &str) -> Option<String> {
    let total: f64 = rows.iter().map(|r| r.values[0]).sum();
    if total <= 0.0 || !total.is_finite() {
        return None;
    }

    let mut slices = String::new();
    let mut legend = String::new();
    let mut offset = 0.0_f64;

    for (idx, row) in rows.iter().enumerate() {
        let val = row.values[0];
        let color_idx = idx % MAX_SERIES;
        let pct = (val / total) * 100.0;
        let slice_len = (val / total) * 1000.0;
        let dash_offset = if offset == 0.0 {
            "0".to_string()
        } else {
            format!("-{offset:.2}")
        };
        let tip = format!("{}: {} ({pct:.1}%)", row.label, format_val_unit(val, unit));
        slices.push_str(&format!(
            r#"<circle class="oc-slice oc-s{color_idx}" cx="100" cy="100" r="68" fill="none" stroke-width="32" pathLength="1000" stroke-dasharray="{slice_len:.2} 1000" stroke-dashoffset="{dash_offset}" transform="rotate(-90 100 100)"><title>{}</title></circle>"#,
            escape(&tip)
        ));
        legend.push_str(&format!(
            r#"<li><span class="oc-swatch oc-s{color_idx}" aria-hidden="true"></span><span>{}</span><span class="oc-donut-pct">{} ({pct:.1}%)</span></li>"#,
            escape(&row.label),
            escape(&format_val_unit(val, unit))
        ));
        offset += slice_len;
    }

    let total_text = format_val_unit(total, unit);
    Some(format!(
        r#"<div class="oc-donut-wrap"><svg class="oc-donut-svg" viewBox="0 0 200 200" role="img" aria-label="{}">{slices}<text class="oc-donut-total" x="100" y="100" text-anchor="middle" dominant-baseline="central">{}</text></svg><ul class="oc-legend oc-donut-legend">{legend}</ul></div>"#,
        escape(aria_label),
        escape(&total_text)
    ))
}

fn render_chart(block: &BlockNode) -> Option<String> {
    let chart_type = ChartType::from_attr(attr(block, "type"));
    let title = attr(block, "title");
    let unit = attr(block, "unit");
    let mut series_names = parse_series(block);
    let mut rows = parse_items(block);
    if rows.is_empty() {
        return None;
    }

    let aria_label = if title.is_empty() {
        chart_type.default_aria_label()
    } else {
        title
    };

    let (body_html, legend_html, table_series): (String, String, Vec<String>) =
        if chart_type == ChartType::Donut {
            rows.retain(|r| r.values[0] > 0.0);
            if rows.is_empty() {
                return None;
            }
            for row in &mut rows {
                row.values.truncate(1);
            }
            let donut = render_donut_html(&rows, unit, aria_label)?;
            (donut, String::new(), Vec::new())
        } else {
            let max_row_series = rows.iter().map(|r| r.values.len()).max().unwrap_or(0);
            let series_count = series_names.len().max(max_row_series).clamp(1, MAX_SERIES);

            for row in &mut rows {
                row.values.truncate(series_count);
                if row.values.len() < series_count {
                    row.values.resize(series_count, 0.0);
                }
            }

            let show_legend = series_count > 1 || !series_names.is_empty();
            if show_legend && series_names.len() < series_count {
                for i in series_names.len()..series_count {
                    series_names.push(format!("Series {}", i + 1));
                }
            }

            let data_max = if chart_type == ChartType::StackedBar {
                rows.iter()
                    .map(|r| r.values.iter().sum::<f64>())
                    .fold(0.0_f64, f64::max)
            } else {
                rows.iter()
                    .flat_map(|r| r.values.iter().copied())
                    .fold(0.0_f64, f64::max)
            };

            let explicit_max = number_attr(block, "max").filter(|v| *v > 0.0);
            let axis_max = match explicit_max {
                Some(m) => m.max(data_max),
                None => nice_axis_max(data_max),
            };
            if axis_max <= 0.0 || !axis_max.is_finite() {
                return None;
            }

            let svg = match chart_type {
                ChartType::Bar => render_bar_svg(
                    &rows,
                    series_count,
                    &series_names,
                    false,
                    axis_max,
                    unit,
                    aria_label,
                ),
                ChartType::StackedBar => render_bar_svg(
                    &rows,
                    series_count,
                    &series_names,
                    true,
                    axis_max,
                    unit,
                    aria_label,
                ),
                ChartType::HorizontalBar => render_horizontal_bar_svg(
                    &rows,
                    series_count,
                    &series_names,
                    axis_max,
                    unit,
                    aria_label,
                ),
                ChartType::Line => render_line_svg(
                    &rows,
                    series_count,
                    &series_names,
                    axis_max,
                    unit,
                    aria_label,
                ),
                ChartType::Donut => unreachable!(),
            };

            let legend = if show_legend {
                render_cartesian_legend(&series_names)
            } else {
                String::new()
            };
            (svg, legend, series_names)
        };

    let title_html = if title.is_empty() {
        String::new()
    } else {
        format!(
            r#"<figcaption class="oc-title">{}</figcaption>"#,
            escape(title)
        )
    };
    let table_html = render_sr_table(
        title,
        unit,
        &table_series,
        &rows,
        chart_type == ChartType::Donut,
    );

    Some(format!(
        r#"<figure class="oc-chart oc-chart-{}">{title_html}{legend_html}{body_html}{table_html}</figure>"#,
        chart_type.class_suffix()
    ))
}

impl Plugin for ChartsPlugin {
    fn render_blocks(&mut self, blocks: Vec<BlockNode>) -> Result<Vec<Option<String>>> {
        let mut styled = false;
        Ok(blocks
            .iter()
            .map(|block| {
                let html = match block.name.as_str() {
                    "official-charts/chart" => render_chart(block),
                    _ => None,
                }?;
                if styled {
                    Some(html)
                } else {
                    styled = true;
                    Some(format!("{STYLES}{html}"))
                }
            })
            .collect())
    }
}

aegis_plugin_sdk::register_plugin!(ChartsPlugin);

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    fn chart_block(attrs: &[(&str, serde_json::Value)]) -> BlockNode {
        BlockNode {
            name: "official-charts/chart".to_string(),
            attributes: attrs
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect::<HashMap<_, _>>(),
            text: None,
            children: Vec::new(),
        }
    }

    #[test]
    fn renders_all_five_chart_types_without_inline_style_attrs() {
        let items = json!([
            {"label": "Jan", "values": "100, 140"},
            {"label": "Feb", "values": "120, 160"},
            {"label": "Mar", "values": "90, 180"}
        ]);

        let blocks = vec![
            chart_block(&[
                ("type", json!("bar")),
                ("title", json!("Monthly Sales")),
                ("unit", json!("k")),
                ("series", json!("2025, 2026")),
                ("items", items.clone()),
            ]),
            chart_block(&[
                ("type", json!("stacked-bar")),
                ("series", json!("2025, 2026")),
                ("items", items.clone()),
            ]),
            chart_block(&[
                ("type", json!("horizontal-bar")),
                ("series", json!("2025, 2026")),
                ("unit", json!("req/s")),
                ("items", items.clone()),
            ]),
            chart_block(&[
                ("type", json!("line")),
                ("series", json!("2025, 2026")),
                ("items", items),
            ]),
            chart_block(&[
                ("type", json!("donut")),
                ("title", json!("Traffic")),
                ("unit", json!("%")),
                (
                    "items",
                    json!([
                        {"label": "Search", "values": "60"},
                        {"label": "Social", "values": "40"}
                    ]),
                ),
            ]),
        ];

        let rendered = ChartsPlugin.render_blocks(blocks).unwrap();
        assert_eq!(rendered.len(), 5);

        // <style> は最初の有効なブロックに 1 回だけ付く
        let bar = rendered[0].as_deref().unwrap();
        assert!(bar.starts_with("<style>"));
        assert!(bar.contains("oc-chart-bar"));
        assert!(bar.contains("<figcaption class=\"oc-title\">Monthly Sales</figcaption>"));
        assert!(bar.contains("2025"));
        assert!(bar.contains("2026"));
        assert_eq!(bar.matches("<rect ").count(), 6);

        let stacked = rendered[1].as_deref().unwrap();
        assert!(!stacked.contains("<style>"));
        assert!(stacked.contains("oc-chart-stacked-bar"));
        assert_eq!(stacked.matches("<rect ").count(), 6);

        let hbar = rendered[2].as_deref().unwrap();
        assert!(hbar.contains("oc-chart-horizontal-bar"));
        assert_eq!(hbar.matches("<rect ").count(), 6);
        assert!(hbar.contains("req/s"));

        let line = rendered[3].as_deref().unwrap();
        assert!(line.contains("oc-chart-line"));
        assert_eq!(line.matches("<polyline ").count(), 2);
        assert_eq!(line.matches("<circle ").count(), 6);

        let donut = rendered[4].as_deref().unwrap();
        assert!(donut.contains("oc-chart-donut"));
        assert_eq!(donut.matches("<circle ").count(), 2);
        assert!(donut.contains("100%"));
        assert!(donut.contains("60.0%"));
        assert!(donut.contains("40.0%"));

        // Strict CSP 準拠: `style="..."` 属性を一切含まないこと
        for (i, out) in rendered.iter().enumerate() {
            let html = out.as_deref().unwrap();
            assert!(!html.contains("style=\""), "block {i} leaked style attr");
            assert!(!html.contains("<script"), "block {i} leaked script tag");
        }
    }

    #[test]
    fn renders_single_series_area_line_and_flexible_value_formats() {
        let blocks = vec![
            // 古い / API 形式: `value: 120` や配列 `values: [120, 150]`、欠損系列の 0 埋め
            chart_block(&[
                ("type", json!("bar")),
                (
                    "items",
                    json!([
                        {"label": "A", "value": 10},
                        {"label": "B", "values": [20, 30]},
                        {"label": "C", "values": "15"}
                    ]),
                ),
            ]),
            // 単一系列の折れ線にはエリアポリゴンが付く
            chart_block(&[
                ("type", json!("line")),
                (
                    "items",
                    json!([
                        {"label": "P1", "value": 10},
                        {"label": "P2", "value": 40}
                    ]),
                ),
            ]),
        ];

        let rendered = ChartsPlugin.render_blocks(blocks).unwrap();
        let bar = rendered[0].as_deref().unwrap();
        // 自動補完された系列名と 2 系列分のバー（3 項目 * 2 系列 = 6 本）
        assert!(bar.contains("Series 1"));
        assert!(bar.contains("Series 2"));
        assert_eq!(bar.matches("<rect ").count(), 6);

        let line = rendered[1].as_deref().unwrap();
        assert!(line.contains(r#"<polygon class="oc-area oc-s0""#));
    }

    #[test]
    fn caps_series_and_items_and_respects_explicit_max() {
        let many_items: Vec<_> = (0..60)
            .map(|i| json!({"label": format!("L{i}"), "values": "1, 2, 3, 4, 5, 6, 7, 8"}))
            .collect();
        let block = chart_block(&[
            ("type", json!("bar")),
            ("series", json!("S1, S2, S3, S4, S5, S6, S7, S8")),
            ("max", json!(100)),
            ("items", json!(many_items)),
        ]);

        let html = ChartsPlugin.render_blocks(vec![block]).unwrap()[0]
            .clone()
            .unwrap();
        // 50 行 × 6 系列 = 300 本に制限
        assert_eq!(html.matches("<rect ").count(), 300);
        assert!(html.contains("S6"));
        assert!(!html.contains("S7"));
        assert!(!html.contains("L50"));
        assert!(html.contains(">100</text>"));
    }

    #[test]
    fn escapes_html_in_all_user_inputs() {
        let block = chart_block(&[
            ("type", json!("bar")),
            ("title", json!("<script>alert(1)</script>")),
            ("unit", json!("\"<b>%</b>\"")),
            ("series", json!("<img onerror=x>")),
            (
                "items",
                json!([{"label": "<svg onload=x>", "values": "42"}]),
            ),
        ]);
        let html = ChartsPlugin.render_blocks(vec![block]).unwrap()[0]
            .clone()
            .unwrap();
        assert!(!html.contains("<script>"));
        assert!(!html.contains("<svg onload"));
        assert!(!html.contains("<img "));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(html.contains("&lt;svg onload=x&gt;"));
        assert!(html.contains("&lt;img onerror=x&gt;"));
    }

    #[test]
    fn returns_none_for_invalid_or_empty_inputs_and_styles_first_valid_only() {
        let blocks = vec![
            chart_block(&[("items", json!([]))]),
            chart_block(&[(
                "items",
                json!([{"label": "Zero", "values": "0"}, {"label": "Neg", "values": "-5"}]),
            )]),
            chart_block(&[("items", json!([{"label": "Bad", "values": "not-a-number"}]))]),
            chart_block(&[
                ("type", json!("donut")),
                ("items", json!([{"label": "Zero", "values": "0"}])),
            ]),
            BlockNode::new("official-charts/unknown"),
            chart_block(&[("items", json!([{"label": "Valid", "values": "50"}]))]),
        ];
        let rendered = ChartsPlugin.render_blocks(blocks).unwrap();
        assert_eq!(&rendered[..5], &[None, None, None, None, None]);
        let valid = rendered[5].as_deref().unwrap();
        assert!(valid.starts_with("<style>"));
        assert!(valid.contains("Valid"));
    }
}
