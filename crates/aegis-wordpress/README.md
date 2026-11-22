# aegis-wordpress

**English** | [日本語](README.ja.md)

The file-reading half of moving from WordPress: it parses a WordPress export (WXR) and converts post HTML into AegisCMS blocks.

## Overview

This crate touches neither the database nor the REST API. It only turns files into plain Rust values. The `aegis-server import wordpress` command (`crates/aegis-server/src/import_wordpress.rs`) uses it, then sends the result to a running server. For the user-facing guide, see [docs/wordpress-migration.md](../../docs/wordpress-migration.md).

## Modules

- **`wxr`**: reads the XML from Tools > Export.
    - `parse_wxr(xml)` returns a `WxrExport`: site info, authors, categories, tags, other terms, and items (posts, pages, attachments) with their comments.
    - Reads one `channel` child at a time, so a large export is not built into one tree.
    - Only reads. Mapping to AegisCMS types (post → article, and so on) is done by the caller.
- **`html`**: a small HTML fragment parser and serializer used by the converter.
- **`blocks`**: `convert(content, &Context)` turns post HTML, Gutenberg comments and shortcodes into `BlockNode`s (from `aegis-ast`).
    - Paragraphs, headings, lists, quotes, code, tables, images, separators and details become the matching blocks. Inline formatting becomes the Markdown subset the text blocks support.
    - Anything that would change meaning when converted (line breaks inside a paragraph, `sup`, nested lists, tables with merged cells) is kept as a `core/html` block, which is sanitized when rendered.
    - Embeds become a URL-only paragraph (link card).
    - Things that could not be converted, or need a manual check, are returned as `Note`s (`shortcode`, `html-kept`, `dropped`, `embed`, `gallery`) for the import report.

## Usage

```rust
use aegis_wordpress::{convert, parse_wxr, Context};

let export = parse_wxr(&std::fs::read_to_string("export.xml")?)?;
for item in &export.items {
    let conversion = convert(&item.content, &Context::default());
    // conversion.blocks: Vec<BlockNode>, conversion.notes: Vec<Note>
}
```

Pass `Context { attachments: Some(&map) }` (attachment ID → URL) so `[gallery ids="1,2"]` can become images.
