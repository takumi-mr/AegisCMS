//! プレーンテキストの本文から HTML メールの本文を作る（`_layout.html` の `body_html` / `footer_html`）。
//! 空行で区切った段落を `<p>`、段落の中の改行を `<br>`、`http(s)://` の URL をリンクにする。
//! `--` だけの行（`_footer.txt` の区切り）から後はフッター（`footer_html`）にする

/// HTML の特殊文字をエスケープする
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// URL の終わり（空白・全角文字・括弧など）
fn ends_url(c: char) -> bool {
    c.is_whitespace() || !c.is_ascii() || matches!(c, '<' | '>' | '"' | '\'' | '`' | '(' | ')')
}

/// 1 行をエスケープし、URL をリンクにする
fn linkify(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(start) = ["https://", "http://"]
        .iter()
        .filter_map(|scheme| rest.find(scheme))
        .min()
    {
        out.push_str(&escape(&rest[..start]));
        let tail = &rest[start..];
        let mut end = tail.find(ends_url).unwrap_or(tail.len());
        // 文末の句読点は URL に含めない
        while end > 0
            && matches!(
                tail.as_bytes()[end - 1],
                b'.' | b',' | b';' | b':' | b'!' | b'?'
            )
        {
            end -= 1;
        }
        let url = &tail[..end];
        if url.len() <= "https://".len() {
            out.push_str(&escape(url));
        } else {
            let url = escape(url);
            out.push_str(&format!(
                "<a href=\"{url}\" style=\"color:#2563eb;word-break:break-all\">{url}</a>"
            ));
        }
        rest = &tail[end..];
    }
    out.push_str(&escape(rest));
    out
}

/// 段落ごとに `<p>`、段落の中の改行は `<br>`
fn paragraphs(lines: &[&str], style: &str) -> String {
    let mut out = String::new();
    for paragraph in lines.split(|line| line.trim().is_empty()) {
        if paragraph.is_empty() {
            continue;
        }
        let inner: Vec<String> = paragraph.iter().map(|line| linkify(line)).collect();
        out.push_str(&format!(
            "<p style=\"{}\">{}</p>\n",
            style,
            inner.join("<br>\n")
        ));
    }
    out
}

/// 本文とフッターの HTML
pub fn text_to_html(text: &str) -> (String, String) {
    let lines: Vec<&str> = text.lines().collect();
    let (body, footer) = match lines.iter().rposition(|line| line.trim_end() == "--") {
        Some(index) => (&lines[..index], &lines[index + 1..]),
        None => (&lines[..], &[][..]),
    };
    (
        paragraphs(body, "margin:0 0 16px;line-height:1.7"),
        paragraphs(footer, "margin:0;line-height:1.6"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_paragraphs_links_and_the_footer() {
        let (body, footer) = text_to_html(
            "Dear <Hanako>,\n\nYour order:\n- Mug x 2\n\nTrack it at https://shop.example.com/orders/1?token=a&b=c.\n--\nAegis & Co\nhttps://shop.example.com\n",
        );
        assert_eq!(
            body,
            "<p style=\"margin:0 0 16px;line-height:1.7\">Dear &lt;Hanako&gt;,</p>\n\
             <p style=\"margin:0 0 16px;line-height:1.7\">Your order:<br>\n- Mug x 2</p>\n\
             <p style=\"margin:0 0 16px;line-height:1.7\">Track it at \
             <a href=\"https://shop.example.com/orders/1?token=a&amp;b=c\" style=\"color:#2563eb;word-break:break-all\">\
             https://shop.example.com/orders/1?token=a&amp;b=c</a>.</p>\n"
        );
        assert!(footer.contains("Aegis &amp; Co<br>"));
        assert!(footer.contains("<a href=\"https://shop.example.com\""));
    }

    #[test]
    fn urls_stop_at_full_width_text_and_unsafe_characters() {
        assert_eq!(
            linkify("確認：https://a.example/x）です"),
            "確認：<a href=\"https://a.example/x\" style=\"color:#2563eb;word-break:break-all\">https://a.example/x</a>）です"
        );
        assert_eq!(
            linkify("https://a.example/\"><script>"),
            "<a href=\"https://a.example/\" style=\"color:#2563eb;word-break:break-all\">https://a.example/</a>&quot;&gt;&lt;script&gt;"
        );
        // スキームだけはリンクにしない
        assert_eq!(linkify("see https:// now"), "see https:// now");
        let (body, footer) = text_to_html("no footer");
        assert!(body.contains("no footer"));
        assert!(footer.is_empty());
    }
}
