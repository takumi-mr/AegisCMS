# AegisCMS TODO

> [!NOTE]
> 作業の再開時はここを読む。各作業の完了時に更新する。
> 完了したタスクは残さない。仕様は `docs/*.md`・`admin/README.md`、設計判断・注意点は [docs/architecture.md](docs/architecture.md)、開発手順は [docs/development.md](docs/development.md) に書いてから消す。
> **技術メモ・申し送り事項は変更ログ（CHANGELOG）を書く場所ではない**。単純な実装ログや変更履歴は記載せず、未解決の設計課題や既知の注意点のみを記録する。経緯は Git のコミットログや PR で追う。

---

## 現在の状況

- プラグイン開発サーバー（`aegis-server plugin dev <dir>` / `task dev:plugin`）のコア API（Phase 1）が稼働。DB・ログイン不要で単体 Wasm フックの即時実行、ログ・Fuel・実行時間計測、KVS/HTTP モックを提供。
- コアエンジン（Rust）、Admin Studio（React/TypeScript）、Wasm プラグインランタイム、テーマエンジン、二段階認証、動的運用設定、一括バックアップ＆リストア機能、アクセス解析集計が安定稼働。全検証（`task check`）を通過。

---

## 直近の着手タスク

1. **Plugin Builder Phase 2: Web UI (`plugin-builder/`) の構築 (MVP)**:
   - `theme-builder` と同等の React 19 + TypeScript + Vite による開発用 Web スタジオの新設。
   - フック選択セレクター、入力 JSON/テキストエディター、プリセット読み込み（フィクスチャ）、ワンクリック実行ボタン。
   - 実行結果の可視化（Diff / JSON 表示、実行時間 ms、消費 Fuel、リアルタイム Wasm ログストリーム、KVS 差分）。
   - `task dev:plugin-builder`、ビルド・リント・テストスクリプトの整備。

2. **AssemblyScript / TinyGo SDK の機能追従**:
   - Rust SDK に追加済みのフック（`calculate-order`, `on-order-status-changed`, `send-notification`, `on-content-published`, `check-comment`, `render-blocks`）およびホスト API（`http_fetch`, `kv_get`, `config_get`）のラッパーを整備（ABI は [plugin_abi_specification.md](docs/plugin_abi_specification.md) 参照）。

---

## 技術メモ・申し送り事項

（現在なし）

---

## 未着手タスク (Backlog)

### 🔌 プラグイン・将来構想
- [ ] **Plugin Builder (プラグイン開発体験向上ツール)**:
  - [x] **Phase 1**: コア開発サーバー API (`aegis plugin dev` / `task dev:plugin`)。DB不要で単体Wasmフック実行、ログ・Fuel取得、モックホストAPI。
  - [ ] **Phase 2**: Plugin Builder Web UI (`plugin-builder/` - React/Vite)。フック選択、モックデータ入力、Diff・ログ表示。
  - [ ] **Phase 3**: フロントエンド系フックの HTML ライブプレビュー (iframe)、KVS/HTTP インスペクター、各言語（Rust/TS/Go）の自動ビルド監視。
- [ ] **読者向けの小さな公式プラグイン**（`official-page-toc` / `official-back-to-top` と同じ作りで足す。それぞれ別プラグイン）:
  - 読了時間・文字数の表示（`on-render-page`）
  - [x] SNS 共有ボタン（X・LINE・URL コピー、設定で任意の共有先を追加可能）: 公式プラグイン [official-share-buttons](plugins/official-share-buttons)
  - コードブロックの行番号・ファイル名表示（`official-code-enhancer` の拡張でも可）
  - 画像ライトボックス（本文の画像をクリックで拡大）
  - 外部リンクの自動処理（`target="_blank" rel="noopener nofollow"` と外部アイコン。`transform-content-ast`）
  - 前後の記事ナビ（同じカテゴリ内の「前の記事 / 次の記事」）
- [ ] **広告配信プラグイン**: `on-render-page` によるヘッダー注入とカスタムブロック表示、動的 CSP 設定との連携、Cookie 同意バナー対応。
- [ ] **テンプレートからの動的コンテンツ取得関数（検討）**: `latest("article", 5, category="news")` などの MiniJinja グローバル関数または事前取得宣言機構。
- [ ] **分散ストレージ・クラスタ対応**: libSQL (Turso) への対応およびマルチノードクラスタ環境の最適化。

### 🚀 性能最適化
- [ ] **監査ログ `database` 出力先の非同期化（保留）**: 有限 `mpsc` キュー + バッチ INSERT。SQLite WAL では約 2,000 req/s と十分なスループットだが、Docker 上の PostgreSQL（約 990 req/s）や MySQL（約 230 req/s）など外部 RDBMS では同期 INSERT のオーバーヘッドが顕著になるため、外部 DB 運用向け最適化として再検討（ベンチマーク詳細は [architecture.md 2.2](docs/architecture.md#22-databaseadapter) および [development.md 6.3](docs/development.md#63-データベース別パフォーマンス比較-task-benchdbs--scriptscompare-dbsmjs) 参照）。
- [ ] **大規模コンテンツ（1 万件超）での最適化**:
  - `on-render-page` フックで HTML 全体を Wasm に渡すオーバーヘッドの軽減。

### 🌐 サイト機能拡張
- [x] **関連記事**: Strategy パターンで差し替え可能な推薦機構（既定: タグ一致数 ＞ 同一カテゴリ ＞ 新着フォールバック）。
- [x] **リンク切れチェック**: 公開記事の内部・外部リンクを検査し、Admin Studio（設定 > リンク切れチェック）で一覧表示・手動再検査を実行可能。
- [ ] **お知らせバナー**: 期間を指定して全ページの上部に出す（メンテナンス・セールの告知）。
- [ ] **コメント・掲示板への CAPTCHA 適用**: `aegis-captcha`（問い合わせフォームで導入済み。[contact.md 1.2](docs/contact.md#12-captcha任意)）を `[comments]` / `[forum]` にも使えるようにする。
