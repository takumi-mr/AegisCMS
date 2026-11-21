//! WordPress の本文（HTML・Gutenberg のコメント・ショートコード）を AegisCMS のブロックに直す。
//!
//! 方針:
//! - ブロックにきれいに直せるもの（段落・見出し・リスト・引用・コード・表・画像・区切り線・折りたたみ）は
//!   対応するブロックにする。段落の中の書式は、ブロックのテキストで使える Markdown の一部
//!   （`**太字**`・`*斜体*`・`[リンク](URL)` など）に直す。
//! - 直すと見た目や意味が変わるもの（段落内の改行・`<sup>`・入れ子のリスト・結合したセルのある表など）は、
//!   元の HTML を `core/html` ブロックに残す（表示するときに無害化される）。
//! - 動画などの埋め込みは、URL だけの段落（リンクカード）にする。
//! - 直せなかったもの・手直しが要るものは [`Note`] に残す（取り込みの報告に使う）。

use std::collections::HashMap;

use aegis_ast::BlockNode;

use crate::html::{parse_fragment, serialize, Element, Node};

/// 手直しの参考にする、変換中の記録
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// `shortcode`（ショートコードが残った）・`html-kept`（HTML のまま残した）・`dropped`（捨てた）・
    /// `embed`（埋め込みを URL にした）・`gallery`（ギャラリーを直せなかった）
    pub code: &'static str,
    pub detail: String,
}

fn note(code: &'static str, detail: impl Into<String>) -> Note {
    Note {
        code,
        detail: detail.into(),
    }
}

/// 変換の結果
#[derive(Debug, Default, PartialEq)]
pub struct Conversion {
    pub blocks: Vec<BlockNode>,
    pub notes: Vec<Note>,
}

/// 変換に使える、本文の外の情報
#[derive(Debug, Default, Clone, Copy)]
pub struct Context<'a> {
    /// 添付ファイルの ID → URL（`[gallery ids="1,2"]` を画像にするため）
    pub attachments: Option<&'a HashMap<u64, String>>,
}

/// 本文を変換する
pub fn convert(content: &str, ctx: &Context<'_>) -> Conversion {
    let mut converter = Converter {
        ctx: *ctx,
        notes: Vec::new(),
        newline_is_break: false,
    };
    let mut blocks = Vec::new();
    for chunk in split_raw_html(content) {
        match chunk {
            Chunk::Raw(html) => {
                if !html.trim().is_empty() {
                    blocks.push(html_block(html.trim()));
                }
            }
            Chunk::Markup(markup) => {
                let expanded = converter.expand_shortcodes(&markup);
                let nodes = parse_fragment(&expanded);
                converter.blocks(&nodes, &mut blocks);
            }
        }
    }
    Conversion {
        blocks,
        notes: converter.notes,
    }
}

enum Chunk {
    /// `<!-- wp:html -->` の中（そのまま残す）
    Raw(String),
    Markup(String),
}

/// `<!-- wp:html -->…<!-- /wp:html -->` を取り出す
fn split_raw_html(content: &str) -> Vec<Chunk> {
    const CLOSE: &str = "<!-- /wp:html -->";
    let mut chunks = Vec::new();
    let mut rest = content;
    while let Some((start, open_len)) = find_open_html_comment(rest) {
        let after_open = start + open_len;
        let Some(end) = rest[after_open..].find(CLOSE) else {
            break;
        };
        if start > 0 {
            chunks.push(Chunk::Markup(rest[..start].to_string()));
        }
        chunks.push(Chunk::Raw(rest[after_open..after_open + end].to_string()));
        rest = &rest[after_open + end + CLOSE.len()..];
    }
    if !rest.is_empty() || chunks.is_empty() {
        chunks.push(Chunk::Markup(rest.to_string()));
    }
    chunks
}

/// `<!-- wp:html -->`（`<!-- wp:html {"x":1} -->` も）の位置と長さ
fn find_open_html_comment(text: &str) -> Option<(usize, usize)> {
    let mut from = 0;
    while let Some(offset) = text[from..].find("<!-- wp:html") {
        let start = from + offset;
        let after = &text[start + "<!-- wp:html".len()..];
        if after.starts_with(' ') || after.starts_with("-->") {
            let end = after.find("-->")?;
            return Some((start, "<!-- wp:html".len() + end + 3));
        }
        from = start + 1;
    }
    None
}

struct Converter<'a> {
    ctx: Context<'a>,
    notes: Vec<Note>,
    /// 段落の外に直接書かれたテキストの改行を、改行として扱うか（クラシックエディターの本文）
    newline_is_break: bool,
}

const INLINE_TAGS: &[&str] = &[
    "a", "abbr", "b", "bdo", "big", "br", "cite", "code", "del", "dfn", "em", "font", "i", "img",
    "ins", "kbd", "label", "mark", "q", "s", "samp", "small", "span", "strike", "strong", "sub",
    "sup", "time", "tt", "u", "var", "wbr",
];

/// 見た目や意味が変わるので、段落を HTML のまま残す要素
const LOSSY_INLINE: &[&str] = &[
    "sub", "sup", "u", "mark", "ins", "kbd", "samp", "var", "big",
];

const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "svg", "avif", "bmp"];

fn html_block(html: &str) -> BlockNode {
    let mut node = BlockNode::new("core/html");
    node.text = Some(html.to_string());
    node
}

/// ブロックのテキストで、書式として読まれる記号を文字として出す
fn escape_md(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '\\' | '`' | '*' | '~' | '[' | ']' | '!') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// 空白の連なりを 1 つの空白にする（`&nbsp;` は残す）
fn collapse_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_space = false;
    for c in text.chars() {
        if matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{c}') {
            if !in_space {
                out.push(' ');
            }
            in_space = true;
        } else {
            out.push(c);
            in_space = false;
        }
    }
    out
}

/// リンクの行き先として安全か（`http(s)`・`mailto`・`tel`・スキームの無いもの）
fn safe_url(url: &str) -> bool {
    let url = url.trim();
    if url.is_empty() || url.chars().any(char::is_control) {
        return false;
    }
    match url.split_once(':') {
        Some((scheme, _)) if !scheme.contains(['/', '?', '#']) => {
            matches!(
                scheme.to_ascii_lowercase().as_str(),
                "http" | "https" | "mailto" | "tel"
            )
        }
        _ => true,
    }
}

/// Markdown のリンクの `(...)` に入れられる形にする
fn md_url(url: &str) -> String {
    url.trim()
        .replace(' ', "%20")
        .replace('(', "%28")
        .replace(')', "%29")
}

fn is_image_url(url: &str) -> bool {
    let path = url.split(['?', '#']).next().unwrap_or("");
    path.rsplit_once('.')
        .is_some_and(|(_, ext)| IMAGE_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
}

fn is_blank(node: &Node) -> bool {
    match node {
        Node::Text(t) => t.trim().is_empty(),
        Node::Comment(_) => true,
        Node::Element(_) => false,
    }
}

/// 画像 1 枚だけ（空白とコメントは無視する）の `img` と、それを囲む `a`
fn sole_image(nodes: &[Node]) -> Option<(&Element, Option<&Element>)> {
    let mut significant = nodes.iter().filter(|n| !is_blank(n));
    let first = significant.next()?;
    if significant.next().is_some() {
        return None;
    }
    let Node::Element(e) = first else { return None };
    match e.tag.as_str() {
        "img" => Some((e, None)),
        "a" => sole_image(&e.children)
            .and_then(|(img, inner)| inner.is_none().then_some((img, Some(e)))),
        _ => None,
    }
}

/// 動画などの埋め込みの URL を、ページの URL にする（直せなければ `None`）
fn embed_page_url(src: &str) -> Option<String> {
    let src = src.trim();
    let src = if let Some(rest) = src.strip_prefix("//") {
        format!("https://{rest}")
    } else {
        src.to_string()
    };
    let rest = src
        .strip_prefix("https://")
        .or_else(|| src.strip_prefix("http://"))?;
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = host.to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    let id_of = |prefix: &str| {
        path.strip_prefix(prefix)
            .map(|p| p.split(['?', '#', '/']).next().unwrap_or(""))
            .filter(|id| !id.is_empty())
    };
    match host {
        "youtube.com" | "youtube-nocookie.com" => {
            if let Some(id) = id_of("embed/") {
                return Some(format!("https://www.youtube.com/watch?v={id}"));
            }
            path.starts_with("watch")
                .then(|| format!("https://www.youtube.com/{path}"))
        }
        "youtu.be" => (!path.is_empty()).then(|| format!("https://youtu.be/{path}")),
        "player.vimeo.com" => id_of("video/").map(|id| format!("https://vimeo.com/{id}")),
        "vimeo.com" => (!path.is_empty()).then(|| format!("https://vimeo.com/{path}")),
        "twitter.com" | "x.com" | "open.spotify.com" | "soundcloud.com" | "speakerdeck.com"
        | "slideshare.net" | "codepen.io" | "gist.github.com" | "github.com" | "instagram.com"
        | "facebook.com" | "tiktok.com" | "dailymotion.com" => Some(src),
        _ => None,
    }
}

/// インラインの変換結果
struct Inline {
    text: String,
    /// 書式を失うので、HTML のまま残すべき理由
    lossy: Option<&'static str>,
}

impl<'a> Converter<'a> {
    // ---- ショートコード ----

    /// `[caption]`・`[embed]`・`[gallery]`・`[more]` を HTML に直す。残ったショートコードは記録する
    fn expand_shortcodes(&mut self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(start) = rest.find('[') {
            out.push_str(&rest[..start]);
            let after = &rest[start..];
            match parse_shortcode_open(after) {
                Some(open) => {
                    let handled = self.expand_one(&open, after, &mut out);
                    match handled {
                        Some(consumed) => rest = &after[consumed..],
                        None => {
                            out.push('[');
                            rest = &after[1..];
                        }
                    }
                }
                None => {
                    out.push('[');
                    rest = &after[1..];
                }
            }
        }
        out.push_str(rest);
        out
    }

    /// 1 つのショートコードを処理して、読み進めた長さを返す（処理しなければ `None`）
    fn expand_one(&mut self, open: &ShortcodeOpen, text: &str, out: &mut String) -> Option<usize> {
        let close_tag = format!("[/{}]", open.name);
        let close_at = text[open.len..].find(&close_tag).map(|i| open.len + i);
        match open.name.as_str() {
            "caption" => {
                let close = close_at?;
                let inner = &text[open.len..close];
                out.push_str(&caption_html(inner));
                Some(close + close_tag.len())
            }
            "embed" => {
                let close = close_at?;
                let url = text[open.len..close].trim();
                out.push_str(&format!(
                    "<p>{}</p>",
                    serialize(&[Node::Text(url.to_string())])
                ));
                Some(close + close_tag.len())
            }
            "gallery" => {
                let ids: Vec<u64> = open
                    .attr("ids")
                    .unwrap_or("")
                    .split(',')
                    .filter_map(|id| id.trim().parse().ok())
                    .collect();
                let urls: Vec<&String> = ids
                    .iter()
                    .filter_map(|id| self.ctx.attachments.and_then(|a| a.get(id)))
                    .collect();
                if urls.is_empty() {
                    self.notes.push(note(
                        "gallery",
                        "a [gallery] without image IDs (or with images that are not in the export) was removed",
                    ));
                } else {
                    for url in urls {
                        let img = Element {
                            tag: "img".to_string(),
                            attrs: vec![("src".to_string(), url.clone())],
                            children: Vec::new(),
                        };
                        out.push_str(&format!("<figure>{}</figure>", img.outer_html()));
                    }
                }
                Some(open.len)
            }
            "more" | "nextpage" => Some(open.len),
            name => {
                // 登録されているかは分からないので、閉じタグか `key=値` があるものだけをショートコードと見なす
                if close_at.is_some() || open.has_attrs {
                    self.notes.push(note("shortcode", name.to_string()));
                }
                None
            }
        }
    }

    // ---- ブロック ----

    fn blocks(&mut self, nodes: &[Node], out: &mut Vec<BlockNode>) {
        let mut run: Vec<Node> = Vec::new();
        for node in nodes {
            match node {
                Node::Comment(_) => {}
                Node::Text(_) => run.push(node.clone()),
                Node::Element(e) if INLINE_TAGS.contains(&e.tag.as_str()) => {
                    run.push(node.clone());
                }
                Node::Element(e) => {
                    self.flush_run(&mut run, out);
                    self.block_element(e, out);
                }
            }
        }
        self.flush_run(&mut run, out);
    }

    /// 段落の外に直接書かれたテキスト（クラシックエディターの本文）を、空行ごとの段落にする
    fn flush_run(&mut self, run: &mut Vec<Node>, out: &mut Vec<BlockNode>) {
        if run.iter().all(is_blank) {
            run.clear();
            return;
        }
        let mut paragraphs: Vec<Vec<Node>> = vec![Vec::new()];
        for node in run.drain(..) {
            match node {
                Node::Text(text) => {
                    let mut parts = split_blank_lines(&text).into_iter();
                    if let Some(first) = parts.next() {
                        paragraphs
                            .last_mut()
                            .expect("never empty")
                            .push(Node::Text(first));
                    }
                    for part in parts {
                        paragraphs.push(vec![Node::Text(part)]);
                    }
                }
                other => paragraphs.last_mut().expect("never empty").push(other),
            }
        }
        let previous = std::mem::replace(&mut self.newline_is_break, true);
        for nodes in paragraphs {
            self.paragraph(&nodes, out);
        }
        self.newline_is_break = previous;
    }

    fn paragraph(&mut self, children: &[Node], out: &mut Vec<BlockNode>) {
        if children.iter().all(is_blank) {
            return;
        }
        if let Some((img, link)) = sole_image(children) {
            self.image(img, link, &[], out);
            return;
        }
        let inline = self.inline(children);
        match inline.lossy {
            Some(reason) => self.keep_html(
                format!("<p>{}</p>", serialize(trim_nodes(children))),
                reason,
                out,
            ),
            None => {
                let text = inline.text.trim();
                if !text.is_empty() {
                    out.push(BlockNode::paragraph(text));
                }
            }
        }
    }

    fn keep_html(&mut self, html: String, reason: &str, out: &mut Vec<BlockNode>) {
        self.notes.push(note("html-kept", reason.to_string()));
        out.push(html_block(&html));
    }

    fn block_element(&mut self, e: &Element, out: &mut Vec<BlockNode>) {
        let previous = std::mem::replace(&mut self.newline_is_break, false);
        self.block_element_inner(e, out);
        self.newline_is_break = previous;
    }

    fn block_element_inner(&mut self, e: &Element, out: &mut Vec<BlockNode>) {
        match e.tag.as_str() {
            // `<p>` の中に iframe などが入っていたら、段落ではなく入れ物として中身を並べる
            "p" if e.children.iter().any(
                |n| matches!(n, Node::Element(c) if !INLINE_TAGS.contains(&c.tag.as_str())),
            ) =>
            {
                self.blocks(&e.children, out)
            }
            "p" => self.paragraph(&e.children, out),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => self.heading(e, out),
            "ul" | "ol" => self.list(e, out),
            "blockquote" => self.quote(e, out),
            "pre" => out.push(code_block(e)),
            "hr" => out.push(BlockNode::new("core/separator")),
            "table" => self.table(e, out),
            "figure" => self.figure(e, out),
            "details" => self.details(e, out),
            "a" => {
                if let Some((img, link)) =
                    sole_image(std::slice::from_ref(&Node::Element(e.clone())))
                {
                    self.image(img, link, &[], out);
                }
            }
            "iframe" => self.iframe(e, out),
            "script" | "style" | "noscript" | "link" | "meta" | "title" => {
                if e.tag == "script" {
                    self.notes.push(note("dropped", "script"));
                }
            }
            "form" | "button" | "input" | "select" | "textarea" | "object" | "embed" | "video"
            | "audio" | "svg" | "canvas" | "map" | "dl" => {
                self.keep_html(e.outer_html(), &e.tag, out);
            }
            // 入れ物（div・section・Gutenberg の columns / group など）は、中身を並べる
            _ => self.blocks(&e.children, out),
        }
    }

    fn heading(&mut self, e: &Element, out: &mut Vec<BlockNode>) {
        let level: u8 = e.tag[1..].parse().unwrap_or(2);
        let inline = self.inline(&e.children);
        match inline.lossy {
            Some(reason) => self.keep_html(e.outer_html(), reason, out),
            None => {
                let text = inline.text.trim();
                if !text.is_empty() {
                    out.push(BlockNode::heading(level, text));
                }
            }
        }
    }

    fn list(&mut self, e: &Element, out: &mut Vec<BlockNode>) {
        // 古いギャラリーは、画像の入った箇条書きの形をしている
        if e.has_class("wp-block-gallery") || e.has_class("blocks-gallery-grid") {
            for child in &e.children {
                if let Node::Element(li) = child {
                    self.blocks(&li.children, out);
                }
            }
            return;
        }
        let mut list = BlockNode::new("core/list").with_attr("ordered", e.tag == "ol");
        for child in &e.children {
            let Node::Element(li) = child else { continue };
            if li.tag != "li" {
                continue;
            }
            if li.contains_tag(&[
                "ul",
                "ol",
                "table",
                "pre",
                "blockquote",
                "figure",
                "img",
                "h1",
                "h2",
                "h3",
                "h4",
                "h5",
                "h6",
            ]) {
                self.keep_html(e.outer_html(), "nested content in a list", out);
                return;
            }
            // `<li><p>…</p></li>` は、段落の記号を取って 1 つの文にする
            let inner: Vec<Node> = li
                .children
                .iter()
                .flat_map(|n| match n {
                    Node::Element(p) if p.tag == "p" => {
                        let mut nodes = p.children.clone();
                        nodes.push(Node::Text(" ".to_string()));
                        nodes
                    }
                    other => vec![other.clone()],
                })
                .collect();
            let inline = self.inline(&inner);
            if let Some(reason) = inline.lossy {
                self.keep_html(e.outer_html(), reason, out);
                return;
            }
            let text = inline.text.trim();
            if !text.is_empty() {
                let mut item = BlockNode::new("core/list-item");
                item.text = Some(text.to_string());
                list.children.push(item);
            }
        }
        if !list.children.is_empty() {
            out.push(list);
        }
    }

    fn quote(&mut self, e: &Element, out: &mut Vec<BlockNode>) {
        // 出典: `<cite>` か Gutenberg の引用元
        let is_cite = |n: &Node| match n {
            Node::Element(c) => {
                c.tag == "cite" || c.tag == "footer" || c.has_class("wp-block-quote__citation")
            }
            _ => false,
        };
        let cite: Option<String> = e
            .children
            .iter()
            .find(|n| is_cite(n))
            .and_then(|n| match n {
                Node::Element(c) => Some(c.text_content().trim().to_string()),
                _ => None,
            });
        let body: Vec<Node> = e.children.iter().filter(|n| !is_cite(n)).cloned().collect();
        let mut inner = Vec::new();
        self.blocks(&body, &mut inner);
        let paragraphs: Option<Vec<&str>> = inner
            .iter()
            .map(|b| (b.name == "core/paragraph").then(|| b.text.as_deref().unwrap_or("")))
            .collect();
        match paragraphs {
            Some(paragraphs) if !paragraphs.is_empty() => {
                let mut node = BlockNode::new("core/quote");
                node.text = Some(paragraphs.join("\n\n"));
                if let Some(cite) = cite.filter(|c| !c.is_empty()) {
                    node = node.with_attr("cite", escape_md(&cite));
                }
                out.push(node);
            }
            _ => self.keep_html(e.outer_html(), "a quote with more than paragraphs", out),
        }
    }

    fn table(&mut self, e: &Element, out: &mut Vec<BlockNode>) {
        let mut rows: Vec<&Element> = Vec::new();
        let mut has_thead = false;
        for section in ["thead", "tbody", "tfoot"] {
            for child in &e.children {
                if let Node::Element(s) = child {
                    if s.tag == section {
                        has_thead |= section == "thead";
                        rows.extend(s.children.iter().filter_map(|n| match n {
                            Node::Element(tr) if tr.tag == "tr" => Some(tr),
                            _ => None,
                        }));
                    }
                }
            }
        }
        // `thead` などを使わず、`tr` が直接入っている表
        rows.extend(e.children.iter().filter_map(|n| match n {
            Node::Element(tr) if tr.tag == "tr" => Some(tr),
            _ => None,
        }));
        let mut table_rows: Vec<Vec<String>> = Vec::new();
        let mut first_row_all_th = false;
        let mut align: Vec<Option<&'static str>> = Vec::new();
        for (index, tr) in rows.iter().enumerate() {
            let mut cells = Vec::new();
            let mut all_th = true;
            for cell in tr.children.iter().filter_map(|n| match n {
                Node::Element(c) if c.tag == "td" || c.tag == "th" => Some(c),
                _ => None,
            }) {
                all_th &= cell.tag == "th";
                let spans = |name| {
                    cell.attr(name)
                        .is_some_and(|v| v.trim() != "1" && !v.trim().is_empty())
                };
                if spans("colspan") || spans("rowspan") {
                    self.keep_html(e.outer_html(), "a table with merged cells", out);
                    return;
                }
                let inline = self.inline(&cell.children);
                if let Some(reason) = inline.lossy {
                    self.keep_html(e.outer_html(), reason, out);
                    return;
                }
                if index == 0 {
                    align.push(cell_align(cell));
                }
                cells.push(inline.text.trim().to_string());
            }
            if index == 0 {
                first_row_all_th = all_th && !cells.is_empty();
            }
            table_rows.push(cells);
        }
        if table_rows.iter().all(Vec::is_empty) {
            return;
        }
        let mut node = BlockNode::new("core/table")
            .with_attr("rows", serde_json::json!(table_rows))
            .with_attr("header", has_thead || first_row_all_th);
        if align.iter().any(Option::is_some) {
            node = node.with_attr("align", serde_json::json!(align));
        }
        out.push(node);
    }

    fn figure(&mut self, e: &Element, out: &mut Vec<BlockNode>) {
        // 埋め込み（動画・SNS など）
        if e.has_class("wp-block-embed") || e.contains_tag(&["iframe"]) {
            let url = e
                .children
                .iter()
                .find_map(|n| match n {
                    Node::Element(w) if w.has_class("wp-block-embed__wrapper") => {
                        Some(w.text_content().trim().to_string())
                    }
                    _ => None,
                })
                .filter(|u| u.starts_with("http"))
                .or_else(|| find_iframe_src(e).and_then(|s| embed_page_url(&s)));
            if let Some(url) = url {
                self.notes.push(note("embed", url.clone()));
                out.push(BlockNode::paragraph(url));
                let caption: Vec<Node> = figcaption(e)
                    .map(|c| c.children.clone())
                    .unwrap_or_default();
                self.caption(&caption, out);
                return;
            }
        }
        let caption_nodes = figcaption(e)
            .map(|c| c.children.clone())
            .unwrap_or_default();
        let content: Vec<Node> = e
            .children
            .iter()
            .filter(|n| !matches!(n, Node::Element(c) if c.tag == "figcaption"))
            .cloned()
            .collect();
        if let Some((img, link)) = sole_image(&content) {
            self.image(img, link, &caption_nodes, out);
            return;
        }
        // 表・コード・引用・ギャラリー（図が入れ子）は、中身を変換する
        if content.iter().any(|n| matches!(n, Node::Element(_)))
            && !e.contains_tag(&["video", "audio"])
        {
            self.blocks(&content, out);
            self.caption(&caption_nodes, out);
            return;
        }
        self.keep_html(e.outer_html(), "figure", out);
    }

    fn details(&mut self, e: &Element, out: &mut Vec<BlockNode>) {
        let summary = e.children.iter().find_map(|n| match n {
            Node::Element(s) if s.tag == "summary" => Some(s),
            _ => None,
        });
        let body: Vec<Node> = e
            .children
            .iter()
            .filter(|n| !matches!(n, Node::Element(s) if s.tag == "summary"))
            .cloned()
            .collect();
        let mut inner = Vec::new();
        self.blocks(&body, &mut inner);
        let paragraphs: Option<Vec<&str>> = inner
            .iter()
            .map(|b| (b.name == "core/paragraph").then(|| b.text.as_deref().unwrap_or("")))
            .collect();
        let Some(paragraphs) = paragraphs else {
            self.keep_html(
                e.outer_html(),
                "a details block with more than paragraphs",
                out,
            );
            return;
        };
        let mut node = BlockNode::new("core/details").with_attr("open", e.attr("open").is_some());
        if let Some(summary) = summary {
            let inline = self.inline(&summary.children);
            node = node.with_attr("summary", inline.text.trim());
        }
        node.text = Some(paragraphs.join("\n\n"));
        out.push(node);
    }

    fn iframe(&mut self, e: &Element, out: &mut Vec<BlockNode>) {
        match e.attr("src").and_then(embed_page_url) {
            Some(url) => {
                self.notes.push(note("embed", url.clone()));
                out.push(BlockNode::paragraph(url));
            }
            None => self.keep_html(e.outer_html(), "iframe", out),
        }
    }

    /// 画像。`link` は画像を囲むリンク、`caption` は図の説明
    fn image(
        &mut self,
        img: &Element,
        link: Option<&Element>,
        caption: &[Node],
        out: &mut Vec<BlockNode>,
    ) {
        let src = ["src", "data-src", "data-lazy-src", "data-orig-file"]
            .iter()
            .filter_map(|name| img.attr(name))
            .map(str::trim)
            .find(|s| !s.is_empty() && !s.starts_with("data:"))
            .unwrap_or("");
        if src.is_empty() || !safe_url(src) {
            return;
        }
        let caption_text = self.inline(caption).text.trim().to_string();
        let alt = img.attr("alt").map(str::trim).filter(|a| !a.is_empty());
        let alt = alt.map(str::to_string).or_else(|| {
            let plain = caption.iter().map(|n| match n {
                Node::Text(t) => t.clone(),
                Node::Element(e) => e.text_content(),
                Node::Comment(_) => String::new(),
            });
            let plain = collapse_whitespace(&plain.collect::<String>())
                .trim()
                .to_string();
            (!plain.is_empty()).then_some(plain)
        });
        let alt = alt.unwrap_or_default();
        let href = link
            .and_then(|a| a.attr("href"))
            .map(str::trim)
            .filter(|h| safe_url(h));
        match href {
            // 画像の拡大表示へのリンクは、画像だけにする。ほかへのリンクは、リンクの中の画像にする
            Some(href) if href != src && !is_image_url(href) => {
                let label = alt.replace(['[', ']', '\n'], " ");
                out.push(BlockNode::paragraph(format!(
                    "[![{}]({})]({})",
                    escape_md(&label).replace("\\!", "!"),
                    md_url(src),
                    md_url(href)
                )));
            }
            _ => out.push(
                BlockNode::new("core/image")
                    .with_attr("src", src)
                    .with_attr("alt", alt),
            ),
        }
        if !caption_text.is_empty() {
            out.push(BlockNode::paragraph(format!(
                "*{}*",
                caption_text.trim_matches('*')
            )));
        }
    }

    /// 図の説明だけを、斜体の段落にする
    fn caption(&mut self, caption: &[Node], out: &mut Vec<BlockNode>) {
        let text = self.inline(caption).text.trim().to_string();
        if !text.is_empty() {
            out.push(BlockNode::paragraph(format!(
                "*{}*",
                text.trim_matches('*')
            )));
        }
    }

    // ---- インライン ----

    fn inline(&mut self, nodes: &[Node]) -> Inline {
        let mut lossy = None;
        let text = self.inline_into(nodes, &mut lossy);
        Inline { text, lossy }
    }

    fn inline_into(&mut self, nodes: &[Node], lossy: &mut Option<&'static str>) -> String {
        let mut out = String::new();
        for node in nodes {
            match node {
                Node::Comment(_) => {}
                Node::Text(text) => {
                    if self.newline_is_break
                        && text.trim_matches([' ', '\t', '\r', '\n']).contains('\n')
                    {
                        lossy.get_or_insert("line breaks");
                    }
                    out.push_str(&escape_md(&collapse_whitespace(text)));
                }
                Node::Element(e) => self.inline_element(e, &mut out, lossy),
            }
        }
        out
    }

    fn inline_element(&mut self, e: &Element, out: &mut String, lossy: &mut Option<&'static str>) {
        match e.tag.as_str() {
            "br" => {
                lossy.get_or_insert("line breaks");
            }
            "strong" | "b" => wrap(out, "**", &self.inline_into(&e.children, lossy)),
            "em" | "i" | "cite" | "dfn" => wrap(out, "*", &self.inline_into(&e.children, lossy)),
            "del" | "s" | "strike" => wrap(out, "~~", &self.inline_into(&e.children, lossy)),
            "code" | "tt" => {
                let code = e.text_content();
                let code = code.trim();
                if code.contains('`') {
                    lossy.get_or_insert("inline code containing a backtick");
                } else if !code.is_empty() {
                    out.push('`');
                    out.push_str(code);
                    out.push('`');
                }
            }
            "a" => {
                let label = self.inline_into(&e.children, lossy);
                match e.attr("href").map(str::trim).filter(|h| safe_url(h)) {
                    Some(href) if !label.trim().is_empty() => {
                        let (lead, body, tail) = split_spaces(&label);
                        out.push_str(lead);
                        out.push_str(&format!("[{}]({})", body, md_url(href)));
                        out.push_str(tail);
                    }
                    _ => out.push_str(&label),
                }
            }
            "img" => {
                let src = e.attr("src").map(str::trim).unwrap_or("");
                if src.is_empty() || src.starts_with("data:") || !safe_url(src) {
                    lossy.get_or_insert("an image that cannot be linked");
                } else {
                    let alt = e.attr("alt").unwrap_or("").replace(['[', ']', '\n'], " ");
                    out.push_str(&format!(
                        "![{}]({})",
                        escape_md(&alt).replace("\\!", "!"),
                        md_url(src)
                    ));
                }
            }
            tag if LOSSY_INLINE.contains(&tag) => {
                lossy.get_or_insert("inline formatting that blocks cannot hold");
                out.push_str(&self.inline_into(&e.children, lossy));
            }
            // span・abbr・time・font など。書式は持たないので、中身だけを残す
            _ => out.push_str(&self.inline_into(&e.children, lossy)),
        }
    }
}

/// `**中身**` のように包む。中身の前後の空白は外に出す（記号の内側が空白だと書式にならないため）
fn wrap(out: &mut String, mark: &str, inner: &str) {
    let (lead, body, tail) = split_spaces(inner);
    if body.is_empty() {
        out.push_str(inner);
        return;
    }
    out.push_str(lead);
    out.push_str(mark);
    out.push_str(body);
    out.push_str(mark);
    out.push_str(tail);
}

/// 前後の空白と、その間
fn split_spaces(text: &str) -> (&str, &str, &str) {
    let trimmed_start = text.trim_start();
    let lead = &text[..text.len() - trimmed_start.len()];
    let body = trimmed_start.trim_end();
    let tail = &trimmed_start[body.len()..];
    (lead, body, tail)
}

/// 前後の空白だけのテキストを取り除く
fn trim_nodes(nodes: &[Node]) -> &[Node] {
    let start = nodes
        .iter()
        .position(|n| !is_blank(n))
        .unwrap_or(nodes.len());
    let end = nodes
        .iter()
        .rposition(|n| !is_blank(n))
        .map_or(start, |i| i + 1);
    &nodes[start..end]
}

/// 空行（改行・空白だけの行）で文字列を分ける
fn split_blank_lines(text: &str) -> Vec<String> {
    let normalized = text.replace("\r\n", "\n");
    let mut parts = vec![String::new()];
    let mut pending_newlines = 0;
    let mut buffered_ws = String::new();
    for line_char in normalized.chars() {
        match line_char {
            '\n' => {
                pending_newlines += 1;
                buffered_ws.clear();
            }
            c if c.is_whitespace() && pending_newlines > 0 => buffered_ws.push(c),
            c => {
                if pending_newlines >= 2 {
                    parts.push(String::new());
                } else if pending_newlines == 1 {
                    parts.last_mut().expect("never empty").push('\n');
                }
                pending_newlines = 0;
                buffered_ws.clear();
                parts.last_mut().expect("never empty").push(c);
            }
        }
    }
    // 末尾の改行は、次のノードとの境目として扱う
    if pending_newlines >= 2 {
        parts.push(String::new());
    } else if pending_newlines == 1 {
        parts.last_mut().expect("never empty").push('\n');
    }
    parts
}

fn cell_align(cell: &Element) -> Option<&'static str> {
    let style = cell
        .attr("style")
        .unwrap_or("")
        .to_ascii_lowercase()
        .replace(' ', "");
    let attr = cell.attr("align").unwrap_or("").to_ascii_lowercase();
    ["left", "center", "right"].into_iter().find(|a| {
        style.contains(&format!("text-align:{a}"))
            || attr == *a
            || cell.has_class(&format!("has-text-align-{a}"))
    })
}

fn figcaption(e: &Element) -> Option<&Element> {
    e.children.iter().find_map(|n| match n {
        Node::Element(c) if c.tag == "figcaption" => Some(c),
        _ => None,
    })
}

fn find_iframe_src(e: &Element) -> Option<String> {
    for child in &e.children {
        if let Node::Element(c) = child {
            if c.tag == "iframe" {
                return c.attr("src").map(str::to_string);
            }
            if let Some(src) = find_iframe_src(c) {
                return Some(src);
            }
        }
    }
    None
}

fn code_block(pre: &Element) -> BlockNode {
    let code = pre.children.iter().find_map(|n| match n {
        Node::Element(c) if c.tag == "code" => Some(c),
        _ => None,
    });
    let language = code
        .and_then(|c| class_language(c.attr("class").unwrap_or("")))
        .or_else(|| class_language(pre.attr("class").unwrap_or("")))
        .or_else(|| pre.attr("data-lang").map(str::to_string))
        .unwrap_or_else(|| "text".to_string());
    let text = pre.text_content();
    let text = text
        .strip_prefix('\n')
        .unwrap_or(&text)
        .trim_end_matches('\n');
    BlockNode::code(&language, text)
}

/// `language-rust`・`lang-js`・SyntaxHighlighter の `brush: php;` から言語を取る
fn class_language(class: &str) -> Option<String> {
    let class = class.trim();
    for token in class.split_whitespace() {
        for prefix in ["language-", "lang-"] {
            if let Some(lang) = token.strip_prefix(prefix) {
                if !lang.is_empty() {
                    return Some(lang.to_ascii_lowercase());
                }
            }
        }
    }
    let brush = class.split_once("brush:")?.1;
    let lang = brush.trim().split([';', ' ']).next()?;
    (!lang.is_empty()).then(|| lang.to_ascii_lowercase())
}

// ---- ショートコードの読み取り ----

struct ShortcodeOpen {
    name: String,
    attrs: Vec<(String, String)>,
    has_attrs: bool,
    /// `[` から `]` までの長さ
    len: usize,
}

impl ShortcodeOpen {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

/// `[name key="value" key2=value2]` を読む（先頭は `[`）
fn parse_shortcode_open(text: &str) -> Option<ShortcodeOpen> {
    let inner_start = 1;
    let bytes = text.as_bytes();
    let mut i = inner_start;
    while i < bytes.len()
        && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'-')
    {
        i += 1;
    }
    if i == inner_start || !bytes[inner_start].is_ascii_alphabetic() {
        return None;
    }
    let name = text[inner_start..i].to_ascii_lowercase();
    let mut attrs = Vec::new();
    loop {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            return None;
        }
        match bytes[i] {
            b']' => {
                return Some(ShortcodeOpen {
                    name,
                    has_attrs: !attrs.is_empty(),
                    attrs,
                    len: i + 1,
                });
            }
            b'[' | b'<' | b'>' | b'\n' => return None,
            _ => {}
        }
        let key_start = i;
        while i < bytes.len()
            && !bytes[i].is_ascii_whitespace()
            && bytes[i] != b'='
            && bytes[i] != b']'
        {
            i += 1;
        }
        let key = text[key_start..i].to_ascii_lowercase();
        if i < bytes.len() && bytes[i] == b'=' {
            i += 1;
            let value = if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
                let quote = bytes[i];
                i += 1;
                let value_start = i;
                while i < bytes.len() && bytes[i] != quote {
                    i += 1;
                }
                if i >= bytes.len() {
                    return None;
                }
                let value = text[value_start..i].to_string();
                i += 1;
                value
            } else {
                let value_start = i;
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b']' {
                    i += 1;
                }
                text[value_start..i].to_string()
            };
            attrs.push((key, value));
        } else if key.is_empty() {
            return None;
        }
    }
}

/// `[caption]` の中身（画像と説明の文）を `<figure>` にする
fn caption_html(inner: &str) -> String {
    let nodes = parse_fragment(inner.trim());
    let Some(index) = nodes.iter().position(|n| matches!(n, Node::Element(_))) else {
        return format!("<p>{}</p>", serialize(&nodes));
    };
    let media = serialize(&nodes[index..=index]);
    let caption = serialize(&nodes[index + 1..]);
    let caption = caption.trim();
    if caption.is_empty() {
        format!("<figure>{media}</figure>")
    } else {
        format!("<figure>{media}<figcaption>{caption}</figcaption></figure>")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(content: &str) -> Conversion {
        convert(content, &Context::default())
    }

    fn names(c: &Conversion) -> Vec<&str> {
        c.blocks.iter().map(|b| b.name.as_str()).collect()
    }

    fn text(c: &Conversion, index: usize) -> &str {
        c.blocks[index].text.as_deref().unwrap_or("")
    }

    #[test]
    fn test_gutenberg_paragraphs_headings_and_inline_formats() {
        let c = run(r#"<!-- wp:heading {"level":3} -->
<h3 class="wp-block-heading">Title &amp; more</h3>
<!-- /wp:heading -->

<!-- wp:paragraph -->
<p>Hello <strong>bold</strong>, <em>italic</em>, <del>gone</del>, <code>x</code> and <a href="https://example.com/a b">a link</a>.</p>
<!-- /wp:paragraph -->"#);
        assert_eq!(names(&c), ["core/heading", "core/paragraph"]);
        assert_eq!(c.blocks[0].attributes["level"], 3);
        assert_eq!(text(&c, 0), "Title & more");
        assert_eq!(
            text(&c, 1),
            "Hello **bold**, *italic*, ~~gone~~, `x` and [a link](https://example.com/a%20b)."
        );
        assert!(c.notes.is_empty());
    }

    #[test]
    fn test_markdown_symbols_in_text_are_escaped() {
        let c = run("<p>2 * 3 [x] ~ `y` \\ ![alt]</p>");
        assert_eq!(text(&c, 0), "2 \\* 3 \\[x\\] \\~ \\`y\\` \\\\ \\!\\[alt\\]");
    }

    #[test]
    fn test_classic_content_without_p_tags_is_split_at_blank_lines() {
        let c = run("First paragraph.\n\nSecond <b>one</b>.\n\n\n\nThird.");
        assert_eq!(names(&c), ["core/paragraph"; 3]);
        assert_eq!(text(&c, 1), "Second **one**.");
        assert!(c.notes.is_empty());
    }

    #[test]
    fn test_single_newlines_in_classic_text_keep_their_line_breaks_as_html() {
        let c = run("line one\nline two");
        assert_eq!(names(&c), ["core/html"]);
        assert_eq!(text(&c, 0), "<p>line one\nline two</p>");
        assert_eq!(c.notes[0], note("html-kept", "line breaks"));
        // `<p>` の中の改行は、ただの空白
        let c = run("<p>line one\nline two</p>");
        assert_eq!(names(&c), ["core/paragraph"]);
        assert_eq!(text(&c, 0), "line one line two");
    }

    #[test]
    fn test_br_and_unsupported_inline_formatting_keep_the_paragraph_as_html() {
        let c = run("<p>a<br>b</p><p>E = mc<sup>2</sup></p>");
        assert_eq!(names(&c), ["core/html", "core/html"]);
        assert_eq!(text(&c, 0), "<p>a<br>b</p>");
        assert_eq!(text(&c, 1), "<p>E = mc<sup>2</sup></p>");
        assert_eq!(c.notes.len(), 2);
    }

    #[test]
    fn test_lists_quotes_code_and_separators() {
        let c = run(r#"<ul><li>one</li><li><p>two <b>x</b></p></li></ul>
<ol><li>first</li></ol>
<blockquote class="wp-block-quote"><p>Quoted</p><p>Again</p><cite>Someone</cite></blockquote>
<pre class="wp-block-code"><code class="language-rust">fn main() {
    println!("&lt;hi&gt;");
}
</code></pre>
<hr class="wp-block-separator">"#);
        assert_eq!(
            names(&c),
            [
                "core/list",
                "core/list",
                "core/quote",
                "core/code",
                "core/separator"
            ]
        );
        assert_eq!(c.blocks[0].attributes["ordered"], false);
        assert_eq!(c.blocks[0].children.len(), 2);
        assert_eq!(c.blocks[0].children[1].text.as_deref(), Some("two **x**"));
        assert_eq!(c.blocks[1].attributes["ordered"], true);
        assert_eq!(text(&c, 2), "Quoted\n\nAgain");
        assert_eq!(c.blocks[2].attributes["cite"], "Someone");
        assert_eq!(c.blocks[3].attributes["language"], "rust");
        assert_eq!(text(&c, 3), "fn main() {\n    println!(\"<hi>\");\n}");
    }

    #[test]
    fn test_nested_lists_are_kept_as_html() {
        let c = run("<ul><li>a<ul><li>b</li></ul></li></ul>");
        assert_eq!(names(&c), ["core/html"]);
        assert!(text(&c, 0).contains("<ul><li>b</li></ul>"));
        assert_eq!(c.notes[0].code, "html-kept");
    }

    #[test]
    fn test_code_language_from_syntax_highlighter_classes() {
        let c = run(r#"<pre class="brush: php; title: x">&lt;?php echo 1;</pre><pre>plain</pre>"#);
        assert_eq!(c.blocks[0].attributes["language"], "php");
        assert_eq!(c.blocks[1].attributes["language"], "text");
    }

    #[test]
    fn test_tables() {
        let c = run(
            r#"<figure class="wp-block-table"><table><thead><tr><th>A</th><th style="text-align:right">B</th></tr></thead><tbody><tr><td>1</td><td><b>2</b></td></tr></tbody></table></figure>"#,
        );
        assert_eq!(names(&c), ["core/table"]);
        assert_eq!(
            c.blocks[0].attributes["rows"],
            serde_json::json!([["A", "B"], ["1", "**2**"]])
        );
        assert_eq!(c.blocks[0].attributes["header"], true);
        assert_eq!(
            c.blocks[0].attributes["align"],
            serde_json::json!([null, "right"])
        );

        let merged = run(r#"<table><tr><td colspan="2">x</td></tr></table>"#);
        assert_eq!(names(&merged), ["core/html"]);
    }

    #[test]
    fn test_images_with_links_and_captions() {
        let c = run(
            r#"<figure class="wp-block-image size-large"><img src="https://e.com/a.jpg" alt="A cat" class="wp-image-5"/><figcaption class="wp-element-caption">Taken by <a href="https://e.com">me</a></figcaption></figure>
<p><a href="https://e.com/big.jpg"><img src="https://e.com/small.jpg" alt="Zoomable"></a></p>
<p><a href="https://other.example/page"><img src="https://e.com/b.png" alt="Banner"></a></p>"#,
        );
        assert_eq!(
            names(&c),
            [
                "core/image",
                "core/paragraph",
                "core/image",
                "core/paragraph"
            ]
        );
        assert_eq!(c.blocks[0].attributes["src"], "https://e.com/a.jpg");
        assert_eq!(c.blocks[0].attributes["alt"], "A cat");
        assert_eq!(text(&c, 1), "*Taken by [me](https://e.com)*");
        // 画像ファイルへのリンク（拡大表示）は、画像だけにする
        assert_eq!(c.blocks[2].attributes["src"], "https://e.com/small.jpg");
        // ほかのページへのリンクは、リンクの中の画像にする
        assert_eq!(
            text(&c, 3),
            "[![Banner](https://e.com/b.png)](https://other.example/page)"
        );
    }

    #[test]
    fn test_caption_shortcode_and_gallery_shortcode() {
        let mut attachments = HashMap::new();
        attachments.insert(7, "https://e.com/g1.jpg".to_string());
        attachments.insert(8, "https://e.com/g2.jpg".to_string());
        let c = convert(
            r#"[caption id="attachment_7" align="aligncenter" width="300"]<a href="https://e.com/x"><img src="https://e.com/c.jpg" alt="C" /></a> A caption[/caption]

[gallery ids="7,8,99"]"#,
            &Context {
                attachments: Some(&attachments),
            },
        );
        // 画像（ほかのページへのリンク付き）、その説明、ギャラリーの 2 枚（本文に無い ID の 99 は飛ばす）
        assert_eq!(
            names(&c),
            [
                "core/paragraph",
                "core/paragraph",
                "core/image",
                "core/image"
            ]
        );
        assert_eq!(text(&c, 0), "[![C](https://e.com/c.jpg)](https://e.com/x)");
        assert_eq!(text(&c, 1), "*A caption*");
        assert_eq!(c.blocks[2].attributes["src"], "https://e.com/g1.jpg");
        assert_eq!(c.blocks[3].attributes["src"], "https://e.com/g2.jpg");
    }

    #[test]
    fn test_embeds_become_url_paragraphs() {
        let c = run(
            r#"<figure class="wp-block-embed is-type-video"><div class="wp-block-embed__wrapper">
https://www.youtube.com/watch?v=abc123
</div><figcaption>Video</figcaption></figure>
<p><iframe src="https://www.youtube.com/embed/xyz789?feature=oembed" width="560"></iframe></p>
<iframe src="https://player.vimeo.com/video/555"></iframe>
<iframe src="https://maps.example.com/embed"></iframe>"#,
        );
        assert_eq!(
            names(&c),
            [
                "core/paragraph",
                "core/paragraph",
                "core/paragraph",
                "core/paragraph",
                "core/html"
            ]
        );
        assert_eq!(text(&c, 0), "https://www.youtube.com/watch?v=abc123");
        assert_eq!(text(&c, 1), "*Video*");
        assert_eq!(text(&c, 2), "https://www.youtube.com/watch?v=xyz789");
        assert_eq!(text(&c, 3), "https://vimeo.com/555");
        assert!(c
            .notes
            .iter()
            .any(|n| n.code == "html-kept" && n.detail == "iframe"));
    }

    #[test]
    fn test_wp_html_blocks_are_kept_raw_and_scripts_are_dropped() {
        let c = run(
            "<!-- wp:html -->\n<div class=\"x\"><b>raw</b></div>\n<!-- /wp:html -->\n<p>after</p><script>alert(1)</script>",
        );
        assert_eq!(names(&c), ["core/html", "core/paragraph"]);
        assert_eq!(text(&c, 0), "<div class=\"x\"><b>raw</b></div>");
        assert!(c.notes.contains(&note("dropped", "script")));
    }

    #[test]
    fn test_containers_are_flattened_and_comments_ignored() {
        let c = run(
            r#"<!-- wp:columns --><div class="wp-block-columns"><div class="wp-block-column"><p>left</p></div><div class="wp-block-column"><p>right</p></div></div><!-- /wp:columns --><!--more--><p>tail</p>"#,
        );
        assert_eq!(names(&c), ["core/paragraph"; 3]);
        assert_eq!(text(&c, 0), "left");
        assert_eq!(text(&c, 2), "tail");
    }

    #[test]
    fn test_details_block() {
        let c = run("<details open><summary>More <b>info</b></summary><p>Body one</p><p>Body two</p></details>");
        assert_eq!(names(&c), ["core/details"]);
        assert_eq!(c.blocks[0].attributes["summary"], "More **info**");
        assert_eq!(c.blocks[0].attributes["open"], true);
        assert_eq!(text(&c, 0), "Body one\n\nBody two");
    }

    #[test]
    fn test_unknown_shortcodes_are_reported_but_plain_brackets_are_not() {
        let c = run(
            "<p>[contact-form-7 id=\"12\" title=\"Contact\"]</p><p>See [1] and [Photo: Taro]</p><p>[fancy]boxed[/fancy]</p>",
        );
        let codes: Vec<_> = c
            .notes
            .iter()
            .map(|n| (n.code, n.detail.as_str()))
            .collect();
        assert_eq!(
            codes,
            [("shortcode", "contact-form-7"), ("shortcode", "fancy")]
        );
        // 残ったショートコードは、本文にそのまま残る
        assert!(text(&c, 0).contains("contact-form-7"));
        assert_eq!(text(&c, 1), "See \\[1\\] and \\[Photo: Taro\\]");
    }

    #[test]
    fn test_empty_and_whitespace_content() {
        assert!(run("").blocks.is_empty());
        assert!(run("  \n\n  ").blocks.is_empty());
        assert!(run("<!-- wp:paragraph --><p></p><!-- /wp:paragraph -->")
            .blocks
            .is_empty());
    }

    #[test]
    fn test_unsafe_links_and_images_are_neutralised() {
        let c = run(
            r#"<p><a href="javascript:alert(1)">click</a> <img src="data:image/png;base64,AAAA"></p>"#,
        );
        // 危ない行き先のリンクは文字だけにし、読めない画像を含む段落は HTML のまま残す（無害化は表示のときに行う）
        assert_eq!(names(&c), ["core/html"]);
    }

    #[test]
    fn test_blocks_render_with_the_real_renderer() {
        let c = run(
            "<h2>Title</h2><p>Hello <strong>world</strong> &amp; <a href=\"https://e.com\">friends</a>.</p><ul><li>a</li></ul>",
        );
        let html = aegis_ast::renderer::HtmlRenderer::render_nodes(&c.blocks);
        assert!(html.contains("<h2>Title</h2>"), "{html}");
        assert!(
            html.contains("<p>Hello <strong>world</strong> &amp; <a href=\"https://e.com\""),
            "{html}"
        );
        assert!(html.contains("<li>a</li>"), "{html}");
    }
}
