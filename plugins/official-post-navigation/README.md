# official-post-navigation

**English** | [日本語](README.ja.md)

An official Wasm plugin that shows navigation links to the "previous article" and "next article" at the end of an article detail page (or at a specified position) (the `on-render-page` hook).

## Features

- **Which articles are previous and next is decided by AegisCMS**, not by this plugin: same category first or overall date order, and by publication or creation date. Set it in Admin Studio › Site, or in `[adjacent_articles]` ([configuration reference](https://github.com/takumi-mr/AegisCMS/blob/main/docs/configuration.md)). Themes get the same articles as `prev_article` / `next_article`.
- **A polished responsive UI**:
    - Choose from a two-column left/right layout (default), a vertical stack, or a card style.
    - Supports showing thumbnail images (`featured_image` / `image`).
    - Placeholder handling keeps the left/right balance intact even for the first and last articles, where only one side exists.
- **Per-item control**:
    - Setting `hide_post_nav = true` in an article's custom field hides the navigation for that article only.

## Settings (`[plugins.settings.official-post-navigation]`)

```toml
[plugins.settings.official-post-navigation]
# Target content types (comma-separated, default: "article")
content_types = "article"

# Display position: "bottom" (end) / "top" (start) / "both"
position = "bottom"

# Layout style: "split" (left/right) / "stacked" (vertical) / "cards" (card-like)
layout = "split"

# Show labels (default: true)
show_labels = true

# Custom labels (when empty, the default text for the language is used)
prev_label = ""
next_label = ""

# Show thumbnail images (default: true)
show_thumbnail = true

# Accent color (a CSS color. If omitted, the theme's --accent)
accent_color = ""
```
