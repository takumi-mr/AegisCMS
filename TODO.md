# AegisCMS TODO

> [!NOTE]
> 作業の再開時はここを読む。各作業の完了時に更新する。
> 完了したタスクは残さない。仕様は `docs/*.md`・`admin/README.md`、設計判断・注意点は [docs/architecture.md](docs/architecture.md)、開発手順は [docs/development.md](docs/development.md) に書いてから消す。
> **技術メモ・申し送り事項は変更ログ（CHANGELOG）を書く場所ではない**。単純な実装ログや変更履歴は記載せず、未解決の設計課題や既知の注意点のみを記録する。経緯は Git のコミットログや PR で追う。

---

## 現在の状況

- コアエンジン（Rust）、Admin Studio（React/TypeScript）、Wasm プラグインランタイム、テーマエンジン、二段階認証（TOTP + バックアップコード 6 枚）、動的運用設定（`system.*`）、一括バックアップ＆リストア機能、検索キーワード／該当なし検索のアクセス解析集計が安定稼働。全検証（`task check`）を通過。

---

## 直近の着手タスク

1. **AssemblyScript / TinyGo SDK の機能追従**:
   - Rust SDK に追加済みのフック（`calculate-order`, `on-order-status-changed`, `send-notification`, `on-content-published`, `check-comment`, `render-blocks`）およびホスト API（`http_fetch`, `kv_get`, `config_get`）のラッパーを整備（ABI は [plugin_abi_specification.md](docs/plugin_abi_specification.md) 参照）。

---

## 技術メモ・申し送り事項

（現在なし）

---

## 未着手タスク (Backlog)

### 🚀 性能最適化
- [ ] **監査ログ `database` 出力先の非同期化（保留）**: 有限 `mpsc` キュー + バッチ INSERT。SQLite WAL では約 2,000 req/s と十分なスループットだが、Docker 上の PostgreSQL（約 990 req/s）や MySQL（約 230 req/s）など外部 RDBMS では同期 INSERT のオーバーヘッドが顕著になるため、外部 DB 運用向け最適化として再検討（ベンチマーク詳細は [architecture.md 2.2](docs/architecture.md#22-databaseadapter) および [development.md 6.3](docs/development.md#63-データベース別パフォーマンス比較-task-benchdbs--scriptscompare-dbsmjs) 参照）。
- [ ] **大規模コンテンツ（1 万件超）での最適化**:
  - `on-render-page` フックで HTML 全体を Wasm に渡すオーバーヘッドの軽減。

### 🌐 サイト機能拡張
- [ ] **関連記事**: タグ・カテゴリ・検索索引（`aegis-search`）から「この記事に近い記事」を出す。
- [ ] **お知らせバナー**: 期間を指定して全ページの上部に出す（メンテナンス・セールの告知）。
- [ ] **リンク切れチェック**: 公開記事の内部・外部リンクを定期検査し、Admin Studio に一覧を出す。
- [ ] **コメント・掲示板への CAPTCHA 適用**: `aegis-captcha`（問い合わせフォームで導入済み。[contact.md 1.2](docs/contact.md#12-captcha任意)）を `[comments]` / `[forum]` にも使えるようにする。

### 🔌 プラグイン・将来構想
- [ ] **読者向けの小さな公式プラグイン**（`official-page-toc` / `official-back-to-top` と同じ作りで足す。それぞれ別プラグイン）:
  - 読了時間・文字数の表示（`on-render-page`）
  - SNS 共有ボタン（X・LINE・URL コピー）
  - コードブロックの行番号・ファイル名表示（`official-code-enhancer` の拡張でも可）
  - 画像ライトボックス（本文の画像をクリックで拡大）
  - 外部リンクの自動処理（`target="_blank" rel="noopener nofollow"` と外部アイコン。`transform-content-ast`）
  - 前後の記事ナビ（同じカテゴリ内の「前の記事 / 次の記事」）
- [ ] **広告配信プラグイン**: `on-render-page` によるヘッダー注入とカスタムブロック表示、動的 CSP 設定との連携、Cookie 同意バナー対応。
- [ ] **テンプレートからの動的コンテンツ取得関数（検討）**: `latest("article", 5, category="news")` などの MiniJinja グローバル関数または事前取得宣言機構。
- [ ] **分散ストレージ・クラスタ対応**: libSQL (Turso) への対応およびマルチノードクラスタ環境の最適化。
