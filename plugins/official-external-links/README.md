# official-external-links

**English** | [日本語](README.ja.md)

An official plugin that makes the handling of **external links** in the body consistent (the [`on-render-page` hook](../../docs/plugin_hooks.md)).

| What it adds      | Details                                                                                                                     |
| ----------------- | --------------------------------------------------------------------------------------------------------------------------- |
| Open in a new tab | `target="_blank"` (links that already have a `target` are left alone)                                                       |
| `rel`             | `noopener` for new-tab links, and `nofollow` by setting. Added to any existing `rel`                                        |
| Icon              | A ↗ after links with text (decorative, so not read aloud). New-tab links also get a screen-reader-only "Opens in a new tab" |

- "External" means `http(s)` links other than those to **this site's domain** and `internal_domains` (including subdomains). Relative paths, `#`, `mailto:`, and `tel:` are not targeted.
- The core's `[security]` options `enforce_external_link_rel` (`rel="noopener noreferrer"`) and `open_external_links_in_new_tab` are applied by the server directly to the HTML. This plugin works in addition to them, and they can be used together.
- **A browser script adds these**, so search engines that do not run scripts will not see `nofollow`. When you must instruct search engines, write `rel` directly on the links in the body.
- It adds nothing to pages without `<a ` in the body. Links open normally even if scripts do not run.

## Usage

Add `"official-external-links"` to `[plugins].enabled` in the config file, or enable it in Admin Studio's **Settings › Plugins**.

```toml
[plugins]
enabled = ["official-external-links"]
```

The theme must output `extra_head` and `extra_css` (the built-in templates do).

## Settings

Change them in the "Settings" of Admin Studio's **Settings › Plugins**, or in `[plugins.settings.official-external-links]` in the config file. Omitted items and invalid values fall back to the defaults.

| Key                | Default        | Description                                                                                           |
| ------------------ | -------------- | ----------------------------------------------------------------------------------------------------- |
| `content_types`    | `article,page` | Content types to run on (comma-separated)                                                             |
| `selector`         | `.prose`       | The scope of the body (CSS selectors; several separated by commas). Only links inside it are targeted |
| `new_tab`          | `true`         | Open in a new tab                                                                                     |
| `nofollow`         | `true`         | Add `rel="nofollow"`                                                                                  |
| `icon`             | `true`         | Show the external link icon                                                                           |
| `internal_domains` | (none)         | Domains to treat as internal (comma-separated; includes subdomains. e.g. `example.com`)               |
| `locale`           | `en`           | Language of the screen-reader text (`en` / `ja`)                                                      |

```toml
[plugins.settings.official-external-links]
internal_domains = "example.com, example.net"
nofollow = false
locale = "ja"
```

## Changing the Appearance

The icon is `a.oel-external::after`, and the screen-reader text is `.oel-sr`. Where the settings are not enough, override with the theme's CSS.

## Build

```bash
task plugin:build:content   # rebuild plugin.wasm (requires rustup target add wasm32-unknown-unknown)
cargo test -p aegis-plugin-external-links
```
