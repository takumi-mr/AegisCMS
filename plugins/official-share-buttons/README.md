# official-share-buttons

**English** | [日本語](README.ja.md)

An official plugin that adds **SNS share buttons** (X, LINE, copy URL, Facebook, Hatena Bookmark, Bluesky, Threads, Pocket, LinkedIn, and custom share targets) to articles and static pages (the [`on-render-page` hook](../../docs/plugin_hooks.md)).

- **Built-in platforms**: X (Twitter), LINE, copy URL, Facebook, Hatena Bookmark, Bluesky, Threads, Pocket, LinkedIn
- **Custom share targets**: by specifying URL templates in the `custom_services` setting, you can add any web service or SNS as a button
- **Display style**: `buttons` (icon + text) or `icons` (icon only), and shape (rounded / pill / square)
- **Placement**: end of the body (`bottom`, default), start of the body (`top`), or both (`both`)
- **Privacy-conscious**: it loads no external tracking scripts or iframes, and works with only clean links, inline SVG, and the clipboard API

## Usage

Add `"official-share-buttons"` to `[plugins].enabled` in the config file, or enable it in Admin Studio's **Settings › Plugins**.

```toml
[plugins]
enabled = ["official-share-buttons"]
```

## Settings

Customize it in the "Settings" of Admin Studio's **Settings › Plugins**, or in `[plugins.settings.official-share-buttons]` in the config file.

| Key               | Default       | Description                                                                                                                                                    |
| ----------------- | ------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `content_types`   | `article`     | Content types to show the share buttons on (comma-separated)                                                                                                   |
| `platforms`       | `x,line,copy` | Share targets to show (comma-separated, in display order). `x`, `line`, `copy`, `facebook`, `hatena`, `bluesky`, `threads`, `pocket`, `linkedin` are available |
| `custom_services` | `""`          | Additional custom share targets (one per line, format: `service name\|share URL template\|background color`). Available placeholders: `{url}`, `{title}`       |
| `position`        | `bottom`      | Placement: `bottom` (end of the body) / `top` (start of the body) / `both` (start and end)                                                                     |
| `style`           | `buttons`     | `buttons` (icon + label text) / `icons` (icon only)                                                                                                            |
| `shape`           | `rounded`     | Corner roundness of the buttons: `rounded` / `pill` / `square`                                                                                                 |
| `heading`         | `""`          | Heading text above the buttons (when empty, "Share this article" in the language in use)                                                                       |
| `x_via`           | `""`          | `@username` when sharing on X (added to the via parameter)                                                                                                     |
| `x_hashtags`      | `""`          | Hashtags when sharing on X (without #, comma-separated)                                                                                                        |

### Example (config.toml)

```toml
[plugins.settings.official-share-buttons]
platforms = "x,line,copy,hatena,bluesky"
position = "bottom"
style = "buttons"
shape = "pill"
custom_services = """
Mastodon|https://mastodon.social/share?text={title}&url={url}|#6364ff
Misskey|https://misskey-hub.net/share?text={title}&url={url}|#86b300
"""
```

### Hiding per article

To hide the share buttons on a specific article, set `hide_share_buttons: true` in the article's custom field.
