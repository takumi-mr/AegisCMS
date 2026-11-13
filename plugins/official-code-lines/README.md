# official-code-lines

**English** | [日本語](README.ja.md)

An official plugin that adds **line numbers** and a **file name heading** to code blocks (the [`on-render-page` hook](../../docs/plugin_hooks.md)).

```text
 ┌ src/main.rs ─────────────┐   ← file name (the code block's filename)
 │ 1  fn main() {           │
 │ 2      println!("hi");   │   ← line numbers (not included when copying)
 │ 3  }                     │
 └──────────────────────────┘
```

- **Line numbers**: A script adds a column of numbers to the `<pre>` (it does not change the characters of the code, so the numbers are not included in selections or copies). Blocks with fewer lines than `min_lines` and Mermaid diagrams do not get them. Blocks with numbers do not wrap and scroll horizontally instead (wrapping would misalign the numbers).
- **File name**: Shows the code block's `filename` above the block. Set it in the "File name" field of the Admin Studio code block, or in Markdown with ` ```rust title="src/main.rs" `. The core outputs it as `<pre data-filename="...">`, so even without this plugin you can use it from the theme's CSS with `attr(data-filename)`.
- It adds nothing to pages without `<pre` in the body. The code remains readable as is even if scripts do not run.
- It can be used together with code coloring and the copy button (`official-code-enhancer`).

## Usage

Add `"official-code-lines"` to `[plugins].enabled` in the config file, or enable it in Admin Studio's **Settings › Plugins**.

```toml
[plugins]
enabled = ["official-code-lines"]
```

The theme must output `extra_head` and `extra_css` (the built-in templates do).

## Settings

Change them in the "Settings" of Admin Studio's **Settings › Plugins**, or in `[plugins.settings.official-code-lines]` in the config file. Omitted items and invalid values fall back to the defaults.

| Key | Default | Description |
| --- | --- | --- |
| `content_types` | `article,page` | Content types to run on (comma-separated) |
| `selector` | `.prose` | The scope of the body (CSS selectors; several separated by commas). Only `<pre>` elements inside it are targeted |
| `line_numbers` | `true` | Add line numbers |
| `show_filename` | `true` | Show the file name heading |
| `min_lines` | `2` | Minimum number of lines to add line numbers (1–1000) |
| `number_color` | The theme's `--text-muted` | Color of the line numbers. `#hex`, `rgb()`, `hsl()`, `var(--name)`, or a color name |

```toml
[plugins.settings.official-code-lines]
min_lines = 5
number_color = "#8b949e"
```

## Changing the Appearance

Blocks with numbers are `pre.ocl-numbered` (the number column is `.ocl-rows`), and the heading is `<div class="ocl-filename">`. Where the settings are not enough, override with the theme's CSS.

## Build

```bash
task plugin:build:content   # rebuild plugin.wasm (requires rustup target add wasm32-unknown-unknown)
cargo test -p aegis-plugin-code-lines
```
