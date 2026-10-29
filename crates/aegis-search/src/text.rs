//! 本文のブロックから、検索の対象にする文字を取り出す

use aegis_ast::{plain_text, BlockNode};

/// ブロックの木の文字（インライン書式の記号は除く）を、ブロックごとに改行で区切って返す。
/// カスタム HTML（`core/html`）・目次・区切り線・プラグインのブロックは対象にしない
pub fn blocks_text(blocks: &[BlockNode]) -> String {
    let mut out = String::new();
    for block in blocks {
        collect(block, &mut out);
    }
    out.trim_end().to_string()
}

fn push_line(out: &mut String, text: &str) {
    let text = text.trim();
    if !text.is_empty() {
        out.push_str(text);
        out.push('\n');
    }
}

fn attr<'a>(block: &'a BlockNode, name: &str) -> Option<&'a str> {
    block.attributes.get(name).and_then(|v| v.as_str())
}

fn collect(block: &BlockNode, out: &mut String) {
    match block.name.as_str() {
        "core/html" | "core/toc" | "core/separator" => return,
        // コードの中の記号は書式ではないので、そのまま
        "core/code" => {
            push_line(out, block.text.as_deref().unwrap_or_default());
            return;
        }
        "core/image" => {
            push_line(out, attr(block, "alt").unwrap_or_default());
            return;
        }
        "core/table" => {
            if let Some(rows) = block.attributes.get("rows").and_then(|v| v.as_array()) {
                for row in rows {
                    let cells: Vec<String> = row
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|c| c.as_str())
                        .map(plain_text)
                        .collect();
                    push_line(out, &cells.join(" "));
                }
            }
            return;
        }
        "core/details" => push_line(out, &plain_text(attr(block, "summary").unwrap_or_default())),
        "core/callout" => push_line(out, &plain_text(attr(block, "title").unwrap_or_default())),
        name if !name.starts_with("core/") => return,
        _ => {}
    }
    if let Some(text) = &block.text {
        push_line(out, &plain_text(text));
    }
    for child in &block.children {
        collect(child, out);
    }
    if block.name == "core/quote" {
        push_line(out, &plain_text(attr(block, "cite").unwrap_or_default()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extracts_text_from_blocks() {
        let blocks: Vec<BlockNode> = serde_json::from_value(serde_json::json!([
            {"name": "core/heading", "attributes": {"level": 2}, "text": "**はじめ**に"},
            {"name": "core/paragraph", "text": "[リンク](/docs) と `code`"},
            {"name": "core/list", "children": [
                {"name": "core/list-item", "text": "項目 1"},
                {"name": "core/list-item", "text": "項目 2"}
            ]},
            {"name": "core/table", "attributes": {"rows": [["名前", "価格"], ["豆", "1,780円"]]}},
            {"name": "core/details", "attributes": {"summary": "質問"}, "children": [
                {"name": "core/paragraph", "text": "答え"}
            ]},
            {"name": "core/callout", "attributes": {"title": "注意"}, "children": [
                {"name": "core/paragraph", "text": "本文"}
            ]},
            {"name": "core/quote", "attributes": {"cite": "出典"}, "children": [
                {"name": "core/paragraph", "text": "引用"}
            ]},
            {"name": "core/code", "text": "let **x** = 1;"},
            {"name": "core/image", "attributes": {"src": "/a.png", "alt": "写真"}},
            {"name": "core/html", "text": "<p>secret</p>"},
            {"name": "core/toc"},
            {"name": "official-blocks/button", "attributes": {"label": "x"}}
        ]))
        .unwrap();
        assert_eq!(
            blocks_text(&blocks),
            "はじめに\nリンク と code\n項目 1\n項目 2\n名前 価格\n豆 1,780円\n質問\n答え\n注意\n本文\n引用\n出典\nlet **x** = 1;\n写真"
        );
    }
}
