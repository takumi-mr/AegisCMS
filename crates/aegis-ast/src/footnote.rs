//! 脚注（GitHub の `[^label]` と `[^label]: 注釈`）。
//!
//! 定義は `core/footnote` ブロック（`attributes.label`・`text`）、本文中の参照は段落などのテキストに書いた
//! `[^label]`。番号は「本文で最初に参照された順」で、文書全体を描画しながら振る（インライン書式の描画は
//! 文脈を持たないので、`HtmlRenderer::render_nodes_with` が描画の間だけ有効にする文脈を使う）。
//!
//! GitHub に合わせた挙動:
//! - 定義の無い `[^x]` は文字のまま出す。参照されない定義は出さない。
//! - 同じ脚注を何度参照しても番号は同じで、末尾の戻りリンクが `↩ ↩²` のように増える。
//! - 注釈は文書の末尾にまとめて出す（定義を書いた位置には何も出さない）。
//!
//! 要素の `id` には、定義の内容から作るハッシュを足す（`fn-1-1a2b3c4d`）。コンテンツ API の一覧など
//! 別々の本文を 1 つのページに並べても、`id` が重ならないようにするため。

use std::cell::RefCell;
use std::collections::HashMap;

use crate::inline::render_inline;
use crate::renderer::escape_html;

/// ラベルに使える文字（英数字・`_`・`-`）と、最大の長さ
const MAX_LABEL_CHARS: usize = 40;

pub(crate) fn is_valid_label(label: &str) -> bool {
    !label.is_empty()
        && label.chars().count() <= MAX_LABEL_CHARS
        && label
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

struct Footnotes {
    /// ラベル → 注釈のテキスト（同じラベルは最初の定義だけ使う）
    definitions: HashMap<String, String>,
    /// `id` に足すハッシュ
    suffix: String,
    /// 最初に参照された順のラベル（番号は位置 + 1）
    order: Vec<String>,
    /// ラベルごとの参照の回数
    refs: HashMap<String, usize>,
}

thread_local! {
    static CONTEXT: RefCell<Option<Footnotes>> = const { RefCell::new(None) };
}

/// FNV-1a（`id` のハッシュ。暗号用ではない）
fn fnv(text: &str) -> u32 {
    text.bytes().fold(0x811c_9dc5u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    })
}

/// 描画の間だけ脚注の文脈を有効にする。`Drop` で元の文脈に戻す
pub(crate) struct Scope {
    previous: Option<Footnotes>,
}

impl Drop for Scope {
    fn drop(&mut self) {
        let previous = self.previous.take();
        CONTEXT.with(|c| *c.borrow_mut() = previous);
    }
}

/// `definitions`（`(ラベル, テキスト)`）で文脈を始める。有効な定義が 1 つも無ければ `None`
pub(crate) fn begin(definitions: Vec<(String, String)>) -> Option<Scope> {
    let mut map = HashMap::new();
    let mut seed = String::new();
    for (label, text) in definitions {
        if !is_valid_label(&label) || text.trim().is_empty() || map.contains_key(&label) {
            continue;
        }
        seed.push_str(&format!("{label}\u{1}{text}\u{2}"));
        map.insert(label, text);
    }
    if map.is_empty() {
        return None;
    }
    let context = Footnotes {
        definitions: map,
        suffix: format!("{:08x}", fnv(&seed)),
        order: Vec::new(),
        refs: HashMap::new(),
    };
    let previous = CONTEXT.with(|c| c.borrow_mut().replace(context));
    Some(Scope { previous })
}

/// 本文中の参照 `[^label]` の HTML。文脈が無い・定義が無いときは `None`（文字のまま出す）
pub(crate) fn reference(label: &str) -> Option<String> {
    CONTEXT.with(|c| {
        let mut guard = c.borrow_mut();
        let ctx = guard.as_mut()?;
        if !ctx.definitions.contains_key(label) {
            return None;
        }
        let number = match ctx.order.iter().position(|l| l == label) {
            Some(i) => i + 1,
            None => {
                ctx.order.push(label.to_string());
                ctx.order.len()
            }
        };
        let count = ctx.refs.entry(label.to_string()).or_insert(0);
        *count += 1;
        let id = ref_id(label, *count, &ctx.suffix);
        Some(format!(
            "<sup class=\"aegis-fnref\"><a href=\"#fn-{label}-{}\" id=\"{id}\" role=\"doc-noteref\">{number}</a></sup>",
            ctx.suffix
        ))
    })
}

fn ref_id(label: &str, nth: usize, suffix: &str) -> String {
    if nth == 1 {
        format!("fnref-{label}-{suffix}")
    } else {
        format!("fnref-{label}-{nth}-{suffix}")
    }
}

/// 文書の末尾の注釈の一覧。参照が 1 つも無ければ空。注釈の中の参照も数える
pub(crate) fn section() -> String {
    // 1. 注釈の本文を描画する。描画で新しい参照が増えると順番が伸びるので、毎回読み直す
    let mut bodies: Vec<(String, String)> = Vec::new();
    while let Some((label, text)) = CONTEXT.with(|c| {
        let guard = c.borrow();
        let ctx = guard.as_ref()?;
        let label = ctx.order.get(bodies.len())?.clone();
        let text = ctx.definitions.get(&label)?.clone();
        Some((label, text))
    }) {
        bodies.push((label, render_inline(text.trim())));
    }
    let Some((suffix, refs)) = CONTEXT.with(|c| {
        let guard = c.borrow();
        let ctx = guard.as_ref()?;
        Some((ctx.suffix.clone(), ctx.refs.clone()))
    }) else {
        return String::new();
    };
    if bodies.is_empty() {
        return String::new();
    }

    // 2. 参照の回数が決まってから、戻りリンクを付ける
    let mut items = String::new();
    for (label, body) in &bodies {
        let mut backrefs = String::new();
        for nth in 1..=refs.get(label).copied().unwrap_or(1) {
            let mark = if nth == 1 {
                "↩".to_string()
            } else {
                format!("↩<sup>{nth}</sup>")
            };
            backrefs.push_str(&format!(
                " <a href=\"#{}\" class=\"aegis-fn-backref\" role=\"doc-backlink\">{mark}</a>",
                escape_html(&ref_id(label, nth, &suffix))
            ));
        }
        items.push_str(&format!(
            "<li id=\"fn-{label}-{suffix}\"><p>{body}{backrefs}</p></li>
"
        ));
    }
    format!(
        "<section class=\"aegis-footnotes\" role=\"doc-endnotes\"><ol>
{items}</ol></section>
"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_restricted() {
        assert!(is_valid_label("1"));
        assert!(is_valid_label("note_a-b"));
        assert!(!is_valid_label(""));
        assert!(!is_valid_label("a b"));
        assert!(!is_valid_label("a\"b"));
        assert!(!is_valid_label(&"x".repeat(41)));
    }

    #[test]
    fn no_context_means_literal_text() {
        assert_eq!(reference("1"), None);
        assert_eq!(section(), "");
    }
}
