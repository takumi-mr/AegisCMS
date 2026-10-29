# Theme Builder

AegisCMS のテーマを、エディターとライブプレビューで作るための開発ツール（React 19 + Vite + CodeMirror 6）。
DB もログインも要りません。`aegis-server theme dev` の JSON API につながります。

```bash
task dev:theme -- themes/my-theme   # 開発サーバー（http://127.0.0.1:8090）
npm --prefix theme-builder ci       # 初回のみ
task dev:theme-builder              # http://localhost:5174/
```

使い方・API・保護の仕組みは [docs/theme_authoring_guide.md](../docs/theme_authoring_guide.md#theme-builderエディター--ライブプレビュー)、設計は [docs/architecture.md](../docs/architecture.md#181-theme-builder-theme-builder) を参照。

| コマンド                                | 内容                                         |
| --------------------------------------- | -------------------------------------------- |
| `npm run dev`                           | 開発サーバー（`THEME_DEV_URL` で接続先を変更） |
| `npm run test`                          | ユニットテスト（Vitest）                     |
| `npm run test:e2e`                      | E2E テスト（Playwright。下記）               |
| `npm run lint` / `npm run format:check` | oxlint（ゼロ警告）/ Prettier                 |
| `npm run build`                         | 型チェックと本番ビルド（`dist/`）            |

## 構成

- `src/api.ts` — 開発サーバーの API クライアント（書き込みには `x-aegis-dev` ヘッダー）
- `src/buffers.ts`・`src/previewDoc.ts`・`src/eol.ts` — 画面から切り離した純粋なロジック（ユニットテストあり）
- `src/components/` — `Editor`（CodeMirror）・`FileTree`・`Preview`・`Problems`
- `src/App.tsx` — 状態（開いているファイル・描画・ディスクの変更の監視）をまとめる

## E2E テスト

`npm run test:e2e`（または `task test:e2e:theme-builder`）。`e2e/start-servers.mjs` が、`aegis-server theme dev`（ポート 18090）と Vite（ポート 15174）を、`e2e/fixtures/e2e-theme` のコピー（`target/e2e-themes/`）に対して起動するので、`task dev:theme` と同時でも動きます。初回はサーバーをビルドします（`E2E_SKIP_BUILD=1` で省略）。ブラウザーは `npx playwright install chromium` で入れます。テストは 1 つずつ実行し、各テストの前にテーマを元に戻します。

- `e2e/editing.spec.ts` — ライブプレビュー、保存、エラーの行表示、幅、CSS、外部の変更、ファイルの作成
- `e2e/line-endings.spec.ts` — CRLF のファイルを開いても差分にならず、保存しても CRLF のまま
- `e2e/api-guard.spec.ts` — ヘッダーなしの書き込み・フォルダー外のパス・ループバック以外の `Host` を拒否
