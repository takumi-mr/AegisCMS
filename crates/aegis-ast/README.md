# aegis-ast

**English** | [日本語](README.ja.md)

The abstract syntax tree (AST) data structure for block content, a deterministic HTML renderer, and a streaming HTML rewriter based on Cloudflare's `lol-html`.

## Overview

AegisCMS does not store or edit content directly in the database as raw HTML strings; it manages content strictly as a structured block tree (AST).

`aegis-ast` handles the following pipeline:

1. **Block AST representation**: Representing and manipulating a Gutenberg-like block data structure.
2. **Deterministic HTML rendering**: Converting the AST into safe, consistent semantic HTML.
3. **Security and streaming rewriting**: Using Cloudflare's `lol-html` to automatically add `rel="noopener noreferrer"` to external links and sanitize XSS-prone tags, with low-latency streaming processing.

## Responsibilities and Features

- **BlockNode**:
    - A tree structure that represents standard blocks such as `core/paragraph`, `core/heading`, `core/image`, and `core/table`, as well as custom blocks.
    - Holds attributes (`attributes`), inline markup text (`text`), and child nodes (`children`).
- **HtmlRenderer**:
    - Fast, deterministic rendering from the AST to standard HTML.
    - Parsing and safe escaping of inline markup (bold, italic, links, code).
    - Link extraction from the rendered result (`extract_links`).
- **StreamingHtmlRewriter (`lol-html`)**:
    - Memory-efficient, zero-copy streaming rewriting of rendered or external HTML.
    - Automatic `rel="noopener noreferrer"` on external-domain links.
    - Hardening of `target="_blank"`.
- **AstPipeline / AstTransformer**:
    - Automatic ID assignment for headings (`HeadingIdTransformer`) and building AST transformation pipelines with plugins.

## Module Layout

| Module      | Description                                                                                         |
| :---------- | :-------------------------------------------------------------------------------------------------- |
| `node`      | Definition of `BlockNode`, the basic block AST struct                                               |
| `inline`    | Parsing inline HTML markup and extracting plain text (`plain_text`, `render_inline`)                |
| `renderer`  | Deterministic AST-to-HTML renderer (`HtmlRenderer`)                                                 |
| `rewriter`  | Streaming HTML rewriting with `lol-html` (`StreamingHtmlRewriter`)                                  |
| `transform` | AST traversal and transformation pipeline (`AstPipeline`, `AstTransformer`, `HeadingIdTransformer`) |
| `config`    | Security and HTML transformation settings (`SecurityConfig`)                                        |

## Usage Example

```rust
use aegis_ast::{BlockNode, HtmlRenderer, SecurityConfig, StreamingHtmlRewriter};

// 1. Build an AST
let mut heading = BlockNode::heading(2, "Hello AegisCMS");
heading.attributes.insert("id".into(), "hello".into());

let paragraph = BlockNode::paragraph("Safe and secure CMS.");
let blocks = vec![heading, paragraph];

// 2. Render to HTML
let renderer = HtmlRenderer::new();
let html = renderer.render(&blocks);

// 3. Add security attributes with the streaming rewriter
let config = SecurityConfig::default();
let rewriter = StreamingHtmlRewriter::new(&config);
let secured_html = rewriter.rewrite(&html).expect("rewrite failed");
```
