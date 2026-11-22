//! WordPress の本文の HTML を読むための、小さくて寛容な HTML パーサー。
//!
//! WordPress の本文は、閉じていない `<p>`・属性の引用符の無い値・Gutenberg のコメントなど、
//! 規格どおりではない HTML を含む。ブロックに直すために必要なだけの木（要素・テキスト・コメント）を
//! 作る。スクリプトを実行したり、外部のものを読み込んだりはしない。

/// 木の 1 つの節
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Text(String),
    Comment(String),
    Element(Element),
}

/// 要素（タグ名は小文字）
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Element {
    pub tag: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

impl Element {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    pub fn has_class(&self, class: &str) -> bool {
        self.attr("class")
            .is_some_and(|c| c.split_whitespace().any(|c| c == class))
    }

    /// 子孫のテキストをつなげたもの
    pub fn text_content(&self) -> String {
        let mut out = String::new();
        collect_text(&self.children, &mut out);
        out
    }

    /// この要素を含む HTML
    pub fn outer_html(&self) -> String {
        let mut out = String::new();
        write_element(self, &mut out);
        out
    }

    /// 子孫に、この名前の要素があるか
    pub fn contains_tag(&self, names: &[&str]) -> bool {
        self.children.iter().any(|n| match n {
            Node::Element(e) => names.contains(&e.tag.as_str()) || e.contains_tag(names),
            _ => false,
        })
    }
}

fn collect_text(nodes: &[Node], out: &mut String) {
    for node in nodes {
        match node {
            Node::Text(t) => out.push_str(t),
            Node::Element(e) => {
                if e.tag == "br" {
                    out.push('\n');
                } else {
                    collect_text(&e.children, out);
                }
            }
            Node::Comment(_) => {}
        }
    }
}

const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];

/// 中身を HTML として読まない要素
const RAW_TEXT: &[&str] = &["script", "style", "textarea", "title"];

/// 開始すると、直前の `<p>` が閉じる要素
const CLOSES_P: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "details",
    "div",
    "dl",
    "fieldset",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hr",
    "main",
    "nav",
    "ol",
    "p",
    "pre",
    "section",
    "table",
    "ul",
];

pub fn is_void(tag: &str) -> bool {
    VOID.contains(&tag)
}

/// 断片を読む。壊れた HTML でも、読めるところまでの木を返す
pub fn parse_fragment(html: &str) -> Vec<Node> {
    let mut parser = Parser {
        input: html,
        pos: 0,
        root: Vec::new(),
        stack: Vec::new(),
    };
    parser.run();
    parser.finish()
}

struct Parser<'a> {
    input: &'a str,
    pos: usize,
    root: Vec<Node>,
    stack: Vec<Element>,
}

impl<'a> Parser<'a> {
    fn rest(&self) -> &'a str {
        &self.input[self.pos..]
    }

    fn push_node(&mut self, node: Node) {
        match self.stack.last_mut() {
            Some(parent) => parent.children.push(node),
            None => self.root.push(node),
        }
    }

    fn push_text(&mut self, raw: &str, decode: bool) {
        if raw.is_empty() {
            return;
        }
        let text = if decode {
            decode_entities(raw)
        } else {
            raw.to_string()
        };
        // 直前もテキストなら、つなげる
        let siblings = match self.stack.last_mut() {
            Some(parent) => &mut parent.children,
            None => &mut self.root,
        };
        if let Some(Node::Text(last)) = siblings.last_mut() {
            last.push_str(&text);
        } else {
            siblings.push(Node::Text(text));
        }
    }

    fn run(&mut self) {
        while self.pos < self.input.len() {
            let rest = self.rest();
            if let Some(after) = rest.strip_prefix("<!--") {
                match after.find("-->") {
                    Some(end) => {
                        let comment = after[..end].to_string();
                        self.pos += 4 + end + 3;
                        self.push_node(Node::Comment(comment));
                    }
                    None => {
                        // 閉じていないコメントは、残りをすべてコメントにする
                        let comment = after.to_string();
                        self.pos = self.input.len();
                        self.push_node(Node::Comment(comment));
                    }
                }
            } else if rest.starts_with("<!") || rest.starts_with("<?") {
                match rest.find('>') {
                    Some(end) => self.pos += end + 1,
                    None => self.pos = self.input.len(),
                }
            } else if let Some(after) = rest.strip_prefix("</") {
                if after.starts_with(|c: char| c.is_ascii_alphabetic()) {
                    let end = after.find('>').unwrap_or(after.len());
                    let name = after[..end]
                        .split(|c: char| c.is_whitespace() || c == '/')
                        .next()
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    self.pos += 2 + (end + 1).min(after.len());
                    self.close(&name);
                } else {
                    self.push_text("</", false);
                    self.pos += 2;
                }
            } else if rest.starts_with('<')
                && rest[1..].starts_with(|c: char| c.is_ascii_alphabetic())
            {
                if !self.start_tag() {
                    // 最後まで閉じない `<` は、文字として扱う
                    self.push_text(rest, true);
                    self.pos = self.input.len();
                }
            } else {
                let end = rest[1..].find('<').map_or(rest.len(), |i| i + 1);
                self.push_text(&rest[..end], true);
                self.pos += end;
            }
        }
    }

    /// 開始タグを読む。タグが閉じていなければ `false`
    fn start_tag(&mut self) -> bool {
        let rest = self.rest();
        let bytes = rest.as_bytes();
        let mut i = 1;
        while i < bytes.len()
            && !bytes[i].is_ascii_whitespace()
            && bytes[i] != b'/'
            && bytes[i] != b'>'
        {
            i += 1;
        }
        let tag = rest[1..i].to_ascii_lowercase();
        let mut attrs: Vec<(String, String)> = Vec::new();
        let mut self_closing = false;
        loop {
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= bytes.len() {
                return false;
            }
            match bytes[i] {
                b'>' => {
                    i += 1;
                    break;
                }
                b'/' => {
                    self_closing = rest[i..].starts_with("/>");
                    i += 1;
                    continue;
                }
                _ => {}
            }
            let name_start = i;
            while i < bytes.len()
                && !bytes[i].is_ascii_whitespace()
                && bytes[i] != b'='
                && bytes[i] != b'>'
                && !(bytes[i] == b'/' && rest[i..].starts_with("/>"))
            {
                i += 1;
            }
            let name = rest[name_start..i].to_ascii_lowercase();
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            let mut value = String::new();
            if i < bytes.len() && bytes[i] == b'=' {
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
                    let quote = bytes[i];
                    i += 1;
                    let value_start = i;
                    while i < bytes.len() && bytes[i] != quote {
                        i += 1;
                    }
                    if i >= bytes.len() {
                        return false;
                    }
                    value = decode_entities(&rest[value_start..i]);
                    i += 1;
                } else {
                    let value_start = i;
                    while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' {
                        i += 1;
                    }
                    value = decode_entities(&rest[value_start..i]);
                }
            }
            if !name.is_empty() && !attrs.iter().any(|(k, _)| *k == name) {
                attrs.push((name, value));
            }
        }
        self.pos += i;

        self.close_implied(&tag);
        let element = Element {
            tag: tag.clone(),
            attrs,
            children: Vec::new(),
        };
        if is_void(&tag) || self_closing {
            self.push_node(Node::Element(element));
        } else if RAW_TEXT.contains(&tag.as_str()) {
            let rest = self.rest();
            let lower = rest.to_ascii_lowercase();
            let close = format!("</{tag}");
            let end = lower.find(&close).unwrap_or(rest.len());
            let mut element = element;
            if end > 0 {
                element.children.push(Node::Text(rest[..end].to_string()));
            }
            self.pos += end;
            // 閉じタグを読み飛ばす
            let after = self.rest();
            match after.find('>') {
                Some(gt) => self.pos += gt + 1,
                None => self.pos = self.input.len(),
            }
            self.push_node(Node::Element(element));
        } else {
            self.stack.push(element);
        }
        true
    }

    /// `<p>` や `<li>` のように、次の要素の開始で暗黙に閉じるものを閉じる
    fn close_implied(&mut self, tag: &str) {
        if CLOSES_P.contains(&tag) && self.stack.last().is_some_and(|e| e.tag == "p") {
            self.pop();
        }
        match tag {
            "li" => self.close_nearest("li", &["ul", "ol"]),
            "dt" | "dd" => {
                self.close_nearest("dt", &["dl"]);
                self.close_nearest("dd", &["dl"]);
            }
            "td" | "th" => {
                self.close_nearest("td", &["tr", "table"]);
                self.close_nearest("th", &["tr", "table"]);
            }
            "tr" => {
                self.close_nearest("tr", &["table"]);
            }
            _ => {}
        }
    }

    /// `name` が `boundaries` より手前（内側）で開いていれば、そこまで閉じる
    fn close_nearest(&mut self, name: &str, boundaries: &[&str]) {
        for (index, element) in self.stack.iter().enumerate().rev() {
            if boundaries.contains(&element.tag.as_str()) {
                return;
            }
            if element.tag == name {
                while self.stack.len() > index {
                    self.pop();
                }
                return;
            }
        }
    }

    fn close(&mut self, name: &str) {
        if let Some(index) = self.stack.iter().rposition(|e| e.tag == name) {
            while self.stack.len() > index {
                self.pop();
            }
        }
        // 開いていない閉じタグは無視する
    }

    fn pop(&mut self) {
        if let Some(element) = self.stack.pop() {
            self.push_node(Node::Element(element));
        }
    }

    fn finish(mut self) -> Vec<Node> {
        while !self.stack.is_empty() {
            self.pop();
        }
        self.root
    }
}

/// 使われる実体参照（名前付きは、WordPress の本文によく出るものだけ）
fn named_entity(name: &str) -> Option<&'static str> {
    Some(match name {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        "nbsp" => "\u{a0}",
        "ndash" => "–",
        "mdash" => "—",
        "hellip" => "…",
        "lsquo" => "‘",
        "rsquo" => "’",
        "ldquo" => "“",
        "rdquo" => "”",
        "sbquo" => "‚",
        "bdquo" => "„",
        "laquo" => "«",
        "raquo" => "»",
        "bull" => "•",
        "middot" => "·",
        "copy" => "©",
        "reg" => "®",
        "trade" => "™",
        "times" => "×",
        "divide" => "÷",
        "deg" => "°",
        "plusmn" => "±",
        "euro" => "€",
        "pound" => "£",
        "yen" => "¥",
        "cent" => "¢",
        "sect" => "§",
        "para" => "¶",
        "larr" => "←",
        "rarr" => "→",
        "uarr" => "↑",
        "darr" => "↓",
        "hearts" => "♥",
        "prime" => "′",
        "Prime" => "″",
        "frac12" => "½",
        "frac14" => "¼",
        "frac34" => "¾",
        "shy" => "\u{ad}",
        "zwj" => "\u{200d}",
        "zwnj" => "\u{200c}",
        _ => return None,
    })
}

/// 実体参照（`&amp;`・`&#8217;`・`&#x2019;`）を文字に直す。知らない名前はそのまま残す
pub fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let decoded = after.find(';').filter(|&end| end <= 10).and_then(|end| {
            let name = &after[..end];
            let value = if let Some(num) = name.strip_prefix('#') {
                let code = match num.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok(),
                    None => num.parse::<u32>().ok(),
                };
                code.and_then(char::from_u32).map(|c| c.to_string())
            } else {
                named_entity(name).map(str::to_string)
            };
            value.map(|v| (v, end))
        });
        match decoded {
            Some((value, end)) => {
                out.push_str(&value);
                rest = &after[end + 1..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn escape_text(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
}

fn escape_attr(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
}

fn write_element(element: &Element, out: &mut String) {
    out.push('<');
    out.push_str(&element.tag);
    for (name, value) in &element.attrs {
        out.push(' ');
        out.push_str(name);
        out.push_str("=\"");
        escape_attr(value, out);
        out.push('"');
    }
    out.push('>');
    if is_void(&element.tag) {
        return;
    }
    write_nodes(&element.children, out);
    out.push_str("</");
    out.push_str(&element.tag);
    out.push('>');
}

fn write_nodes(nodes: &[Node], out: &mut String) {
    for node in nodes {
        match node {
            Node::Text(t) => escape_text(t, out),
            Node::Element(e) => write_element(e, out),
            Node::Comment(_) => {}
        }
    }
}

/// 節を HTML にする（コメントは出さない。テキストと属性はエスケープし直す）
pub fn serialize(nodes: &[Node]) -> String {
    let mut out = String::new();
    write_nodes(nodes, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el(nodes: &[Node], index: usize) -> &Element {
        match &nodes[index] {
            Node::Element(e) => e,
            other => panic!("not an element: {other:?}"),
        }
    }

    #[test]
    fn test_parses_nested_elements_attributes_and_entities() {
        let nodes = parse_fragment(
            r#"<p class="a b" data-x='1'>Tom &amp; <a href=/x?y=1&amp;z=2>Jerry</a>&hellip;</p>"#,
        );
        let p = el(&nodes, 0);
        assert_eq!(p.tag, "p");
        assert!(p.has_class("a") && p.has_class("b"));
        assert_eq!(p.attr("data-x"), Some("1"));
        assert_eq!(p.text_content(), "Tom & Jerry…");
        let a = match &p.children[1] {
            Node::Element(a) => a,
            _ => panic!(),
        };
        assert_eq!(a.attr("href"), Some("/x?y=1&z=2"));
    }

    #[test]
    fn test_unclosed_p_and_li_are_closed_by_the_next_one() {
        let nodes = parse_fragment("<p>one<p>two<ul><li>a<li>b</ul>");
        assert_eq!(nodes.len(), 3);
        assert_eq!(el(&nodes, 0).text_content(), "one");
        assert_eq!(el(&nodes, 1).text_content(), "two");
        let ul = el(&nodes, 2);
        let items: Vec<_> = ul
            .children
            .iter()
            .filter_map(|n| match n {
                Node::Element(e) if e.tag == "li" => Some(e.text_content()),
                _ => None,
            })
            .collect();
        assert_eq!(items, ["a", "b"]);
    }

    #[test]
    fn test_nested_lists_keep_their_structure() {
        let nodes = parse_fragment("<ul><li>a<ul><li>a1</li></ul></li><li>b</li></ul>");
        let ul = el(&nodes, 0);
        assert_eq!(ul.children.len(), 2);
        assert!(ul.contains_tag(&["ul"]));
    }

    #[test]
    fn test_comments_void_elements_and_stray_end_tags() {
        let nodes = parse_fragment("a<!-- wp:more --><br>b</span></div><img src=x.png alt=\"\">");
        assert!(matches!(&nodes[1], Node::Comment(c) if c.contains("wp:more")));
        assert_eq!(el(&nodes, 2).tag, "br");
        assert_eq!(nodes[3], Node::Text("b".to_string()));
        let img = el(&nodes, 4);
        assert_eq!(img.attr("src"), Some("x.png"));
        assert_eq!(img.attr("alt"), Some(""));
    }

    #[test]
    fn test_script_and_style_are_kept_as_raw_text() {
        let nodes = parse_fragment("<script>if (a < b && c) {}</script><p>x</p>");
        let script = el(&nodes, 0);
        assert_eq!(script.tag, "script");
        assert_eq!(script.text_content(), "if (a < b && c) {}");
        assert_eq!(el(&nodes, 1).tag, "p");
    }

    #[test]
    fn test_broken_markup_does_not_panic() {
        for html in [
            "<",
            "<p",
            "<p class=\"x",
            "</",
            "<!--",
            "<!-- x",
            "a < b",
            "<a href='x'>y",
            "<div><p>x</div></p>",
            "&",
            "&#xZZ;&#99999999;",
        ] {
            let _ = parse_fragment(html);
        }
        assert_eq!(
            decode_entities("&#xZZ; &unknown; &amp"),
            "&#xZZ; &unknown; &amp"
        );
    }

    #[test]
    fn test_decode_entities() {
        assert_eq!(
            decode_entities("&lt;b&gt; &#8217; &#x2019; &mdash; &nbsp;|"),
            "<b> ’ ’ — \u{a0}|"
        );
    }

    #[test]
    fn test_serialize_escapes_again() {
        let nodes = parse_fragment(r#"<p title="a &quot;b&quot;">1 &lt; 2 &amp; <br>x</p>"#);
        assert_eq!(
            serialize(&nodes),
            r#"<p title="a &quot;b&quot;">1 &lt; 2 &amp; <br>x</p>"#
        );
    }
}
