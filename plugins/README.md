# plugins/

**English** | [日本語](README.ja.md)

The directory where the Wasm plugins installed in AegisCMS are placed (`[paths].plugins_dir`).

- One plugin = one directory (`plugins/<name>/`). Make `<name>` the same as the `name` in `plugin.manifest.json`.
- Placing a plugin here does not load it. It is enabled by turning it on in the Plugins screen of Admin Studio (no restart needed), or by adding its name to `[plugins].enabled` in the config file.
- You can check the installed plugins and whether they are enabled with `aegis-server plugin list`.

```text
plugins/
└── my-plugin/
    ├── plugin.manifest.json   # "name": "my-plugin"
    └── plugin.wasm
```

For how to build a plugin, see [docs/plugin_sdk_guide.md](../docs/plugin_sdk_guide.md) (in Japanese); templates and samples are in [`sdk/`](../sdk). To build a block that can be placed in the body, the quickest start is [Creating your first block plugin](../docs/plugin_block_tutorial.md) (template: [`sdk/templates/rust-block`](../sdk/templates/rust-block)).

> [!NOTE]
> If you create a new Rust plugin inside this repository (`plugins/<name>/`), it is not part of the root `Cargo.toml` workspace, so `cargo build` fails with `current package believes it's in a workspace when it's not`. To bring it in as an official plugin, add it to the root `[workspace] members`. For a personal plugin, write an empty `[workspace]` at the end of the plugin's `Cargo.toml` to make it a standalone crate ([SDK guide 2.1](../docs/plugin_sdk_guide.md#21-テンプレートの準備)).

## Official Plugins

| Plugin | Hooks | Description |
| --- | --- | --- |
| [`official-code-enhancer`](official-code-enhancer/) | `transform-content-ast`, `on-render-page` | Syntax highlighting for code (Prism.js), external themes, a copy button, and inline code styling. Choose the theme in Admin Studio's Settings › Plugins. Adds nothing to pages without code |
| [`official-mermaid`](official-mermaid/) | `on-render-page` | Turns ` ```mermaid ` code blocks into Mermaid diagrams. Loads Mermaid only on pages that have diagrams. The theme, security level, and script URL can be changed in Admin Studio's Settings › Plugins |
| [`official-plantuml`](official-plantuml/) | `on-render-page` | Turns ` ```plantuml ` code blocks into images of diagrams drawn by a PlantUML server (no script is added to the page). The server URL and image format can be changed in Admin Studio's Settings › Plugins |
| [`official-shop-rules`](official-shop-rules/) | `calculate-order`, `on-order-status-changed` | E-commerce shipping (free-shipping conditions), coupons (usage limits), and tax ([E-commerce](../docs/commerce.md#71-公式プラグイン)) |
| [`official-order-notifier`](official-order-notifier/) | `on-order-status-changed` | Notifies Slack / webhooks of order creation and status changes (`http::fetch`) |
| [`official-blocks`](official-blocks/) | `render-blocks` | Button, link card, and video (YouTube, Vimeo) blocks that can be placed in the body ([Body blocks](../docs/blocks.md#4-プラグインのブロック)). A model example of custom blocks |
| [`official-link-embed`](official-link-embed/) | `transform-content-ast`, `render-blocks` | Turns a URL-only paragraph into a link card built from that page's OGP / oEmbed (fetched inside the sandbox at save time and stored in the block; no network access at display time). Fetch destinations can be added with `extra_allowed_domains` ([Body blocks](../docs/blocks.md#5-ogp--oembed)) |
| [`official-page-toc`](official-page-toc/) | `on-render-page` | Shows links to headings (a table of contents) and scroll progress beside articles and static pages. Position, color, depth, and more can be changed in `[plugins.settings.official-page-toc]` |
| [`official-back-to-top`](official-back-to-top/) | `on-render-page` | Shows a "Back to top" button at the end of the body (or fixed on the screen). Shape, color, position, and more can be changed in `[plugins.settings.official-back-to-top]` |
| [`official-share-buttons`](official-share-buttons/) | `on-render-page` | Shows SNS share buttons (X, LINE, copy URL, etc.) at the end (or top / both) of articles and static pages. Where to show them and custom share targets are configurable |
| [`official-reading-time`](official-reading-time/) | `on-render-page` | Calculates the estimated reading time and the character / word count from the body of articles and static pages, and shows them as a badge at the start or end. Can be changed in `[plugins.settings.official-reading-time]` |
| [`official-post-navigation`](official-post-navigation/) | `on-render-page` | Shows previous / next article navigation (Prev/Next) at the end of article detail pages. The ordering setting (same category first, or overall date order), the design, and more can be changed in `[plugins.settings.official-post-navigation]` |
| [`official-code-lines`](official-code-lines/) | `on-render-page` | Shows line numbers and a file name heading on code blocks (the file name is the code block's `filename`). The minimum number of lines, colors, and more can be changed in `[plugins.settings.official-code-lines]` |
| [`official-lightbox`](official-lightbox/) | `on-render-page` | Enlarges images in the body when clicked (previous / next image, keyboard controls, captions). Small images and images inside links are excluded. Can be changed in `[plugins.settings.official-lightbox]` |
| [`official-external-links`](official-external-links/) | `on-render-page` | Opens external links in the body in a new tab and adds `rel="noopener nofollow"` and an external link icon (works with a browser script). Can be changed in `[plugins.settings.official-external-links]` |
| [`official-ads`](official-ads/) | `on-render-page`, `render-blocks` | Google AdSense and custom ad / banner delivery. Script injection into `<head>`, a custom block (`official-ads/display`), dynamic CSP integration, and support for the cookie consent banner (`[consent]`) |
| [`official-clarity`](official-clarity/) | `on-render-page` | Adds Microsoft Clarity (heatmaps and session recordings) to public pages. Just enter the project ID: it waits for cookie consent and adds the CSP hosts automatically. It does not record member, shopping, or forum pages, or members-only articles. Configured in `[plugins.settings.official-clarity]` |

A built `plugin.wasm` is bundled with each. After changing the source, rebuild with `task plugin:build:shop` (`official-blocks`: `task plugin:build:blocks`, `official-link-embed`: `task plugin:build:link-embed`, `official-mermaid`: `task plugin:build:mermaid`, `official-plantuml`: `task plugin:build:plantuml`, `official-page-toc` / `official-back-to-top` / `official-share-buttons` / `official-reading-time` / `official-post-navigation`: `task plugin:build:reading`, `official-code-lines` / `official-lightbox` / `official-external-links`: `task plugin:build:content`, `official-ads`: `task plugin:build:ads`, `official-clarity`: `task plugin:build:clarity`).
