# aegis-search

**English** | [日本語](README.ja.md)

A fast, memory-efficient on-site full-text search engine built on Tantivy (`feature = "search"`).

## Overview

`aegis-search` is the crate that extracts plain text from the titles and body blocks (AST) of published content and builds and queries a full-text search index.

It uses the high-performance Rust full-text search library `tantivy` as its engine, and provides a 2-character bigram tokenizer for accurately searching CJK (Chinese, Japanese, Korean) text including Japanese, scoring that favors title matches, and `<mark>` highlighted snippet generation.

## Responsibilities and Features

- **CJK bigram tokenizer (`tokenizer`)**:
  - 2-character bigram splitting that provides lightweight Japanese full-text search without gaps, with no large external morphological analysis dictionary.
  - Proper boundary handling against English words and symbols.
- **Index management (`SearchIndex`)**:
  - Builds the index in memory or on disk (`data/search_index/`).
  - Real-time index updates tied to publishing, updating, and deleting articles.
  - Designed so that a full reindex from the database is possible at any time.
- **Flexible queries and filtering**:
  - Field boosting that scores title matches higher than body matches.
  - Filtering by category ID and content type.
  - AND search across words and phrase search.
- **Highlighted snippet generation (`snippet`)**:
  - Extracts the text around the matched keywords and generates a search result summary highlighted with `<mark>` tags.
- **AST text extraction (`text`)**:
  - Quickly extracts plain text with HTML tags removed from nested block syntax trees (`BlockNode`).

## Module Layout

| Module | Description |
| :--- | :--- |
| `lib` | `SearchIndex` implementation; index building and query execution |
| `tokenizer` | CJK bigram tokenizer |
| `snippet` | Excerpts around search keywords and `<mark>` highlight generation |
| `text` | Plain text extraction from the block AST |
