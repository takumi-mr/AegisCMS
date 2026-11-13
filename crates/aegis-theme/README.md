# aegis-theme

**English** | [日本語](README.ja.md)

AegisCMS's template engine and theme system, built on MiniJinja.

## Overview

`aegis-theme` is the template engine crate responsible for rendering the HTML of the public site. It uses `minijinja`, which is highly compatible with the Jinja2 / Django template syntax of Python, to deliver fast and safe SSR (Server-Side Rendering).

Theme authors place template files (`.html`, `.jinja`) and static assets in the `themes/<theme_name>/` directory, and define theme settings, localization, and custom block layouts in `theme.toml`.

## Responsibilities and Features

- **ThemeEngine**:
  - Template discovery, compilation, caching, and rendering.
  - Collection of diagnostics and warnings during rendering (`RenderDiagnostic`).
  - Automatic template reloading in development mode.
- **Rich built-in functions and filters (`builtins`)**:
  - **Formatting**: date formatting (`date_format`), currency display (`format_money`), number formatting.
  - **HTML and text handling**: escaping, excerpt generation (`truncate_words`), slugification (`slugify`), Markdown parsing.
  - **URL generation**: post permalinks (`post_url`), category and tag list URLs, public media URLs.
  - **Localization**: looking up per-locale message catalogs with `t("key")`.
- **Theme manifest (`manifest`)**:
  - Parsing and validation of `theme.toml`. Defines the theme name, version, author, supported template hierarchy (single post, static page, listing, 404, etc.), and custom options.
- **Localized messages (`messages`)**:
  - Provides en / ja catalogs for the built-in localized system strings (pagination, post date, comment section, cart, etc.).

## Module Layout

| Module | Description |
| :--- | :--- |
| `engine` | Template loading and rendering (`ThemeEngine`) |
| `builtins` | Registration of Jinja2-compatible custom filters and global functions |
| `manifest` | Definition and validation of the theme settings file `theme.toml` |
| `messages` | Standard localized message catalogs for the system (en, ja) |
| `config` | Site theme settings (`ThemeConfig`, `FrontPage`) |
