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
| [theme_and_publishing_guide.md](theme_and_publishing_guide.md) / [theme_authoring_guide.md](theme_authoring_guide.md) | 公開ルーティング・テーマエンジン / テーマの作り方 |
| [plugin_sdk_guide.md](plugin_sdk_guide.md) / [plugin_hooks.md](plugin_hooks.md) / [plugin_abi_specification.md](plugin_abi_specification.md) | プラグイン開発 / フックとホスト API / 言語共通の ABI |
| [development.md](development.md) | 開発環境・検証・CI・Docker |
| [admin/README.md](../admin/README.md) | Admin Studio（構成・i18n・設計メモ） |

---

## 1. クレート構成

依存の向きは `aegis-core` ← ドメインクレート ← `aegis-server` の一方向です（クレートの一覧と各モジュールは [README 2.3](../README.md#23-クレート構成と各モジュールの責務-crates)）。

- **`aegis-core` は共有契約だけ**: 抽象トレイト（`StorageBackend` / `CacheBackend` / `AuthProvider` / `AuditSink` / `Notifier`）、監査イベントのモデル、プラグインに渡すコンテキスト型、`PluginManifest`、`AegisError`。Wasm プラグイン SDK も依存するため、実装や重い依存を足さないこと。
- **実装はドメインクレート**: `aegis-db`（アダプタ・注文）、`aegis-storage`、`aegis-cache`、`aegis-audit`（`AuditService`・出力先・メトリクス・`AuditOrigin`）、`aegis-auth`（ユーザー・セッション・レート制限・OIDC・登録。feature `oidc` / `mock-idp`）、`aegis-notify`、`aegis-commerce`、`aegis-theme`、`aegis-runtime`。HTTP には依存しない。
- **`aegis-server` は HTTP 層**: ハンドラ・認可・エクストラクタ・CLI・組み立て（`main.rs`）。
- **設定**: 各クレートが自分のセクションの型を持つ（`aegis_db::DatabaseConfig`、`SecurityConfig` は `aegis-ast`、`ThemeConfig` は `aegis-theme`、`PluginsConfig` / `RuntimeConfig` は `aegis-runtime` など）。`aegis.toml` 全体の `AegisConfig` は `aegis-server/src/config.rs`。
  - 全構造体が `#[serde(default)]` 付きで、省略した項目は `Default` 実装の値になる。**設定項目を足すときは必ず `Default` に既定値を入れる**。
  - `AegisConfig::load_or_default` はファイルが無いときだけ既定値にフォールバックし、パースエラーでは起動を止める（設定ミスに気付かず既定値で動き続けないため）。
- **axum のエクストラクタはサーバー側に置く**: 監査の `AuditContext` は `aegis_audit::AuditOrigin` の newtype（孤児ルールのため）で `Deref` する。ドメインサービスは依存を構造体で受け取る（例: `CommerceService` は `CommerceDeps`、`AppState::commerce_deps()`）。

---

## 2. コンテンツと DB

### 2.1 テーブルとデータ

- スキーマ `content_type: article` のテーブルは `content_article`。本文は Block AST の JSON を `blocks_json TEXT NOT NULL DEFAULT '[]'` に保存する。描画済み HTML は保存しない（`[security]` の変更を保存済みのコンテンツにも反映するため）。
- システムテーブルは `_aegis_` 接頭辞（`_aegis_users`、`_aegis_user_identities`、`_aegis_audit_logs`、`_aegis_orders`、`_aegis_settings` など）。
- `_aegis_settings` は Admin Studio から変更するサイト全体の設定（`name` → `value`。今はサイト名 `site.title` と favicon `site.favicon`）。カラム名は MySQL の予約語 `key` を避けて `name` にした。上書きは SQLite / PostgreSQL が `ON CONFLICT`、MySQL が `ON DUPLICATE KEY UPDATE`。設定ファイルの値は既定値として残し、行を消すと設定ファイルの値に戻る（プラグイン設定のようにキャッシュへ置くと、メモリのキャッシュでは再起動と TTL で消えるため DB にした）。
- コンテンツ作成 API（`POST /api/v1/contents/{type}`）のペイロードはトップレベルに `title` / `slug` / `status` / `blocks` / `author_id` とカスタムフィールドを持つ。応答ではカスタムフィールドは `fields`、解決済みリレーションは `relations` に入る（[Admin Studio の型](../admin/README.md#-設計メモ)に注意）。

### 2.2 `DatabaseAdapter`

- SQLite / PostgreSQL / MySQL の違い（プレースホルダ `?1` / `$1` / `?`、カラム情報の取得 `PRAGMA table_info` / `information_schema.columns`、接続プール）は `aegis_db::DatabaseAdapter` の実装に閉じ込める。ハンドラはアダプタ経由でのみ DB を扱う。
- **スキーマ進化**: 起動時の `run_migrations` は `CREATE TABLE IF NOT EXISTS` に加え、既存テーブルのカラムを調べて YAML に増えたフィールドを `ALTER TABLE ... ADD COLUMN` してからインデックスを作る。システムテーブルの列追加（例: `_aegis_users.avatar_key`）も同じ方式（SQLite は `PRAGMA table_info`、PostgreSQL は `ADD COLUMN IF NOT EXISTS`、MySQL は `information_schema`）。MySQL は `CREATE INDEX IF NOT EXISTS` が無いため、重複エラー（1061）だけを無視する。
- **カスタムフィールドの型**: `CustomFieldValue::from_json` がスキーマの型に合わせて変換する（jsonb → JSON、integer → i64、boolean → bool、その他は文字列）。変換できない値は `AegisError::Validation`（400）。SQLite は文字列で保存し、`null` は NULL で保存する（以前は空文字になり、数値の計算で 0 扱いになっていた）。読み出しは各アダプタの `custom_fields_from_row` で型ごとにデコードし、NULL は `fields` に含めない。**フィールド型を足すときは `SchemaMigration` の型対応・`CustomFieldValue`・各アダプタのデコードを揃えて変える**。
- **リレーション**: `type: relation` / `target: <schema>.<column>` のフィールドは、応答の `relations` に参照先（`id` / `content_type` / `title` / `slug`）を入れる。解決は `resolve_relations_for_records` がリレーション列ごとに `IN (...)` の 1 回でまとめて行う（単一レコードも同じ関数）。参照の値は `relation_ref`（空文字・文字列以外は参照なし）。
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

### 3.2 テーマエンジン (`aegis-theme`)

- テンプレートの探索は「`register_template` で登録したもの」→「`themes/{active}/templates/`」→「バイナリに埋め込んだ組み込みテンプレート」の順。
- `ThemeEngine` は内部状態を `RwLock` で持ち、`set_active_theme` で再起動せずにテーマを切り替える。コンパイル済みの MiniJinja 環境は再利用し、テーマの切り替え・テンプレートの登録で破棄する（`[theme].reload_templates = true` なら毎回読み直す）。
- フィルター（`date_short`、`truncate` など）は引数を `&str` ではなく `minijinja::Value` で受ける。`&str` だと `None` や未定義の値で `value is not a string` の実行時エラー（500）になる。
- テーマパッケージ（`theme.json`・`preview.*`・`templates/`・`assets/`）の検証は `ThemeEngine::validate_theme_package`。ストアからの ZIP 導入はこれで検証してから `themes/<id>/` に展開する前提。
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
- 配信は `tokio::spawn` で非同期・メモリ上（再起動で失われる）。`SendError::permanent`（SMTP の 5xx・宛先不正）は再試行しない。
- **プラグインのチャネル**: `NotificationMessage` / `Delivered` / `SendError` は `aegis_core::notify` にあり、ランタイムと共有する。`aegis-notify` はランタイムに依存せず、`plugin:<名前>` のチャネルは `NotificationService::from_config_with_plugins` に渡すファクトリでサーバー（`notify_plugins.rs`）が作る。起動時に `validate_plugin_channels` で読み込み済み・フック宣言済みかを確かめる。`Skipped` は配信結果にも監査ログにも出さない。Wasm の失敗はフック未宣言（`PermissionDenied`）だけ permanent、タイムアウト・トラップは再試行する。
- **監査イベントの通知**: `AuditNotificationSink` を `AuditService::add_sink` で加える。`NotificationService` は監査ログを使うため後から作り、`attach` で渡す（`OnceLock`）。`notification.send` 自体は通知しない（配信の失敗が通知を呼び続けるため）。`max_per_hour` はルールごとにメモリ上で数える。ログインのロックは `session.create` の `failure` で `details.locked` があるもの（ロックがかかった 1 回）、ロック中の試行は `denied` + `details.error = "rate_limited"`。
- プラグインの設定値の空文字列は、`config_get` の ABI で長さ 0 =「値なし」のため `None` として届く。

---

## 9. EC (`aegis-commerce`)

- 金額はすべて通貨の最小単位の `i64`（Stripe の `unit_amount` と同じ）。表示は `aegis_core::format_money` / テンプレートの `money` フィルター / Admin Studio の `formatMoney`。
- 在庫の確保は `UPDATE ... SET stock = stock - ? WHERE id = ? AND (stock IS NULL OR stock >= ?)` を注文の INSERT と同じトランザクションで行う（売り越し防止）。`stock` が NULL の商品は無制限。
- 状態の変更は `transition_order`（`WHERE status = 前の状態`）で、Webhook の重複・期限切れ処理との競合を防ぐ。プラグインへの通知（`on-order-status-changed`）は `tokio::spawn` で応答を待たない。
- `calculate-order` の結果は `adjustments` / `metadata` だけを反映し、合計はホストが計算し直す。拒否メッセージは顧客に見せるため、ホストが付けるプラグイン名の接頭辞を外す（ログには残す）。
- Stripe の注文は支払い済みになった時点（Webhook）で `order.placed` を送る（決済ページで離脱した注文に受付メールを送らない）。注文確認ページのトークンは平文を保存しないので、URL は `order.placed` のメールにしか入らない。
- 結合テストは `aegis-server/src/handlers/shop_tests.rs`。

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
- AssemblyScript SDK の `escapeJson` / `findStringField` は 1 文字ずつ連結するため入力の大きさの二乗で遅い（`on_before_save` で約 2ms、`official-code-enhancer` と併用すると `on_render_page` が約 13ms）。
