//! `on-render-page` プラグインが足すスクリプトの検査とタグの組み立て（docs/csp.md の 4）。
//!
//! プラグインは `PageRenderContext.scripts`（構造化した注入）でだけスクリプトを足せる。ここで、マニフェストの
//! 宣言と照合し、ホストが `<script>` のタグを作る。属性はホストが決める。プラグインが返した HTML の断片
//! （`extra_head` など）の中の `<script>` と `on*` 属性は、[`strip_scripts`] で取り除く。

use aegis_core::{InjectedScript, PluginManifest, ScriptSource, MAX_INLINE_SCRIPT_BYTES};

use aegis_core::csp::escape_inline_script as escape_inline;

/// 検査して作ったタグと、捨てたスクリプト（理由つき）
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ScriptTags {
    /// `<head>` に足す HTML（スクリプトごとに 1 行）
    pub html: String,
    /// 検査で捨てたもの（ログ・監査用の短い説明）
    pub rejected: Vec<String>,
    /// 出したインラインのスクリプトの本文（`<script>` の中身そのもの。ハッシュを取るための記録）
    pub inline_bodies: Vec<String>,
}

/// プラグインが返した HTML の断片から、`<script>` と `on*` 属性を取り除く。
/// 解析できないときは、実行されうるものを残さないよう、全部を捨てる
pub fn strip_scripts(html: &str) -> String {
    // タグが無ければ、実行されるものも無い
    if !html.contains('<') {
        return html.to_string();
    }
    aegis_ast::rewriter::remove_scripts(html).unwrap_or_default()
}

/// 同意の区分に使える文字列か（`[a-z0-9_-]` の 1〜32 文字）
fn valid_category(category: &str) -> bool {
    (1..=32).contains(&category.len())
        && category
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// `https://<ホスト>/...` の URL の、ホスト（小文字）。ユーザー情報・ポート・空白・引用符・制御文字を含むものは `None`
fn https_host(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://")?;
    if url.bytes().any(|b| {
        b <= b' ' || b == b'"' || b == b'\'' || b == b'<' || b == b'>' || b == b'\\' || b == 0x7f
    }) {
        return None;
    }
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let host = &rest[..end];
    if host.is_empty() || host.contains(['@', ':']) {
        return None;
    }
    Some(host.to_ascii_lowercase())
}

/// 同じサイトの URL（`/` で始まるパス。`//` と `\` を含むものは別のホストになりうるので除く）。引用符・空白・制御文字も除く
fn is_same_origin_path(url: &str) -> bool {
    url.starts_with('/') && !url.starts_with("//") && !url.bytes().any(is_unsafe_url_byte)
}

/// URL に入れてはいけないバイト（制御文字・空白・引用符・`<` `>`・バックスラッシュ・DEL）
fn is_unsafe_url_byte(b: u8) -> bool {
    matches!(b, 0..=0x20 | 0x22 | 0x27 | 0x3c | 0x3e | 0x5c | 0x7f)
}

/// 属性の値に入れる文字列のエスケープ
fn escape_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// 検査して、`<head>` に足すタグを作る。
///
/// - `external`: このサイトのパス（`/assets/x.js`）か、ホストがマニフェストの `capabilities.csp.script_domains` にある `https://` の URL
/// - `inline`: マニフェストに `capabilities.inline_scripts: true` の宣言があり、`MAX_INLINE_SCRIPT_BYTES` 以下で、NUL を含まないこと
/// - 同意の区分は `[a-z0-9_-]{1,32}`
///
/// 違反したものは捨て、`rejected` に理由を入れる（ページは壊さない）。
pub fn render_script_tags(manifest: &PluginManifest, scripts: &[InjectedScript]) -> ScriptTags {
    let mut tags = ScriptTags::default();
    for script in scripts {
        let consent = match &script.consent {
            Some(category) if valid_category(category) => {
                format!(
                    r#" type="text/plain" data-aegis-consent="{}""#,
                    escape_attribute(category)
                )
            }
            Some(category) => {
                tags.rejected
                    .push(format!("invalid consent category {category:?}"));
                continue;
            }
            None => String::new(),
        };
        match &script.source {
            ScriptSource::External {
                url,
                async_load,
                defer,
                crossorigin,
            } => {
                let allowed = is_same_origin_path(url)
                    || https_host(url).is_some_and(|host| {
                        manifest
                            .csp_script_domains()
                            .iter()
                            .any(|d| d.eq_ignore_ascii_case(&host))
                    });
                if !allowed {
                    tags.rejected.push(format!(
                        "external script {url:?}: not a path on this site or an https URL on a host in capabilities.csp.script_domains"
                    ));
                    continue;
                }
                let flags = format!(
                    "{}{}{}",
                    if *async_load { " async" } else { "" },
                    if *defer { " defer" } else { "" },
                    if *crossorigin {
                        " crossorigin=\"anonymous\""
                    } else {
                        ""
                    }
                );
                tags.html.push_str(&format!(
                    "<script{consent} src=\"{}\"{flags}></script>\n",
                    escape_attribute(url)
                ));
            }
            ScriptSource::Inline { code } => {
                if !manifest.capabilities.inline_scripts {
                    tags.rejected.push(
                        "inline script: capabilities.inline_scripts is not declared".to_string(),
                    );
                    continue;
                }
                if code.len() > MAX_INLINE_SCRIPT_BYTES || code.contains('\0') {
                    tags.rejected.push(format!(
                        "inline script: over {MAX_INLINE_SCRIPT_BYTES} bytes or has a NUL"
                    ));
                    continue;
                }
                let body = escape_inline(code);
                tags.html
                    .push_str(&format!("<script{consent}>{body}</script>\n"));
                // 出したとおりの本文を、`script-src` のハッシュにする（`[security].strict_csp`）
                tags.inline_bodies.push(body);
            }
        }
    }
    tags
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_core::manifest::CspCapability;
    use aegis_core::PluginCapabilities;

    fn manifest(inline: bool, domains: &[&str]) -> PluginManifest {
        PluginManifest {
            name: "p".to_string(),
            capabilities: PluginCapabilities {
                inline_scripts: inline,
                csp: Some(CspCapability {
                    script_domains: domains.iter().map(|d| d.to_string()).collect(),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn inline_scripts_need_the_declaration() {
        let scripts = [InjectedScript::inline("window.a=1;")];
        let off = render_script_tags(&manifest(false, &[]), &scripts);
        assert!(off.html.is_empty());
        assert_eq!(off.rejected.len(), 1, "{:?}", off.rejected);
        let on = render_script_tags(&manifest(true, &[]), &scripts);
        assert_eq!(on.html, "<script>window.a=1;</script>\n");
        assert!(on.rejected.is_empty());
    }

    #[test]
    fn external_scripts_must_be_https_on_a_declared_host() {
        let m = manifest(false, &["www.clarity.ms"]);
        let ok = render_script_tags(
            &m,
            &[
                InjectedScript::external("https://www.clarity.ms/tag/x?a=1&b=2")
                    .async_load()
                    .deferred(),
            ],
        );
        assert_eq!(
            ok.html,
            "<script src=\"https://www.clarity.ms/tag/x?a=1&amp;b=2\" async defer></script>\n"
        );
        for bad in [
            "http://www.clarity.ms/a.js",
            "https://evil.example/a.js",
            "https://www.clarity.ms.evil.example/a.js",
            "https://www.clarity.ms@evil.example/a.js",
            "https://evil.example@www.clarity.ms/a.js",
            "https://www.clarity.ms:8443/a.js",
            "https://www.clarity.ms/a.js\" onload=\"x()",
            "//www.clarity.ms/a.js",
            r"/\evil.example/a.js",
            "javascript:alert(1)",
        ] {
            let out = render_script_tags(&m, &[InjectedScript::external(bad)]);
            assert!(out.html.is_empty(), "{bad}: {}", out.html);
            assert_eq!(out.rejected.len(), 1, "{bad}");
        }
        // このサイトのパスは、宣言なしで通る
        let local = render_script_tags(
            &manifest(false, &[]),
            &[InjectedScript::external("/assets/mermaid.min.js")],
        );
        assert_eq!(
            local.html,
            "<script src=\"/assets/mermaid.min.js\"></script>
"
        );
        // crossorigin は、頼んだものだけ
        let cors = render_script_tags(
            &m,
            &[InjectedScript::external("https://www.clarity.ms/a.js").crossorigin()],
        );
        assert!(
            cors.html.contains(" crossorigin=\"anonymous\""),
            "{}",
            cors.html
        );
        // ホストは大文字小文字を区別しない
        assert!(!render_script_tags(
            &m,
            &[InjectedScript::external("https://WWW.Clarity.MS/a.js")]
        )
        .html
        .is_empty());
    }

    #[test]
    fn consent_is_built_by_the_host_and_validated() {
        let m = manifest(true, &["a.example.com"]);
        let out = render_script_tags(
            &m,
            &[
                InjectedScript::inline("x()").with_consent("analytics"),
                InjectedScript::external("https://a.example.com/t.js").with_consent("marketing"),
            ],
        );
        assert!(
            out.html.contains(
                r#"<script type="text/plain" data-aegis-consent="analytics">x()</script>"#
            ),
            "{}",
            out.html
        );
        assert!(out.html.contains(r#"<script type="text/plain" data-aegis-consent="marketing" src="https://a.example.com/t.js"></script>"#), "{}", out.html);
        let bad = render_script_tags(
            &m,
            &[InjectedScript::inline("x()").with_consent("a\" onload=\"b")],
        );
        assert!(bad.html.is_empty());
        assert_eq!(bad.rejected.len(), 1);
    }

    #[test]
    fn inline_code_cannot_close_the_tag_or_open_a_comment() {
        let m = manifest(true, &[]);
        let out = render_script_tags(
            &m,
            &[InjectedScript::inline(
                "a='</script><script>evil()</script>';b='<!--';c=\"日本語\";",
            )],
        );
        assert!(
            !out.html.to_lowercase().contains("</script><script>"),
            "{}",
            out.html
        );
        assert!(!out.html.contains("<!--"), "{}", out.html);
        assert_eq!(out.html.matches("</script>").count(), 1, "{}", out.html);
        assert!(out.html.contains("日本語"));
        // 大きすぎる・NUL
        assert!(render_script_tags(
            &m,
            &[InjectedScript::inline(
                "a".repeat(MAX_INLINE_SCRIPT_BYTES + 1)
            )]
        )
        .html
        .is_empty());
        assert!(render_script_tags(&m, &[InjectedScript::inline("a\0b")])
            .html
            .is_empty());
    }

    #[test]
    fn strip_scripts_removes_what_runs_and_keeps_the_rest() {
        let html = r#"<meta name="a" content="b"><script>alert(1)</script><link rel="stylesheet" href="/x.css" onload="evil()"><b onclick="x()">t</b>"#;
        let out = strip_scripts(html);
        assert!(
            !out.contains("alert") && !out.contains("evil") && !out.contains("x()"),
            "{out}"
        );
        assert!(out.contains(r#"<meta name="a" content="b">"#), "{out}");
        // 実行されるものが無い断片は、そのまま
        assert_eq!(
            strip_scripts("<meta name=\"a\" content=\"b\">"),
            "<meta name=\"a\" content=\"b\">"
        );
        assert_eq!(strip_scripts(""), "");
        assert_eq!(strip_scripts("<p>one on one</p>"), "<p>one on one</p>");
    }
}
