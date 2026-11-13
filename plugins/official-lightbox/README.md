# official-lightbox

**English** | [日本語](README.ja.md)

An official plugin that lets readers **click to enlarge** images in the body (the [`on-render-page` hook](../../docs/plugin_hooks.md)).

- Clicking an image (or focusing it and pressing Enter / Space) opens it in a full-screen view (`<dialog>`). It closes with **Esc**, a click on the background, or the close button, and focus returns to the original image.
- If the body has multiple images, you can go **to the previous / next image** (buttons, ← → keys; it wraps around at the ends). The `alt` can be shown as a caption.
- **Images inside links** (ones where a click goes to the link target) and **small images** (under `min_width_px`, such as icons) are not targeted.
- The image shown (`currentSrc`) is enlarged as is. It does not swap in a larger original image.
- It adds nothing to pages without `<img` in the body. Images remain visible as is even if scripts do not run.

## Usage

Add `"official-lightbox"` to `[plugins].enabled` in the config file, or enable it in Admin Studio's **Settings › Plugins**.

```toml
[plugins]
enabled = ["official-lightbox"]
```

The theme must output `extra_head` and `extra_css` (the built-in templates do).

## Settings

Change them in the "Settings" of Admin Studio's **Settings › Plugins**, or in `[plugins.settings.official-lightbox]` in the config file. Omitted items and invalid values fall back to the defaults.

| Key | Default | Description |
| --- | --- | --- |
| `content_types` | `article,page` | Content types to run on (comma-separated) |
| `selector` | `.prose` | The scope of the body (CSS selectors; several separated by commas). Only images inside it are targeted |
| `min_width_px` | `120` | Minimum width of images to target (0–2000) |
| `show_caption` | `true` | Show the `alt` as a caption |
| `animate` | `true` | Fade in on display (not when `prefers-reduced-motion` is set) |
| `bg_color` | `rgba(0,0,0,.88)` | Background color. `#hex`, `rgb()`, `hsl()`, `var(--name)`, or a color name |
| `locale` | `en` | Language of the screen-reader names (`en` / `ja`) |

## Changing the Appearance

The enlarged view is `<dialog class="olb">` (the image is `.olb-full`, the caption is `.olb-caption`, and the buttons are `.olb-btn`), and targeted images are `img.olb-img`. Where the settings are not enough, override with the theme's CSS.

## Build

```bash
task plugin:build:content   # rebuild plugin.wasm (requires rustup target add wasm32-unknown-unknown)
cargo test -p aegis-plugin-lightbox
```
