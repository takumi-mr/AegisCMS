# AegisCMS エージェント運用規約 (AGENTS.md)

本プロジェクトにおける AI エージェントおよび共同作業者の開発運用規約です。

---

## 1. タスク・進捗管理原則 (TODOリスト運用)

- **状態管理の一元化**: プロジェクトの進捗、現在のステータス、未着手タスクはすべてルートの [TODO.md](file:///d:/projects/cms/TODO.md) にて管理します。
- **作業再開プロトコル**: 作業を開始・再開する際は、まず [TODO.md](file:///d:/projects/cms/TODO.md) を確認してください。そこを読めば直前のコンテキストおよび次に着手すべき作業が即座に把握できるように維持します。
- **更新の必須化**: **各修正・機能実装が完了したタイミングで、必ず [TODO.md](file:///d:/projects/cms/TODO.md) を更新してください**（完了チェック、次ステップの追加、状態の更新）。
- **メモの活用**: 実装時の設計判断、既知の注意点、技術的メモは [TODO.md](file:///d:/projects/cms/TODO.md) の「技術メモ・申し送り事項」欄に随時記録してください。

---

## 2. 開発環境・基本コマンド

```bash
# 全クレートのテスト実行
cargo test --workspace

# フォーマット検証・適用
cargo fmt --check
cargo fmt

# 静的解析・リント検証 (ゼロ警告ポリシー)
cargo clippy --workspace -- -D warnings

# Admin Studio フロントエンドの型検証・ビルド
npm --prefix admin run build

# 宣言的スキーマのSQLiteマイグレーション実行
cargo run -p aegis-server -- migrate

# コアサーバーの起動 (ポート 8080)
cargo run -p aegis-server -- run
```

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
  - `plugin`: `plugins/` (Wasm プラグイン)
- **記述例**:
  - `feat(ast): integrate lol-html streaming rewriter for security attributes`
  - `fix(runtime): resolve msvc trap handling in wasm engine`
  - `feat(admin): add category selection dropdown to content editor`
  - `docs: update TODO.md with Phase 2 progress`

---

## 5. Pull Request (PR) 運用規約 (Pull Request Workflow)

1. **単一責任の原則**:
   - 1 PR につき 1 つの目的・タスクに集中し、レビューしやすい粒度を維持します。
2. **PR作成前のローカル検証チェックリスト (全て必須)**:
   - [ ] `cargo test --workspace` が 100% パスすること
   - [ ] `cargo fmt --check` に違反がないこと
   - [ ] `cargo clippy --workspace -- -D warnings` で警告が 0 件であること
   - [ ] （フロントエンド変更時）`npm --prefix admin run build` でビルドが成功すること
3. **PR テンプレート構造**:
   - **概要 (Summary)**: 変更の背景と目的を簡潔に記述。
   - **変更内容 (Changes)**: 具体的な追加・修正内容を箇条書き。
   - **検証結果 (Verification)**: 実行したテスト、E2E動作確認、キャプチャや実行ログ。
   - **関連TODO (Related Tasks)**: [TODO.md](file:///d:/projects/cms/TODO.md) の対応項目リンク。
4. **マージ方針**:
   - 原則として **Squash and Merge** を推奨し、`main` ブランチの履歴をクリーンかつ追跡可能に保ちます。

