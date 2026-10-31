[← ドキュメント一覧に戻る](README.md)

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
| `task dump` / `task restore` | データベースとアップロードストレージの一括バックアップ ZIP の生成 / 復元 |
| `task codegen`（`codegen:ts` / `codegen:openapi`） | `schemas/*.yaml` から `admin/src/types/schemas.ts` と `docs/openapi.json` を生成。スキーマを変えたら実行する |
| `task build` / `build:server` / `build:admin` | 本番ビルド |
| `task test:admin` | Admin Studio のユニットテスト（Vitest。[8.1](#81-ユニットテスト-vitestadminsrctests)） |
| `task test:e2e` | Admin Studio の E2E テスト（Playwright。[8.2](#82-e2e-テスト-playwrightadmine2e)） |
| `task storybook` / `build:storybook` | Admin Studio の Storybook（ポート 6006）/ 静的ビルド |
| `task plugin:build*` | プラグインの Wasm ビルド（[4](#4-プラグインのビルド)） |
| `task bench:plugins` | プラグインのフック 1 回あたりの所要時間と同時実行時のスループット（`crates/aegis-runtime/examples/plugin_profile.rs`） |
| `task bench:http`（`bench`） | サーバーの HTTP パフォーマンスベンチマーク（トップページ、記事、API、プレビュー、見積もり。`scripts/benchmark.mjs`） |
| `task bench:all` | プラグイン性能計測と HTTP ベンチマークを一括実行 |
| `task bench:dbs` | SQLite / PostgreSQL / MySQL の 3 エンジン間でベンチマーク比較を実行（[6.3](#63-データベース別パフォーマンス比較-task-benchdbs--scriptscompare-dbsmjs)） |
| `task demo:shop` / `demo:shop:seed` | サンプルショップの起動 / データ投入（[examples/coffee-shop](../examples/coffee-shop)） |
| `task demo:forum` / `demo:forum:seed` | サンプル掲示板の起動 / データ投入（[examples/hiking-forum](../examples/hiking-forum)） |
| `task docs:screenshots` | `docs/images/` のスクリーンショットを撮り直す（サンプルのデータで使い捨てのサーバーを立て、Playwright で撮る。`scripts/capture-docs-screenshots.mjs`。画面を変えたら実行して差分をコミットする） |
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
- 本番の公開（Caddy で HTTPS、複数サイト）は [`deploy/`](../deploy)。`aegis-server deploy generate`（`crates/aegis-server/src/deploy.rs`）が `deploy/sites.toml` から `compose.yaml`・`Caddyfile`・サイトごとの `aegis.toml` を作る。手元では `task deploy:generate` で `deploy/sites.toml` から生成できる（組み込みのテーマ・プラグインはリポジトリの `themes/`・`plugins/` で確かめる）。
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

# 一覧 API とプレビューも計測する（Viewer 以上のユーザーでログイン）
AEGIS_BENCH_PASSWORD=... node scripts/benchmark.mjs --user admin
node scripts/benchmark.mjs --token <セッション ID>   # 環境変数 AEGIS_BENCH_TOKEN でも可
```

- **一覧 API（`api`）とプレビュー（`preview`）は Viewer 以上のセッションが必要**です。`--user` / `--token` を指定しない場合、この 2 つは計測しません（`-t api` などで個別に指定するとエラー）。セッションなしで回すと、本来の処理ではなく 401 の応答と、拒否を記録する監査ログ（`access.denied`）の書き込みを測ることになるためです。記事ページに使う slug の検索にも同じセッションを使います。
- **カート見積もり（`quote`）は `[commerce].enabled = true` かつ公開中の商品が必要**です。ショップが無効、または商品が無い場合は計測しません（`-t quote` で明示指定した場合は理由を表示して終了）。ショップが無効（404）や商品が存在しない（404）状態で回すと、本来の見積もり・プラグインフック処理ではなくエラー応答を測ることになるためです。商品は API または公開の `/products` から自動で探して使用します。
- 記事が多い状態で測るには、先に `scripts/seed-benchmark.mjs`（`task bench:seed`）でデータを入れます。カテゴリ 20 件と記事 1 万件（約 10 ブロック・約 4KB、カテゴリへのリレーションつき、10 件に 1 件は下書き）を API 経由で作り、投入の速さ（件数/秒）も表示します。同じ接頭辞（`--prefix`、既定 `bench`）で 2 回流すと slug が重複して失敗するので、測り直すときは DB を作り直してください。

  ```bash
  export AEGIS_BENCH_PASSWORD=...
  task bench:seed -- --user admin            # 記事 1 万件（-n で件数、-c で並行数）
  task bench:http -- --user admin
  ```

- 4xx / 5xx の応答が混ざったシナリオは ⚠️ と応答の内訳（例: `HTTP 422 x 32106`）を表示します。この数字はエンドポイント本来の性能ではありません。

### 6.2 Wasm プラグインのプロファイリング (`task bench:plugins`)

Wasm プラグイン（`sdk/` および `plugins/` 内のモジュール）を対象に、フック 1 回あたりの段階別所要時間（Store 作成、インスタンス化、入力書き込み、呼び出し、出力読み出し）と同時実行時のスループットを計測します（[architecture.md 12](architecture.md#12-性能の記録)）。

```bash
task bench:plugins
```

両方を一括実行する場合は `task bench:all` を使用します。

### 6.3 データベース別パフォーマンス比較 (`task bench:dbs` / `scripts/compare-dbs.mjs`)

同一のリリースバイナリ（LTO 有効化・最適化済み）を用いて、組み込み **SQLite (WAL モード)**、Docker 上の **PostgreSQL 16**、および **MySQL 8.0** の 3 つのデータベースエンジン間で主要エンドポイントの負荷耐性とスループットを計測・比較します。

```bash
# 事前にテスト用 DB コンテナを起動（ヘルスチェック待機付き）
task docker:dbs:up

# 3 つの DB に対する自動比較ベンチマーク（ビルド、マイグレーション、データ投入、測定、比較レポート出力）を一括実行
task bench:dbs
```

#### 実測ベンチマーク結果（Windows 11 / AMD Ryzen 7 / Docker Desktop、並行度 50）

| エンドポイント / ワークロード | SQLite (WAL) | PostgreSQL 16 (Docker) | MySQL 8.0 (Docker) | 特性と分析 |
| :--- | :--- | :--- | :--- | :--- |
| **公開トップページ** (`GET /`)<br>*(参照中心・キャッシュ・テンプレート描画)* | **2,733 req/s**<br>(p50: 17.8ms / p99: 32.0ms) | **494 req/s**<br>(p50: 94.8ms / p99: 180.9ms) | **486 req/s**<br>(p50: 96.5ms / p99: 170.4ms) | SQLite はインプロセス・ゼロコピーアクセスのため約 5.5 倍高速。外部 RDBMS は TCP ループバックと接続プール（既定 10）によるオーバーヘッドが支配的。 |
| **未認証アクセス** (`GET /api/v1/contents/article`)<br>*(401 Unauthorized + 監査ログ同期 INSERT)* | **2,014 req/s**<br>(p50: 16.4ms / p99: 89.8ms) | **992 req/s**<br>(p50: 45.2ms / p99: 138.7ms) | **230 req/s**<br>(p50: 178.7ms / p99: 675.1ms) | SQLite は `synchronous = NORMAL` により高速。PostgreSQL も MVCC/WAL が優秀。MySQL は InnoDB トランザクションコミットの fsync とプール枯渇で急激にスループットが低下。 |
| **セッションログイン** (`POST /api/v1/auth/session`)<br>*(Argon2id 検証 + セッション発行 + 監査ログ)* | **119 req/s**<br>(p50: 405.0ms / p99: 509.9ms) | **106 req/s**<br>(p50: 454.4ms / p99: 588.6ms) | **78 req/s**<br>(p50: 610.7ms / p99: 846.5ms) | Argon2id の計算コスト（メモリ 64MB / 反復 3 回）がボトルネックとなり、DB 間の差は比較的小さい。 |

#### 設計上の考察と示唆
1. **読み込み性能**: SQLite はプロセス内メモリ・ローカルファイル直結のため圧倒的な性能（2,700+ req/s）を誇ります。外部 DB ではコネクションプールのチューニング（`max_connections` の拡大など）を行わない限り、接続待ちによるレイテンシが増加します。
2. **監査ログの同期 INSERT**: `database` シンクを有効にした場合、SQLite では WAL モードにより 2,000 req/s を維持できますが、MySQL では同期トランザクション書き込みの待ち時間によりスループットが約 1/9 まで制限されます。外部 RDBMS（特に MySQL）を主系 DB として高トラフィック運用する場合は、監査ログを `database` ではなく `log` シンク（ファイル/stdout 出力 + Fluentd/Logstash 転送）に逃がすか、内部で有限チャネル（`mpsc`）による非同期バッチ INSERT を導入することが強く推奨されます。
 
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

## 8. Admin Studio のテストと Storybook

### 8.1 ユニットテスト (Vitest、`admin/src/**/*.test.ts`)

```bash
task test:admin                 # = npm --prefix admin run test:unit
npm --prefix admin run test:unit:watch # ファイル変更監視モード
```

- **高速なユーティリティおよびロジックの検証**: `src/utils/`（Markdown パーサー・シリアライザー、通貨フォーマッター、権限判定、日付フォーマット、SSO / 認証リダイレクト等）や `src/api/` のヘルパー・クラスを対象に、Vite 設定と連動した Vitest で実行します。
- **CI / task check への統合**: `task check` および GitHub Actions の `frontend-ci` ジョブ内で自動実行されます。

### 8.2 E2E テスト (Playwright、`admin/e2e/`)

```bash
task test:e2e                      # = npm --prefix admin run test:e2e
npm --prefix admin run test:e2e -- --ui   # Playwright の UI モード
```

- **実サーバーに対して動かす**（API はモックしない）。`playwright.config.ts` の `webServer` が `e2e/start-server.mjs` を起動し、Admin Studio と `aegis-server` をビルドしてから、`target/e2e/` を消し、admin ユーザー（`e2e/credentials.mjs`）を作って、`e2e/aegis.e2e.toml` でポート 18080 に立てる。API の形と画面の型の食い違い（プラグイン画面で実際にあった）はモックでは見つからないため。
- **包括的なテストスイート（全 17 テスト）**:
  - `auth.spec.ts`: ログイン成功、サインアウト（セッション破棄・リダイレクト）、誤ったパスワード時のエラー表示、レートリミットによるアカウント一時ロック（3回連続失敗後の 429 応答）。
  - `content.spec.ts`: 記事新規作成、下書き（Draft）保存とサイドバー一覧反映、公開（Published）切り替え、リロード時の永続化検証、Markdown モードと Blocks モードの相互変換。
  - `themes.spec.ts`: インストール済みテーマ一覧表示、アクティブテーマ切り替え（Aegis Horizon ↔ Nordic Minimal Editorial）、トースト通知およびリロード永続化検証。
  - `users.spec.ts`: 既存管理者表示、新規ユーザー作成（エディターロール追加）、メール招待（`InviteUserModal` による招待リンク発行）。
  - `commerce.spec.ts`: EC 有効環境（`[commerce].enabled = true`）での商品作成（価格・在庫設定）とカタログ一覧表示、注文一覧・ステータスフィルター画面の動作検証。
  - `responsive.spec.ts`: モバイルビューポート（375x667）におけるハンバーガーメニュー開閉・ビュー遷移、エディタ画面の 3 ペイン（記事一覧 / 編集 / プレビュー）タブ切り替え。
  - `plugins.spec.ts`: プラグイン一覧・フック表示、無停止での有効化・無効化、設定値の保存、フック実行順序の変更と設定ファイル復帰。
- 初回はサーバーのコンパイルで数分かかる。ビルド済みなら `E2E_SKIP_BUILD=1` で省ける。ローカルでは起動済みのサーバー（18080）を使い回す（`reuseExistingServer`）。サーバーのログは `RUST_LOG=warn`（`RUST_LOG=debug` で全リクエスト）。
- テストはサーバーの DB を共有するので 1 本ずつ動かす（`workers: 1`）。状態を変えたテストは元に戻すか、他のテストが前提にしない状態にする（ランダムサフィックス等を活用）。
- プラグインはリポジトリの `plugins/`（公式プラグインのビルド済み Wasm）を使い、どれも無効の状態から始める。
- 画面の要素は役割と名前（`getByRole`）で探す。英語（`locale: 'en-US'`）で動かす。プラグインのカードは表示名で名前の付いた `article`。

### 8.3 Storybook (`admin/.storybook/`、`src/**/*.stories.tsx`)

```bash
task storybook          # http://localhost:6006
task build:storybook    # admin/storybook-static/（CI でもビルド）
```

- 自分でデータを読み込む画面（`PluginManager` など）は、`.storybook/mockApi.ts` の `mockApi` で `/api/v1/*` への `fetch` を差し替えて表示する（ストーリーの `beforeEach` で呼び、戻り値の関数で元に戻す）。サーバーと同じ応答の形にする。
- ツールバーの Language で英語・日本語を切り替えられる。
- **コンポーネント & インタラクションテスト (`task test:storybook`)**: `@storybook/addon-vitest`（Vitest Browser Mode + Playwright Chromium）により、すべてのストーリーと `play` 関数をヘッドレスブラウザで自動検証します。`task check` および CI（`frontend-ci`）内で自動実行されます。
- バージョンは固定（`--save-exact`）。公開から 2 週間以上たったものを選ぶ（脆弱性の修正版は除く。10.6.2 は GHSA-f2ph-cmg5-wxc8 の修正）。

### 8.4 依存パッケージを追加したとき（`package-lock.json`）

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


---

## 9. CLI サブコマンドリファレンス (`aegis`)

`aegis-server` バイナリ（`cargo run -p aegis-server -- <command>` またはインストール済み `aegis`）で利用可能な主要サブコマンドです。

```bash
# 全体ヘルプ
aegis --help

# サーバー起動 (既定: ポート 8080)
aegis run [--port <PORT>] [--host <HOST>]

# データベーススキーマのマイグレーション実行
aegis migrate

# バックアップアーカイブの生成 (DB + アップロードストレージ)
aegis dump --output backup.zip
# DB のみバックアップ (ストレージ除外)
aegis dump --output db_only.zip --no-storage
# ストレージのみバックアップ (DB 除外)
aegis dump --output storage_only.zip --no-database

# バックアップの整合性・チェックサム事前検証 (復元は行わない)
aegis restore --input backup.zip --verify-only

# バックアップアーカイブからの復元
aegis restore --input backup.zip
# 既存レコードと衝突する場合は上書き置換して復元
aegis restore --input backup.zip --overwrite

# TypeScript 型定義 / OpenAPI 3.1 仕様書の生成
aegis codegen --target typescript --output admin/src/types/schemas.ts
aegis codegen --target openapi --output docs/openapi.json

# ユーザー管理
aegis user add <username> --role admin [--email <email>]
aegis user passwd <username>
aegis user list
aegis user role <username> <editor|admin|viewer>
aegis user disable <username>
aegis user enable <username>
aegis user link-oidc <username> <sub_claim>
aegis user link-saml <username> <name_id>

# プラグイン一覧と稼働状況の確認
aegis plugin list

# 通知設定と配信チャネルのテスト
aegis notify test --to <email> [--staff]
```


---

## 10. トラブルシューティング & 開発Tips

### 10.1 ビルド時のディスク容量対策
クラウド開発環境や CI コンテナ等でディスク容量が逼迫する場合（`target/debug` のインクリメンタルコンパイル成果物が 30GB 近くまで膨らむことがあります）、以下の対策を行ってください：
```bash
# インクリメンタルコンパイルを無効化してビルド
CARGO_INCREMENTAL=0 cargo build

# または不要になったインクリメンタルキャッシュを削除
rm -rf target/debug/incremental
```

### 10.2 Storybook / Playwright 実行環境の注意点
- **ブラウザシェルの互換性**: Linux / クラウド環境において Playwright のバージョンと `/opt/pw-browsers` 内のヘッドレスシェルバージョンが一致しないとブラウザが起動しない場合があります。その場合はシンボリックリンク等で利用可能なバージョンへエイリアスを設定してください。
- **Storybook mockApi での 204 応答**: `.storybook/mockApi.ts` において `204 No Content` をモックする際は、必ず本文（body）なしでレスポンスを返却してください（本文付きの 204 はブラウザの `Response` オブジェクトが例外をスローします）。
