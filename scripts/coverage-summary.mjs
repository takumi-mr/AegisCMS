#!/usr/bin/env node
// `cargo llvm-cov report --json --summary-only` の出力から、クレートごとの行カバレッジの表（Markdown）を作る。
//
//   cargo llvm-cov report --json --summary-only --output-path target/coverage.json
//   node scripts/coverage-summary.mjs target/coverage.json
//
// 表は標準出力へ。CI では `$GITHUB_STEP_SUMMARY` に追記する。しきい値で落とすことはしない（計測だけ）。

import { readFileSync } from "node:fs";

const file = process.argv[2];
if (!file) {
  console.error("usage: node scripts/coverage-summary.mjs <coverage.json>");
  process.exit(2);
}

const report = JSON.parse(readFileSync(file, "utf8"));
const files = report.data?.[0]?.files ?? [];

/** ファイルのパスから、クレート名（`crates/<name>/…`）を取り出す。ワークスペースの外は `(other)` */
export function crateOf(filename) {
  const match = /(?:^|[\\/])crates[\\/]([^\\/]+)[\\/]/.exec(filename);
  return match ? match[1] : "(other)";
}

const crates = new Map();
for (const { filename, summary } of files) {
  if (summary.lines.count === 0) continue;
  const name = crateOf(filename);
  const entry = crates.get(name) ?? { count: 0, covered: 0 };
  entry.count += summary.lines.count;
  entry.covered += summary.lines.covered;
  crates.set(name, entry);
}

const percent = ({ count, covered }) => (count === 0 ? 100 : (covered / count) * 100);
const rows = [...crates.entries()].sort((a, b) => percent(a[1]) - percent(b[1]));
const total = rows.reduce(
  (sum, [, e]) => ({ count: sum.count + e.count, covered: sum.covered + e.covered }),
  { count: 0, covered: 0 },
);

const lines = [
  "## Line coverage by crate",
  "",
  `Total: **${percent(total).toFixed(1)}%** (${total.covered} / ${total.count} lines)`,
  "",
  "| Crate | Lines | Covered | Coverage |",
  "| --- | ---: | ---: | ---: |",
  ...rows.map(
    ([name, e]) => `| ${name} | ${e.count} | ${e.covered} | ${percent(e).toFixed(1)}% |`,
  ),
  "",
  "Sorted from the lowest. This only measures: it does not fail the build.",
];
console.log(lines.join("\n"));
