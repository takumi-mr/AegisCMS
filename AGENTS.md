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

# 宣言的スキーマのSQLiteマイグレーション実行
cargo run -p aegis-server -- migrate

# コアサーバーの起動 (ポート 8080)
cargo run -p aegis-server -- run
```
