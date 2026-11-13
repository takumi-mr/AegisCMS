# official-post-navigation

**English** | [日本語](README.ja.md)

An official Wasm plugin that shows navigation links to the "previous article" and "next article" at the end of an article detail page (or at a specified position) (the `on-render-page` hook).

## Features

- **Flexible rules for finding the previous / next article**:
  - `mode = "category"` (default): searches first for the previous / next articles within the same category. If there is no previous / next article in the same category, it can fall back automatically to date order (`fallback_to_all`).
  - `mode = "date"`: regardless of category, finds the previous / next articles in overall date order (publication date or creation date).
- **A polished responsive UI**:
  - Choose from a two-column left/right layout (default), a vertical stack, or a card style.
  - Supports showing thumbnail images (`featured_image` / `image`).
  - Placeholder handling keeps the left/right balance intact even for the first and last articles, where only one side exists.
- **Per-item control**:
  - Setting `hide_post_nav = true` in an article's custom field hides the navigation for that article only.

## Settings (`[plugins.settings.official-post-navigation]`)

```toml
[plugins.settings.official-post-navigation]
# Calculation mode: "category" (same category first) / "date" (overall date order)
mode = "category"

# Fall back to overall date order when there is no previous / next article in the same category (default: true)
fallback_to_all = true

# Ordering basis: "published_at" (by publication date, default) / "created_at" (by creation date)
order_by = "published_at"

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
