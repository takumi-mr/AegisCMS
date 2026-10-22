//! Code and Diagram theme configurations for AegisCMS Code Enhancer.

/// Base CSS for enhanced inline code, copy buttons, and diagram containers.
pub const BASE_ENHANCER_CSS: &str = r#"
/* AegisCMS Code & Diagram Enhancer Base Styles */
.prose code.inline-code-enhanced,
.prose p > code,
.prose li > code {
  font-family: 'JetBrains Mono', 'Fira Code', Menlo, Monaco, Consolas, monospace;
  font-size: 0.88em;
  font-weight: 500;
  padding: 0.16em 0.44em;
  border-radius: 6px;
  letter-spacing: -0.01em;
  vertical-align: baseline;
  display: inline-block;
  line-height: 1.4;
  transition: background-color 0.15s ease, border-color 0.15s ease, color 0.15s ease;
}

/* Code block wrapper with copy button */
.prose .code-block-wrapper {
  position: relative;
  margin-bottom: 1.5rem;
}

.prose .code-block-wrapper pre {
  margin-bottom: 0 !important;
}

.code-copy-btn {
  position: absolute;
  top: 0.65rem;
  right: 0.75rem;
  z-index: 10;
  background: rgba(255, 255, 255, 0.08);
  border: 1px solid rgba(255, 255, 255, 0.12);
  color: #9ca3af;
  border-radius: 6px;
  padding: 0.3rem 0.55rem;
  font-size: 0.75rem;
  font-family: 'JetBrains Mono', monospace;
  cursor: pointer;
  opacity: 0;
  transition: opacity 0.2s ease, background 0.15s ease, color 0.15s ease, border-color 0.15s ease;
  backdrop-filter: blur(8px);
  -webkit-backdrop-filter: blur(8px);
  user-select: none;
}

.prose .code-block-wrapper:hover .code-copy-btn {
  opacity: 1;
}

.code-copy-btn:hover {
  background: rgba(255, 255, 255, 0.18);
  color: #ffffff;
  border-color: rgba(255, 255, 255, 0.25);
}

.code-copy-btn.copied {
  opacity: 1;
  background: rgba(16, 185, 129, 0.2);
  color: #34d399;
  border-color: rgba(16, 185, 129, 0.4);
}

/* Mermaid diagram container */
.mermaid-diagram-container {
  display: flex;
  justify-content: center;
  align-items: center;
  padding: 2rem 1.5rem;
  margin: 1.75rem 0;
  border-radius: 12px;
  background: rgba(15, 23, 42, 0.6);
  border: 1px solid rgba(255, 255, 255, 0.08);
  box-shadow: 0 8px 24px -4px rgba(0, 0, 0, 0.25);
  overflow-x: auto;
  transition: border-color 0.2s ease, box-shadow 0.2s ease;
}

.mermaid-diagram-container:hover {
  border-color: rgba(99, 102, 241, 0.3);
  box-shadow: 0 12px 32px -4px rgba(99, 102, 241, 0.1);
}

.mermaid-diagram-container svg {
  max-width: 100%;
  height: auto;
}
"#;

pub struct ThemeDefinition {
    pub name: &'static str,
    pub display_name: &'static str,
    pub mermaid_theme: &'static str,
    pub css: &'static str,
}

pub const THEME_DRACULA: ThemeDefinition = ThemeDefinition {
    name: "dracula",
    display_name: "Dracula",
    mermaid_theme: "dark",
    css: r#"
/* Dracula Theme for AegisCMS */
.prose code.inline-code-enhanced,
.prose p > code,
.prose li > code {
  background: rgba(40, 42, 54, 0.85);
  color: #ff79c6;
  border: 1px solid rgba(189, 147, 249, 0.35);
  box-shadow: 0 2px 4px rgba(0, 0, 0, 0.25);
}
.prose pre {
  background: #282a36 !important;
  border: 1px solid #44475a !important;
}
.prose pre code {
  color: #f8f8f2 !important;
}
.token.comment, .token.prolog, .token.doctype, .token.cdata { color: #6272a4; font-style: italic; }
.token.punctuation { color: #f8f8f2; }
.token.property, .token.tag, .token.constant, .token.symbol, .token.deleted { color: #ff79c6; }
.token.boolean, .token.number { color: #bd93f9; }
.token.selector, .token.attr-name, .token.string, .token.char, .token.builtin, .token.inserted { color: #f1fa8c; }
.token.operator, .token.entity, .token.url, .language-css .token.string, .style .token.string { color: #f8f8f2; }
.token.atrule, .token.attr-value, .token.keyword { color: #ff79c6; font-weight: 600; }
.token.function, .token.class-name { color: #50fa7b; }
.token.regex, .token.important, .token.variable { color: #ffb86c; }
"#,
};

pub const THEME_GITHUB_DARK: ThemeDefinition = ThemeDefinition {
    name: "github-dark",
    display_name: "GitHub Dark",
    mermaid_theme: "dark",
    css: r#"
/* GitHub Dark Theme for AegisCMS */
.prose code.inline-code-enhanced,
.prose p > code,
.prose li > code {
  background: rgba(22, 27, 34, 0.9);
  color: #79c0ff;
  border: 1px solid #30363d;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3);
}
.prose pre {
  background: #0d1117 !important;
  border: 1px solid #30363d !important;
}
.prose pre code {
  color: #c9d1d9 !important;
}
.token.comment, .token.prolog, .token.doctype, .token.cdata { color: #8b949e; font-style: italic; }
.token.punctuation { color: #c9d1d9; }
.token.property, .token.tag, .token.constant, .token.symbol, .token.deleted { color: #7ee787; }
.token.boolean, .token.number { color: #79c0ff; }
.token.selector, .token.attr-name, .token.string, .token.char, .token.builtin, .token.inserted { color: #a5d6ff; }
.token.operator, .token.entity, .token.url { color: #79c0ff; }
.token.atrule, .token.attr-value, .token.keyword { color: #ff7b72; font-weight: 600; }
.token.function, .token.class-name { color: #d2a8ff; }
.token.regex, .token.important, .token.variable { color: #ffa657; }
"#,
};

pub const THEME_ONE_DARK: ThemeDefinition = ThemeDefinition {
    name: "one-dark",
    display_name: "One Dark Pro",
    mermaid_theme: "dark",
    css: r#"
/* One Dark Pro Theme for AegisCMS */
.prose code.inline-code-enhanced,
.prose p > code,
.prose li > code {
  background: rgba(33, 37, 43, 0.9);
  color: #61afef;
  border: 1px solid #3e4451;
  box-shadow: 0 2px 4px rgba(0, 0, 0, 0.25);
}
.prose pre {
  background: #21252b !important;
  border: 1px solid #3e4451 !important;
}
.prose pre code {
  color: #abb2bf !important;
}
.token.comment, .token.prolog, .token.doctype, .token.cdata { color: #5c6370; font-style: italic; }
.token.punctuation { color: #abb2bf; }
.token.property, .token.tag, .token.constant, .token.symbol, .token.deleted { color: #e06c75; }
.token.boolean, .token.number { color: #d19a66; }
.token.selector, .token.attr-name, .token.string, .token.char, .token.builtin, .token.inserted { color: #98c379; }
.token.operator, .token.entity, .token.url { color: #56b6c2; }
.token.atrule, .token.attr-value, .token.keyword { color: #c678dd; font-weight: 600; }
.token.function, .token.class-name { color: #61afef; }
.token.regex, .token.important, .token.variable { color: #e5c07b; }
"#,
};

pub const THEME_NORD: ThemeDefinition = ThemeDefinition {
    name: "nord",
    display_name: "Nord Arctic",
    mermaid_theme: "dark",
    css: r#"
/* Nord Theme for AegisCMS */
.prose code.inline-code-enhanced,
.prose p > code,
.prose li > code {
  background: rgba(46, 52, 64, 0.9);
  color: #88c0d0;
  border: 1px solid #4c566a;
  box-shadow: 0 2px 4px rgba(0, 0, 0, 0.25);
}
.prose pre {
  background: #2e3440 !important;
  border: 1px solid #434c5e !important;
}
.prose pre code {
  color: #d8dee9 !important;
}
.token.comment, .token.prolog, .token.doctype, .token.cdata { color: #616e88; font-style: italic; }
.token.punctuation { color: #eceff4; }
.token.property, .token.tag, .token.constant, .token.symbol, .token.deleted { color: #bf616a; }
.token.boolean, .token.number { color: #b48ead; }
.token.selector, .token.attr-name, .token.string, .token.char, .token.builtin, .token.inserted { color: #a3be8c; }
.token.operator, .token.entity, .token.url { color: #81a1c1; }
.token.atrule, .token.attr-value, .token.keyword { color: #81a1c1; font-weight: 600; }
.token.function, .token.class-name { color: #88c0d0; }
.token.regex, .token.important, .token.variable { color: #ebcb8b; }
"#,
};

pub const THEME_GITHUB_LIGHT: ThemeDefinition = ThemeDefinition {
    name: "github-light",
    display_name: "GitHub Light",
    mermaid_theme: "default",
    css: r#"
/* GitHub Light Theme for AegisCMS */
.prose code.inline-code-enhanced,
.prose p > code,
.prose li > code {
  background: #f6f8fa;
  color: #0969da;
  border: 1px solid #d0d7de;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.05);
}
.prose pre {
  background: #ffffff !important;
  border: 1px solid #d0d7de !important;
}
.prose pre code {
  color: #24292f !important;
}
.token.comment, .token.prolog, .token.doctype, .token.cdata { color: #6e7781; font-style: italic; }
.token.punctuation { color: #24292f; }
.token.property, .token.tag, .token.constant, .token.symbol, .token.deleted { color: #116329; }
.token.boolean, .token.number { color: #0550ae; }
.token.selector, .token.attr-name, .token.string, .token.char, .token.builtin, .token.inserted { color: #0a3069; }
.token.operator, .token.entity, .token.url { color: #0550ae; }
.token.atrule, .token.attr-value, .token.keyword { color: #cf222e; font-weight: 600; }
.token.function, .token.class-name { color: #8250df; }
.token.regex, .token.important, .token.variable { color: #953800; }
"#,
};

pub const THEME_MONOKAI: ThemeDefinition = ThemeDefinition {
    name: "monokai",
    display_name: "Monokai Pro",
    mermaid_theme: "dark",
    css: r#"
/* Monokai Theme for AegisCMS */
.prose code.inline-code-enhanced,
.prose p > code,
.prose li > code {
  background: rgba(39, 40, 34, 0.95);
  color: #a6e22e;
  border: 1px solid #49483e;
  box-shadow: 0 2px 4px rgba(0, 0, 0, 0.3);
}
.prose pre {
  background: #272822 !important;
  border: 1px solid #3e3d32 !important;
}
.prose pre code {
  color: #f8f8f2 !important;
}
.token.comment, .token.prolog, .token.doctype, .token.cdata { color: #75715e; font-style: italic; }
.token.punctuation { color: #f8f8f2; }
.token.property, .token.tag, .token.constant, .token.symbol, .token.deleted { color: #f92672; }
.token.boolean, .token.number { color: #ae81ff; }
.token.selector, .token.attr-name, .token.string, .token.char, .token.builtin, .token.inserted { color: #e6db74; }
.token.operator, .token.entity, .token.url { color: #f8f8f2; }
.token.atrule, .token.attr-value, .token.keyword { color: #f92672; font-weight: 600; }
.token.function, .token.class-name { color: #a6e22e; }
.token.regex, .token.important, .token.variable { color: #fd971f; }
"#,
};

pub const BUILTIN_THEMES: &[ThemeDefinition] = &[
    THEME_DRACULA,
    THEME_GITHUB_DARK,
    THEME_ONE_DARK,
    THEME_NORD,
    THEME_GITHUB_LIGHT,
    THEME_MONOKAI,
];

/// Finds a theme definition by name (case-insensitive) or defaults to Dracula.
pub fn get_theme(name: &str) -> &'static ThemeDefinition {
    let lower = name.trim().to_lowercase();
    for theme in BUILTIN_THEMES {
        if theme.name == lower {
            return theme;
        }
    }
    &THEME_DRACULA
}
