**English** | [日本語](README.ja.md)

# Test themes

Themes for AegisCMS's own tests. They are **not shipped** and **not in the store** (the official themes are in [AegisCMS-Plugin](https://github.com/takumi-mr/AegisCMS-Plugin/tree/main/themes)).

| Theme        | What it does                                                         | Used by                                            |
| ------------ | -------------------------------------------------------------------- | -------------------------------------------------- |
| `test-fonts` | Declares Google Fonts in `theme.json`'s `csp`, with a minimal layout | E2E (`themes.spec.ts`): switching, CSP hosts shown |

The E2E server (`admin/e2e/start-server.mjs`) copies this directory into its installed themes (`target/e2e/themes`).
