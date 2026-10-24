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
| `task bench:http`（`bench`） | サーバーの HTTP パフォーマンスベンチマーク（トップページ、記事、API、プレビュー、見積もり。`scripts/benchmark.mjs`） |
| `task bench:all` | プラグイン性能計測と HTTP ベンチマークを一括実行 |
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
| `docker-ci` | Docker イメージをビルド（上の 2 つと並列。GitHub Actions キャッシュ `type=gha`、プロファイル `ci-release`） |

- `docker-ci` はビルドできるかの検証なので、LTO を省いた `ci-release` プロファイル（`--build-arg CARGO_PROFILE=ci-release`。最適化レベルは release と同じ）でビルドする。release の `lto = true` / `codegen-units = 1` は依存クレート全体を最終リンクでまとめて最適化し直すため、依存がキャッシュされていても毎回約 3 分かかる。
- dev / test プロファイルのデバッグ情報は `line-tables-only`（バックトレースの行番号は残る）。フルのデバッグ情報だと `aegis-server` のテストバイナリの PDB が約 500 MB になる（`line-tables-only` で約 200 MB。Windows で `aegis-server` を変えたときの再ビルドが約 10 秒 → 6.5 秒）。デバッガで変数を見たいときは `CARGO_PROFILE_DEV_DEBUG=true` で上書きする（依存も再ビルドされる）。

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
- ビルドプロファイルは `ARG CARGO_PROFILE`（既定 `release`）で切り替えられる。CI の検証では `ci-release` を渡す（[3](#3-ci-githubworkflowsciyml)）。
- ワークスペースのメンバー（`crates/`・`sdk/`・`plugins/`）は `cargo chef prepare` と `cargo build` の時点でコンテキストに必要（`COPY plugins/ ./plugins/` など）。実行環境には `themes/` も置く。
- UID 10001 の非 root ユーザー `aegis` で実行し、HEALTHCHECK を持つ。データは `/app/data` をボリュームにする（例: `-v aegis-data:/app/data`）。
- 設定は `config/docker.toml`（`host = "0.0.0.0"`, `port = 8080`）。`AEGIS_CONFIG`・`HOST`・`PORT` で上書きできる。Admin Studio の API 接続先を別にする場合は `--build-arg VITE_AEGIS_SERVER_URL=...`（[configuration.md 4.3](configuration.md#43-admin-studio-の接続先)）。

---

## 6. パフォーマンスベンチマーク

AegisCMS では、HTTP サーバー全体の負荷耐性と Wasm プラグイン単体のオーバーヘッドの双方を検証するベンチマーク環境を提供しています。

### 6.1 HTTP エンドポイントのベンチマーク (`scripts/benchmark.mjs`)

Node.js 18+ の標準機能（外部依存なし）で動作し、トップページ、記事詳細、コンテンツ一覧 API、プレビュー、EC 見積もりなどの主要エンドポイントのスループット（req/s）およびレイテンシ（p50, p90, p99, max）を計測します。

```bash
# 1. サーバーを起動（別ターミナル）
task dev
# または最適化されたリリースバイナリで起動
cargo run --release -p aegis-server -- run

# 2. ベンチマークを実行（既定: 並行 50、各エンドポイント 5 秒）
task bench:http

# 3. オプション指定（クイック実行・高負荷・ターゲット指定など）
task bench:http:quick                    # 並行 20、各 2 秒
task bench:http:stress                   # 並行 100、各 10 秒
node scripts/benchmark.mjs -t index      # トップページのみ
node scripts/benchmark.mjs -t api        # 一覧 API のみ
node scripts/benchmark.mjs --auto-server # サーバーを自動起動して計測後終了
node scripts/benchmark.mjs --json        # 結果を JSON 形式で出力
```

### 6.2 Wasm プラグインのプロファイリング (`task bench:plugins`)

Wasm プラグイン（`sdk/` および `plugins/` 内のモジュール）を対象に、フック 1 回あたりの段階別所要時間（Store 作成、インスタンス化、入力書き込み、呼び出し、出力読み出し）と同時実行時のスループットを計測します（[architecture.md 12](architecture.md#12-性能の記録)）。

```bash
task bench:plugins
```

両方を一括実行する場合は `task bench:all` を使用します。

