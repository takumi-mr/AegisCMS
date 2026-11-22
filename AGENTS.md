# AegisCMS エージェント運用規約 (AGENTS.md)

本プロジェクトにおける AI エージェントおよび共同作業者の開発運用規約です。

---

## 1. タスク・進捗管理原則 (TODOリスト運用)

- **状態管理の一元化**: プロジェクトの進捗、現在のステータス、未着手タスクはすべてルートの [TODO.md](TODO.md) にて管理します。
- **作業再開プロトコル**: 作業を開始・再開する際は、まず [TODO.md](TODO.md) を確認してください。そこを読めば直前のコンテキストおよび次に着手すべき作業が即座に把握できるように維持します。
- **更新の必須化**: **各修正・機能実装が完了したタイミングで、必ず [TODO.md](TODO.md) を更新してください**（完了チェック、次ステップの追加、状態の更新）。
- **メモの性質（CHANGELOG 化の禁止）**: **「技術メモ・申し送り事項」は変更ログ（CHANGELOG）を書く場所ではありません**。単純な実装ログや変更履歴は記載せず、作業再開時に必要な未解決の設計判断、既知の注意点・制約、環境固有の申し送りのみを簡潔に記録してください。機能の仕様や設計判断は `docs/*.md` に記述し、作業経緯は Git のコミットログや PR で追跡します。完了したタスクのメモは TODO.md に残さずクリーンに保ちます。

---

## 1a. 実装規約

- **API レスポンスのエラーメッセージは英語のみ**: REST API が返すエラーメッセージ（JSON の `error` など）は英語を標準とし、多言語対応しません（ロケール設定で切り替えない）。クライアントが言語ごとの文言を出したいときは、`code` などの機械可読な値で自分の側で訳します。訪問者の画面に出す文言（組み込みテンプレートなど）は `[theme].locale` で多言語対応します。詳細は [docs/architecture.md の 19](docs/architecture.ja.md#19-多言語対応の方針)。

---

## 2. 開発環境・基本コマンド (go-task 対応)

プロジェクトルートで `task` コマンドを使用することで、主要なビルド・テスト・検証を簡単に実行できます。

```bash
# 全タスク一覧の表示
task

# PR/コミット前の全検証チェック一括実行 (fmt, prettier, clippy, oxlint, test, admin:build)
task check

# コアサーバーの起動 (ポート 8080)
task dev

# 宣言的スキーマの SQLite 自動マイグレーション実行
task migrate

# データベースおよびアップロードストレージの一括バックアップ生成 / 復元
task dump
task restore

# ワークスペース全体のテスト実行
task test

# Admin Studio ユニットテスト実行 (Vitest)
task test:admin

# Admin Studio Storybook インタラクションテスト実行 (Vitest Browser Mode)
task test:storybook

# 静的解析・リント検証 (ゼロ警告ポリシー: Rust は Clippy、Admin Studio は oxlint)
task lint
task lint:admin

# コードフォーマット適用 / 検証 (Rust: rustfmt)
task fmt
task fmt:check

# コードフォーマット適用 / 検証 (Admin Studio: Prettier)
task fmt:admin
task fmt:admin:check

# コードフォーマット適用 / 検証 (リポジトリ全体の Markdown: Prettier)
task fmt:md
task fmt:md:check

# Admin Studio フロントエンドの本番ビルド / 開発サーバー
task build:admin
task dev:admin

# Docker イメージビルド / 起動
task docker:build
task docker:run
```

### 2a. 実機の PostgreSQL / MySQL / Redis での動作確認（CI では行わない）

CI は SQLite だけで動き、PostgreSQL・MySQL・Redis のテストは**動かしません**（接続できないと、各テストは `Skipping ...` と出して成功扱いで飛ばします）。そのため、**次の変更では、ローカルで Docker の実機コンテナに対してテストを動かして確かめてください**。

- 対象: `crates/aegis-db`（`postgres.rs`・`mysql.rs`・`migrations/`）、DB を直接扱うクレート（`aegis-backup`・`aegis-comments` など）の SQL、キャッシュ・イベントバス（`aegis-cache` の Redis）
- 理由の例: MySQL 8.0 の標準の文字セット（utf8mb4）では、索引が 3072 バイトを超えると `init` が止まる（v0.6.0 から入っていた `_aegis_purchases` の索引がそうだった）。SQLite でも CI でも気づけない

```bash
# 実機コンテナを起動（PostgreSQL 16 :5432 / MySQL 8.0 :3306 / Redis 7 :6379）
task docker:dbs:up

# 実機 DB の統合テストを一括実行
task test:dbs

# 実機 DB を使うテストを、DB のクレートごとにすべて実行する場合
cargo test -p aegis-db --tests
cargo test -p aegis-backup --test cross_db_roundtrip

# 終わったら停止・削除（DB のデータも消える）
task docker:dbs:down
```

- **`Skipping` と出たテストは、確かめたことになりません**。コンテナが起動しているか（ポートに繋がるか）を確認します。
- マイグレーション（`crates/aegis-db/migrations/{sqlite,postgres,mysql}/`）を足す・変えるときは、3 つの方言すべてで実機に流して確かめます。リリース済みの手順は編集しない（チェックサムが変わる）のが原則です（[docs/versioning.ja.md の 6](docs/versioning.ja.md#6-テーブル定義の更新方針)）。
- MySQL の索引の長さは、`crates/aegis-db/tests/mysql_index_size.rs` が CI でも検査します（`VARCHAR(255)` の列を 4 列並べるような索引は、`col(64)` のように先頭の文字数を絞る）。ただし実機での確認の代わりにはなりません。
- カバレッジを実機 DB つきで測るときは、コンテナを起動したうえで `task coverage` を実行します。

---

## 3. ブランチ運用規約 (Branch Strategy)

- **メインブランチ (`main`)**:
    - 常にビルド・テストがパスする安定版コードベース。
    - 直接プッシュはプロトタイプ初期フェーズを除き原則禁止。機能開発は作業ブランチを作成して行います。
- **作業ブランチ命名規則**:
    - `feat/<feature-name>`: 新機能開発（例: `feat/streaming-rewriter`, `feat/schema-relations`）
    - `fix/<bug-name>`: バグ修正（例: `fix/wasm-timeout-trap`, `fix/cors-preflight`）
    - `refactor/<scope>`: リファクタリング（例: `refactor/ast-visitor`）
    - `docs/<topic>`: ドキュメント追加・改訂（例: `docs/plugin-sdk-guide`）
    - `chore/<task>`: ビルド構成・CI/CD・依存パッケージ更新（例: `chore/ci-setup`）

---

## 4. コミットメッセージ規約 (Commit Message Convention)

Conventional Commits 形式に準拠します。

```text
<type>(<scope>): <subject>

[optional body]
```

- **Type**:
    - `feat`: 新機能の追加
    - `fix`: バグ修正
    - `docs`: ドキュメントや TODO.md のみの変更
    - `style`: コードの意味に影響しない書式・空白などの変更
    - `refactor`: バグ修正や機能追加を含まないコードのリファクタリング
    - `perf`: パフォーマンス向上を目的としたコード変更
    - `test`: テストコードの追加・既存テストの修正
    - `chore`: ビルドプロセスや補助ツールの変更
- **Scope (主要モジュール)**:
    - `core`: `crates/aegis-core`
    - `ast`: `crates/aegis-ast`
    - `schema`: `crates/aegis-schema`
    - `runtime`: `crates/aegis-runtime`
    - `server`: `crates/aegis-server`
    - `admin`: `admin/` (Admin Studio フロントエンド)
    - `plugin-builder`: `plugin-builder/` (Plugin Builder: プラグイン開発用のエディター + 実行サンドボックス)
    - `theme-builder`: `theme-builder/` (Theme Builder: テーマ開発用のエディター + ライブプレビュー)、`crates/aegis-server/src/theme_dev.rs` (開発サーバー `aegis-server theme dev`)
    - `plugin`: `sdk/` (Wasm プラグインのテンプレート・サンプル)、`crates/aegis-plugin-sdk`
- **コミットメッセージ・PR の本文に生の HTML（`<script>` / `<style>` など）を書かない**: release-please がリリース PR の本文を HTML として読むため、タグと GitHub Release が作られなくなります。バッククォートで囲んでも防げないので、「script 要素」のように言葉で書きます（`commit-lint.yml` で検査。[docs/versioning.md の 3](docs/versioning.ja.md#3-コミットメッセージとバージョンの上がり方)）。
- **破壊的変更**は `feat(scope)!:` のように `!` を付けるか、本文に `BREAKING CHANGE:` を書きます。バージョン・タグ・CHANGELOG はこの規約から release-please が自動で決めます（[docs/versioning.ja.md](docs/versioning.ja.md)）。
- **記述例**:
    - `feat(ast): integrate lol-html streaming rewriter for security attributes`
    - `fix(runtime): resolve msvc trap handling in wasm engine`
    - `feat(admin): add category selection dropdown to content editor`
    - `feat(theme-builder): add a theme dev server and a live-preview editor`
    - `docs: update TODO.md with Phase 2 progress`

---

## 5. Pull Request (PR) 運用規約 (Pull Request Workflow)

1. **単一責任の原則**:
    - 1 PR につき 1 つの目的・タスクに集中し、レビューしやすい粒度を維持します。
2. **PR作成前のローカル検証チェックリスト (全て必須)**:
    - [ ] `node scripts/check-versions.mjs`（`task version:check`）でバージョン表記の食い違いがないこと
    - [ ] `cargo test --workspace` が 100% パスすること
    - [ ] `cargo fmt --check` に違反がないこと
    - [ ] `cargo clippy --workspace -- -D warnings` で警告が 0 件であること
    - [ ] （フロントエンド変更時）`npm --prefix admin run test:unit` が 100% パスすること
    - [ ] （フロントエンド変更時）`npm --prefix admin run test:storybook` が 100% パスすること
    - [ ] （フロントエンド変更時）`npm --prefix admin run build` でビルドが成功すること
    - [ ] （フロントエンド変更時）`npm --prefix admin run format:check` に違反がないこと（VS Code では保存時に自動整形）
    - [ ] （フロントエンド変更時）`npm --prefix admin run lint`（oxlint `--deny-warnings`）で警告が 0 件であること
    - [ ] （Markdown 変更時）`task fmt:md:check`（`npx prettier@3.9.10 --check "**/*.md"`）に違反がないこと（VS Code では保存時に自動整形。対象外は `.prettierignore`）
    - [ ] （Plugin Builder 変更時）`npm --prefix plugin-builder run test`・`run build`・`run format:check`・`run lint` がすべてパスすること
    - [ ] （Theme Builder 変更時）`npm --prefix theme-builder run test`・`run test:e2e`・`run build`・`run format:check`・`run lint` がすべてパスすること
    - [ ] （DB・マイグレーション・Redis 変更時）本ファイルの 2a のとおり、実機コンテナで `task test:dbs` を動かし、結果を PR の検証結果に書くこと（CI は実機 DB を動かさない）
3. **PR テンプレート構造**:
    - **概要 (Summary)**: 変更の背景と目的を簡潔に記述。
    - **変更内容 (Changes)**: 具体的な追加・修正内容を箇条書き。
    - **検証結果 (Verification)**: 実行したテスト、E2E動作確認、キャプチャや実行ログ。
    - **関連TODO (Related Tasks)**: [TODO.md](TODO.md) の対応項目リンク。
4. **マージ方針**:
    - 原則として **Squash and Merge** を推奨し、`main` ブランチの履歴をクリーンかつ追跡可能に保ちます。
