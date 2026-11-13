# official-reading-time

**English** | [日本語](README.ja.md)

An official Wasm plugin that analyzes the body of articles and static pages and shows an indicator (badge) with the estimated reading time and the character / word count (the `on-render-page` hook).

## Features

- **Hybrid Japanese and English support**:
  - CJK characters such as Japanese, Chinese, and Korean are calculated per character (default: 500 characters/minute).
  - Western text such as English is calculated per space-separated word (default: 200 words/minute).
  - It calculates an accurate reading time even for mixed text.
- **Flexible appearance and placement**:
  - Choose from a pill badge (default), a bordered card, or plain text display.
  - Can be placed at the start of the body (default), the end, or both.
  - Comes with a clock icon that follows the theme's `--accent` color.
- **Per-article hiding**:
  - Setting `hide_reading_time = true` in an article's custom field hides it for that article only.

## Settings (`[plugins.settings.official-reading-time]`)

```toml
[plugins.settings.official-reading-time]
# Target content types (comma-separated, default: "article")
content_types = "article"

# Display mode: "both" (reading time and character count) / "time_only" (time only) / "chars_only" (character count only)
display_mode = "both"

# Display position: "top" (start) / "bottom" (end) / "both"
position = "top"

# Badge style: "badge" (rounded pill) / "simple" (plain text) / "card" (bordered card)
badge_style = "badge"

# Show the clock icon (default: true)
show_icon = true

# Reading speed (words/minute, for Western text, default: 200)
words_per_minute = 200

# Reading speed (characters/minute, for CJK, default: 500)
chars_per_minute = 500

# Accent color (a CSS color. If omitted, the theme's --accent)
accent_color = ""

# Custom display format ({time}, {chars}, {words} can be used. Automatic when empty)
format = ""
```
