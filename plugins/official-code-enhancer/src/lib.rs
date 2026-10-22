pub mod themes;

use aegis_plugin_sdk::prelude::*;
use std::fmt::Write as _;

#[derive(Default)]
pub struct CodeEnhancerPlugin;

impl CodeEnhancerPlugin {
    /// Resolves the active theme name from post fields or theme_vars.
    fn resolve_theme_name(&self, ctx: &PageRenderContext) -> String {
        // 1. Article-specific field override
        if let Some(val) = ctx.fields.get("code_theme").and_then(|v| v.as_str()) {
            if !val.trim().is_empty() {
                return val.trim().to_string();
            }
        }
        // 2. Global theme_vars override
        if let Some(val) = ctx.theme_vars.get("code_theme") {
            if !val.trim().is_empty() {
                return val.trim().to_string();
            }
        }
        // 3. Default theme
        "dracula".to_string()
    }

    /// Resolves external CSS stylesheet URL if configured.
    fn resolve_external_url(&self, ctx: &PageRenderContext) -> Option<String> {
        // 1. Article-specific field
        if let Some(val) = ctx.fields.get("code_theme_url").and_then(|v| v.as_str()) {
            if !val.trim().is_empty() {
                return Some(val.trim().to_string());
            }
        }
        // 2. Theme vars
        if let Some(val) = ctx.theme_vars.get("code_theme_url") {
            if !val.trim().is_empty() {
                return Some(val.trim().to_string());
            }
        }
        None
    }

    /// Resolves raw custom CSS string if configured.
    fn resolve_custom_css(&self, ctx: &PageRenderContext) -> Option<String> {
        // 1. Article-specific field
        if let Some(val) = ctx.fields.get("code_theme_css").and_then(|v| v.as_str()) {
            if !val.trim().is_empty() {
                return Some(val.trim().to_string());
            }
        }
        // 2. Theme vars
        if let Some(val) = ctx.theme_vars.get("code_theme_css") {
            if !val.trim().is_empty() {
                return Some(val.trim().to_string());
            }
        }
        None
    }

    /// Generates the client-side JavaScript bundle for Mermaid rendering,
    /// safe inline-code formatting, and the copy-to-clipboard button.
    fn generate_client_script(&self, mermaid_theme: &str) -> String {
        format!(
            r#"<!-- AegisCMS Code & Diagram Enhancer Client Bundle -->
<script src="https://cdn.jsdelivr.net/npm/prismjs@1.29.0/prism.min.js"></script>
<script src="https://cdn.jsdelivr.net/npm/prismjs@1.29.0/plugins/autoloader/prism-autoloader.min.js"></script>
<script type="module">
  import mermaid from 'https://cdn.jsdelivr.net/npm/mermaid@10/dist/mermaid.esm.min.mjs';

  mermaid.initialize({{
    startOnLoad: false,
    theme: '{mermaid_theme}',
    securityLevel: 'loose'
  }});

  window.AegisCodeEnhancer = {{
    theme: '{mermaid_theme}',
    // Allows external scripts or admin previews to switch stylesheet dynamically
    setThemeUrl(url) {{
      let link = document.getElementById('aegis-external-code-theme');
      if (!link) {{
        link = document.createElement('link');
        link.id = 'aegis-external-code-theme';
        link.rel = 'stylesheet';
        document.head.appendChild(link);
      }}
      link.href = url;
    }},
    renderAll() {{
      formatInlineCode();
      setupCodeCopyButtons();
      renderMermaidDiagrams();
      if (window.Prism) {{
        window.Prism.highlightAll();
      }}
    }}
  }};

  // 1. Safe inline code formatting from `inline code` in paragraphs and lists
  function formatInlineCode() {{
    const containers = document.querySelectorAll('.prose p, .prose li, .prose blockquote');
    containers.forEach(container => {{
      const walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT, null);
      const textNodes = [];
      let node;
      while ((node = walker.nextNode())) {{
        if (node.parentElement && !['CODE', 'PRE', 'SCRIPT', 'STYLE'].includes(node.parentElement.tagName)) {{
          if (node.nodeValue && node.nodeValue.includes('`')) {{
            textNodes.push(node);
          }}
        }}
      }}
      textNodes.forEach(tNode => {{
        const text = tNode.nodeValue;
        const regex = /`([^`\n]+)`/g;
        if (!regex.test(text)) return;
        const frag = document.createDocumentFragment();
        let lastIdx = 0;
        let match;
        regex.lastIndex = 0;
        while ((match = regex.exec(text)) !== null) {{
          if (match.index > lastIdx) {{
            frag.appendChild(document.createTextNode(text.substring(lastIdx, match.index)));
          }}
          const codeEl = document.createElement('code');
          codeEl.className = 'inline-code-enhanced';
          codeEl.textContent = match[1];
          frag.appendChild(codeEl);
          lastIdx = match.index + match[0].length;
        }}
        if (lastIdx < text.length) {{
          frag.appendChild(document.createTextNode(text.substring(lastIdx)));
        }}
        tNode.replaceWith(frag);
      }});
    }});
  }}

  // 2. Add copy button to code blocks
  function setupCodeCopyButtons() {{
    const preBlocks = document.querySelectorAll('.prose pre');
    preBlocks.forEach((pre) => {{
      const code = pre.querySelector('code');
      // Skip mermaid diagrams from code copy
      if (code && code.classList.contains('language-mermaid')) return;
      if (pre.parentElement && pre.parentElement.classList.contains('code-block-wrapper')) return;

      const wrapper = document.createElement('div');
      wrapper.className = 'code-block-wrapper';
      pre.parentNode.insertBefore(wrapper, pre);
      wrapper.appendChild(pre);

      const copyBtn = document.createElement('button');
      copyBtn.className = 'code-copy-btn';
      copyBtn.setAttribute('type', 'button');
      copyBtn.setAttribute('aria-label', 'Copy code to clipboard');
      copyBtn.textContent = 'Copy';

      copyBtn.addEventListener('click', async () => {{
        const textToCopy = (code || pre).textContent || '';
        try {{
          await navigator.clipboard.writeText(textToCopy);
          copyBtn.textContent = 'Copied!';
          copyBtn.classList.add('copied');
          setTimeout(() => {{
            copyBtn.textContent = 'Copy';
            copyBtn.classList.remove('copied');
          }}, 2000);
        }} catch (err) {{
          console.error('Failed to copy code: ', err);
        }}
      }});

      wrapper.appendChild(copyBtn);
    }});
  }}

  // 3. Render Mermaid diagrams
  async function renderMermaidDiagrams() {{
    const mermaidCodes = document.querySelectorAll('pre code.language-mermaid, pre.mermaid, div.mermaid');
    for (let i = 0; i < mermaidCodes.length; i++) {{
      const el = mermaidCodes[i];
      const container = el.closest('pre') || el;
      const graphDef = (el.textContent || '').trim();
      if (!graphDef) continue;
      const diagramId = 'mermaid-diag-' + i + '-' + Math.floor(Math.random() * 10000);
      try {{
        const {{ svg }} = await mermaid.render(diagramId, graphDef);
        const card = document.createElement('div');
        card.className = 'mermaid-diagram-container';
        card.innerHTML = svg;
        container.replaceWith(card);
      }} catch (err) {{
        console.warn('Mermaid rendering failed for block:', err);
      }}
    }}
  }}

  if (document.readyState === 'loading') {{
    document.addEventListener('DOMContentLoaded', () => window.AegisCodeEnhancer.renderAll());
  }} else {{
    window.AegisCodeEnhancer.renderAll();
  }}
</script>
"#
        )
    }
}

#[aegis_plugin]
impl Plugin for CodeEnhancerPlugin {
    /// Detects mermaid and code blocks and attaches metadata attributes.
    fn transform_content_ast(&mut self, mut blocks: Vec<BlockNode>) -> Result<Vec<BlockNode>> {
        for block in &mut blocks {
            if block.name == "core/code" {
                let lang = block
                    .attribute("language")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_lowercase();

                if lang == "mermaid" || lang == "diagram" {
                    block.set_attribute("data-diagram-type", serde_json::json!("mermaid"));
                    log_info!("Found Mermaid diagram block in AST.");
                } else {
                    block.set_attribute("data-enhanced-code", serde_json::json!(true));
                }
            }
        }
        Ok(blocks)
    }

    /// Injects theme styles, external CSS links, and the client-side enhancer bundle.
    fn on_render_page(&mut self, mut ctx: PageRenderContext) -> Result<PageRenderContext> {
        let theme_name = self.resolve_theme_name(&ctx);
        let theme_def = themes::get_theme(&theme_name);
        log_info!(
            "Applying code enhancer with theme: '{}' (mermaid: '{}')",
            theme_def.name,
            theme_def.mermaid_theme
        );

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
            );
        }

        // Client bundle script
        head_snippet.push_str(&self.generate_client_script(theme_def.mermaid_theme));

        if ctx.extra_head.is_empty() {
            ctx.extra_head = head_snippet;
        } else {
            ctx.extra_head.push('\n');
            ctx.extra_head.push_str(&head_snippet);
        }

        Ok(ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_mermaid_block() {
        let mut plugin = CodeEnhancerPlugin::default();
        let blocks = vec![
            BlockNode::code("mermaid", "graph TD;\n  A-->B;"),
            BlockNode::code("rust", "fn main() {}"),
        ];

        let result = plugin
            .transform_content_ast(blocks)
            .expect("Hook should succeed");
        assert_eq!(result.len(), 2);
        assert_eq!(
            result[0].attribute("data-diagram-type"),
            Some(&serde_json::json!("mermaid"))
        );
        assert_eq!(
            result[1].attribute("data-enhanced-code"),
            Some(&serde_json::json!(true))
        );
    }

    #[test]
    fn test_on_render_page_injects_theme_and_bundle() {
        let mut plugin = CodeEnhancerPlugin::default();
        let mut ctx = PageRenderContext::default();
        ctx.title = "Sample Post".to_string();

        let updated = plugin.on_render_page(ctx).expect("Hook should succeed");

        // Checks CSS injection
        assert!(updated.extra_css.contains("inline-code-enhanced"));
        assert!(updated.extra_css.contains("Dracula Theme"));

        // Checks Script injection
        assert!(updated.extra_head.contains("mermaid.initialize"));
        assert!(updated.extra_head.contains("AegisCodeEnhancer"));
    }

    #[test]
    fn test_custom_theme_selection_and_external_url() {
        let mut plugin = CodeEnhancerPlugin::default();
        let mut ctx = PageRenderContext::default();
        ctx.fields
            .insert("code_theme".to_string(), serde_json::json!("github-light"));
        ctx.fields.insert(
            "code_theme_url".to_string(),
            serde_json::json!("https://example.com/custom-prism.css"),
        );

        let updated = plugin.on_render_page(ctx).expect("Hook should succeed");

        // Selected GitHub Light
        assert!(updated.extra_css.contains("GitHub Light Theme"));
        // Injected external stylesheet link
        assert!(updated
            .extra_head
            .contains(r#"href="https://example.com/custom-prism.css""#));
        // Mermaid theme set to default for light theme
        assert!(updated.extra_head.contains("theme: 'default'"));
    }
}
