# official-back-to-top

**English** | [日本語](README.ja.md)

An official plugin that adds a **"Back to top" button** to articles and static pages (the [`on-render-page` hook](../../docs/plugin_hooks.md)).

| `style` | Behavior |
| --- | --- |
| `inline` (default) | Places a link at the end of the body. Works without JavaScript (`href="#top"`). Also shows in the Admin Studio preview |
| `fixed` | Fixed on the screen; appears after scrolling `show_after_px`. Always shown where scripts do not run |

It is placed at the end of the body, so the theme's comment section and footer are below it (use `fixed` if you want it at the very bottom of the page).

## Usage

Add `"official-back-to-top"` to `[plugins].enabled` in the config file, or enable it in Admin Studio's **Settings › Plugins**.

```toml
[plugins]
enabled = ["official-back-to-top"]
```

The theme must output `extra_head` and `extra_css` (the built-in templates do).

## Settings

Change them in the "Settings" of Admin Studio's **Settings › Plugins** (input fields appear; the items are declared in the manifest's `settings`, and values are validated on save), or in `[plugins.settings.official-back-to-top]` in the config file. Omitted items and invalid values fall back to the defaults.

| Key | Default | Description |
| --- | --- | --- |
| `content_types` | `article,page` | Content types to show it on (comma-separated) |
| `style` | `inline` | `inline` / `fixed` |
| `position` | `right` | `right` / `left` / `center` |
| `shape` | `circle` | `circle` (a circle with only an arrow) / `square` (a rounded square with only an arrow) / `pill` (arrow and text) |
| `label` | `Back to top` / `トップへ戻る` | The text (`pill`) and the name for screen readers |
| `locale` | `en` | The default language of `label` (`en` / `ja`) |
| `bg_color` | The theme's `--accent` | Background color. `#hex`, `rgb()`, `hsl()`, `var(--name)`, or a color name |
| `fg_color` | The theme's `--accent-contrast` (white if absent) | Color of the arrow and text |
| `size_px` | `44` | Button size (32–72) |
| `offset_px` | `24` | Distance from the screen edge when `fixed` (8–96) |
| `show_after_px` | `400` | When `fixed`, it appears after scrolling this far (0–5000) |
| `smooth_scroll` | `true` | Scroll smoothly (not when `prefers-reduced-motion` is set) |

```toml
[plugins.settings.official-back-to-top]
style = "fixed"
shape = "pill"
bg_color = "#111827"
fg_color = "#ffffff"
locale = "ja"
```

To turn it off per article, add a boolean field `hide_back_to_top` to the schema and set it to `true`.

## Changing the Appearance

The button is `<a class="obt obt-inline|obt-fixed obt-{position} obt-{shape}">` (`inline` is inside `<div class="obt-row">`). The link color and underline are specified with `a.obt.obt` so they do not lose to the theme's body link styles. Where the settings are not enough, override with a selector of higher specificity. Only validated values go into the CSS.

## Build

```bash
task plugin:build:reading   # rebuild plugin.wasm (requires rustup target add wasm32-unknown-unknown)
cargo test -p aegis-plugin-back-to-top
```
