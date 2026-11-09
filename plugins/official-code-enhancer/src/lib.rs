//! 公式プラグイン `official-code-enhancer`: コードのシンタックスハイライト・外部テーマ・コピーボタン・
//! インラインコードの整形（`transform-content-ast` / `on-render-page`）。
//!
//! - ハイライトは Prism.js（CDN の `cdn.jsdelivr.net`）。言語は autoloader が、必要になったときに読み込む
//! - Mermaid の図は別のプラグイン `official-mermaid`。`language-mermaid` のブロックは、ここでは何もしない
//! - 本文に `<pre` も `` ` `` も無いページには、何も足さない（スクリプトも CSS も入れない）
//! - テーマは、記事のフィールド › テーマ変数（`theme_vars`）› プラグインの設定（Admin Studio の
//!   Settings › Plugins）› 既定（`dracula`）の順で決める

pub mod themes;

use aegis_plugin_sdk::prelude::*;
use std::fmt::Write as _;

#[derive(Default)]
pub struct CodeEnhancerPlugin;

/// Prism の本体とオートローダー（`cdn.jsdelivr.net`。マニフェストの `script_domains`）
const PRISM_URL: &str = "https://cdn.jsdelivr.net/npm/prismjs@1.29.0/prism.min.js";
const PRISM_AUTOLOADER_URL: &str =
    "https://cdn.jsdelivr.net/npm/prismjs@1.29.0/plugins/autoloader/prism-autoloader.min.js";

/// Prism より先に実行して、自動のハイライトを止める（Mermaid のブロックを Prism に渡さないため、
/// ハイライトは `renderAll` が対象を選んで行う）。`<script data-manual>` と同じ（Prism の設定）
const PRISM_MANUAL: &str = "window.Prism = window.Prism || {}; window.Prism.manual = true;";

/// コピーボタン、インラインコードの整形、Prism の呼び出し
const CLIENT_SCRIPT: &str = r#"
  (function() {
    window.AegisCodeEnhancer = {
      setThemeUrl: function(url) {
        var link = document.getElementById('aegis-external-code-theme');
        if (!link) {
          link = document.createElement('link');
          link.id = 'aegis-external-code-theme';
          link.rel = 'stylesheet';
          document.head.appendChild(link);
        }
        link.href = url;
      },
      renderAll: function() {
        formatInlineCode();
        setupCodeCopyButtons();
        highlightCode();
      }
    };

    function isDiagram(code) {
      return !!code && /(^|\s)language-(mermaid|diagram)(\s|$)/.test(code.className);
    }

    // 1. Safe inline code formatting from `inline code` in paragraphs and lists
    function formatInlineCode() {
      var containers = document.querySelectorAll('.prose p, .prose li, .prose blockquote');
      containers.forEach(function(container) {
        var walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT, null);
        var textNodes = [];
        var node;
        while ((node = walker.nextNode())) {
          if (node.parentElement && !['CODE', 'PRE', 'SCRIPT', 'STYLE'].includes(node.parentElement.tagName)) {
            if (node.nodeValue && node.nodeValue.indexOf('`') !== -1) {
              textNodes.push(node);
            }
          }
        }
        textNodes.forEach(function(tNode) {
          var text = tNode.nodeValue;
          var regex = /`([^`\n]+)`/g;
          if (!regex.test(text)) return;
          var frag = document.createDocumentFragment();
          var lastIdx = 0;
          var match;
          regex.lastIndex = 0;
          while ((match = regex.exec(text)) !== null) {
            if (match.index > lastIdx) {
              frag.appendChild(document.createTextNode(text.substring(lastIdx, match.index)));
            }
            var codeEl = document.createElement('code');
            codeEl.className = 'inline-code-enhanced';
            codeEl.textContent = match[1];
            frag.appendChild(codeEl);
            lastIdx = match.index + match[0].length;
          }
          if (lastIdx < text.length) {
            frag.appendChild(document.createTextNode(text.substring(lastIdx)));
          }
          tNode.replaceWith(frag);
        });
      });
    }

    // 2. Add copy button to code blocks
    function setupCodeCopyButtons() {
      var preBlocks = document.querySelectorAll('.prose pre');
      preBlocks.forEach(function(pre) {
        var code = pre.querySelector('code');
        if (isDiagram(code)) return;
        if (pre.parentElement && pre.parentElement.classList.contains('code-block-wrapper')) return;

        var wrapper = document.createElement('div');
        wrapper.className = 'code-block-wrapper';
        pre.parentNode.insertBefore(wrapper, pre);
        wrapper.appendChild(pre);

        var copyBtn = document.createElement('button');
        copyBtn.className = 'code-copy-btn';
        copyBtn.setAttribute('type', 'button');
        copyBtn.setAttribute('aria-label', 'Copy code to clipboard');
        copyBtn.textContent = 'Copy';

        copyBtn.addEventListener('click', async function() {
          var textToCopy = (code || pre).textContent || '';
          try {
            await navigator.clipboard.writeText(textToCopy);
            copyBtn.textContent = 'Copied!';
            copyBtn.classList.add('copied');
            setTimeout(function() {
              copyBtn.textContent = 'Copy';
              copyBtn.classList.remove('copied');
            }, 2000);
          } catch (err) {
            console.error('Failed to copy code: ', err);
          }
        });

        wrapper.appendChild(copyBtn);
      });
    }

    // 3. Syntax highlighting (everything but diagrams, which official-mermaid draws)
    function highlightCode() {
      if (!window.Prism) return;
      document.querySelectorAll('pre code').forEach(function(code) {
        if (!isDiagram(code)) window.Prism.highlightElement(code);
      });
    }

    if (document.readyState === 'loading') {
      document.addEventListener('DOMContentLoaded', function() { window.AegisCodeEnhancer.renderAll(); });
    } else {
      window.AegisCodeEnhancer.renderAll();
    }
  })();
"#;

impl CodeEnhancerPlugin {
    /// Resolves a code-theme option. Priority: article field > theme_vars >
    /// plugin setting (Admin Studio: Settings > Plugins) > `None`.
    fn resolve_option(&self, ctx: &PageRenderContext, key: &str) -> Option<String> {
        let non_empty = |s: &str| {
            let s = s.trim();
            (!s.is_empty()).then(|| s.to_string())
        };
        ctx.fields
            .get(key)
            .and_then(|v| v.as_str())
            .and_then(non_empty)
            .or_else(|| ctx.theme_vars.get(key).and_then(|v| non_empty(v)))
            .or_else(|| config::get(key).and_then(|v| non_empty(&v)))
    }

    /// Resolves the active theme name.
    fn resolve_theme_name(&self, ctx: &PageRenderContext) -> String {
        self.resolve_option(ctx, "code_theme")
            .unwrap_or_else(|| "dracula".to_string())
    }

    /// Resolves external CSS stylesheet URL if configured.
    fn resolve_external_url(&self, ctx: &PageRenderContext) -> Option<String> {
        self.resolve_option(ctx, "code_theme_url")
    }

    /// Resolves raw custom CSS string if configured.
    fn resolve_custom_css(&self, ctx: &PageRenderContext) -> Option<String> {
        self.resolve_option(ctx, "code_theme_css")
    }

    /// 本文にコードブロック（`<pre`）か、整形する `` `inline code` `` が無ければ、何も足さない
    fn has_code(&self, ctx: &PageRenderContext) -> bool {
        ctx.content_html.contains("<pre") || ctx.content_html.contains('`')
    }
}

#[aegis_plugin]
impl Plugin for CodeEnhancerPlugin {
    /// Marks code blocks (diagram blocks are left alone: `official-mermaid` draws them).
    fn transform_content_ast(&mut self, mut blocks: Vec<BlockNode>) -> Result<Vec<BlockNode>> {
        for block in &mut blocks {
            if block.name == "core/code" {
                let lang = block
                    .attribute("language")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_lowercase();

                if lang != "mermaid" && lang != "diagram" {
                    block.set_attribute("data-enhanced-code", serde_json::json!(true));
                }
            }
        }
        Ok(blocks)
    }

    /// Injects theme styles, external CSS links, and the client-side enhancer bundle.
    fn on_render_page(&mut self, mut ctx: PageRenderContext) -> Result<PageRenderContext> {
        if !self.has_code(&ctx) {
            return Ok(ctx);
        }
        let theme_name = self.resolve_theme_name(&ctx);
        let theme_def = themes::get_theme(&theme_name);
        log_info!("Applying code enhancer with theme: '{}'", theme_def.name);

        // 1. Assemble extra_css
        let mut combined_css = String::new();
        combined_css.push_str(themes::BASE_ENHANCER_CSS);
        combined_css.push_str(theme_def.css);

        // Append custom CSS if specified
        if let Some(custom_css) = self.resolve_custom_css(&ctx) {
            combined_css.push_str("\n/* Custom CSS Override */\n");
            combined_css.push_str(&custom_css);
        }

        if ctx.extra_css.is_empty() {
            ctx.extra_css = combined_css;
        } else {
            ctx.extra_css.push('\n');
            ctx.extra_css.push_str(&combined_css);
        }

        // 2. Assemble extra_head
        let mut head_snippet = String::new();

        // Optional external CSS stylesheet URL (e.g. Prism CDN theme)
        if let Some(ext_url) = self.resolve_external_url(&ctx) {
            let _ = writeln!(
                head_snippet,
                r#"<link id="aegis-external-code-theme" rel="stylesheet" href="{}">"#,
                ext_url
                    .replace('&', "&amp;")
                    .replace('"', "&quot;")
                    .replace('<', "&lt;")
            );
        }

        if ctx.extra_head.is_empty() {
            ctx.extra_head = head_snippet;
        } else {
            ctx.extra_head.push('\n');
            ctx.extra_head.push_str(&head_snippet);
        }

        // Client bundle: Prism (loaded from the CDN, run by hand), then the enhancer. The host
        // checks the CDN host against the manifest's `script_domains` and builds the tags
        ctx.scripts.push(InjectedScript::inline(PRISM_MANUAL));
        ctx.scripts.push(InjectedScript::external(PRISM_URL));
        ctx.scripts
            .push(InjectedScript::external(PRISM_AUTOLOADER_URL));
        ctx.scripts.push(InjectedScript::inline(CLIENT_SCRIPT));

        Ok(ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_plugin_sdk::host::mock;

    fn page(content_html: &str) -> PageRenderContext {
        PageRenderContext {
            title: "Sample Post".to_string(),
            content_html: content_html.to_string(),
            ..Default::default()
        }
    }

    const WITH_CODE: &str = "<p>Hi</p><pre><code class=\"language-rust\">fn main() {}</code></pre>";

    #[test]
    fn test_transform_marks_code_but_not_diagrams() {
        let mut plugin = CodeEnhancerPlugin;
        let blocks = vec![
            BlockNode::code("mermaid", "graph TD;\n  A-->B;"),
            BlockNode::code("rust", "fn main() {}"),
        ];

        let result = plugin
            .transform_content_ast(blocks)
            .expect("Hook should succeed");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].attribute("data-enhanced-code"), None);
        assert_eq!(result[0].attribute("data-diagram-type"), None);
        assert_eq!(
            result[1].attribute("data-enhanced-code"),
            Some(&serde_json::json!(true))
        );
    }

    #[test]
    fn test_on_render_page_injects_theme_and_bundle() {
        mock::reset();
        let mut plugin = CodeEnhancerPlugin;
        let updated = plugin
            .on_render_page(page(WITH_CODE))
            .expect("Hook should succeed");

        // Checks CSS injection
        assert!(updated.extra_css.contains("inline-code-enhanced"));
        assert!(updated.extra_css.contains("Dracula Theme"));

        // Checks Script injection: Prism only, started by the bundle. The manual flag runs before
        // Prism loads (it replaces `<script data-manual>`)
        assert!(updated.head_html().contains("AegisCodeEnhancer"));
        assert_eq!(
            updated.scripts[0],
            InjectedScript::inline(
                "window.Prism = window.Prism || {}; window.Prism.manual = true;"
            )
        );
        assert_eq!(updated.scripts[1], InjectedScript::external(PRISM_URL));
        assert_eq!(
            updated.scripts[2],
            InjectedScript::external(PRISM_AUTOLOADER_URL)
        );
        assert!(updated.head_html().contains("prism-autoloader.min.js"));
        // Mermaid is official-mermaid's job
        assert!(!updated
            .head_html()
            .to_lowercase()
            .contains("mermaid.min.js"));
        assert!(!updated.head_html().contains("mermaid.initialize"));
        assert!(!updated.extra_css.contains("mermaid-diagram-container"));
        // Diagrams are skipped by the highlighter and the copy button
        assert!(updated.head_html().contains("language-(mermaid|diagram)"));
    }

    /// 本文にコードも `inline code` も無いページには、何も足さない
    #[test]
    fn test_pages_without_code_are_left_alone() {
        mock::reset();
        let mut plugin = CodeEnhancerPlugin;
        for html in ["", "<p>No code here.</p><h2>Title</h2>"] {
            let updated = plugin.on_render_page(page(html)).unwrap();
            assert!(
                updated.extra_css.is_empty() && updated.head_html().is_empty(),
                "{html:?}"
            );
        }
        // `inline code` を書いただけのページには入れる（スクリプトが <code> に整形する）
        let updated = plugin
            .on_render_page(page("<p>Use `cargo test` here.</p>"))
            .unwrap();
        assert!(updated.head_html().contains("AegisCodeEnhancer"));
        assert!(!updated.extra_css.is_empty());
    }

    #[test]
    fn test_plugin_settings_are_the_fallback() {
        mock::reset();
        mock::set_config("code_theme", "nord");
        mock::set_config("code_theme_css", ".x { color: red; }");
        let mut plugin = CodeEnhancerPlugin;

        // Plugin setting applies when nothing else is given
        let updated = plugin
            .on_render_page(page(WITH_CODE))
            .expect("Hook should succeed");
        assert!(updated.extra_css.contains("Nord"));
        assert!(updated.extra_css.contains(".x { color: red; }"));

        // An article field wins over the plugin setting
        let mut ctx = page(WITH_CODE);
        ctx.fields
            .insert("code_theme".to_string(), serde_json::json!("github-light"));
        let updated = plugin.on_render_page(ctx).expect("Hook should succeed");
        assert!(updated.extra_css.contains("GitHub Light Theme"));
        mock::reset();
    }

    #[test]
    fn test_custom_theme_selection_and_external_url() {
        mock::reset();
        let mut plugin = CodeEnhancerPlugin;
        let mut ctx = page(WITH_CODE);
        ctx.fields
            .insert("code_theme".to_string(), serde_json::json!("github-light"));
        ctx.fields.insert(
            "code_theme_url".to_string(),
            serde_json::json!("https://example.com/custom-prism.css?a=1&b=\"2\""),
        );

        let updated = plugin.on_render_page(ctx).expect("Hook should succeed");

        // Selected GitHub Light
        assert!(updated.extra_css.contains("GitHub Light Theme"));
        // Injected external stylesheet link (the URL is escaped for the attribute)
        assert!(updated
            .head_html()
            .contains(r#"href="https://example.com/custom-prism.css?a=1&amp;b=&quot;2&quot;""#));
    }
}
