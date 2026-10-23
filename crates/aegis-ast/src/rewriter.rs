use lol_html::{element, HtmlRewriter, OutputSink, Settings};
use std::collections::HashSet;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RewriterError {
    #[error("HTML rewrite engine error: {0}")]
    Engine(String),
    #[error("UTF-8 decoding error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
}

/// Configuration options for the streaming HTML security and performance rewriter.
#[derive(Debug, Clone)]
pub struct HtmlRewriterConfig {
    /// Enforce `rel="noopener noreferrer"` on external links (default: true).
    pub enforce_external_link_rel: bool,
    /// Automatically set `target="_blank"` on external links (default: false).
    pub open_external_links_in_new_tab: bool,
    /// List of domains considered internal (e.g. "example.com", "localhost:8080").
    pub internal_domains: Vec<String>,
    /// Enforce `loading="lazy"` on `<img>` tags if not already set (default: true).
    pub enforce_image_lazy_loading: bool,
    /// Enforce `decoding="async"` on `<img>` tags if not already set (default: true).
    pub enforce_image_async_decoding: bool,
}

impl Default for HtmlRewriterConfig {
    fn default() -> Self {
        Self {
            enforce_external_link_rel: true,
            open_external_links_in_new_tab: false,
            internal_domains: Vec::new(),
            enforce_image_lazy_loading: true,
            enforce_image_async_decoding: true,
        }
    }
}

impl From<crate::config::SecurityConfig> for HtmlRewriterConfig {
    fn from(cfg: crate::config::SecurityConfig) -> Self {
        Self {
            enforce_external_link_rel: cfg.enforce_external_link_rel,
            open_external_links_in_new_tab: cfg.open_external_links_in_new_tab,
            internal_domains: cfg.internal_domains,
            enforce_image_lazy_loading: cfg.enforce_image_lazy_loading,
            enforce_image_async_decoding: cfg.enforce_image_async_decoding,
        }
    }
}

pub struct StreamingHtmlRewriter;

impl StreamingHtmlRewriter {
    /// Rewrite an HTML string with security and performance transformations.
    pub fn rewrite(html: &str, config: &HtmlRewriterConfig) -> Result<String, RewriterError> {
        let bytes = Self::rewrite_bytes(html.as_bytes(), config)?;
        Ok(String::from_utf8(bytes)?)
    }

    /// Rewrite raw HTML bytes into a new byte buffer.
    pub fn rewrite_bytes(
        html_bytes: &[u8],
        config: &HtmlRewriterConfig,
    ) -> Result<Vec<u8>, RewriterError> {
        let mut output = Vec::with_capacity(html_bytes.len() + 64);
        {
            let mut rewriter = Self::build_rewriter(|c: &[u8]| output.extend_from_slice(c), config);
            rewriter
                .write(html_bytes)
                .map_err(|e| RewriterError::Engine(e.to_string()))?;
            rewriter
                .end()
                .map_err(|e| RewriterError::Engine(e.to_string()))?;
        }
        Ok(output)
    }

    /// Create a streaming HtmlRewriter instance bound to an output sink.
    pub fn build_rewriter<'a, O: OutputSink + 'a>(
        sink: O,
        config: &'a HtmlRewriterConfig,
    ) -> HtmlRewriter<'a, O> {
        let mut settings = Settings::new();

        // 1. <a> link security handler
        if config.enforce_external_link_rel || config.open_external_links_in_new_tab {
            let internal_domains = config.internal_domains.clone();
            let enforce_rel = config.enforce_external_link_rel;
            let target_blank = config.open_external_links_in_new_tab;

            settings = settings.append_element_content_handler(element!("a[href]", move |el| {
                if let Some(href) = el.get_attribute("href") {
                    if is_external_url(&href, &internal_domains) {
                        if enforce_rel {
                            let current_rel = el.get_attribute("rel").unwrap_or_default();
                            let merged_rel =
                                merge_rel_tokens(&current_rel, &["noopener", "noreferrer"]);
                            el.set_attribute("rel", &merged_rel)?;
                        }
                        if target_blank && !el.has_attribute("target") {
                            el.set_attribute("target", "_blank")?;
                        }
                    }
                }
                Ok(())
            }));
        }

        // 2. <img> optimization handler
        if config.enforce_image_lazy_loading || config.enforce_image_async_decoding {
            let lazy = config.enforce_image_lazy_loading;
            let async_decoding = config.enforce_image_async_decoding;

            settings = settings.append_element_content_handler(element!("img", move |el| {
                if lazy && !el.has_attribute("loading") {
                    el.set_attribute("loading", "lazy")?;
                }
                if async_decoding && !el.has_attribute("decoding") {
                    el.set_attribute("decoding", "async")?;
                }
                Ok(())
            }));
        }

        HtmlRewriter::new(settings, sink)
    }
}

/// Helper function to check if a URL is external.
fn is_external_url(href: &str, internal_domains: &[String]) -> bool {
    let trimmed = href.trim();

    // Relative links, fragments, tel, mailto, etc. are not external web links
    if (trimmed.starts_with('/') && !trimmed.starts_with("//"))
        || trimmed.starts_with('#')
        || trimmed.starts_with('?')
        || trimmed.starts_with("mailto:")
        || trimmed.starts_with("tel:")
        || trimmed.starts_with("javascript:")
    {
        return false;
    }

    let is_absolute = trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("//");

    if !is_absolute {
        // Relative path without leading slash (e.g. "about.html")
        return false;
    }

    // Extract domain/host from URL
    let without_proto = if let Some(stripped) = trimmed.strip_prefix("https://") {
        stripped
    } else if let Some(stripped) = trimmed.strip_prefix("http://") {
        stripped
    } else if let Some(stripped) = trimmed.strip_prefix("//") {
        stripped
    } else {
        trimmed
    };

    let domain = without_proto
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .to_lowercase();

    // Check if matching internal domains
    for internal in internal_domains {
        let internal_lower = internal.to_lowercase();
        if domain == internal_lower || domain.ends_with(&format!(".{}", internal_lower)) {
            return false;
        }
    }

    true
}

/// Merge existing `rel` tokens with required security tokens, preserving order and avoiding duplicates.
fn merge_rel_tokens(existing: &str, required: &[&str]) -> String {
    let mut tokens: Vec<String> = existing.split_whitespace().map(|s| s.to_string()).collect();

    let set: HashSet<String> = tokens.iter().cloned().collect();

    for &req in required {
        if !set.contains(req) {
            tokens.push(req.to_string());
        }
    }

    tokens.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_external_link_rel_injection() {
        let input = r#"<p>Check <a href="https://github.com/takumi-mr/AegisCMS">Repo</a> and <a href="/local/path">Local</a></p>"#;
        let config = HtmlRewriterConfig::default();

        let rewritten =
            StreamingHtmlRewriter::rewrite(input, &config).expect("rewrite should succeed");

        assert!(rewritten.contains(r#"href="https://github.com/takumi-mr/AegisCMS""#));
        assert!(rewritten.contains(r#"rel="noopener noreferrer""#));
        assert!(rewritten.contains(r#"<a href="/local/path">"#));
        assert!(!rewritten.contains(r#"<a href="/local/path" rel="#));
    }

    #[test]
    fn test_external_link_with_existing_rel() {
        let input = r#"<a href="https://example.com" rel="nofollow">Link</a>"#;
        let config = HtmlRewriterConfig::default();

        let rewritten = StreamingHtmlRewriter::rewrite(input, &config).unwrap();
        assert!(rewritten.contains(r#"rel="nofollow noopener noreferrer""#));
    }

    #[test]
    fn test_internal_domains_bypass() {
        let input = r#"<a href="https://mycms.internal/docs">Docs</a><a href="https://external.org">External</a>"#;
        let config = HtmlRewriterConfig {
            internal_domains: vec!["mycms.internal".to_string()],
            ..Default::default()
        };

        let rewritten = StreamingHtmlRewriter::rewrite(input, &config).unwrap();
        assert!(!rewritten.contains(r#"href="https://mycms.internal/docs" rel="#));
        assert!(rewritten.contains(r#"href="https://external.org" rel="noopener noreferrer""#));
    }

    #[test]
    fn test_image_attributes_injection() {
        let input = r#"<div><img src="/photo.jpg" alt="Photo"><img src="/banner.png" loading="eager"></div>"#;
        let config = HtmlRewriterConfig::default();

        let rewritten = StreamingHtmlRewriter::rewrite(input, &config).unwrap();
        // First img gets loading="lazy" and decoding="async"
        assert!(rewritten.contains(r#"src="/photo.jpg""#));
        assert!(rewritten.contains(r#"loading="lazy""#));
        assert!(rewritten.contains(r#"decoding="async""#));

        // Second img preserves loading="eager" but gets decoding="async"
        assert!(rewritten.contains(r#"loading="eager""#));
    }

    #[test]
    fn test_streaming_chunk_rewrite() {
        let config = HtmlRewriterConfig::default();
        let mut out = Vec::new();
        {
            let mut rewriter =
                StreamingHtmlRewriter::build_rewriter(|c: &[u8]| out.extend_from_slice(c), &config);
            rewriter.write(b"<p><a href=\"https://").unwrap();
            rewriter.write(b"rust-lang.org\">Rust</a></p>").unwrap();
            rewriter.end().unwrap();
        }

        let res = String::from_utf8(out).unwrap();
        assert!(res.contains("rel=\"noopener noreferrer\""));
    }

    #[test]
    fn test_target_blank_option() {
        let input = r#"<a href="https://example.com">External</a>"#;
        let config = HtmlRewriterConfig {
            open_external_links_in_new_tab: true,
            ..Default::default()
        };

        let rewritten = StreamingHtmlRewriter::rewrite(input, &config).unwrap();
        assert!(rewritten.contains(r#"target="_blank""#));
        assert!(rewritten.contains(r#"rel="noopener noreferrer""#));
    }

    #[test]
    fn test_benchmark_large_document_throughput() {
        // Construct a realistic ~120KB HTML document with 500 links and 500 images
        let mut large_html = String::with_capacity(120_000);
        large_html.push_str("<!DOCTYPE html><html><body><article>");
        for i in 0..500 {
            large_html.push_str(&format!(
                r#"<section><h2>Section {i}</h2><p>Visit <a href="https://partner{i}.com">Partner</a> or <a href="/local">Local</a></p><img src="/img/{i}.jpg" alt="Item {i}"></section>"#
            ));
        }
        large_html.push_str("</article></body></html>");

        let config = HtmlRewriterConfig::default();

        let start = std::time::Instant::now();
        let rewritten = StreamingHtmlRewriter::rewrite(&large_html, &config)
            .expect("rewriting large doc should succeed");
        let elapsed = start.elapsed();

        assert!(rewritten.contains(r#"rel="noopener noreferrer""#));
        assert!(rewritten.contains(r#"loading="lazy""#));
        assert!(rewritten.contains(r#"decoding="async""#));

        // Ensure throughput is high: 120KB should take only a few milliseconds (< 20ms in debug)
        println!(
            "Benchmark: Rewrote {} bytes into {} bytes in {:?}",
            large_html.len(),
            rewritten.len(),
            elapsed
        );
        assert!(elapsed.as_millis() < 100);
    }
}
