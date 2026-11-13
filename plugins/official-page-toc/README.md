# official-page-toc

**English** | [日本語](README.ja.md)

An official plugin that shows **links to headings (a table of contents)** and **scroll progress** beside articles and static pages (the [`on-render-page` hook](../../docs/plugin_hooks.md)).

- The table of contents is built on the server from the headings in the body (`<h2 id="...">` etc.; the built-in processing adds the `id`), so the links work even without JavaScript.
- Only the progress bar and the highlighting of "the heading currently being read" run on an inline script that goes into `extra_head`. The Admin Studio preview is a `sandbox` where scripts do not run, so it works only on public pages.
- It is not shown when the screen is narrow (under 1200px by default) or when printing. With `prefers-reduced-motion`, scrolling is not smooth.

## Usage

Add `"official-page-toc"` to `[plugins].enabled` in the config file, or enable it in Admin Studio's **Settings › Plugins**.

```toml
[plugins]
enabled = ["official-page-toc"]
```

The theme must output `extra_head` and `extra_css` (the built-in templates do). If you removed `'unsafe-inline'` from the CSP `script-src`, the progress script does not run (the table of contents links still work).

## Settings

Change them in the "Settings" of Admin Studio's **Settings › Plugins** (input fields appear; the items are declared in the manifest's `settings`, and values are validated on save), or in `[plugins.settings.official-page-toc]` in the config file. Omitted items and invalid values fall back to the defaults.

| Key               | Default                         | Description                                                                                                         |
| ----------------- | ------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| `content_types`   | `article,page`                  | Content types to show it on (comma-separated)                                                                       |
| `heading_levels`  | `2,3`                           | Heading levels to include in the table of contents (`1` to `6`)                                                     |
| `min_headings`    | `2`                             | Do not show the table of contents when there are fewer headings than this (progress is still shown)                 |
| `show_toc`        | `true`                          | Show the table of contents                                                                                          |
| `show_progress`   | `true`                          | Show the progress bar                                                                                               |
| `position`        | `right`                         | `right` / `left`                                                                                                    |
| `accent_color`    | The theme's `--accent`          | Color of the progress and the current heading. `#hex`, `rgb()`, `hsl()`, `var(--name)`, or a color name             |
| `top_offset_px`   | `96`                            | Position from the top. Also used as the top margin (the height of a fixed header) when jumping to a heading (0–400) |
| `width_px`        | `220`                           | Width of the table of contents (120–480)                                                                            |
| `min_viewport_px` | `1200`                          | Not shown when the screen is narrower than this (320–4000)                                                          |
| `smooth_scroll`   | `true`                          | Scroll smoothly to links                                                                                            |
| `locale`          | `en`                            | Language of the built-in text (`en` / `ja`)                                                                         |
| `title`           | `On this page` / `目次`         | Heading of the table of contents                                                                                    |
| `progress_label`  | `Reading progress` / `読了位置` | Screen-reader name of the progress bar                                                                              |

```toml
[plugins.settings.official-page-toc]
position = "left"
accent_color = "#e11d48"
heading_levels = "2,3,4"
locale = "ja"
```

To turn it off per article, add a boolean field `hide_toc` to the schema and set it to `true`.

## Changing the Appearance

It is a fixed-position `<nav class="apt">` (`.apt-track` / `.apt-fill` are the progress, `.apt-list` is the table of contents, and the current heading's link gets `.apt-active`). Where the settings are not enough, you can override with the theme's CSS. Only validated values go into the CSS (setting values are never put into CSS as is).

## Build

```bash
task plugin:build:reading   # rebuild plugin.wasm (requires rustup target add wasm32-unknown-unknown)
cargo test -p aegis-plugin-page-toc
```
