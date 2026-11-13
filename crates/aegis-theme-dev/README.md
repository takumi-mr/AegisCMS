# aegis-theme-dev

**English** | [日本語](README.ja.md)

A standalone local development server for AegisCMS theme authors, plus the API that integrates with Theme Builder.

## Overview

`aegis-theme-dev` is a lightweight development server for previewing and developing a given theme directory on its own, without setting up a database, authentication, or a plugin environment.

It can be started with the `aegis-server theme dev <dir>` command (or this crate's standalone binary), and provides a live preview feature that immediately reflects changes to templates and CSS/assets in the browser. It also has a dedicated REST API for integrating with the Theme Builder GUI (`theme-builder/`).

## Responsibilities and Features

- **Zero-Config preview**:
  - Runs without an external DB. Renders templates using built-in dummy posts, static pages, and category data.
  - Placing a `sample-data.json` in the theme folder also lets you preview with the author's own mock data.
- **Live preview and hot reload**:
  - Watches for changes to template files, assets, and settings files (file polling / Last Modified checks) and reloads the browser automatically.
- **Theme Builder UI integration API (`/__dev/api/*`)**:
  - APIs to get the file tree of a theme, and to read and save files.
  - A live preview API that instantly renders unsaved content (drafts) in the editor.
  - Syntax validation of `theme.toml` and real-time feedback on template syntax errors.
- **Security**:
  - Because it is for development, it listens only on the `127.0.0.1` loopback address by default. This guards against DNS rebinding attacks and unauthorized access.

## Usage

```bash
# Start from the integrated AegisCMS command
aegis-server theme dev themes/my-theme

# Specify a port
aegis-server theme dev themes/my-theme --port 3000
```
