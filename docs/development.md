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
| `task test:e2e` | Admin Studio の E2E テスト（Playwright。[8](#8-admin-studio-の-e2e-テストと-storybook)） |
| `task storybook` / `build:storybook` | Admin Studio の Storybook（ポート 6006）/ 静的ビルド |
| `task plugin:build*` | プラグインの Wasm ビルド（[4](#4-プラグインのビルド)） |
| `task bench:plugins` | プラグインのフック 1 回あたりの所要時間と同時実行時のスループット（`crates/aegis-runtime/examples/plugin_profile.rs`） |
| `task bench:http`（`bench`） | サーバーの HTTP パフォーマンスベンチマーク（トップページ、記事、API、プレビュー、見積もり。`scripts/benchmark.mjs`） |
| `task bench:all` | プラグイン性能計測と HTTP ベンチマークを一括実行 |
| `task demo:shop` / `demo:shop:seed` | サンプルショップの起動 / データ投入（[examples/coffee-shop](../examples/coffee-shop)） |
| `task docker:build` / `docker:run` / `docker:stop` / `docker:logs` | Docker イメージ（[5](#5-docker-イメージ)） |
| `task docker:dbs:up` / `docker:dbs:down` | 実機検証用コンテナ（PostgreSQL 16 / MySQL 8.0 / Redis 7）の起動 / 停止（[7](#7-データベース実機検証-postgresql--mysql)） |
| `task test:dbs` | 実機コンテナ環境での注文・在庫の統合テストを一括実行 |

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
- **公開サイトのスマートフォン表示**: テーマを変えたら幅 320 / 360 / 390px で主要ページの横スクロールの有無を確かめる（手順は [テーマ作成ガイド 6](theme_authoring_guide.md#7-開発の流れ)）。

---

## 3. CI (`.github/workflows/ci.yml`)

`main` への push と PR で次を実行します。

| ジョブ | 内容 |
| --- | --- |
| `rust-ci` | `cargo fmt --check` → `cargo clippy --workspace -- -D warnings` → `cargo test --workspace` |
| `frontend-ci` | `npm ci` → Prettier → oxlint → 型検査とビルド → Storybook のビルド |
| `admin-e2e` | サーバーと Admin Studio をビルドして Playwright の E2E テスト（失敗時はレポートを成果物に残す） |
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
 
---

## 7. データベース実機検証 (PostgreSQL / MySQL)

SQLite（`sqlite::memory:`）以外の実機 RDBMS 環境（PostgreSQL 16 / MySQL 8.0）での注文・在庫トランザクションやスキーママイグレーションの動作確認を行うため、専用の Docker Compose 構成（[`docker-compose.test-dbs.yml`](../docker-compose.test-dbs.yml)）とテストスイートを提供しています。

```bash
# 1. テスト用 DB コンテナを起動（ヘルスチェック待機付き）
task docker:dbs:up

# 2. 統合テストを一括実行（aegis-db 単体トランザクション検証 + aegis-server E2E API 検証）
task test:dbs

# 3. テスト完了後にコンテナとボリュームを破棄
task docker:dbs:down
```

- **自動スキップ機能**: コンテナが起動していない通常開発時（`task test` や `task check`）は、TCP ポートプローブ（タイムアウト 150ms）により実機テストが即座にスキップされ、全体のテスト実行速度を損ないません。
- **検証項目**:
  - `_aegis_orders` テーブルおよびインデックスの初期化
  - 注文作成時のアトミックな在庫減算（`stock >= quantity`）
  - 部分的な在庫不足時のトランザクション自動ロールバック（複数行中 1 行でも不足なら全件破棄）
  - 無制限在庫（`stock IS NULL`）商品の取り扱い
  - 注文キャンセル時の在庫返却（`restock`）
  - 注文確認ページ・スタッフ用注文一覧/詳細 API・状態変更 API の E2E 動作
  - Redis の Pub/Sub によるサーバー間のイベントバス（2 台分の接続で送受信、購読を切ったあとの再接続と `Resync`。`REDIS_URL` で接続先を変えられます）

---

## 8. Admin Studio の E2E テストと Storybook

### 8.1 E2E テスト (Playwright、`admin/e2e/`)

```bash
task test:e2e                      # = npm --prefix admin run test:e2e
npm --prefix admin run test:e2e -- --ui   # Playwright の UI モード
```

- **実サーバーに対して動かす**（API はモックしない）。`playwright.config.ts` の `webServer` が `e2e/start-server.mjs` を起動し、Admin Studio と `aegis-server` をビルドしてから、`target/e2e/` を消し、admin ユーザー（`e2e/credentials.mjs`）を作って、`e2e/aegis.e2e.toml` でポート 18080 に立てる。API の形と画面の型の食い違い（プラグイン画面で実際にあった）はモックでは見つからないため。
- 初回はサーバーのコンパイルで数分かかる。ビルド済みなら `E2E_SKIP_BUILD=1` で省ける。ローカルでは起動済みのサーバー（18080）を使い回す（`reuseExistingServer`）。サーバーのログは `RUST_LOG=warn`（`RUST_LOG=debug` で全リクエスト）。
- テストはサーバーの DB を共有するので 1 本ずつ動かす（`workers: 1`）。状態を変えたテストは元に戻すか、他のテストが前提にしない状態にする。
- プラグインはリポジトリの `plugins/`（公式プラグインのビルド済み Wasm）を使い、どれも無効の状態から始める。
- 画面の要素は役割と名前（`getByRole`）で探す。英語（`locale: 'en-US'`）で動かす。プラグインのカードは表示名で名前の付いた `article`。

### 8.2 Storybook (`admin/.storybook/`、`src/**/*.stories.tsx`)

```bash
task storybook          # http://localhost:6006
task build:storybook    # admin/storybook-static/（CI でもビルド）
```

- 自分でデータを読み込む画面（`PluginManager` など）は、`.storybook/mockApi.ts` の `mockApi` で `/api/v1/*` への `fetch` を差し替えて表示する（ストーリーの `beforeEach` で呼び、戻り値の関数で元に戻す）。サーバーと同じ応答の形にする。
- ツールバーの Language で英語・日本語を切り替えられる。
- `play` 関数は Storybook の Interactions パネルで動く（テストランナーはまだ入れていない）。
- バージョンは固定（`--save-exact`）。公開から 2 週間以上たったものを選ぶ（脆弱性の修正版は除く。10.6.2 は GHSA-f2ph-cmg5-wxc8 の修正）。

### 8.3 依存パッケージを追加したとき（`package-lock.json`）

Windows の npm 11 で `npm install` すると、他のプラットフォーム向けの任意依存（`@oxc-parser/binding-wasm32-wasi` などが使う `@emnapi/*`）がロックファイルに書かれないことがあります。ローカルでは動いても、CI と Docker（Linux・Node 22 の npm 10）の `npm ci` が `package.json and package-lock.json ... are in sync` / `Missing: @emnapi/core@... from lock file` で失敗します（#74 で起きた）。

- 依存を足したら、CI と同じ npm 10 で `npm ci` が通るか確かめる（ローカルの `npm ci --dry-run` は npm 11 の判定なので足りない）:

  ```bash
  # リポジトリのルートで。admin/ の package.json と package-lock.json だけを空のディレクトリに置いて確かめる
  docker run --rm -v "$PWD/admin:/src:ro" node:22.22-alpine sh -c \
    "mkdir /app && cp /src/package.json /src/package-lock.json /app/ && cd /app && npm ci --ignore-scripts && echo NPM_CI_OK"
  # Windows の Git Bash ではパスが変換されるので: MSYS_NO_PATHCONV=1 docker run ... -v "$(pwd -W)/admin:/src:ro" ...
  ```

- 失敗したら、同じイメージの中で `npm install --ignore-scripts` してできた `package-lock.json` を使う（足りない記録だけが補われる）。`main` の `package-lock.json` と比べて、既存のパッケージのバージョンが変わっていないことを確かめる。
- `npm ci` の出力を捨てない（`>/dev/null` で失敗に気づかず、古い `node_modules` のまま検証してしまう）。
