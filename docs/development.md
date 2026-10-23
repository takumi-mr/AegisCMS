# 開発ガイド (Development)

AegisCMS の開発環境、検証、ビルド、CI、Docker イメージについてまとめます。ブランチ・コミット・PR の規約は [AGENTS.md](../AGENTS.md)、内部設計は [architecture.md](architecture.md) を参照してください。

---

## 1. タスク (`Taskfile.yml`)

ルートで [go-task](https://taskfile.dev/) の `task` を使います（`task` だけで一覧）。

| タスク | 内容 |
| --- | --- |
| `task dev` / `task dev:admin` | サーバー（ポート 8080）/ Admin Studio の開発サーバー |
| `task check` | PR 前の検証一式: `fmt:check`・`fmt:admin:check`・`lint`・`lint:admin`・`test`・`build:admin` |
| `task migrate` | スキーマのマイグレーション |
| `task codegen`（`codegen:ts` / `codegen:openapi`） | `schemas/*.yaml` から `admin/src/types/schemas.ts` と `docs/openapi.json` を生成。スキーマを変えたら実行する |
| `task build` / `build:server` / `build:admin` | 本番ビルド |
| `task plugin:build*` | プラグインの Wasm ビルド（[4](#4-プラグインのビルド)） |
| `task bench:plugins` | プラグインのフック 1 回あたりの所要時間と同時実行時のスループット（`crates/aegis-runtime/examples/plugin_profile.rs`） |
| `task demo:shop` / `demo:shop:seed` | サンプルショップの起動 / データ投入（[examples/coffee-shop](../examples/coffee-shop)） |
| `task docker:build` / `docker:run` / `docker:stop` / `docker:logs` | Docker イメージ（[5](#5-docker-イメージ)） |

---

## 2. 検証とリント

- **Rust**: `cargo fmt --check`、`cargo clippy --workspace -- -D warnings`（警告ゼロ。`[workspace.lints]`）、`cargo test --workspace`。
  - `--all-targets` を付けると `sdk/templates/rust` のテストで `default_constructed_unit_structs` が出る（CI / `task check` は `--all-targets` なしなので未対応）。
- **Admin Studio**: Prettier（`format:check`）、oxlint（`--deny-warnings`。訳し漏れの検査 `scripts/check-locales.mjs` も `npm run lint` で実行）、`npm run build`（型検査を含む）。
- **Prettier と VS Code**:
  - Prettier は `.editorconfig` のインデントを読む。`[*]` の 4 スペースが CSS / HTML に効かないよう、`[*.{ts,tsx,js,jsx,css,html}]` を 2 にしている。
  - VS Code の Prettier 拡張は既定でワークスペース直下の `.prettierignore` を探すため、`.vscode/settings.json` で `prettier.ignorePath` を `admin/.prettierignore` に向けている。`prettier.requireConfig` で `admin/` 外の JSON などは整形しない。
  - `src/types/schemas.ts`（生成物）と Markdown は整形しない。
- **見た目を変えないリファクタリングの検証**: Playwright で全画面の `getComputedStyle`（約 90 プロパティ × 全要素）を取り、前後を比較する（CSS Modules への分割で使った）。
- **公開サイトのスマートフォン表示**: テーマを変えたら幅 320 / 360 / 390px で主要ページの横スクロールの有無を確かめる（手順は [テーマ作成ガイド 6](theme_authoring_guide.md#6-開発の流れ)）。

---

## 3. CI (`.github/workflows/ci.yml`)

`main` への push と PR で次を実行します。

| ジョブ | 内容 |
| --- | --- |
| `rust-ci` | `cargo fmt --check` → `cargo clippy --workspace -- -D warnings` → `cargo test --workspace` |
| `frontend-ci` | `npm ci` → Prettier → oxlint → 型検査とビルド |
| `docker-ci` | 上の 2 つが通ったら Docker イメージをビルド（GitHub Actions キャッシュ `type=gha`） |

---

## 4. プラグインのビルド

| タスク | 対象 | 必要なもの |
| --- | --- | --- |
| `task plugin:build` | `sdk/templates/rust/plugin.wasm` | `rustup target add wasm32-unknown-unknown` |
| `task plugin:build:as` | `sdk/templates/assemblyscript/plugin.wasm` | Node.js（`npm install`） |
| `task plugin:build:tinygo` | `sdk/templates/tinygo/plugin.wasm` | TinyGo 0.31+ |
| `task plugin:build:code-enhancer` | `plugins/official-code-enhancer/plugin.wasm` | 同上（Rust） |
| `task plugin:build:shop` | `plugins/official-shop-rules` / `official-order-notifier` | 同上（Rust） |
| `task build:wasm` | `sdk/examples/slack-notifier/plugin.wasm`（手書きの WAT から） | — |

ビルド済みの `plugin.wasm` はコミットしています。`crates/aegis-runtime/tests/sdk_plugins.rs` が全テンプレートを実ホストで実行するので、**SDK・テンプレート・公式プラグインを変えたら再ビルドしてコミット**してください（slack-notifier は SDK と無関係）。

`sdk/` はテンプレートとサンプル、`plugins/` はインストール先です。`plugins/` に置いただけでは読み込まれず、`[plugins].enabled` に書いたものだけが有効になります（[architecture.md 10.2](architecture.md#102-インストールと有効化-wasmpluginloader)）。

---

## 5. Docker イメージ

- マルチステージビルド: `node:22.22-alpine`（Admin Studio）→ `rust:1.99.1-slim-bookworm`（サーバー）→ `debian:bookworm-slim`（実行）。約 163 MB。
- ベースイメージはバージョンを固定している（`latest` は Rust のリリースごとに変わってレイヤーキャッシュが無効になり、実行環境の bookworm とも OS がずれるため）。Rust を更新するときは `Dockerfile` のタグを上げる。
- Rust は cargo-chef で「依存クレートのビルド（`cargo chef cook`）」と「ワークスペースのビルド」を分けており、`Cargo.toml` / `Cargo.lock` が変わらない限り依存のレイヤー（約 1.7 GB）は CI のキャッシュから再利用される。残りの時間の大半は release プロファイルの `lto = true` / `codegen-units = 1`（約 3 分）。
- ワークスペースのメンバー（`crates/`・`sdk/`・`plugins/`）は `cargo chef prepare` と `cargo build` の時点でコンテキストに必要（`COPY plugins/ ./plugins/` など）。実行環境には `themes/` も置く。
- UID 10001 の非 root ユーザー `aegis` で実行し、HEALTHCHECK を持つ。データは `/app/data` をボリュームにする（例: `-v aegis-data:/app/data`）。
- 設定は `config/docker.toml`（`host = "0.0.0.0"`, `port = 8080`）。`AEGIS_CONFIG`・`HOST`・`PORT` で上書きできる。Admin Studio の API 接続先を別にする場合は `--build-arg VITE_AEGIS_SERVER_URL=...`（[configuration.md 4.3](configuration.md#43-admin-studio-の接続先)）。
