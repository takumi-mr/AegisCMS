//! MySQL の索引が、InnoDB の上限（3072 バイト）に収まっているかを、手順の SQL を読んで調べる。
//!
//! MySQL 8.0 の標準の文字セットは utf8mb4 で、`VARCHAR(255)` の 1 列は索引の中で最大 255 × 4 = 1020 バイト
//! になる。複合索引が 4 列だと 4080 バイトで上限を超え、`1071 Specified key was too long` で `init` が
//! 止まる。CI に MySQL は無いので、実機の MySQL を使わずに、ここで防ぐ（`task docker:dbs:up` の実機テストは
//! 手元で動かす: AGENTS.md の 2 を参照）。
//!
//! 見るのは `CREATE TABLE` の中の索引（`INDEX`・`KEY`・`UNIQUE`・`PRIMARY KEY`）と、列の `UNIQUE` /
//! `PRIMARY KEY`。列の型は `VARCHAR(n)` / `CHAR(n)`（n × 4 バイト）と整数（8 バイト以下）として数える。

use std::collections::HashMap;
use std::path::Path;

/// InnoDB の索引の上限（バイト）
const MAX_KEY_BYTES: usize = 3072;
/// utf8mb4 の 1 文字の最大バイト数
const BYTES_PER_CHAR: usize = 4;

/// 括弧の外側のカンマで分ける
fn split_top_level(body: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    for c in body.chars() {
        match c {
            '(' => {
                depth += 1;
                current.push(c);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                current.push(c);
            }
            ',' if depth == 0 => parts.push(std::mem::take(&mut current)),
            _ => current.push(c),
        }
    }
    if !current.trim().is_empty() {
        parts.push(current);
    }
    parts.into_iter().map(|p| p.trim().to_string()).collect()
}

/// 列の型から、索引の中での最大バイト数
fn column_bytes(definition: &str) -> usize {
    let upper = definition.to_ascii_uppercase();
    for prefix in ["VARCHAR(", "CHAR("] {
        if let Some(rest) = upper.strip_prefix(prefix) {
            let n: usize = rest
                .split(')')
                .next()
                .and_then(|n| n.trim().parse().ok())
                .unwrap_or(255);
            return n * BYTES_PER_CHAR;
        }
    }
    // 整数・日時など。TEXT / BLOB は長さの指定なしでは索引にできない（そのときは MySQL が別のエラーにする）
    8
}

struct Violation {
    table: String,
    index: String,
    bytes: usize,
}

/// 手順の SQL（複数の文）から、上限を超える索引を探す
fn oversized_indexes(sql: &str) -> Vec<Violation> {
    let mut found = Vec::new();
    // 行頭の `--` のコメントは外す（文の先頭にあると `CREATE TABLE` と読めない）
    let sql: String = sql
        .lines()
        .filter(|line| !line.trim_start().starts_with("--"))
        .collect::<Vec<_>>()
        .join(" ");
    for statement in sql.split(';') {
        let statement = statement.trim();
        let upper = statement.to_ascii_uppercase();
        if !upper.starts_with("CREATE TABLE") {
            continue;
        }
        let (Some(open), Some(close)) = (statement.find('('), statement.rfind(')')) else {
            continue;
        };
        let name = statement[..open]
            .split_whitespace()
            .last()
            .unwrap_or("?")
            .trim_matches('`')
            .to_string();
        let parts = split_top_level(&statement[open + 1..close]);

        // 列の型（`id VARCHAR(255) PRIMARY KEY` の先頭 2 語）
        let mut columns: HashMap<String, usize> = HashMap::new();
        for part in &parts {
            let first = part
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_uppercase();
            if matches!(
                first.as_str(),
                "PRIMARY" | "UNIQUE" | "INDEX" | "KEY" | "FOREIGN" | "CONSTRAINT" | "FULLTEXT"
            ) {
                continue;
            }
            let mut words = part.splitn(2, char::is_whitespace);
            let (Some(column), Some(rest)) = (words.next(), words.next()) else {
                continue;
            };
            columns.insert(
                column.trim_matches('`').to_string(),
                column_bytes(rest.trim()),
            );
        }

        let mut check = |index: String, key_columns: &str| {
            let bytes: usize = key_columns
                .split(',')
                .map(|c| {
                    let c = c.trim();
                    // `col(64)` は、先頭の 64 文字だけを索引にする
                    let (column, prefix) = match c.split_once('(') {
                        Some((column, rest)) => (
                            column.trim().trim_matches('`'),
                            rest.trim_end_matches(')').trim().parse::<usize>().ok(),
                        ),
                        None => (c.trim_matches('`'), None),
                    };
                    let full = columns.get(column).copied().unwrap_or(8);
                    prefix.map_or(full, |p| (p * BYTES_PER_CHAR).min(full))
                })
                .sum();
            if bytes > MAX_KEY_BYTES {
                found.push(Violation {
                    table: name.clone(),
                    index,
                    bytes,
                });
            }
        };

        for part in &parts {
            let upper = part.to_ascii_uppercase();
            let is_index = ["PRIMARY KEY", "UNIQUE", "INDEX", "KEY", "FULLTEXT"]
                .iter()
                .any(|k| upper.starts_with(k));
            if is_index {
                // 索引の名前（あれば）と、列の並び（最初の括弧の中）
                let Some(open) = part.find('(') else { continue };
                let Some(close) = part.rfind(')') else {
                    continue;
                };
                let label = part[..open].trim().to_string();
                check(label, &part[open + 1..close]);
            } else if upper.contains(" UNIQUE") || upper.contains(" PRIMARY KEY") {
                // 列の定義の中の `UNIQUE` / `PRIMARY KEY`
                if let Some(column) = part.split_whitespace().next() {
                    check(format!("{column} (column)"), column);
                }
            }
        }
    }
    found
}

#[test]
fn test_mysql_indexes_fit_in_the_innodb_key_limit_for_utf8mb4() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations/mysql");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "sql"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no migrations in {}", dir.display());

    let mut problems = Vec::new();
    for file in files {
        let sql = std::fs::read_to_string(&file).unwrap();
        for v in oversized_indexes(&sql) {
            problems.push(format!(
                "{}: {} / {} is {} bytes (limit {MAX_KEY_BYTES}). Shorten it with a prefix such as `col(64)`",
                file.file_name().unwrap().to_string_lossy(),
                v.table,
                v.index,
                v.bytes
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// 検査自体が働く: 直す前の `_aegis_purchases` の索引（4 列 × 1020 バイト）は見つけ、
/// 先頭の文字数を絞った索引と、1 列の `UNIQUE` は通す
#[test]
fn test_the_checker_finds_an_index_over_the_limit() {
    let before = "CREATE TABLE IF NOT EXISTS t (id VARCHAR(255) PRIMARY KEY, a VARCHAR(255), b VARCHAR(255), \
                  c VARCHAR(255), d VARCHAR(255), INDEX idx_owner (a, b, c, d));";
    let found = oversized_indexes(before);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].index, "INDEX idx_owner");
    assert_eq!(found[0].bytes, 4080);

    let after = "CREATE TABLE IF NOT EXISTS t (id VARCHAR(255) PRIMARY KEY, a VARCHAR(255) UNIQUE, b VARCHAR(255), \
                 c VARCHAR(255), d VARCHAR(255), INDEX idx_owner (a(64), b(64), c(128), d(32)));";
    assert!(oversized_indexes(after).is_empty());
}
