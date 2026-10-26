# AegisCMS TODO

> [!NOTE]
> 作業の再開時はここを読む。各作業の完了時に更新する。
> 完了したタスクは残さない。仕様は `docs/*.md`・`admin/README.md`、設計判断・注意点は [docs/architecture.md](docs/architecture.md)、開発手順は [docs/development.md](docs/development.md) に書いてから消す。経緯は git の履歴（PR）で追う。

## 現在の状況
- `task check` がすべて通る状態を保つ。

## 技術メモ・申し送り事項
- コンテンツ版履歴・Admin Studio 履歴 UI と、公開版・作業版の分離を追加。作業版の保存と履歴追加・公開ポインター変更は同一トランザクション。SQLite / PostgreSQL / MySQL 対応。公開中の通常保存は公開ページに反映せず、「公開版を更新」で反映する。既存の公開中コンテンツは起動時に初期公開スナップショットへ移行する。`task check`（workspace tests、Clippy、Admin unit 90件、Storybook 39件、build）と Content Editor E2E 4件が通過。設計は [docs/architecture.md](docs/architecture.md#2-コンテンツと-db) を参照。

**次に着手する項目**

1. 下の「セキュリティ」の 2 件

---

## 未着手タスク

### セキュリティ

- [ ] **カスタム HTML ブロック（`core/html`）の扱い**: `HtmlRenderer` がそのまま出力し、`<script>`・イベント属性も公開ページに出る（`editor` が書ける）。サニタイズする（許可リスト方式）か、`admin` だけに許すか、`[security]` の設定で切り替えるかを決める。
- [ ] **パッケージ署名の既定値**: `signature.json` の `target` の既定が `manifest`（Wasm を守らない）で、`trusted_public_keys` が空だとどの鍵でも通る。既定を `package` にする・`require_signatures` 時は信頼する鍵を必須にする、などを検討（[plugin_hooks.md 5.4](docs/plugin_hooks.md#54-zip-パッケージからのインストールと署名)）。

### 性能

- **監査ログの `database` 出力先の非同期化（保留）**: 有限の `mpsc` キュー + まとめて INSERT。終了時に残りを書き切る処理と、溢れたときの扱い（監査ログの取りこぼしをどこまで許すか）を決めてから。WAL 化で当面は足りている。
- **記事 1 万件での改善候補**（`task bench:seed`。SQLite・WAL・並行 50 で記事ページ 約 2,900 req/s、トップ 約 1,060、一覧 API 約 400。SQL は 1ms 未満で、時間の大半は描画）:
  - `official-code-enhancer` を外すと記事ページ 約 6,400・トップ 約 2,100 req/s。`on-render-page` でページの HTML 全体を Wasm に渡すコストを減らす（`task bench:plugins`）。
  - 一覧 API の応答が約 400KB（全件の `blocks` を含む）。一覧では `blocks` を省く・`fields` で選べるようにする。
  - コンテンツの一覧は `OFFSET` 方式。必要ならキーセット方式に（掲示板は済み）。SQLite の `cache_size` / `mmap_size` の影響も未確認。

### 認証・アカウント

- [ ] **SAML の拡張**（要望があれば）: IdP のメタデータ URL からの読み込み、暗号化したアサーション、AuthnRequest の署名、シングルログアウト、ECDSA（[authentication.md 8](docs/authentication.md#8-シングルサインオン-saml-20)）。

### Admin Studio

- [ ] **翻訳の続き**: サーバーのエラーメッセージ（API がエラーコードを返す。アカウント登録が先行例）、公開サイトの組み込みテンプレート・カートのスクリプト、翻訳サービス（Crowdin など）と 3 言語目。

### プラグイン

- [ ] **AssemblyScript / TinyGo SDK の追従**: `calculate-order` / `on-order-status-changed` / `send-notification` / `on-content-published` / `check-comment` と、ホスト API `http_fetch` / `kv_get` / `config_get` のラッパー（ABI は [plugin_abi_specification.md](docs/plugin_abi_specification.md)）。

### コメント・掲示板・通知（要望があれば）

- [ ] **コメント**: コメント欄の翻訳（保留）。コメントが 1 件も無いコンテンツの受付の切り替えを、コンテンツの編集画面からもできるようにする（今はコメントの行のボタンか API のみ）。
- [ ] **掲示板**: 承認制・NG ワード（コメントの `moderation` と共有）、通報、新着の通知。
- [ ] **掲示板の組み込みテンプレートの文言**: 英語のみ（上の「翻訳の続き」と一緒に対応）。
- [ ] **通知**: テンプレートの編集履歴、宛先の言語ごとのテンプレート、送信待ちの一括再送、送信の速度の上限。

### 将来

- [ ] **分散ストレージ**: libSQL（Turso）への対応とクラスタ環境の最適化。
- [ ] **On-demand ISR / Cache-Tag Purge**: コンテンツ更新時の CDN キャッシュの無効化（EC 売上レポート・在庫通知とは切り離して保留）。
