# AegisCMS TODO

> [!NOTE]
> 作業の再開時はここを読む。各作業の完了時に更新する。
> 完了したタスクは残さない。仕様は `docs/*.md`・`admin/README.md`、設計判断・注意点は [docs/architecture.md](docs/architecture.md)、開発手順は [docs/development.md](docs/development.md) に書いてから消す。経緯は git の履歴（PR）で追う。

## 現在の状況
- `task check` がすべて通る状態を保つ。

## 技術メモ・申し送り事項
- 依存パッケージの一括更新（Dependabot #78 の tokio 1.54.1 追従を含む）:
  - 互換性維持と破壊的変更回避を優先し、マイナー・パッチバージョンの最新版へ安全に更新。
  - Rust: tokio (1.54.1), lol_html (3.0.3), uuid (1.29.0), rustix (1.1.6), icu_* (2.4.0) 等 21 パッケージを更新。
  - 各クレート（crates/*、plugins/*）で個別指定されていた外部クレートをすべてルートの [workspace.dependencies] に集約（workspace = true 化）し、バージョン指定を実態（major.minor）に合わせて統一。
  - Admin Studio: i18next (26.5.11), react-i18next (17.1.9), lucide-react (1.63.0), oxlint (1.90.0), storybook (10.6.5), @playwright/test (1.64.1) を更新。
- パッケージ署名検証の強化（`aegis_core::package`）:
  - `signature.json` の `target` の既定値を `"package"` に変更し、Wasm やテンプレート等の改ざんを許してしまう `"manifest"` ターゲットを廃止（`"package"` のみ受付）。
  - `require_signatures = true` のときは `trusted_public_keys`（32 バイト hex 公開鍵 1 件以上）を必須とし、起動時の設定検証（`PluginsConfig::validate` / `ThemeConfig::validate`）および `verify_package_signature` の双方で空リストを拒否。
- 共通ページヘッダー管理とブラウザー向け防御ヘッダー（F-03 対応）を追加。
  - 全レスポンスに適用される `security_headers_middleware` を導入し、デフォルトで安全なブラウザー防御ヘッダー（`Content-Security-Policy`、`X-Content-Type-Options: nosniff`、`X-Frame-Options: SAMEORIGIN`、`Referrer-Policy: strict-origin-when-cross-origin`、`Permissions-Policy`、`Cross-Origin-Opener-Policy: same-origin`）を付与。Admin Studio の「設定 › ヘッダー」から個別の防御ヘッダー設定や任意のカスタム HTTP ヘッダー（名前・値）を動的に追加・編集・削除可能。
  - 設定は `_aegis_settings` に永続化されサーバー再起動不要で即時反映、監査ログ `headers.update` を記録。
- コンテンツ版履歴・Admin Studio 履歴 UI と、公開版・作業版の分離を追加。
    - 作業版の保存と履歴追加・公開ポインター変更は同一トランザクション。
    - SQLite / PostgreSQL / MySQL 対応。公開中の通常保存は公開ページに反映せず、「公開版を更新」で反映する。
    - 既存の公開中コンテンツは起動時に初期公開スナップショットへ移行する。
    - 設計は [docs/architecture.md](docs/architecture.md#2-コンテンツと-db) を参照。
- カスタム HTML ブロック（`core/html`）のハイブリッド保護（F-01 対応）を追加。
    - 既定の `sanitized` モードでは `aegis-ast` の `lol_html` サニタイザーにより `<script>`・`<iframe>`・`on*` イベント属性・`javascript:` URL 等を除去してテーマ内に安全に直展開し、`attributes.mode = "sandbox"` モードでは `allow-same-origin` を持たない不透明オリジン（`Origin: null`）の `<iframe sandbox="allow-scripts allow-forms allow-popups">`（`ResizeObserver` による高さ自動追従付き）に隔離して独自 JS の実行とゼロトラスト境界を両立。
    - Admin Studio の `BlockItem` モード選択 UI、日英翻訳、Markdown の ```` ```html:sandbox ```` 変換を追加。
- On-demand Cache-Tag Purge（`aegis_cache::cdn`, `aegis-server/src/cdn.rs`）を追加。
    - バックエンドでは HTML ページキャッシュを持たず、`[cdn].enabled = true` のときに公開レスポンスへ `Cache-Tag`（カンマ区切り）と `Surrogate-Key`（スペース区切り）および `Cache-Control` を自動付与。
    - マルチ CDN 抽象化（`CdnPurger` トレイト: `none`, `log`, `cloudflare`, `fastly`, `webhook`）を `aegis-cache` に実装し、コンテンツの作成・更新（旧スラッグ含む）・削除・予約公開/非公開・バリエーション変更・コメント/レビュー/掲示板更新・サイト/SEO/テーマ/ヘッダー設定変更時にタグベースで自動パージ。設計は [docs/architecture.md](docs/architecture.md#37-on-demand-cache-tag-purge-aegis-cachesrccdnrs-aegis-serversrccdnrs) および [docs/configuration.md](docs/configuration.md#217-cdn--on-demand-cache-tag-purge-cdn-エッジキャッシュ連携) を参照。

- コンテンツ編集画面からのコメント受付切り替えと、掲示板（`aegis-forum`）のモデレーション・通報・新着通知を追加。
    - コメント対象コンテンツ（`[comments].content_types`）の編集画面および商品編集画面にコメント受付状態（受付中 / 受付停止中）の切り替えボタンを追加し、コメントが 0 件のコンテンツでも編集画面から直接開閉可能に（`ContentResponse.comments_open` および `GET /api/v1/contents/{type}/{id}/comments/state` を追加）。
    - 掲示板に `aegis_comments::moderation`（承認制 `require_approval`・既知の投稿者の自動承認 `auto_approve_known`・NG ワード `hold_words` / `spam_words`・リンク数上限 `max_links`・IP レート制限 `max_per_ip_per_hour`）を共有統合。スレッド・投稿ごとの `status`（`approved` / `pending` / `spam`）管理、通報機能（`_aegis_forum_reports`、公開通報フォーム、管理 API）、および運営者向け通知（`forum.posted` / `forum.pending` / `forum.reported` の日英組み込みテンプレート）を追加。

**次に着手する項目**

1. 依存ライブラリの段階的メジャーバージョンアップ:
   - [x] フェーズ 1: Admin Studio (TypeScript 7, @types/node 26)
   - [x] フェーズ 2: パーサー・ユーティリティ系 (toml 1.1, zip 9.1, syn 3.0, minijinja 3.0)
   - [x] フェーズ 3: ネットワーク・Web・サーバー系 (reqwest 0.13, tower-http 0.7, redis 1.8)
   - [x] フェーズ 4: 認証・暗号系 (RustCrypto, jsonwebtoken 11.1 等)
   - [x] フェーズ 5: データベース層 (sqlx 0.9)
   - [x] フェーズ 6: Wasm プラグイン実行基盤 (wasmtime 50.0)
2. AssemblyScript / TinyGo SDK の追従、または翻訳の続き

---

## 未着手タスク

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

- [ ] **コメント**: コメント欄の翻訳（保留）。
- [ ] **掲示板の組み込みテンプレートの文言**: 英語のみ（上の「翻訳の続き」と一緒に対応）。
- [ ] **通知**: テンプレートの編集履歴、宛先の言語ごとのテンプレート、送信待ちの一括再送、送信の速度の上限。

### 将来

- [ ] **分散ストレージ**: libSQL（Turso）への対応とクラスタ環境の最適化。
- [ ] **On-demand ISR（保留・検討）**: バックエンド側での HTML キャッシュや静的再生成（ISR）は、現時点では単体描画性能と CDN の Cache-Tag Purge で十分なため保留とし、将来必要になった際に検討する。
