# AegisCMS Official Code Enhancer Plugin (`official-code-enhancer`)

**English** | [日本語](README.ja.md)

An official WebAssembly plugin that provides **syntax highlighting** for code ([Prism.js](https://prismjs.com/)), **code themes**, a **copy button**, and **inline code styling**. Mermaid diagrams are now handled by a separate plugin, [`official-mermaid`](../official-mermaid/).

---

## How It Works

- It injects its script and CSS only on pages that have a `<pre>` (code block) or `` `inline code` `` in the body. It adds nothing to pages without code.
- Highlighting uses Prism.js and its autoloader (`cdn.jsdelivr.net`). Languages that appear on the page are loaded only when needed (any language Prism supports works by just writing the language name on the code block. For aliases such as `golang`, write Prism's language name (`go`)).
- ` ```mermaid ` blocks are left alone here (`official-mermaid` turns them into diagrams).
- **Because it loads from an external CDN, the IP address and other details of people who open a page with code are sent to the CDN.**
- While this plugin is enabled, the server adds the manifest's `capabilities.csp` (`script_domains`: `cdn.jsdelivr.net`; `style_domains`: `cdn.jsdelivr.net`, `cdnjs.cloudflare.com`, and `unpkg.com` for external themes) to the CSP.
- A "Copy" button appears at the top right of a code block on hover. `` `inline code` `` is formatted into `<code>` in an XSS-safe way (by splitting text nodes).

---

## Code Themes

In this plugin's settings screen under Admin Studio's **Settings › Plugins**, set `code_theme`, `code_theme_url`, and `code_theme_css`. To vary it per article or per theme, you can also specify it with an article's custom field and theme variables (`theme_vars`).

The priority is "article field › theme variable › plugin setting › default (`dracula`)".

| Theme ID            | Theme name   | Characteristics                                                      |
| ------------------- | ------------ | -------------------------------------------------------------------- |
| `dracula` (default) | Dracula      | An elegant dark background with vivid pink, purple, and cyan accents |
| `github-dark`       | GitHub Dark  | GitHub's calm dark blue-gray                                         |
| `one-dark`          | One Dark Pro | The classic dark of VS Code / Atom                                   |
| `nord`              | Nord Arctic  | A Nordic palette of blue and snow white                              |
| `github-light`      | GitHub Light | A white-based light theme                                            |
| `monokai`           | Monokai Pro  | A classic with high contrast                                         |

```json
{ "code_theme": "nord" }
```

### External CSS (CDN)

Load the CSS of any theme, such as Prism.js's, by URL.

```json
{ "code_theme_url": "https://cdnjs.cloudflare.com/ajax/libs/prism/1.29.0/themes/prism-tomorrow.min.css" }
```

The hosts it can load from are the ones the manifest declares: `cdn.jsdelivr.net`, `cdnjs.cloudflare.com`, and `unpkg.com` (other hosts need to be allowed in the site's CSP).

### Custom CSS

```json
{ "code_theme_css": ".prose code.inline-code-enhanced { background: #ffeaa7; color: #d63031; }" }
```

### JavaScript API

`window.AegisCodeEnhancer.setThemeUrl(url)` swaps the external theme's CSS (for the Admin Studio preview, for example). `renderAll()` re-runs highlighting and the rest.

---

## Mermaid Diagrams

These have been split out into `official-mermaid`. Sites that use diagrams should enable that plugin too (add `"official-mermaid"` to `[plugins].enabled`; the bundled `config/default.toml` enables both).

---

## Build

```bash
task plugin:build:code-enhancer
```
