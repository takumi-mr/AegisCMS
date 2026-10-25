# AegisCMS TODO

> [!NOTE]
> 作業の再開時はここを読む。各作業の完了時に更新する。
> 完了したタスクは残さない。仕様は `docs/*.md`・`admin/README.md`、設計判断・注意点は [docs/architecture.md](docs/architecture.md)、開発手順は [docs/development.md](docs/development.md) に書いてから消す。経緯は git の履歴（PR）で追う。

## 現在の状況

- 最新の `main`: 掲示板（#92）まで。実装済みの機能は [README 2.3](README.md#23-クレート構成と各モジュールの責務-crates) とそこからのドキュメントの一覧で確認する。
- `task check` がすべて通る状態を保つ。

**次に着手する項目**

1. EC Phase 4（顧客アカウント・売上レポートなど）の設計
2. 下の「セキュリティ」の 2 件

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
- [ ] **Forum タブの権限**: タブは `editor` 以上に出るが、モデレーションの API は `admin` のみ（`editor` は操作すると 403）。どちらに揃えるか決める。

### プラグイン

- [ ] **AssemblyScript / TinyGo SDK の追従**: `calculate-order` / `on-order-status-changed` / `send-notification` / `on-content-published` / `check-comment` と、ホスト API `http_fetch` / `kv_get` / `config_get` のラッパー（ABI は [plugin_abi_specification.md](docs/plugin_abi_specification.md)）。

### EC

- [ ] **Phase 4**: 顧客アカウント（会員登録・注文履歴）、売上レポート、在庫切れの通知、On-demand ISR との連動。
- [ ] **商品レビュー**: 顧客アカウントと一緒に作る。方針は [architecture.md 13](docs/architecture.md#13-コメント-aegis-comments)（別の表 `_aegis_reviews`・審査はコメントと共有）。購入者だけ・1 注文 1 件、JSON-LD の `AggregateRating`。アカウントの無い購入者には注文確認と同じトークン方式のリンクをメールで送る案もある。運用の注意: ステマ規制（2023 年 10 月施行）、低評価だけを非承認にしない、非承認の基準を残す。
- [ ] **返金時の在庫の二重戻し**: 取り消しで在庫を戻した注文を返金すると、さらに戻る（`orders_integration.rs`、#79 で期待値を 5 → 7 に変更）。取り消し済みの注文の返金では戻さないのが正しいか確かめる（`record_order_refund` の `restock`）。

### コメント・掲示板・通知（要望があれば）

- [ ] **コメント**: コンテンツごとの受付の切り替え、Admin Studio からの本文の編集、アバター、承認時の投稿者への通知、コメント欄の翻訳、同じ内容の連投の検出。
- [ ] **掲示板**: 承認制・NG ワード（コメントの `moderation` と共有）、通報、新着の通知。
- [ ] **通知**: テンプレートの編集履歴、宛先の言語ごとのテンプレート、送信待ちの一括再送、送信の速度の上限。

### 将来

- [ ] **分散ストレージ**: libSQL（Turso）への対応とクラスタ環境の最適化。
- [ ] **On-demand ISR / Cache-Tag Purge**: コンテンツ更新時の CDN キャッシュの無効化。
