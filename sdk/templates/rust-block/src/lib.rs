//! カスタムブロックを足すプラグインのテンプレート（`render-blocks` フック）。
//!
//! ブロックの定義（名前・入力欄）は `plugin.manifest.json` の `blocks`、HTML を作るのはこのファイルです。
//! 保存されるのはブロック名（`<プラグイン名>/note`）と `attributes` だけで、HTML は表示のたびに
//! `render_blocks` が作ります。詳しくは docs/plugin_block_tutorial.md。

use aegis_plugin_sdk::prelude::*;

/// 最初に表示するブロックの前に 1 回だけ付ける CSS。`:where()` で詳細度を 0 にして、テーマの CSS で
/// 上書きしやすくしている
const STYLES: &str = "<style>\
:where(.nb-note){margin:1.5em 0;padding:.75em 1em;border-left:4px solid #2563eb;background:#eff6ff;border-radius:4px}\
:where(.nb-note--warning){border-left-color:#d97706;background:#fffbeb}\
:where(.nb-note) p{margin:0}\
</style>";

/// 利用者が入力した文字列を HTML に入れるときは、必ずエスケープする
/// （サニタイズはスクリプトを取り除くだけで、レイアウトを壊すタグは残る）
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

/// 文字列の属性（前後の空白を除く）。無い・文字列でないときは空文字列。
/// API や Markdown から保存されたブロックは任意の JSON を持てるので、型が違っても落ちない
fn attr<'a>(block: &'a BlockNode, name: &str) -> &'a str {
    block
        .attribute(name)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or("")
}

/// `<プラグイン名>/note` のブロックの HTML。入力が足りないときは `None`（何も表示しない）
fn render_note(block: &BlockNode) -> Option<String> {
    let text = attr(block, "text");
    if text.is_empty() {
        return None;
    }
    // 知らない値は既定（info）にする
    let tone = match attr(block, "tone") {
        "warning" => "warning",
        _ => "info",
    };
    Some(format!(
        "<aside class=\"nb-note nb-note--{}\"><p>{}</p></aside>",
        tone,
        escape(text)
    ))
}

#[derive(Default)]
pub struct NoteBlockPlugin;

#[aegis_plugin]
impl Plugin for NoteBlockPlugin {
    /// 入力と同じ数・同じ順の HTML を返す（`None` のブロックは何も表示しない）。
    /// 1 回の描画で、このプラグインのブロックがまとめて渡される
    fn render_blocks(&mut self, blocks: Vec<BlockNode>) -> Result<Vec<Option<String>>> {
        let mut styled = false;
        Ok(blocks
            .iter()
            .map(|block| {
                let html = match block.name.rsplit('/').next() {
                    Some("note") => render_note(block),
                    _ => None,
                }?;
                // CSS は最初に表示するブロックにだけ付ける
                if styled {
                    Some(html)
                } else {
                    styled = true;
                    Some(format!("{}{}", STYLES, html))
                }
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_plugin_sdk::serde_json::json;

    fn note(attrs: &[(&str, &str)]) -> BlockNode {
        let mut block = BlockNode::new("aegis-plugin-block-template/note");
        for (key, value) in attrs {
            block.set_attribute(*key, json!(value));
        }
        block
    }

    #[test]
    fn renders_the_note_with_the_styles_once() {
        let mut plugin = NoteBlockPlugin;
        let html = plugin
            .render_blocks(vec![
                note(&[("text", "Remember this"), ("tone", "warning")]),
                note(&[("text", "Second")]),
            ])
            .unwrap();
        let first = html[0].as_deref().unwrap();
        assert!(first.starts_with("<style>"));
        assert!(first
            .contains("<aside class=\"nb-note nb-note--warning\"><p>Remember this</p></aside>"));
        // The second block has no styles and falls back to the info tone
        assert_eq!(
            html[1].as_deref(),
            Some("<aside class=\"nb-note nb-note--info\"><p>Second</p></aside>")
        );
    }

    #[test]
    fn escapes_the_text_and_skips_empty_blocks() {
        let mut plugin = NoteBlockPlugin;
        let html = plugin
            .render_blocks(vec![
                note(&[("text", "<b>1 & 2</b>")]),
                note(&[("text", "  ")]),
                note(&[]),
            ])
            .unwrap();
        assert!(html[0]
            .as_deref()
            .unwrap()
            .contains("&lt;b&gt;1 &amp; 2&lt;/b&gt;"));
        assert_eq!(html[1], None);
        assert_eq!(html[2], None);
    }

    #[test]
    fn ignores_attributes_of_the_wrong_type() {
        let mut block = BlockNode::new("aegis-plugin-block-template/note");
        block.set_attribute("text", json!(42));
        let mut plugin = NoteBlockPlugin;
        assert_eq!(plugin.render_blocks(vec![block]).unwrap(), vec![None]);
    }
}
