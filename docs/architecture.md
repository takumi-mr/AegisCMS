# AegisCMS 内部設計 (Architecture)

AegisCMS の実装上の設計判断と、コードを変更するときに守るべき前提をまとめた設計書です。設計思想と全体構成は [README](../README.md)、各機能の使い方・API・設定はリファレンス（下表）を参照してください。本書はそれらに書かれていない「なぜそうなっているか」「どこを変えればよいか」を扱います。

| 文書 | 内容 |
| --- | --- |
| [configuration.md](configuration.md) | 設定ファイル（`aegis.toml`）の全項目、リバースプロキシ・サブパス運用 |
| [authentication.md](authentication.md) | ユーザー・ロール・セッション・OIDC・アカウント登録 |
| [audit.md](audit.md) | 監査ログのイベント・API・出力先 |
| [media.md](media.md) | アップロード・メディア一覧・削除 API |
| [commerce.md](commerce.md) | EC（商品・注文・在庫・決済） |
| [notifications.md](notifications.md) | 通知（メール・監査イベントの通知・プラグインのチャネル） |
| [comments.md](comments.md) | コメント（投稿・承認・スパム対策・通知） |
| [search.md](search.md) / [analytics.md](analytics.md) | サイト内検索 / アクセス解析 |
| [contact.md](contact.md) | 問い合わせフォーム（フォームの設定・受信箱・通知） |
| [theme_and_publishing_guide.md](theme_and_publishing_guide.md) / [theme_authoring_guide.md](theme_authoring_guide.md) | 公開ルーティング・テーマエンジン / テーマの作り方 |
| [plugin_sdk_guide.md](plugin_sdk_guide.md) / [plugin_hooks.md](plugin_hooks.md) / [plugin_abi_specification.md](plugin_abi_specification.md) | プラグイン開発 / フックとホスト API / 言語共通の ABI |
| [development.md](development.md) | 開発環境・検証・CI・Docker |
| [admin/README.md](../admin/README.md) | Admin Studio（構成・i18n・設計メモ） |

---

## 1. クレート構成

依存の向きは `aegis-core` ← ドメインクレート ← `aegis-server` の一方向です（クレートの一覧と各モジュールは [README 2.3](../README.md#23-クレート構成と各モジュールの責務-crates)）。

- **`aegis-core` は共有契約だけ**: 抽象トレイト（`StorageBackend` / `CacheBackend` / `AuthProvider` / `AuditSink` / `Notifier`）、監査イベントのモデル、プラグインに渡すコンテキスト型、`PluginManifest`、`AegisError`。Wasm プラグイン SDK も依存するため、実装や重い依存を足さないこと。
- **実装はドメインクレート**: `aegis-db`（アダプタ・注文）、`aegis-storage`、`aegis-cache`、`aegis-audit`（`AuditService`・出力先・メトリクス・`AuditOrigin`）、`aegis-auth`（ユーザー・セッション・レート制限・OIDC・登録。feature `oidc` / `mock-idp`）、`aegis-notify`、`aegis-commerce`、`aegis-comments`、`aegis-forum`、`aegis-theme`、`aegis-runtime`、`aegis-backup`（バックアップ・リストア。feature `backup`）。HTTP には依存しない。
- **`aegis-server` は HTTP 層**: ハンドラ・認可・エクストラクタ・CLI・組み立て（`main.rs`）。
- **設定**: 各クレートが自分のセクションの型を持つ（`aegis_db::DatabaseConfig`、`SecurityConfig` は `aegis-ast`、`ThemeConfig` は `aegis-theme`、`PluginsConfig` / `RuntimeConfig` は `aegis-runtime` など）。`aegis.toml` 全体の `AegisConfig` は `aegis-server/src/config.rs`。
  - 全構造体が `#[serde(default)]` 付きで、省略した項目は `Default` 実装の値になる。**設定項目を足すときは必ず `Default` に既定値を入れる**。
  - `AegisConfig::load_or_default` はファイルが無いときだけ既定値にフォールバックし、パースエラーでは起動を止める（設定ミスに気付かず既定値で動き続けないため）。
- **axum のエクストラクタはサーバー側に置く**: 監査の `AuditContext` は `aegis_audit::AuditOrigin` の newtype（孤児ルールのため）で `Deref` する。ドメインサービスは依存を構造体で受け取る（例: `CommerceService` は `CommerceDeps`、`AppState::commerce_deps()`）。

---

## 2. コンテンツと DB

### 2.1 テーブルとデータ

- スキーマ `content_type: article` のテーブルは `content_article`。本文は Block AST の JSON を `blocks_json TEXT NOT NULL DEFAULT '[]'` に保存する。描画済み HTML は保存しない（`[security]` の変更を保存済みのコンテンツにも反映するため）。
- システムテーブルは `_aegis_` 接頭辞（`_aegis_users`、`_aegis_user_identities`、`_aegis_audit_logs`、`_aegis_orders`、`_aegis_comments`、`_aegis_settings` など）。
- `_aegis_settings` は Admin Studio から変更するサイト全体の設定（`name` → `value`。サイト名 `site.title`、favicon `site.favicon`、SEO & Bot 設定 `seo.*`（`robots_txt`, `meta_description`, `meta_keywords`, `canonical_base_url`, `og_image`, `noindex_all`, `twitter_card`）、HTTP 共通ヘッダー・防御ポリシー `headers.*`（`content_security_policy`, `x_frame_options`, `x_content_type_options`, `referrer_policy`, `permissions_policy`, `cross_origin_opener_policy`, `custom_headers`）、コメントの承認の設定 `comments.require_approval` / `comments.auto_approve_known`）。カラム名は MySQL の予約語 `key` を避けて `name` にした。上書きは SQLite / PostgreSQL が `ON CONFLICT`、MySQL が `ON DUPLICATE KEY UPDATE`。設定ファイルの値は既定値として残し、行を消すと設定ファイルの値に戻る（プラグイン設定のようにキャッシュへ置くと、メモリのキャッシュでは再起動と TTL で消えるため DB にした）。
- コンテンツ作成 API（`POST /api/v1/contents/{type}`）のペイロードはトップレベルに `title` / `slug` / `status` / `blocks` / `author_id` とカスタムフィールドを持つ。応答ではカスタムフィールドは `fields`、解決済みリレーションは `relations` に入る（[Admin Studio の型](../admin/README.md#-設計メモ)に注意）。

### 2.2 `DatabaseAdapter`

- **SQLite は WAL モード**（`synchronous = NORMAL`）で開く。既定の rollback journal では書き込みが読み込みを止め、コミットごとに fsync するため、監査ログ（`database` 出力先はリクエスト中に同期で INSERT する）を書くリクエストが並ぶと詰まった（認証なしの 401 が並行 50 で約 490 req/s → WAL で約 3,900 req/s、p99 610ms → 44ms）。NORMAL は電源断で直前のコミットを失うことがあるが、DB が壊れることはない。
- SQLite / PostgreSQL / MySQL の違い（プレースホルダ `?1` / `$1` / `?`、カラム情報の取得 `PRAGMA table_info` / `information_schema.columns`、接続プール）は `aegis_db::DatabaseAdapter` の実装に閉じ込める。ハンドラはアダプタ経由でのみ DB を扱う。
- **スキーマ進化**: 起動時の `run_migrations` は `CREATE TABLE IF NOT EXISTS` に加え、既存テーブルのカラムを調べて YAML に増えたフィールドを `ALTER TABLE ... ADD COLUMN` してからインデックスを作る。システムテーブルの列追加（例: `_aegis_users.avatar_key`）も同じ方式（SQLite は `PRAGMA table_info`、PostgreSQL は `ADD COLUMN IF NOT EXISTS`、MySQL は `information_schema`）。MySQL は `CREATE INDEX IF NOT EXISTS` が無いため、重複エラー（1061）だけを無視する。
- **カスタムフィールドの型**: `CustomFieldValue::from_json` がスキーマの型に合わせて変換する（jsonb → JSON、integer → i64、boolean → bool、その他は文字列）。変換できない値は `AegisError::Validation`（400）。SQLite は文字列で保存し、`null` は NULL で保存する（以前は空文字になり、数値の計算で 0 扱いになっていた）。読み出しは各アダプタの `custom_fields_from_row` で型ごとにデコードし、NULL は `fields` に含めない。**フィールド型を足すときは `SchemaMigration` の型対応・`CustomFieldValue`・各アダプタのデコードを揃えて変える**。
- **リレーション**: `type: relation` / `target: <schema>.<column>` のフィールドは、応答の `relations` に参照先（`id` / `content_type` / `title` / `slug`）を入れる。解決は `resolve_relations_for_records` がリレーション列ごとに `IN (...)` の 1 回でまとめて行う（単一レコードも同じ関数）。参照の値は `relation_ref`（空文字・文字列以外は参照なし）。
- **コンテンツ版履歴と公開版**: `_aegis_content_versions` に保存時の `ContentRecord` 全体を不変スナップショットとして追加する。revision はコンテンツ ID ごとに単調増加し、`GET /api/v1/contents/{type}/{slug}/versions` で一覧、`POST .../versions/{revision}/restore` で復元する。コンテンツ本体と履歴スナップショットは SQLite / PostgreSQL / MySQL 各アダプタの同一トランザクションで保存し、どちらかが失敗すれば両方をロールバックする。復元時に戻すのはタイトル・slug・本文・カスタムフィールドで、status・公開日時・著者・作成日時は現在値を維持する。
- `_aegis_content_publications` は公開中スナップショットの revision を指し、編集可能なコンテンツ行とは分離する。公開中の記事を通常保存すると作業版と履歴だけが更新され、公開ページ・公開記事一覧・サイトマップは公開ポインターのスナップショットを読む。「公開版を更新」操作または初回公開でポインターを進める。作業版保存・履歴追加・公開操作は同一トランザクションで行う。起動時に既存の `published` レコードがポインター未作成なら初期公開スナップショットを作る。
- **一覧**: `list_contents_page`（`ContentListQuery`: status / limit / offset、`ORDER BY created_at DESC, id DESC`）で取得し、総件数は同じ条件の `COUNT(*)`。インデックス `(created_at, id)` / `(status, created_at, id)` で並べ替えを避ける。全件が要る処理（メディアの使用中チェック）だけ `list_contents` を使う。
- **注文**: 注文の DB 操作は `OrderStorage` トレイト（`DatabaseAdapter` のスーパートレイト）。`#[async_trait]` はマクロが impl ブロック内に生成したメソッドを変換しないため、`impl_order_storage!` が `#[async_trait] impl OrderStorage for ...` ごと生成する。
  - **ダイアレクト共通の SQL 構築と実機検証**: プレースホルダ（SQLite: `?1`, PostgreSQL: `$1`, MySQL: `?`）や型キャスト、排他制御（注文作成と在庫減算のアトミックトランザクション、キャンセル時の restock 復帰）は SQLite だけでなく、Docker コンテナを用いた PostgreSQL 16 および MySQL 8.0 の実機環境で統合テスト（ストレージ層: `orders_integration`、E2E: `shop_tests`）を実施・パス済み。
  - **テスト環境設計**: 専用 Compose（`docker-compose.test-dbs.yml`）と `task test:dbs` によりヘルスチェック待機付きで一括実行可能。CI や通常の開発時（コンテナ非稼働時）は高速 TCP プローブ（150ms）により即座にスキップされ、`cargo test` を阻害しない。

### 2.3 スキーマからのコード生成とシステム標準カラム

- **システム標準カラムの分離**: `id`, `content_type`, `title`, `slug`, `status`, `blocks`, `blocks_json`, `author_id`, `published_at`, `unpublish_at`, `created_at`, `updated_at` は全コンテンツ共通の基本属性であり、DB テーブル作成時および各種ハンドラで最初から提供される。そのため `schemas/*.yaml` にこれらを記述する必要はなく、カスタムフィールド（`excerpt`, `price`, `layout` 等）のみを記述する。
- **コード生成 (`task codegen`)**: `schemas/*.yaml` から TypeScript 型（`admin/src/types/schemas.ts`）と OpenAPI 3.1（`docs/openapi.json`）を生成する。生成器はシステム標準カラムを自動的に付与し、YAML 側の `fields` に重複定義があっても自動でスキップする。稼働中は `GET /api/v1/openapi.json` でも最新仕様を配信する。

---


## 3. レンダリングと公開

### 3.1 HTML の生成経路 (`aegis-server/src/render.rs`)

- 保存（`transform_blocks` → `render_blocks_html`）、公開ページ・コンテンツ API（保存済みブロックを `render_blocks_html`）、エディタのプレビュー（`transform_blocks` → `render_blocks_html`）はすべてこのモジュールを通る。**描画の手順を変えるときはここだけを変える**。
- `render_blocks_html` は `HtmlRenderer`（AST → HTML）の出力を `StreamingHtmlRewriter`（`lol-html`。外部リンクの `rel`、`<img>` の `loading` / `decoding`。[`[security]`](configuration.md#27-security--html-出力のセキュリティ--最適化)）に通す。
- テーマ適用済みのページ全体は `render::render_page`（リレーション解決 → `on-render-page` → テンプレート）。公開ページとプレビューの `page_html` が共用する。
- プレビューは編集のたびに `transform-content-ast` を呼ぶため、このフックは副作用なしが前提（[plugin_hooks.md](plugin_hooks.md)）。
- **ブロックの種類と Markdown の対応**は [blocks.md](blocks.md)。テキストのインライン書式（`aegis_ast::inline`）は描画時に変換し、保存するテキストは Markdown の記号のまま（Admin Studio の Markdown 編集と往復できるように）。HTML のタグは書けず、リンクの URL はスキームの許可リストで絞る。
- **プラグインのカスタムブロック**（`<プラグイン名>/<ブロック名>`）は `render::render_content` が `HtmlRenderer` の前に解決する。そのプラグインのブロックをまとめて 1 回の `render-blocks` で HTML にし、`core/html` の標準モードと同じサニタイズ（そのプラグインの `embed.frame_domains` の `<iframe>` だけは残す。`sanitize_html_fragment_with_frames`）を掛けてから、`HtmlRenderer::render_nodes_with` で文書の中に差し込む（目次などは文書全体を見たまま）。リンク・画像のリライターは他のブロックと同じく後から掛かる。保存せずに表示のたびに呼ぶのは、プラグインの更新や設定の変更をすぐ反映するため（Wasm の呼び出しはインスタンスのプールで軽くし、ページは CDN でキャッシュできる）。HTML を描画する場所（公開ページ・フィード・コンテンツ API・プレビュー）はすべて `render_content` を通す。`render_blocks_html` を直接使うのは、プラグインを呼ばない描画だけ。
- **目次（`core/toc`）は保存せず描画のたびに作る**。`HtmlRenderer::render_nodes` が文書全体の見出しから組み立てるので、見出しを直せば目次も追従する。リンク先の見出しの `id` は `HeadingIdTransformer` が保存時に付け、文書内で重ならないよう `-2` などを足す（既にある `id` は変えないので、公開済みのリンクは切れない）。
- 表・目次・注記の最低限の CSS は `aegis_theme::BLOCK_STYLES`（`:where()` で詳細度 0）を全ページの `extra_head` に入れる。テーマごとに対応しなくても崩れず、テーマの CSS がいつも優先される。
- **カスタム HTML ブロック（`core/html`）は `HtmlRenderer` がそのまま出力する**。後段のリライターはリンク・画像の属性を足すだけで、`<script>` やイベント属性は取り除かない（書けるのは `editor` 以上）。
- **固定ページのレイアウト**: スキーマ `page` の既存の `layout` フィールドを使い（マイグレーション不要）、公開ページ（`render_page_or_slug`）とプレビューで同じ規則でテンプレートを選ぶ: `blank` → 組み込みの `blank.html`、空・`default` → `page.html`、それ以外 → テーマの `<layout>.html`。

### 3.2 テーマエンジン (`aegis-theme`)

- テンプレートの探索は「`register_template` で登録したもの」→「`themes/{active}/templates/`」→「バイナリに埋め込んだ組み込みテンプレート」の順。
- `ThemeEngine` は内部状態を `RwLock` で持ち、`set_active_theme` で再起動せずにテーマを切り替える。コンパイル済みの MiniJinja 環境は再利用し、テーマの切り替え・テンプレートの登録で破棄する（`[theme].reload_templates = true` なら毎回読み直す）。
- フィルター（`date_short`、`truncate` など）は引数を `&str` ではなく `minijinja::Value` で受ける。`&str` だと `None` や未定義の値で `value is not a string` の実行時エラー（500）になる。
- テーマパッケージ（`theme.json`・`preview.*`・`templates/`・`assets/`）の検証は `ThemeEngine::validate_theme_package`。ZIP 導入（`POST /api/v1/themes/install`）は署名の方針を加えた `validate_theme_package_with_policy` で検証してから `themes/<id>/` に置く（[10.2](#102-インストールと有効化-wasmpluginloader) のパッケージの共通部分を使う）。
- アセット（`GET /theme/assets/{*path}`）はアクティブテーマの `assets/` からリクエスト時に解決し、`..` や絶対パスを拒否する。MIME は `aegis_core::guess_content_type`（拡張子から。`text/css` でないと CSS がブラウザに拒否される）。
- サイト全体の値（サイト名・説明・favicon・`custom_css`）は `render::site_page_context` でまとめて作り、公開ページ・一覧・404・ショップの各ページはそこから組み立てる。サイト名と favicon は `AppState::site()`（Admin Studio の値 → `[theme]`）で、起動時に `_aegis_settings` から読み、`PUT /api/v1/site` で書き換える（再起動不要）。favicon は新しいテンプレート変数ではなく `extra_head` に `<link rel="icon">` として入れる（既存のテーマがすべて出力しているため、テーマの改修が要らない。プラグインの ABI の `PageRenderContext` も変えずに済む）。

### 3.3 サブパス公開 (`[server].base_path`)

- ルーターはすべて `/` 起点のまま、`app::with_base_path` が外側でベースパスを取り除いてから渡す（ベースパスの無いリクエストもそのまま受け付ける。ヘルスチェックやパスを取り除くプロキシのため）。**ルートを足すときにベースパスを意識する必要はない**。
- 逆に、サーバーが**出力する** URL にはベースパスを付ける。Rust では `app::under_base_path`、テンプレートではグローバル変数 `{{ base_path }}`（`ThemeEngine::with_base_path`）。対象: テーマのリンク、ローカルストレージの公開 URL、`/admin` のリダイレクト、OIDC の `post_login_redirect`。
- 値は `ServerConfig::normalized_base_path` で正規化・検証（URL で安全な文字のみ）するので、テンプレートでは自動エスケープしない（`Value::from_safe_string`）。
- アップロード URL はベースパス込みで本文に保存されるため、運用中に `base_path` を変えると既存の画像が切れる。必要になったら「保存は相対・描画時に付与」へ変える。

### 3.4 予約公開・予約非公開化・スケジュール管理 (`aegis-server/src/scheduler.rs`, `handlers/content.rs`)

使い方は [theme_and_publishing_guide.md](theme_and_publishing_guide.md#予約公開スケジュール公開)。

- **予約公開（`published_at`）**:
  - 予約日時は既存の `published_at` 列に UTC（`...Z`、秒精度）で保存する。スキーマ・マイグレーションの変更は無く、公開後もその日時が公開日になる。
  - 公開側（`/`、`/articles/{slug}`、`/{slug}`、`/products`、カート）は `status == "published"` だけを見るので、`scheduled` は何もしなくても隠れる。
  - 公開は `DatabaseAdapter::publish_scheduled_content` の `UPDATE ... WHERE id = ? AND status = 'scheduled' AND published_at = ?`。一覧を取った後に編集者が日時を変えた・下書きに戻した場合や、複数台のサーバーが同時に動いた場合でも、古い予約で公開したり監査ログ `content.publish` を二重に残したりしない。
  - 公開成功時、監査ログ `content.publish` を記録した後にプラグインフック `on-content-published`（`PublishedContent`）を呼び出す。外部 Webhook や SNS 自動投稿、静的サイト再ビルドとの連携を可能にし、プラグインのエラーは `plugin.error` に記録して公開自体はロールバックしない。
  - 保存時、`published` の `published_at` は「今の状態が `published` のときだけ」引き継ぐ（`scheduled` の未来の日時を引き継がないため）。
- **予約非公開化（`unpublish_at`）**:
  - `ContentRecord` および各スキーマテーブルに `unpublish_at` 列（UTC RFC 3339 文字列）を追加（既存テーブルは起動時マイグレーションで自動追加）。
  - バリデーション: `unpublish_at` は公開日時（予約公開の `published_at` または現在時刻）より未来である必要がある（`unpublish_at <= published_at` は 400 Bad Request）。下書き（`draft`）やアーカイブ（`archived`）へ変更された場合は自動でクリアされる。
  - リクエスト時防御: 公開側（`public.rs`, `storefront.rs`）では、DB上のスケジューラ実行を待たずに `unpublish_at <= now` のコンテンツを即座に 404 と判定する。
  - スケジューラ処理: 30 秒ごとに `DatabaseAdapter::unpublish_scheduled_content` で期限を迎えたコンテンツをアトミックに `status = 'archived'` へ更新し、監査ログ `content.unpublish` を記録する。
- **公開スケジュール一覧 API (`GET /api/v1/schedule`)**:
  - 全コンテンツタイプ（記事・商品等）を横断し、予約公開中（`scheduled`）、非公開予約あり、または公開中のコンテンツを日時順（`published_at` / `unpublish_at`）で集約して返す。
  - Admin Studio のカレンダービュー・アジェンダビューのデータソースとなる。
- 確認間隔は定数 `PUBLISH_INTERVAL`（30 秒）。設定にしたくなったら `[server]` などに足す。

### 3.5 SEO & Bot 制御 (`aegis-server/src/seo.rs`, `handlers/seo.rs`)

- **設計思想**:
  - テーマのベーステンプレート（`base.html`）は `{{ extra_head | safe }}` を `<head>` 内に出力する規約を採用しており、SEO メタタグ・OGP・Twitter Card・Canonical タグ・noindex 指示は `render::build_extra_head` により自動構築されて `extra_head` に挿入される。既存テーマを改修することなく、全公開ページ・個別商品・記事へ透過的に適用される。
  - 設定値は `_aegis_settings` に永続化され、`AppState.seo`（`Arc<RwLock<SeoOverrides>>`）を通じてサーバー再起動不要で即時反映される。設定変更時は監査ログ `seo.update` を記録する。
- **エンドポイント**:
  - `GET /robots.txt`:
    - 管理画面でカスタマイズされた `robots.txt` を配信する。未設定時は安全なデフォルト（`/admin/`, `/api/` を Disallow、Sitemap への参照を含む）を自動生成する。
    - リクエストの `Host` ヘッダーまたは設定済みの Canonical Base URL を参照し、`Sitemap: http(s)://.../sitemap.xml` を動的に注入する。
  - `GET /sitemap.xml`:
    - 公開中のコンテンツ（記事、固定ページ、商品など `status == "published"` かつ未期限切れ）を DB から動的集約し、Sitemap 0.9 準拠の XML を配信する。
    - 各 URL の `<lastmod>` にはコンテンツの更新日時（`updated_at`）を使用する。
  - `GET /api/v1/seo` / `PUT /api/v1/seo`:
    - 管理画面（Admin Studio）から SEO・Bot 設定の取得・更新を行う REST API。
    - バリデーション: `canonical_base_url` は空または有効な HTTP/HTTPS URL、`robots_txt` のサイズ制限（最大 64 KB）、`twitter_card` は `"summary"` または `"summary_large_image"`。
- **メタタグ自動注入の優先順位**:
  - ページ個別設定（個別記事の `excerpt` や商品のサムネイル画像）が存在する場合はそれを優先し、未設定の場合はサイト全体の共通設定（`meta_description`, `og_image`）へフォールバックする。
  - サイト全体で `noindex_all = true` が有効化されている場合、全ページの `<head>` に `<meta name="robots" content="noindex, nofollow">` が強制注入される（ステージング環境や構築中サイトの検索流入防止）。

### 3.6 共通ページヘッダー & ブラウザー防御ポリシー (`aegis-server/src/headers.rs`, `handlers/headers.rs`)

- **設計思想**:
  - OWASP ZAP / ペネトレーションテスト（F-03）に対応し、公開サイトの全ページ（トップ、記事、固定ページ、フォーラム、EC）および Admin Studio、API を含む全 HTTP レスポンスに対して、デフォルトで安全なブラウザー防御ヘッダーを付与する。
  - レイヤーミドルウェア `security_headers_middleware`（`from_fn_with_state`）により、ルーター通過時に透過的にヘッダーを注入する。
  - 管理者（`admin`）は Admin Studio の「設定 › ヘッダー」画面から、防御ヘッダー（CSP、X-Frame-Options、nosniff 等）のカスタマイズや任意のカスタム HTTP ヘッダーの動的追加・編集・削除が行える。
- **デフォルト防御ヘッダー値**:
  - 有効なプラグインの `capabilities.embed.frame_domains` は、ミドルウェアがレスポンスごとに CSP の `frame-src` に足す（`headers::with_frame_sources`。管理者の CSP の値は書き換えず、効いている `frame-src` / `child-src` / `default-src` から作る）。プラグインの有効・無効を切り替えると次のレスポンスから変わる。
  - `Content-Security-Policy`: SPA（Vite/React）やテーマのインラインスクリプト・フォント・メディアを壊さない安全なディレクティブ (`default-src 'self'; script-src 'self' 'unsafe-inline' 'unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https:; font-src 'self' data: https:; connect-src 'self'; frame-ancestors 'self'; object-src 'none'; base-uri 'self'; form-action 'self';`)。
  - `X-Frame-Options: SAMEORIGIN`（クリックジャッキング保護）
  - `X-Content-Type-Options: nosniff`（MIME スニッフィング防止）
  - `Referrer-Policy: strict-origin-when-cross-origin`（リファラー漏洩防止）
  - `Permissions-Policy: camera=(), microphone=(), geolocation=()`（不要なブラウザAPIの制限）
  - `Cross-Origin-Opener-Policy: same-origin`（COOP ポップアップ分離）
- **DB 永続化と即時反映**:
  - `_aegis_settings` に `headers.*` キーで保存され、`AppState.headers`（`Arc<RwLock<HeaderOverrides>>`）を通じてサーバー再起動不要で即時反映される。設定変更時は監査ログ `headers.update` を記録する。
  - フィールドを空にするとデフォルト値に戻る。特定のヘッダーを意図的に無効化したい場合は `"disabled"` を指定することでヘッダーの送出を停止できる。
- **エンドポイント**:
  - `GET /api/v1/headers`: 現在の設定値とデフォルト値を返す（admin 必須）。
  - `PUT /api/v1/headers`: 設定値を更新する（admin 必須。CRLF インジェクションや不正文字のバリデーションを実施）。

### 3.6a トップページ・記事一覧・フィード (`handlers/public.rs`, `handlers/feed.rs`)

- トップページ（`/`）は `[theme].front_page` で記事一覧・テーマの `front.html`・固定ページを切り替える。設定値は起動時に検証する（`ThemeConfig::validate`）。サイトの構成はコードで管理する方針なので、Admin Studio の設定ではなく `aegis.toml` に置いた。`front.html` の組み込みは `index.html` を継承するだけなので、`"template"` にしてもテーマが対応していなければ表示は変わらない。
- 固定ページを `/` に出すときは canonical・OGP の URL を `/` にし（`PageInput.path`）、そのページ自身の URL（`/{slug}`）でも同じ URL を指す。
- 記事一覧・カテゴリの一覧・フィードは同じ取得処理（`list_published_articles`）を使う。カテゴリの絞り込みは記事のスキーマの `target: category.<フィールド>` の関係フィールドで行う。公開版の一覧は DB から全件を読んでから並べ替えている（`merge_published_content_records`）ので、絞り込みもその後に Rust 側で行い、`DatabaseAdapter` には条件を足していない。記事が多くなり一覧を SQL でページングするようにするときは、絞り込みも SQL に移す。
- フィードは Atom 1.0。エントリの `<id>` は記事の ID の URL（`/articles/{id}`。`get_published_content` はスラッグと ID のどちらでも引ける）にして、スラッグを変えても購読者に別の記事として届かないようにした。自動検出用の `<link>` は `render::build_extra_head` で全ページの `extra_head` に入れるので、テーマ側の対応は要らない。

### 3.7 On-demand Cache-Tag Purge (`aegis-cache/src/cdn.rs`, `aegis-server/src/cdn.rs`)

- **設計思想（バックエンド HTML キャッシュレス + エッジ Cache-Tag パージ）**:
  - ベンチマーク上、バックエンド単体で十分な描画スループット（記事ページ数千〜1.6万 req/s）が出ているため、バックエンドプロセスや Redis 内に描画済み HTML のページキャッシュは持たず（二重キャッシュによる古いページの残留やメモリ圧迫を回避）、前段の CDN / エッジプロキシとタグベースで連携する方式を採用した。
  - クレートは新設せず既存の `aegis-cache`（`aegis_cache::cdn`）に抽象トレイト `CdnPurger`・`CacheTags`・各プロバイダ実装（`NoopCdnPurger`, `LogCdnPurger`, `CloudflareCdnPurger`, `FastlyCdnPurger`, `WebhookCdnPurger`, `RecordingCdnPurger`）を置き、Axum の `Response` / `AppState` と結合するヘルパーを `aegis-server/src/cdn.rs` に分離した。
  - `[cdn].enabled = false`（既定）のときはヘッダー生成もパージ通信も一切行わず、CDN を使わない構成でオーバーヘッド 0 となる。
- **マルチ CDN 対応のデュアルヘッダー出力 (`CacheTags`)**:
  - `CacheTags::write_headers` は、1 つのタグ集合から **`Cache-Tag`**（カンマ区切り: Cloudflare, BunnyCDN, Netlify 等）と **`Surrogate-Key`**（スペース区切り: Fastly, Varnish, Google Cloud CDN 等）の両ヘッダーを同時に注入する（`200 OK` の公開レスポンスのみ。404 等には付与しない）。
  - タグ文字列の正規化（`sanitize_tag`）: URL パス（`add_url`）はクエリ文字列を除き `/` を `-` に変換（`/` は `url-index`、`/articles/hello` は `url-articles-hello`）。日本語スラッグ等の非 ASCII 文字やスペース・カンマが含まれる場合は `:x<hex>` で安全にエスケープし、`HeaderValue` の生成失敗やタグ区切り文字の衝突を防ぐ。
- **付与されるタグ体系と自動パージのトリガー**:
  - トップ・一覧（`/`）：`url-index`, `url-articles`, `site-global`, `collection-article`, 表示中の各記事の `article-{id}`, `article-{slug}`, および参照先リレーションの `{rel_type}-{id}` / `{rel_type}-{slug}`。
  - カテゴリの記事一覧（`/category/{slug}`）・フィード（`/feed.xml`, `/category/{slug}/feed.xml`）：対応する `url-*`, `site-global`, `collection-article`, 載せた各記事の `article-{id}` / `article-{slug}`、カテゴリでは `category-{id}` / `category-{slug}`。
  - トップページに固定ページを出している場合（`[theme].front_page = "page:<slug>"`）、`/` にはそのページの `page-{id}` / `page-{slug}` が付くので、ページの更新で `/` もパージされる。
  - 個別記事（`/articles/{slug}`）・固定ページ（`/{slug}`）・商品（`/products`, `/products/{slug}`）：対応する `url-*`, `site-global`, `{type}-{id}`, `{type}-{slug}`, リレーションタグ、`comments-{id}`, `reviews-{id}`。
  - SEO・テーマ・掲示板：`url-robots.txt` (`seo-robots`), `url-sitemap.xml` (`seo-sitemap`), `theme-{active}`, `collection-forum`, `forum-category-{slug}`, `forum-thread-{id}`。
  - **パージ発火タイミング**:
    - コンテンツの作成・更新・削除・バリエーション変更（公開状態に関わる場合）、およびスケジューラによる予約公開（`publish_due_contents`）・予約非公開（`unpublish_due_contents`）の成立時に、対象コンテンツの ID・新旧スラッグ・`collection-{type}`・`seo-sitemap`・関連 URL タグを一括パージする。
    - サイト設定（`PUT /api/v1/site`）、テーマ切り替え（`PUT /api/v1/themes/active`）、共通ヘッダー更新（`PUT /api/v1/headers`）、SEO 設定更新（`PUT /api/v1/seo`）時は `site-global`（および `seo-*`）をパージする。
    - コメント・商品レビュー・フォーラム投稿が公開状態で追加または管理変更された際は `comments-{id}` / `reviews-{id}` / `collection-forum` 等をパージする。
  - 外部 CDN API の一時的な通信エラーは警告ログに記録し、CMS 側の保存・公開リクエスト自体は失敗させない。

---

## 4. メディア・ストレージ

- `StorageBackend`（`put` / `get` / `delete` / `exists` / `list` / `public_url`）で、ローカル（`LocalStorageBackend`。パストラバーサル防止）と S3 互換（`S3StorageBackend`。AWS SDK を使わず SigV4 を自前実装）を切り替える。
- `exists` の既定実装はキーを prefix にした `list` の完全一致（ローカルは `metadata` で上書き）。S3 の DELETE は存在しないキーでも成功するため、404 を返すには事前の確認が要る。
- 一覧（`GET /api/v1/storage/objects`）: ストレージに並び替え・絞り込みが無いため、最大 10,000 件（`MAX_LIST_SCAN`）を走査してからメモリ上で絞り込み・新しい順・ページングを行い、超過は `truncated: true`。mime type は保存していないので拡張子から推定。ローカルは隠しファイル・シンボリックリンク、S3 は末尾 `/` のオブジェクトを除外。アバター画像（`avatars/`）はメディアライブラリの目的外かつプライバシー保護のため、`prefix` で明示的に `avatars` を指定するか `include_avatars = true` を指定しない限り除外される。
- 使用中の判定（削除時）: 全スキーマの `list_contents` を読み、`blocks_json` とカスタムフィールドの JSON に公開 URL が含まれるかの文字列一致、および全ユーザーの `avatar_key` の完全一致を検査する。コンテンツまたはユーザーのアバターとして使用中の場合は参照元（コンテンツまたは `content_type: "user"`）を返して 409 Conflict で拒否する（使用中で拒否した 409 は監査に記録しない）。`force=true` で強制削除された場合は、参照していたユーザーの `avatar_key` を自動的にクリア（`None`）して孤立参照やリンク切れを防ぐ。遅くなったら保存時に参照 URL を別テーブルに記録する方式にする。
- サイズ上限: axum の `Multipart` は既定で 2 MB。`/api/v1/storage/upload` には `DefaultBodyLimit::max([storage].max_upload_mb)` を個別に設定している（プロキシ側の上限も合わせる）。アバターは `DefaultBodyLimit` を 2 MB + 64 KB（multipart のヘッダー分）にし、画像自体の 2 MB はハンドラで確かめる。

---

## 5. キャッシュとセッション

- `CacheBackend`（`get` / `set` / `delete` / `exists` / `increment`）。`memory`（既定。追加のミドルウェア無しで動く）と `redis`（`ConnectionManager` で自動再接続、`[cache.redis].timeout_ms`）。
- `MemoryCacheBackend` は読まれないまま期限切れになったキーが溜まらないよう、書き込み時にエントリ数が閾値（掃除後の 2 倍、最小 1024）を超えたら期限切れを一括削除する。
- セッションは `SessionData` を JSON にしてキャッシュに置く（DB には保存しない）。トークンは `ses_{UUIDv4}`、`Authorization: Bearer` または `X-Session-Id` から取り出す。Redis にすればサーバープロセスはステートレスに水平スケールできる。
- キャッシュに置いているもの: セッション、ログイン試行の計数、OIDC の state / nonce / PKCE（10 分）、アカウント登録の申し込み、プラグインの KV。`memory` では再起動で消える。
- `RedisCacheBackend` の `get` / `set` / `delete` / `exists` は、接続のエラー（`is_connection_dropped` / `is_io_error`）のときだけ 1 回送り直す。`ConnectionManager` は切断後の最初のコマンドをエラーにしてから繋ぎ直すため（Redis の再起動直後の 1 リクエストが失敗しないように）。`increment` は二重に数えるおそれがあるので送り直さない。
- **イベントバス**（`aegis_cache::EventBus`、`aegis-server/src/cluster.rs`）: Admin Studio での変更をサーバー間で知らせる。`[cache]` と同じバックエンドで、`MemoryEventBus` は `tokio::sync::broadcast`、`RedisEventBus` は Pub/Sub（チャネル `aegis:events`、送信は `ConnectionManager`、購読は専用の接続）。どちらも**送ったサーバー自身にも届く**ので、受け取る側は「知らせが来たら共有の状態を読み直し、違っていれば差し替える」だけにしてあり（知らせの中身は話題だけで、状態そのものは載せない）、1 台でも複数台でも同じ経路を通る。順序の入れ替わり・二重の配達・送信の失敗があっても、最後は DB の値に揃う。購読が切れたら待ちを伸ばしながら（最大 30 秒）繋ぎ直し、繋ぎ直したときと受け取りが溢れたとき（`broadcast` の `Lagged`）は `Resync` を流して全部を読み直す。購読は起動時、プラグインの一覧を DB から読むより前に始める（間の変更を取りこぼさないため）。
- **`memory` は 1 台構成専用**（`cluster::SingleServerGuard`）: セッションも知らせもプロセスの中だけなので、複数台では動かない。起動時に `_aegis_settings` の `server.memory_cache_instance` に `{id, beat, at}` を書き、10 秒ごとに `beat` を増やして書き直す。起動時に別の `id` の印が 30 秒以内のものなら 20 秒待って読み直し、`beat` が進んでいれば（動いている）起動を止める。時計のずれに左右されないよう、生存は時刻ではなく値の変化で判断する。普通に止めたとき（graceful shutdown）は印を消す。2 台が同時に起動してすり抜けた場合も、書き直すたびに別の `id` を見つけてエラーログを出す。

---

## 6. 認証・認可

### 6.1 拡張の方針

認証判定と暗号処理（パスワード、OIDC の JWT / JWKS 検証、SAML の XML 署名）は Wasm プラグインに開放せず、`AuthProvider` を実装した Rust のコード（Cargo feature）として持つ。プラグインに委ねるのは認証が済んだ後の安全な拡張（ロールのマッピング、メタデータの付加、ログイン通知など）だけで、`on-user-authenticated` フックで行う。

### 6.2 ローカル認証とロール

- パスワードは Argon2id。存在しないユーザーにもダミーハッシュを検証し、応答時間でユーザーの有無を漏らさない。
- セッションの `user_id` はユーザー名（変更不可）で、監査ログの actor もユーザー名。改名機能を作る場合はこの前提を見直す。
- **セッションの即時失効**: ロール変更・無効化・削除・パスワード変更時は、キャッシュに `auth:revoked_user:{username}`（失効日時、TTL はセッション最大 TTL）を書き込む。`SessionService::get_session` はセッション作成日時が失効日時以前の場合に自動でセッションを破棄して `None` を返すため、ユーザー → セッションの全索引を持たずに一括で即時失効できる。失効後に再ログインした新セッション（作成日時 > 失効日時）は自動的に有効となる。
- 「最後の有効な admin を残す」確認は API（`users::update_user` / `delete_user`）だけで行い、CLI（`aegis user role` / `disable`）は締め出されたときの復旧手段として確かめない。
- アバターはストレージキー（`avatar_key`）だけを保存し、応答の `avatar_url` は `StorageBackend::public_url` で毎回求める（`base_path` や S3 の公開 URL の変更に追従する）。

### 6.3 ログイン試行のレート制限 (`aegis_auth::rate_limit::LoginRateLimiter`)

- 失敗回数は `CacheBackend::increment`（無いキーは 0 から、TTL は作成時のみの固定ウィンドウ）で数える。Redis では `INCR` と初回だけの `EXPIRE` を Lua で原子的に行う。上限に達したら `login:lock:{scope}:{id}` に解除時刻を TTL 付きで書き、計数キーは消す。ユーザー名のキーは小文字化した名前の SHA-256。
- クライアント IP は `ClientIp` エクストラクタ（`AuditContext` と同じ判定。`[audit].trust_forwarded_for`）。
- チェック時のキャッシュエラーは安全側に倒して 500、記録時のエラーは警告ログのみ。サーバー側のエラー（DB 障害など）は失敗に数えない。

### 6.4 ログインの完了と認証後フック

- パスワードと SSO は、セッション発行・監査・フックを `handlers::auth::complete_login` で共有する。
- `auth::hook::run_auth_hook` はプロバイダに依存せず `AuthIdentity` を受け取って返す（OIDC / SAML からも同じ関数を通す）。`provider` / `user_id` は変更不可（なりすまし防止）、`role` は既知のロールのみ。
- フックの拒否・失敗はログインの失敗として扱う（fail-closed。失敗時に通すと、プラグインで実装したアクセス制限を障害で迂回できる）。ただしログイン試行の失敗回数には数えない（`record_success` は資格情報の確認直後に呼ぶ）。

### 6.5 OpenID Connect (`aegis_auth::oidc`)

- `AuthProvider` の `initiate_login`（state / nonce / PKCE をキャッシュに 10 分置いて認可 URL を返す）と `authenticate`（`kind = "oidc_callback"`）で実装。ハンドラ（`handlers/oidc.rs`）は feature に依存せず `AppState::oidc: Option<OidcLogin>` を見る。
- ID 連携の subject は `oidc_subject(issuer, sub)`（`{issuer}#{sub}`、末尾 `/` を除去）。Issuer を変えると既存の紐付けは使えなくなる。
- トークンエンドポイントの認証は `client_secret_basic`（IdP が basic を挙げない場合のみ `client_secret_post`）。
- テストは `oidc_mock::MockIdp`（テスト内で起動する axum の IdP。鍵は起動ごとに生成し、秘密鍵をリポジトリに置かない）。
- IdP のユーザーをアカウントに対応付ける処理（ID 連携の検索・自動作成・同名ユーザーの拒否・ロールとプロフィールの同期）は `aegis_auth::sso::AccountPolicy` で SAML と共有する。ハンドラーの共通部分（IdP へのリダイレクト、検証後のセッション発行と `#session=` での戻り）は `handlers/sso.rs`。

### 6.5a SAML 2.0 (`aegis_auth::saml`)

- **依存の判断**: XML 署名の検証に xmlsec（libxmlsec1 / libxml2 の C ライブラリ）や `samael` は使わず、純 Rust で実装した。Windows・Docker のビルドに C ライブラリが要らず、検証する範囲を SAML に必要な最小限（Exclusive C14N、enveloped-signature、RSA-SHA256/384/512）に絞れるため。部品は `quick-xml`（字句解析）、`rsa`（PKCS#1 v1.5 の検証のみ。RUSTSEC-2023-0071 は秘密鍵の操作の問題で検証には影響しない）、`x509-cert`（証明書から公開鍵を取り出すだけ。有効期限や発行者は見ず、信頼は設定した証明書そのものに置く）、`flate2`（HTTP-Redirect の DEFLATE）。
- `saml::xml` は接頭辞を保つ最小限の木（C14N の出力に接頭辞が要る）。DTD・処理命令・未宣言の接頭辞・重複属性・深い入れ子を拒否し、コメントは捨てる（コメントなしの C14N と同じ）。属性値と改行の正規化は XML 1.0 どおり。
- `saml::c14n` は Exclusive XML Canonicalization（コメントなし、`InclusiveNamespaces` の `PrefixList` 対応）。名前空間宣言は「その要素か属性が使っていて、出力済みの祖先が同じ値で宣言していない」場合だけ出す。
- **XML Signature Wrapping**: `dsig::verify_enveloped` は、署名の親要素の `ID` と参照が一致すること・その `ID` が文書内で一意なことを確かめた上で、ダイジェストを計算した正規形をもう一度解析した木を返す。呼び出し側は検証後に受け取った文書を読まない（読むのは署名されたバイト列だけ）。署名を探すのは応答の直下とアサーションの直下だけで、アサーションが応答の直下にちょうど 1 つでなければ拒否する。
- 失敗の `StatusCode` だけは検証前の文書から読む（エラーの応答は署名されないことが多く、ログインを失敗させる方向にしか働かない）。
- リプレイ対策は AuthnRequest の `ID` の 1 回限りの消費（`saml:request:<ID>`、10 分）。bearer の `InResponseTo` を必須にしているので IdP-initiated は受け付けない。AuthnRequest は署名しない（SP の秘密鍵を持たない）。
- テストは `saml::mock::MockSamlIdp`（文字列に署名を差し込む。鍵はプロセスごとに生成）と、別の実装（libxml2 の C14N）で署名したフィクスチャ `crates/aegis-auth/tests/fixtures/saml/`。自前の署名と検証だけでは正規化の誤りが打ち消し合って見つからないため。signxml は `PrefixList` を参照の変換に置かない（自身でも検証できない）ため、アサーション側は lxml で組み立てている。RSA の鍵生成が遅いので `num-bigint-dig` は dev プロファイルでも最適化する（ルートの `Cargo.toml`）。

### 6.6 アカウント登録 (`aegis_auth::registration`, `handlers/registration.rs`)

- 申し込みはユーザーテーブルに入れず、キャッシュ（`registration:<トークンの SHA-256>`、TTL は `token_ttl_secs`）に置く。DB の変更が要らず、確認されない申し込みでユーザー名がふさがらない。
- 承認待ちは `disabled = true` かつ `pending_approval = true` のユーザーとして作成する。管理者がユーザー編集で有効化（`disabled = false`）すると、`pending_approval` が自動的に `false` にクリアされ、承認通知メール（`account.approved`）が送信される。単なるアカウント無効化の解除では `pending_approval` が `false` であるため、承認通知メールは送信されない。
- 確認リンクの URL は設定（`admin_url` / `[commerce].public_url`）だけから作る。リクエストの Host から作ると、Host を偽装した申し込みで本人に攻撃者のサイトへのリンクを送れてしまう。
- 既存ユーザーのメールアドレスでの申し込みは、応答を変えずにメールを送らない（登録済みかを伏せる）。ユーザー名の重複は伏せない（画面で知らせる必要がある）。
- エラーは `code`（`invalid_username` など）付きで返し、Admin Studio が `login.register.errors.<code>` で訳す。サーバーのエラーを訳す仕組みの先行例。

### 6.7 パスワードの再設定 (`aegis_auth::password_reset`, `handlers/password_reset.rs`)

- 再設定トークン（256ビット乱数）は、平文を保存せず SHA-256 ハッシュをキーにしてキャッシュ（`password_reset:<hash>`、TTL は `token_ttl_secs`）にユーザー名とともに保存する。
- **アカウント列挙対策**: 入力されたメールアドレスが存在しない場合や無効化されている場合でも、固定で `202 Accepted` を返すとともに、擬似ハッシュ計算（Argon2id）を実行して応答時間による判定を防止する。
- トークンは URL のフラグメント（`#reset-password=<token>`）で渡され、Admin Studio で読み取り直後にブラウザ履歴から除去される。新パスワードの確定（`POST /api/v1/auth/password-reset/confirm`）でトークンを消費し、即時キャッシュから削除（単一使用）する。

### 6.8 ユーザー招待リンク (`aegis_auth::invitation`, `handlers/invitation.rs`)

- 管理者がメールアドレスとロールを指定して発行する。トークンの SHA-256 ハッシュをキャッシュ（`invitation:<hash>`、TTL は `token_ttl_secs`）にメールアドレスおよびロールとともに保持する。
- 招待リンク（`#invite=<token>`）を開いたユーザーは、`GET /api/v1/auth/invitations/info` で招待内容（メールアドレス・ロール）を確認した上で、ユーザー名とパスワードを設定して承諾する（`POST /api/v1/auth/invitations/accept`）。
- 招待による登録では、アカウントは `disabled = false, pending_approval = false` の即時有効状態で作成され、管理者の事後承認フロー（`require_approval`）をバイパスして即座にログインが可能となる。

---

## 7. 監査ログ (`aegis-audit`)

- 出力先は `AuditSink` で抽象化し、`AuditService` が `[audit].sinks` の全出力先へ順に配送する。出力先の失敗はエラーログに留め、リクエストは失敗させない。`AuditService::add_sink` で後から足せる（監査イベントの通知はこれを使う）。
- `occurred_at` は UTC・マイクロ秒・`Z` 終端の RFC 3339 に正規化して保存し、文字列比較で期間検索する（API の `since` / `until` も同じ形式に正規化）。
- 操作主体はセッションから解決する `AuditContext`（失敗しない `FromRequestParts`）。クライアント IP のため、サーバーは `into_make_service_with_connect_info::<SocketAddr>()` で起動する。
- セッショントークンは保存せず、`session_fingerprint`（SHA-256 の先頭 16 桁）で突き合わせる。パスワード・通知の宛先アドレスも残さない。
- 保持期間: `spawn_retention_task` が起動直後と `purge_interval_secs` ごとに `occurred_at` が基準より古いものを削除し、1 件以上あれば `audit.purge` を記録する。既定の `retention_days = 0` は無期限（アップデートで既存の監査ログが消えないように）。

---

## 8. 通知 (`aegis-notify`)

- 通知を出す側は `aegis_core::Notifier` に `NotificationEvent { kind, recipients, data }` を渡すだけで、`aegis-notify` に依存しない（例: `CommerceDeps.notifier`）。宛先は `Recipient::Person`（アドレス）と `Recipient::Staff`（チャネルごとの設定で解決。メールは `staff_recipients`）。
- テンプレートは `<kind>.subject` / `<kind>.txt`（運営者向けは `<kind>.staff.*` を先に探す）。見つからなければ最後の `.` 以降を外した名前へフォールバックし（`audit.session.create` → `audit`）、それでも無ければ送らない。MiniJinja の `trim_blocks` は行末の `{% endif %}` の後の改行も消すため、行の途中の条件は `{{ "..." if cond else "" }}` で書く。
- **送信待ちは DB の outbox（`_aegis_notification_queue`）**: 外部のキュー（Redis・SQS など）は使わない。どの構成にもある DB だけで済み（1 台の SQLite でも、複数台の PostgreSQL / MySQL でも同じ）、`[cache] = memory` の 1 台構成でも再起動で失われない。Redis の Stream なども検討したが、`[cache]` が memory の構成で使えず、DB と二重の障害点になるため採らなかった。
  - **行は宛先 × チャネル**: テンプレートは入れるときに描画し、`NotificationMessage`（JSON）を保存する。チャネルごとに行を分けるので、メールは届いて Slack だけ失敗した、という場合に失敗した方だけを再試行できる。描画を送るときにしないのは、注文の状態などが「通知が起きた時点」の内容になるようにするため（テンプレートの誤りも入れる時点で分かる）。送れたら行を消す（本文にパスワード再設定などのリンクが入るため、長く残さない）。
  - **複数台の排他は条件付き UPDATE のリース**: `UPDATE … SET locked_until = <今 + 5 分> WHERE id = ? AND status = 'pending' AND (locked_until IS NULL OR locked_until < <今>)` の更新件数が 1 のサーバーだけが送る。`SELECT … FOR UPDATE SKIP LOCKED` は SQLite に無く、3 つのダイアレクトで同じ SQL にできないため使わない。送信中に止まったサーバーの行は期限が過ぎれば取り直せる（少なくとも 1 回は送る。まれに重複する）。日時は固定長の RFC 3339（UTC・マイクロ秒、`format_audit_timestamp`）で保存し、文字列の比較で前後を決める（サーバー間の時計のずれはそのまま期限のずれになる）。
  - **ワーカー**: `notify` は保存して `tokio::sync::Notify` でワーカーを起こす（新しい通知は待たない）。再試行の時刻はポーリング（`poll_interval_secs`）で拾う。1 回に 20 件を取り出し並行に送る（遅い SMTP が 1 件あっても後続が詰まらないように）。保存に失敗したら従来どおりメモリ上で送る（通知を落とさない）。`deliver` / `deliver_now`（CLI・Admin Studio のテスト送信）は送信待ちを通らない。
  - MySQL の `TEXT` は 64KB までなので、本文の列は `LONGTEXT`（HTML と注文の値で超えうる）。
- **HTML メール**: 種類ごとの HTML テンプレートを用意せず、プレーンテキストの本文を共通のレイアウト `_layout.html` に入れて作る（`html::text_to_html`: 段落・改行・URL のリンク・`--` から後をフッター）。組み込みのテンプレートを 2 言語 × 20 種類ほど二重に持たずに済み、文面の修正がテキストとHTMLでずれない。`<kind>.html` があればそれを使う（運営者向けの本文があるときに顧客向けの HTML を使わないよう、探すのは件名・本文が見つかった名前まで）。エスケープは名前が `.html` で終わるテンプレートだけ（`set_auto_escape_callback`）。
- **Admin Studio で編集したテンプレートは `_aegis_settings`（`notification_template:<名前>`）**: ローダーが DB の値 → `templates_dir` → 組み込みの順に探す。MiniJinja はローダーで読んだテンプレートをキャッシュするので、差し替えのたびに `clear_templates` する（`Templates` は `RwLock<Environment>`）。ほかのサーバーへはイベントバス（`notifications.templates`）で知らせ、受け取ったサーバーが DB から読み直す。プレビューは同じ設定の `Templates` を作り、保存前の内容だけを重ねる。
- `SendError::permanent`（SMTP の 5xx・宛先不正）は再試行しない。
- **プラグインのチャネル**: `NotificationMessage` / `Delivered` / `SendError` は `aegis_core::notify` にあり、ランタイムと共有する。`aegis-notify` はランタイムに依存せず、`plugin:<名前>` のチャネルは `NotificationService::from_config_with_plugins` に渡すファクトリでサーバー（`notify_plugins.rs`）が作る。起動時に `validate_plugin_channels` で読み込み済み・フック宣言済みかを確かめる。`Skipped` は配信結果にも監査ログにも出さない。Wasm の失敗はフック未宣言（`PermissionDenied`）だけ permanent、タイムアウト・トラップは再試行する。
- **監査イベントの通知**: `AuditNotificationSink` を `AuditService::add_sink` で加える。`NotificationService` は監査ログを使うため後から作り、`attach` で渡す（`OnceLock`）。`notification.send` 自体は通知しない（配信の失敗が通知を呼び続けるため）。`max_per_hour` はルールごとにメモリ上で数える。ログインのロックは `session.create` の `failure` で `details.locked` があるもの（ロックがかかった 1 回）、ロック中の試行は `denied` + `details.error = "rate_limited"`。
- プラグインの設定値の空文字列は、`config_get` の ABI で長さ 0 =「値なし」のため `None` として届く。

---

## 9. EC (`aegis-commerce`)

- 金額はすべて通貨の最小単位の `i64`（Stripe の `unit_amount` と同じ）。表示は `aegis_core::format_money` / テンプレートの `money` フィルター / Admin Studio の `formatMoney`。
- 在庫の確保は `UPDATE ... SET stock = stock - ? WHERE id = ? AND (stock IS NULL OR stock >= ?)` を注文の INSERT と同じトランザクションで行う（売り越し防止）。`stock` が NULL の商品は無制限。
- **バリエーションは別表 `_aegis_product_variants`**: 商品はスキーマごとに動的なコンテンツの表なので、サイズ・色ごとの価格・在庫は固定の表に分けた。`StockReservation` が `variant_id` を持ち、在庫の確保・戻しは同じトランザクションの中で親商品の表かバリエーションの表かを切り替える。カート・明細は `(product_id, Option<variant_id>)` で区別し、明細には注文時の `variant_title` / `sku` を残す。
- **売上レポート**: `_aegis_orders.paid_at` を UTC 日付・通貨で集計し、支払い済み系の状態を対象に元売上と返金額を別々に返す。日付範囲は半開区間で DB 側集計し、SQLite / PostgreSQL / MySQL 共通の `SUBSTR` を使う。Admin Studio は通貨ごとに返金後金額（売上 − 返金）を表示する。
- **在庫閾値通知**: 注文後に商品／バリエーションの残数を確認し、閾値を上から下へ越えた場合だけ `stock.low` を `Recipient::Staff` に送る。通知無効または `[commerce].low_stock_threshold = null` なら送らない。On-demand ISR はこの通知フローに含めない。
- **複数通貨**: 金額は通貨ごとの最小単位の整数のまま扱い、換算は `aegis_core::convert_currency`（`currency_decimals` で 0 / 2 / 3 桁を合わせて四捨五入、`i64` の範囲で飽和）。単価は `CommerceService::resolve_item_price` で、通貨ごとの固定価格（`price_<通貨>`）を換算より優先する（端数を店側で決められるように）。注文は 1 つの通貨で計算・保存し、Stripe にもその通貨で渡す。
- 状態の変更は `transition_order`（`WHERE status = 前の状態`）で、Webhook の重複・期限切れ処理との競合を防ぐ。プラグインへの通知（`on-order-status-changed`）は `tokio::spawn` で応答を待たない。
- **外部連携決済の抽象化層 (`PaymentProvider`)**:
  - `PaymentProvider` トレイト（`create_checkout_session`, `verify_webhook`, `supports_refund`, `refund`, `allows_manual_confirmation`）によりゲートウェイ固有の通信とロジックを分離。
  - Stripe 以外の決済サービス（PayPal, Square, 銀行振込代行サービス等）を追加する際も、`CommerceService` コアロジックや DB 層を変更することなくプロバイダ実装を追加するだけで対応可能。
  - 取引識別子の分離: チェックアウト開始時の識別子（Stripe Checkout Session ID `cs_...`）を `payment_ref`、確定取引 ID（Stripe Payment Intent ID `pi_...`）を `transaction_id` として明確に分離管理し、確実な返金処理を実現。
- **返金処理と在庫復帰の ACID 保証 (`record_order_refund`)**:
  - 返金（部分返金・全額返金）の記録、`OrderStatus::Refunded` への自動遷移（累計返金額が合計に達した場合）、およびオプションの在庫復帰（`restock`）を単一の RDBMS トランザクション内で実行。SQLite / PostgreSQL / MySQL のすべてで整合性を検証済み。
  - `can_refund`（`order.status != PendingPayment && refunded_amount < total`）による二重返金の防止と、Stripe API `Idempotency-Key` による外部呼び出しの安全性を担保。
  - Webhook の `charge.refunded` イベントによる自動同期。
- `calculate-order` の結果は `adjustments` / `metadata` だけを反映し、合計はホストが計算し直す。拒否メッセージは顧客に見せるため、ホストが付けるプラグイン名の接頭辞を外す（ログには残す）。
- Stripe の注文は支払い済みになった時点（Webhook）で `order.placed` を送る（決済ページで離脱した注文に受付メールを送らない）。注文確認ページのトークンは平文を保存しないので、URL は `order.placed` のメールにしか入らない。
- **顧客アカウント (`_aegis_customers`, `CustomerService`)**:
  - CMS 管理スタッフ（`_aegis_users`）とは完全に独立したテーブルおよび専用セッショントークン（`cses_<uuid>`、キャッシュ保存）で管理。Admin Studio のロール権限・管理セッションとは混ざらない。
  - Argon2id によるパスワードハッシュ化と、タイミング攻撃を防ぐダミー計算。
  - チェックアウト時に `Authorization: Bearer cses_...` があればセッションから `customer_id` を注文に自動紐付け。
  - 顧客マイページ向け API: プロフィール更新、自身の注文履歴一覧・詳細、投稿レビュー一覧。
- **商品レビュー (`_aegis_reviews`, `ReviewService`)**:
  - **購入者限定 (Verified Purchase)**: 注文ステータスが支払い済み/完了（`paid`, `completed`）かつ明細に対象商品を含む顧客のみ投稿可能。アカウント無しのゲスト購入者も、注文アクセストークンによる購入証明付き投稿に対応。
  - **1 注文 1 商品 1 件の制約**: 同一注文から同一商品に対する重複レビューを厳格に拒否。
  - **モデレーション共有**: `aegis-comments::moderation`（承認制、NGワード、リンク制限等）のポリシーをそのまま流用。
  - **ステマ規制対応（景表法ステマ告示）**: 低評価（★1〜2）を機械的に非承認とせず、非承認時は `rejection_reason` の記録を可能にして運用の透明性を確保。
  - **集計と SEO**: ★1〜5 の平均評価、件数、星別分布を集計し、JSON-LD の `AggregateRating` 形式を出力。
  - **運営者返信**: 管理者からの 1 段の公式返信（`reply`）に対応。
- 結合テストは `aegis-server/src/handlers/shop_tests.rs` および `customers_tests.rs`。

---

## 10. Wasm プラグインランタイム (`aegis-runtime`)

### 10.1 実行モデル

- Wasmtime は `async_support(true)` で使い、`tokio::time::timeout` と Fuel の二重で暴走を止める。Windows MSVC では同期呼び出しのトラップが `extern "C"` の non-unwinding 境界チェックと衝突して abort することがあったため。
- データの受け渡しは、ホストがゲストの `allocate(len) -> ptr` で確保した領域に JSON を書き、ゲストは `(ptr << 32) | len` の `i64` で結果を返す（[ABI 仕様](plugin_abi_specification.md)）。
- フックの呼び出しは `WasmPluginInstance::call_hook`（インスタンス化・入力の書き込み・タイムアウト付き呼び出し・結果の読み出し）に集約。
- **インスタンスは呼び出しごとに新しく作る**。インスタンスそのものの再利用は、状態が呼び出し間に残る（別ユーザーのデータの持ち越し、トラップ後の壊れたヒープ）ため行わない方針。必要になったら、失敗したインスタンスの破棄・回数制限・機密フック（`on_user_authenticated`）の除外・マニフェストでの明示を条件に検討する。
- オブジェクトを返すフック（`on_before_save` / `on_render_page` / `on_user_authenticated`）の返り値は、トップレベルのキー単位で入力に上書きする（`parse_object_output`）。軽量 JSON しか持たない言語の SDK が一部のフィールドだけ返しても記事一覧などが消えない。`{"error": ...}` は `PluginRejected` に変換する（拒否できるのは `on_before_save` / `on_user_authenticated`。`transform_content_ast` / `on_render_page` の失敗はエラーログを出して入力をそのまま使う）。入力 JSON のパース失敗も拒否にする（検証フックが黙って素通りしないように）。

### 10.2 インストールと有効化 (`WasmPluginLoader`)

- インストール = `plugins_dir/<name>/plugin.manifest.json`（ディレクトリ名とマニフェストの `name` は一致必須、`is_valid_plugin_name`）。有効化 = `[plugins].enabled`、または Admin Studio で切り替えて DB に保存した一覧（順序 = フックの実行順）。テーマの `themes/<id>/` + `[theme].active` と同じ考え方で、ストアからのインストールもこの形に展開する前提。
- `load_enabled` は有効なものだけをコンパイルし、失敗したら起動を止める（fail-closed。アクセス制限系のプラグインが黙って外れるのを防ぐ）。無効なものは `discover` でマニフェストだけ読み、不備は警告ログ・`aegis plugin list`・`GET /api/v1/plugins` に出す。
- **実行時の切り替え**（`aegis-server/src/plugin_registry.rs`、`PUT /api/v1/plugins/{name}/enabled`）: 有効化リストは `[plugins].enabled` を DB の `_aegis_settings`（`plugins.enabled`、JSON 配列）で上書きする（サイト名と同じ置き場所。キャッシュは消えうるので使わない）。保存した値が壊れていれば起動を止める（fail-closed）。切り替えは `PluginRegistry` の `Mutex` で直列化し、`PluginLoader::rebuild` で新しい `PluginSupervisor` を組み立ててから `RwLock` の中身を差し替える（読み込み済みのインスタンスは `Arc` で使い回し、新しく有効にしたものだけを `spawn_blocking` でコンパイル）。順序は「読み込み → DB 保存 → 差し替え」で、どこで失敗しても状態は変わらない。`supervisor` の `Arc` は EC・通知チャネルと共有しているので中身の差し替えだけで全体に効く。プラグインの登録テンプレートも差し替え時に入れ替える。通知チャネルに使っているプラグインは外させない（起動時の `validate_plugin_channels` と同じ条件を保つため。無効化・設定ファイルの値に戻すときとも）。有効化・無効化・並べ替え（`PUT /api/v1/plugins/order`、今有効なものの並べ替えに限る）・設定ファイルの値に戻す（`DELETE`、DB の行を消す）は `handlers/plugin.rs` の `switch_plugins` を共有する。差し替えたあとイベントバスで知らせ、受け取ったサーバー（自分を含む）は `plugin_registry::sync_from_db` で DB の一覧（無ければ自分の `[plugins].enabled`）と比べ、違えば同じ `prepare` → `install` で差し替える。プラグインの設定値（メモリに載せた `plugin_settings`）も同じように知らせ、受け取ったらキャッシュから読み直す。
- **ZIP パッケージ（`aegis_core::package`）**: プラグインとテーマで共通。`extract_zip_safely`（ZipSlip・サイズ・エントリ数の制限）→ `check_min_aegis_version`（semver）→ `verify_package_signature`（Ed25519、`ring`）。署名の対象は `package`（既定・それ以外は拒否。`signature.json` 以外の全ファイルを相対パスで並べ、`パス:SHA-256\n` を連ねたものの SHA-256）。`require_signatures = true` のときは `trusted_public_keys`（32 バイト hex 1 件以上）が必須で、起動時の設定検証（`validate_signature_policy`）と `verify_package_signature` の両方で空リストを拒否する。検証は一時ディレクトリで済ませてから `plugins/<name>/` に複写するので、失敗しても既存のプラグインは変わらない（置き換えは削除 → 複写で、原子的ではない）。インストールは有効化しない。
- `WasmPluginInstance::from_bytes` で `Linker::instantiate_pre` してインポートを読み込み時に検査する。`_initialize` があればインスタンス化の直後に呼ぶ（TinyGo `wasm-unknown`）。

### 10.3 ホスト API (`HostServices`)

- ランタイムは I/O を持たず、サーバーが `HostServices`（KV・HTTP・設定値。`aegis-server/src/plugin_host.rs`）を実装して渡す。KV は `[cache]` に保存する。
- **動的プラグイン設定**: プラグインごとの設定値（`config_get`）は、静的設定（`aegis.toml` の `[plugins.settings.<plugin>]`）に加え、Admin API（`PUT /api/v1/plugins/{name}/settings`）からの動的設定に対応している。動的設定はメモリ内の `RwLock<HashMap>` とキャッシュ（`plugin_settings:{name}`）に即座に反映され、静的設定を上書きする。サーバー起動時にもキャッシュから自動復元されるため、再起動後も設定値が保持される。
- 結果を返すホスト関数（`kv_get` / `http_fetch` / `config_get`）は `func_wrap_async` で、ゲストの `allocate` を呼んで書き込み `ptr << 32 | len` を返す。
- `http_fetch` は `allowed_domains` だけ・リダイレクトを追わない・1 MiB・1 呼び出し 8 回まで。ドメインの判定は URL パーサーで行う（文字列の後方一致だと `https://evil.com#.allowed.com` を許してしまう）。

### 10.4 インスタンスのプール (`WasmEngine::with_instance_pool`)

- サーバーは `[runtime].instance_pool_size` が 1 以上ならプーリングアロケータのエンジンを使う（`main.rs` の `create_wasm_engine`）。`WasmEngine::new()`（テスト・ツール用）は従来の方式のまま（テストで多数のエンジンを作ると枠ごとの仮想アドレス空間の予約が重なるため）。予約できない環境では警告を出して従来の方式にフォールバックする。
- 枠と同じ数のセマフォで同時実行数を制限し、`call_hook` で Store より先に枠を確保する（Store の破棄で枠が返るまで保持するため。ローカル変数は宣言の逆順に破棄される）。
- プールでは線形メモリの上限・テーブル要素数が枠の大きさで決まるため、`WasmEngine::check_fits_pool` で `max_memory_mb`・初期メモリ・テーブルを読み込み時に確かめる。GC ヒープの枠（`total_gc_heaps`）もインスタンスごとに 1 つ要る（0 にするとインスタンス化に失敗する）。
- `linear_memory_keep_resident`（解放時に `madvise` ではなく memset でゼロに戻す）は、小さいモジュールでは速くなったが Rust 製の `official-code-enhancer` では遅くなったため使っていない。

### 10.5 SDK

- Rust の `#[aegis_plugin]` は `allocate` と各フックのエクスポート、ポインタのパッキング、JSON 変換を生成する。ネイティブ（`cargo test`）ではホスト API がモックになる。
- AssemblyScript テンプレートは外部依存なしの JSON（`assembly/sdk/json.ts`）を持ち、ホスト API は `@external("host", ...)`。`env.abort` は差し替えている（ホストに無いインポートで読み込みに失敗するため）。IDE 用に `tsconfig.json` で `"ignoreDeprecations": "6.0"`、`@external` に `// @ts-ignore`。
- TinyGo テンプレートは `//go:wasmimport host <func>` でインポートし、`-target=wasm-unknown` でビルドする。ホストに渡すバッファが GC に回収されないよう `allocatedBuffers` で追跡する。
- テンプレートのビルド済み `plugin.wasm` はコミットしており、`crates/aegis-runtime/tests/sdk_plugins.rs` が実ホストで全フックを実行する（[development.md](development.md#4-プラグインのビルド)）。

---

## 11. Admin Studio の配信

- `aegis-server` は起動時に `admin/dist/index.html` があれば `/admin/` で SPA を配信する（`ServeDir` + `ServeFile` のフォールバック）。`/` はテーマの公開サイト、`/admin` はスラッシュ付きへ 307。Vite の `base: './'` でアセットを相対参照し、`/admin/assets/*` から配信する。フロントエンドを変えたら `npm --prefix admin run build` だけで反映される（サーバーの再コンパイル不要）。
- Admin Studio 側の設計（API の接続先、画面構成、モバイル・レスポンシブ設計、i18n、エディタ）は [admin/README.md の設計メモ](../admin/README.md#-設計メモ)。モバイル環境（幅 900px 以下）では 3 列固定からタブ切り替えペイン（一覧・編集・プレビュー）へ動的に切り替え、ビューポートは動的アドレスバーを考慮した `height: 100dvh` を採用。


---

## 12. 性能の記録

簡易ベンチマーク（4 vCPU・同一マシンから oha・同時 50・記事 50 件）。一覧の N+1 解消・ページング・インデックス・テンプレートのキャッシュの前後:

| 対象 | 前 | 後 |
| --- | --- | --- |
| トップページ | 470 req/s（記事 200 件で 118） | 5,100 req/s（記事 200 件で 5,500） |
| 記事ページ | 8,900 | 16,000 |
| プレビュー | 4,700 | 7,300 |
| 一覧 API（50 件/ページ） | 1,100 | 1,200 |

- プラグインの呼び出しは単発で 1 回 40〜190µs（うちインスタンス化 10〜20µs。`task bench:plugins`）。遅かったのは同時実行時で、32 並列でスループットが 0.2〜0.5 倍に落ちていた（呼び出しごとの `mmap` / `munmap` のカーネル内の競合）。プールの導入で 32 並列のスループットが 3〜8 倍、`official-code-enhancer` を有効にした記事ページが 2,600 → 4,500 req/s。
- プラグイン 3 つ（描画フックあり）を有効にすると記事ページは約 1,500 req/s（フック 1 回約 1ms）。
- AssemblyScript SDK の `escapeJson` / `findStringField` は 1 文字ずつ `+=` で連結していたため入力の大きさの二乗で遅かった（`on_before_save` で約 2〜3.6ms、`official-code-enhancer` と併用すると `on_render_page` が約 13ms）。結果の長さを先に数えて `__new` で 1 回だけ確保し、UTF-16 を直接書き込む方式に変えて、テンプレートの `on-before-save` が 3,600 → 160µs、`on-render-page` が 170 → 46µs（`task bench:plugins`）。区間ごとに `substring` して `Array#join` する方式でも二乗は消えるが、確保の回数がエスケープの数に比例し、Wasmtime 上では AssemblyScript の確保 1 回が重い（同じ処理が V8 の 10〜20 倍。ランタイムを `stub` にしても残る）ため 1.5ms までしか縮まなかった。`blocks_json` は JSON の中の JSON でエスケープが多いので特に効く。

---

## 13. コメント (`aegis-comments`)

- **コメントと商品レビューは表を分け、審査の仕組みを共有する**: レビューには評価（★）の集計、購入者だけ・1 注文 1 件の制約、返信は運営者の 1 段だけ、通知先は運営者、という違いがあり、同じ表にすると（WooCommerce の `comment_type = review` のように）コメントを扱う箇所すべてに分岐が要る。そこで表は `_aegis_comments`（今後 `_aegis_reviews`）に分け、共通部分を `aegis_comments::moderation` に置いた: 承認制の設定（`ModerationConfig` / `PolicyOverrides`。`_aegis_settings` の `<接頭辞>.require_approval` など）、入力の正規化（`normalize_name` / `normalize_email` / `normalize_body`）、NG ワード・リンク数の振り分け（`screen`）、IP ごとの投稿数（`within_rate_limit`。キャッシュのキーの名前空間で分ける）、本文の HTML 化（`body_html`）。状態 `ModerationStatus` は `aegis-core`。レビューは顧客アカウント（EC フェーズ 4）と一緒に作る（購入の確認に要るため）。
- **最初の状態は「厳しい方を採る」**: NG ワード・リンク数 → 承認制（WordPress と同じく、同じ名前とメールアドレスの承認済みコメントがあれば自動承認）→ `check-comment` プラグインの順で、`moderation::stricter` で合わせる。プラグインは承認待ちを公開にできない（スパム判定サービスの誤判定で公開されないように）。プラグインの失敗は承認待ちにする（投稿を失わず、公開もしない）。
- **スパム・ハニーポットにも承認待ちと同じ応答を返す**: 判定の結果をボットに教えない。ハニーポットに掛かった投稿は保存もしない（スパムの一覧を埋めないため）。
- **承認制の設定は投稿のたびに DB から読む**: サイト名（`site.rs`）のようにメモリに持つと、複数のサーバーではイベントバスでの同期が要る。投稿は頻繁でないので、`_aegis_settings` を毎回読めば同期の仕組みが要らない。
- **本文はプレーンテキストで保存し、表示のときにエスケープする**: `body_html` は段落・改行だけの HTML を作る（リンクにしない）。テンプレートでは `body_html | safe`、Admin Studio は本文をテキストとして出す。
- **投稿日時はマイクロ秒まで**（ほかの表は秒）: 同じ秒の投稿・返信がスレッドで投稿順に並ぶように（並びは `created_at, id`）。
- **スレッド**: 公開ページは承認済みを古い順に 1 回で読み（上限 1,000 件）、`thread()` で親の直後に子を並べる。親が見えない（承認されていない・削除された）返信はトップレベルに出す。深さは `max_depth` で打ち切って表示する。返信の投稿時は親をたどって深さを確かめる。
- **返信通知の停止トークンは平文で保存する**: 後から送る `comment.replied` のメールにリンクを入れるため（注文確認のトークンと違い、ハッシュでは後から URL を作れない）。できるのは返信通知の停止だけで、Admin Studio の API でも返さない。比較は定数時間。
- **コンテンツごとの受付の切り替えは `_aegis_settings` に置く**（`comments.closed.<種類>.<ID>` = `true`。行が無ければ受付中）: コンテンツの表はスキーマごとに動的で、全ダイアレクトに列を足す移行も要るため、承認制の設定と同じ表を使った。`CommentService::target` がページ表示・投稿のたびに `list_settings` で読む（表が小さいので 1 回の軽い問い合わせ。複数のサーバーでもすぐ効く）。コンテンツを削除するときに行も消す（`delete_for_content`）。
- **重複の検出は DB の完全一致**（`has_duplicate_comment`: 種類・コンテンツ・本文が同じで、メールアドレスか IP アドレスが同じ。ゴミ箱は除く）。ボット（ハニーポット）は先に捨て、IP ごとの投稿数の判定の後に見る（探りを数えるため）。判定の結果（`409`）は本人の投稿にしか影響しないので、スパム判定の隠し方（承認待ちと同じ応答）とは違い理由を返す。
- **承認の通知は希望した人だけ**: メールアドレスは誰でも他人のものを入力できるので、`notify_replies`（フォームのチェックボックス。停止リンクのトークン付き）を選んだ人だけに `comment.approved` を送る。承認待ち・スパムからの承認だけが対象（ゴミ箱の復元・承認を待たない公開・運営者の返信は送らない）。
- **アバターは URL を作るだけ**（`CommentsConfig::avatar_url`）: Gravatar（SHA-256）の URL を `PublicComment.avatar_url` に入れ、画像は読者のブラウザが取得する。サーバーは外部に接続しない。既定は `none`。
- **コンテンツの削除でコメントも消す**: 外部キーは使わず、`delete_content` のハンドラで `delete_content_comments` を呼ぶ（コンテンツの表はスキーマごとに動的なため）。
- **MySQL の索引**: utf8mb4 の索引は 3,072 バイトまでなので、`VARCHAR(255)` の列は 1 つの索引に 3 つまで（`idx_aegis_comments_content` は `content_id, status, created_at`）。
- **テンプレート**: 公開ページの `PageRenderContext.comments`（`CommentSection`）は `skip_serializing_if` で、コメントを受け付けないページには出ない。プラグインの `on-render-page` は返したキーだけを上書きするので、SDK の `PageRenderContext` に `comments` が無くても消えない。`items` は MiniJinja の辞書のメソッド名と重なるため、テンプレートでは `comments["items"]` で参照する。
- 結合テストは `aegis-comments/src/tests.rs`（サービス）、`aegis-server/src/handlers/comments_tests.rs`（ルーター経由）、`aegis-db/tests/comments_integration.rs`（SQLite と、コンテナがあれば PostgreSQL / MySQL）。

---

## 14. 掲示板 (`aegis-forum`)

- **記事・コメントとは別の表**（`_aegis_forum_categories` / `_aegis_forum_threads` / `_aegis_forum_posts`、`aegis_db::ForumStorage`）。スレッドの最終投稿日時・レス数・ロックなど掲示板だけの列と索引が要り、記事の描画の経路（`on-render-page` などのフック）も通さないため。
- **専用の DB に分けられる（`[forum].database_url`）**: 投稿が集中したときの SQLite の書き込みロックや PostgreSQL / MySQL の接続数が、本体（記事・EC・セッション）に響かないように。分けた場合は `ForumService` にだけ別の `DatabaseAdapter` を渡し、起動時と `aegis migrate` でその DB に表を作る。省略時は本体の DB を共有する。
- **ページ送りはキーセット方式**: スレッドは `(is_pinned DESC, last_post_at DESC, id DESC)`、レスは `post_number > ?`。`OFFSET` だと後ろのページほど遅くなるため。
- **レス番号はスレッドごとの連番**で、レスの削除は論理削除（`is_deleted`。番号と「>>N」の参照を崩さない）。スレッドの削除は物理削除。`auto_lock_post_count` に達したら自動でロックする。
- 連投の制限は `CacheBackend` で IP・ユーザーごとに数える。監査ログは `forum.*`。
- 結合テストは `aegis-forum/src/tests.rs`（サービス）、`aegis-server/src/handlers/forum_tests.rs`（ルーター経由。専用 DB への分離も確かめる）。

## 15. サイト内検索 (`aegis-search`) とアクセス解析 (`aegis-analytics`)

- **Cargo の feature で切り離す**: `aegis-server` の feature `search` / `analytics`（どちらも既定で有効）。無効でビルドすると、設定の項目・ルート・`AppState` のフィールドごと無くなる（`cargo check -p aegis-server --no-default-features`）。どちらのクレートも HTTP に依存しない。
- **検索の索引は tantivy、正は DB**: 索引は作り直せるものとして扱い、起動時に公開中の版から作り直す。形式が読めないディレクトリは消して作り直し、使えない（別のプロセスが使っている）ときはメモリの索引にする。
- **日本語は辞書なしの 2 文字分割**: 形態素解析の辞書（数十 MB）を持たずに済み、新語・固有名詞も漏れない。索引には 1 文字のトークンも同じ位置に入れて 1 文字の検索に応え、検索語は 2 文字のトークンのフレーズ（位置の差を索引と揃える）で探して「東京都」が「京都」に一致しないようにする。
- **索引の更新はイベントバス**: コンテンツの変更（作成・更新・削除・予約公開・予約非公開）で `search.content` を流し、各サーバーが DB から公開中の版を読み直す。書き込みは 1 つずつ（tokio の Mutex）で、読み直しは順番が来てから行うので、作り直しと個別の更新が重なっても新しい方が残る。Redis への再接続（`Resync`）では作り直す。
- **アクセス解析は個々のアクセスを残さない**: 訪問者の見分けは日ごとの秘密の値（キャッシュにだけ置く）を混ぜたハッシュで、重複の判定もキャッシュの `increment`（TTL 2 日）で行う。DB（`_aegis_analytics`、`aegis_db::AnalyticsStorage`）には日 × 切り口（サイト・ページ・参照元・端末）× 値の数だけを、メモリで溜めて一定間隔と終了時にまとめて加算する（方言ごとの UPSERT）。

## 16. 問い合わせフォーム (`aegis-contact`)

- **項目は設定ファイルで決める**（`[[contact.forms]]`）: サーバー側の検査の基準（必須・長さ・種類・選択肢）が 1 か所に決まり、編集者が記事ごとに項目を変えて検査の抜けを作ることがない。本文のブロック `core/contact-form` はフォームの名前だけを持ち、描画のたびに設定から HTML を作る（プラグインのブロックと同じ「トップレベルのブロックを先に描く」経路）。
- **JavaScript なしで動く**: 普通の HTML のフォームを `POST /contact/{form}` に送り、成功は `303` で `?sent=1` へ（PRG）、入力の誤りは同じページを `400` で返して値と誤りを出す。入力を含むページは `Cache-Control: private, no-store`。
- **スパム対策はコメントと共有**（`aegis_comments::moderation` の IP ごとの数・リンク数・メールアドレスの検査）。ボット除けの見えない欄に値があれば、成功と同じ応答で何も保存しない。スパムと判定したものは保存するが通知しない（受信箱のスパムで確かめられる）。
- **表は `_aegis_contact_messages`**（`aegis_db::ContactStorage`）。入力はフォームの項目の順に `[{name, label, value}]` の JSON で残し（後から設定の項目を変えても、届いた時点の表示名で読める）、一覧用に名前・メールアドレス・件名を列に持つ。保存期間はサービスが 1 日に 1 回消す。


## 17. バックアップとリストア (`aegis-backup`)

- **独立クレートと feature フラグ（`feature = "backup"`）**:
  - `aegis-db`（全システムテーブル・動的コンテンツテーブル）と `aegis-storage`（アップロードファイル群）の上位オーケストレーション層として `crates/aegis-backup` に分離。`aegis-server` ではオプショナルな feature（既定有効）として切り離し可能（`cargo check -p aegis-server --no-default-features` で最小限のフットプリントを維持）。
  - CLI コマンド `aegis dump`（バックアップ生成）および `aegis restore`（復元・整合性検証）として利用可能。
- **異種 DB 間のポータビリティ（ダイアレクト非依存の JSON 表現）**:
  - 特定 DB のバイナリダンプ（`pg_dump` や SQLite ファイルコピー）ではなく、各ドメインモデル（`ContentRecord`, `ContentVersionRecord`, `UserRecord`, `OrderRecord` 等）の構造化 JSON としてエクスポートする。これにより、「開発環境の SQLite から dump したアーカイブを、本番環境の PostgreSQL や MySQL へそのまま restore する」クロス DB マイグレーションを完全サポート。
- **コンテンツ版履歴と公開ポインターの完全保持**:
  - コンテンツごとに最新作業版（`record`）、編集履歴スナップショット（`versions`、リビジョン昇順）、および公開版ポインター（`published_version_index`）をバンドルして保存。復元時も `insert_content_with_versions` を通じて作業版と公開スナップショットの分離関係をそのまま再現する。
- **整合性・改ざん防止の事前検証（Fail-Safe）**:
  - アーカイブ（ZIP）のルートに `manifest.json`（フォーマットバージョン `format_version: 1`、CMS バージョン、テーブル一覧・件数、ストレージオブジェクト一覧、および全ファイルの SHA-256 チェックサム）を記録。
  - リストア時は、データベースやストレージへの書き込みを一切行う前に、ZIP 内の全エントリのパス安全性（ZipSlip・パストラバーサル防止）と SHA-256 ハッシュを完全検証し、不整合や破損・改ざんがあれば変更を 1 つも適用せずに拒否する。`--verify-only` オプションで復元せずに検証のみを実行することも可能。
- **安全な衝突制御（`--overwrite`）**:
  - 既定（`overwrite = false`）では、既存のユーザーやコンテンツ、設定などの ID/キーが衝突した場合にバリデーションエラーとして処理を停止し、誤操作による既存データの破壊を防止する。`--overwrite` を指定した場合のみ、既存レコードを置換して復元する。
