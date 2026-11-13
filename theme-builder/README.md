# Theme Builder

**English** | [日本語](README.ja.md)

A development tool for building AegisCMS themes with an editor and a live preview (React 19 + Vite + CodeMirror 6).
It needs no database and no login. It connects to the JSON API of `aegis-server theme dev`.

```bash
task dev:theme -- themes/my-theme   # dev server (http://127.0.0.1:8090)
npm --prefix theme-builder ci       # first time only
task dev:theme-builder              # http://localhost:5174/
```

For usage, the API, and how it is protected, see [docs/theme_authoring_guide.md](../docs/theme_authoring_guide.md#theme-builder-editor--live-preview); for the design, see [docs/architecture.md](../docs/architecture.md#181-theme-builder-theme-builder).

| Command                                 | Description                                              |
| --------------------------------------- | -------------------------------------------------------- |
| `npm run dev`                           | Dev server (change the target with `THEME_DEV_URL`)      |
| `npm run test`                          | Unit tests (Vitest)                                      |
| `npm run test:e2e`                      | E2E tests (Playwright; see below)                        |
| `npm run lint` / `npm run format:check` | oxlint (zero warnings) / Prettier                        |
| `npm run build`                         | Type check and production build (`dist/`)                |

## Structure

- `src/api.ts` — API client for the dev server (the `x-aegis-dev` header is sent with writes)
- `src/buffers.ts`, `src/previewDoc.ts`, `src/eol.ts` — pure logic separated from the UI (unit-tested)
- `src/components/` — `Editor` (CodeMirror), `FileTree`, `Preview`, `Problems`
- `src/App.tsx` — holds the state (open files, rendering, watching for changes on disk)

## E2E Tests

`npm run test:e2e` (or `task test:e2e:theme-builder`). `e2e/start-servers.mjs` starts `aegis-server theme dev` (port 18090) and Vite (port 15174) against a copy of `e2e/fixtures/e2e-theme` (`target/e2e-themes/`), so it also works alongside `task dev:theme`. The first run builds the server (skip with `E2E_SKIP_BUILD=1`). Install the browser with `npx playwright install chromium`. Tests run one at a time, and the theme is restored before each test.

- `e2e/editing.spec.ts` — live preview, saving, error line display, width, CSS, external changes, file creation
- `e2e/line-endings.spec.ts` — opening a CRLF file does not produce a diff, and saving keeps it CRLF
- `e2e/api-guard.spec.ts` — rejects writes without the header, paths outside the folder, and non-loopback `Host`
