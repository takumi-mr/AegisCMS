# AegisCMS 設定リファレンス (Configuration Reference)

AegisCMS コアサーバー (`aegis-server`) の設定ファイル (`config/*.toml`) で指定できる項目の一覧です。
設定値の定義元は [`crates/aegis-server/src/config.rs`](../crates/aegis-server/src/config.rs) の `AegisConfig` です。各セクションの型はそれを使うクレートの `config` モジュールにあります（例: `[database]` は `aegis-db`、`[storage]` は `aegis-storage`、`[commerce]` は `aegis-commerce`）。

---

## 1. 設定ファイルの読み込み

### 1.1 読み込むファイルの決定

| 優先度 | 指定方法 | 例 |
| --- | --- | --- |
| 1 | CLI 引数 `-c` / `--config` | `aegis-server --config config/prod.toml run` |
| 2 | 環境変数 `AEGIS_CONFIG` | `AEGIS_CONFIG=/app/config/docker.toml` |
| 3 | 既定値 | `config/default.toml` |

同梱の設定ファイル:

- [`config/default.toml`](../config/default.toml): ローカル開発用
- [`config/docker.toml`](../config/docker.toml): コンテナ用 (`Dockerfile` が `AEGIS_CONFIG` に設定)

### 1.2 CLI / 環境変数によるオーバーライド

`run` サブコマンドに限り、以下で `[server]` の値を上書きできます。

| CLI 引数 | 環境変数 | 上書き対象 |
| --- | --- | --- |
| `-p`, `--port <PORT>` | `PORT` | `server.port` |
| `-H`, `--host <HOST>` | `HOST` | `server.host` |

ログレベルは標準の `RUST_LOG` 環境変数で制御します（未指定時: `info,aegis_server=debug,tower_http=debug`）。

### 1.3 パース時の注意事項

- **すべてのセクション・項目は省略可能**です。省略した項目には下表の既定値が使われます。
  - 例: `[server]` に `port = 9000` だけを書いた場合、`host` は既定値の `"127.0.0.1"` になります。
- **ファイルが存在しない場合**は警告ログ (`Config file ... not found`) を出して、組み込みの既定値で起動します。
- **ファイルの読み込みやパースに失敗した場合**（型の誤り、未知の `backend` 値など）は、エラーメッセージを表示して起動を中止します（終了コード 1）。
- バックエンド種別（`backend`）は TOML 上では下表に記載した値のみ受け付けます（大文字・小文字も区別）。`"r2"` や `"minio"` などの別名は使えません。

---

## 2. 設定項目一覧

### 2.1 `[server]` — HTTP サーバー

| キー | 型 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `host` | string | `"127.0.0.1"` | リッスンするアドレス。コンテナでは `"0.0.0.0"` を指定 |
| `port` | integer (u16) | `8080` | リッスンするポート |
| `base_path` | string | `""` | サブパス配下で公開する場合のパス（例: `"/cms"`）。空ならルート `/` で公開。使える文字は英数字と `- . _ ~`（不正な値はサーバーが起動しない）。[4.2](#42-サブパスで公開する) を参照 |

### 2.2 `[database]` — データベース

| キー | 型 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `backend` | `"sqlite"` \| `"postgres"` \| `"mysql"` | `"sqlite"` | 使用する RDBMS |
| `url` | string | `"sqlite://aegis.db?mode=rwc"` | 接続 URL |
| `max_connections` | integer (u32) | `10` | 接続プールの最大接続数 |

`backend` が `"sqlite"`（既定値）の場合は、`url` のスキームから実際のバックエンドが推論されます。

SQLite のファイルは WAL モード（`journal_mode = WAL`、`synchronous = NORMAL`）で開きます。読み込みが書き込みを待たず、コミットごとのディスク同期も減ります（監査ログのようにリクエストのたびに書く処理が詰まりにくい）。DB ファイルの横に `<名前>-wal` / `<名前>-shm` ができるので、バックアップやボリュームでは 3 つをまとめて扱ってください。WAL は NFS などのネットワークファイルシステムでは使えないため、DB はローカルのディスク（コンテナならボリューム）に置きます。

| `url` のスキーム | 推論されるバックエンド |
| --- | --- |
| `postgres://`, `postgresql://` | PostgreSQL |
| `mysql://`, `mariadb://` | MySQL |
| それ以外 | SQLite |

```toml
[database]
url = "postgres://user:password@localhost:5432/aegis"  # backend を省略しても postgres になる
max_connections = 10
```

### 2.3 `[storage]` — アセットストレージ

| キー | 型 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `backend` | `"local"` \| `"s3"` | `"local"` | アップロードファイルの保存先 |
| `max_upload_mb` | integer (u64) | `10` | アップロード 1 回あたりの最大サイズ（MB）。超過すると `413 Payload Too Large`。リバースプロキシを置く場合はその上限（nginx の `client_max_body_size` など）も合わせる |

#### `[storage.local]` — ローカルディスク

`backend = "local"` のとき使用されます。サーバー起動時に `upload_dir` が作成され、`public_url_prefix` のパスで静的配信されます。

| キー | 型 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `upload_dir` | path | `"data/uploads"` | 保存先ディレクトリ |
| `public_url_prefix` | string | `"/uploads"` | 配信 URL のプレフィックス（ルーティングにも使用） |

#### `[storage.s3]` — S3 互換オブジェクトストレージ

`backend = "s3"` のとき使用されます。AWS S3 / Cloudflare R2 / MinIO / Wasabi などに対応しています。

| キー | 型 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `bucket` | string | `"aegis-bucket"` | バケット名 |
| `region` | string | `"auto"` | リージョン（SigV4 署名に使用。R2 は `"auto"`） |
| `endpoint` | string | なし | カスタムエンドポイント。R2 / MinIO では必須。未指定時は `https://{bucket}.s3.{region}.amazonaws.com` |
| `access_key_id` | string | `""` | アクセスキー ID |
| `secret_access_key` | string | `""` | シークレットアクセスキー |
| `force_path_style` | bool | `false` | `true` でパススタイル URL (`{endpoint}/{bucket}/{key}`) を使用。MinIO では `true` |
| `public_url_prefix` | string | なし | 公開 URL のプレフィックス（CDN / カスタムドメイン）。未指定時はエンドポイント URL を返す |

```toml
[storage]
backend = "s3"

[storage.s3]
bucket = "aegis-assets"
region = "auto"
endpoint = "https://<account_id>.r2.cloudflarestorage.com"
access_key_id = "..."
secret_access_key = "..."
force_path_style = false
public_url_prefix = "https://cdn.example.com"
```

### 2.4 `[cache]` — キャッシュ & セッション

| キー | 型 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `backend` | `"memory"` \| `"redis"` | `"memory"` | `memory`: プロセス内 TTL キャッシュ（1 台構成専用） / `redis`: 複数インスタンスでセッション・変更の知らせを共有 |
| `default_ttl_secs` | integer (u64) | `3600` | キャッシュ・セッションの既定 TTL（秒）。ログイン時に指定する `ttl_secs` の上限も兼ねる |

- キャッシュと同じバックエンドで、Admin Studio での変更（プラグインの切り替え・並べ替え・設定値）をサーバー間で知らせます（[プラグインフックリファレンス 5.3](./plugin_hooks.md#53-複数台での反映)）。`redis` では Pub/Sub のチャネル `aegis:events` を使います。
- **`memory` は 1 台構成専用です。** 起動時に DB（`_aegis_settings` の `server.memory_cache_instance`）に印を書き、10 秒ごとに書き直します。同じ DB で `memory` のサーバーがすでに動いていると、2 台目は起動しません（`Another AegisCMS server with [cache].backend = "memory" is running on this database`）。複数台で動かすときは `redis` にしてください。
  - 普通に止めた（Ctrl+C・SIGTERM）サーバーは印を消すので、すぐに再起動できます。強制終了した直後（30 秒以内）の再起動は、前のサーバーが止まっていることを確かめるため 20 秒ほど待ってから起動します。

#### `[cache.redis]`

`backend = "redis"` のとき使用されます。

| キー | 型 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `url` | string | `"redis://127.0.0.1:6379"` | Redis 接続 URL |
| `timeout_ms` | integer (u64) | `2000` | Redis への接続 1 回あたりのタイムアウト、および各コマンドの応答タイムアウト（ミリ秒）。接続に失敗した場合は最大 2 回まで再試行し（待機は最大 1 秒）、それでも失敗すれば起動エラー・コマンドエラーになる |

### 2.5 `[paths]` — ディレクトリ

| キー | 型 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `schemas_dir` | path | `"schemas"` | 宣言的スキーマ (`*.yaml`) の読み込み元 |
| `plugins_dir` | path | `"plugins"` | Wasm プラグインのインストール先（`<plugins_dir>/<name>/` に 1 プラグイン）。読み込むのは有効化リスト（`[plugins].enabled`、または Admin Studio で切り替えた一覧）にあるものだけ |
| `admin_dist_dir` | path | `"admin/dist"` | Admin Studio ビルド成果物。存在すれば `/` で SPA として配信 |

### 2.6 `[runtime]` — Wasm プラグインランタイム

| キー | 型 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `enable_wasm_plugins` | bool | `true` | `false` でプラグインの読み込みを無効化 |
| `default_max_memory_mb` | integer (u32) | `16` | マニフェストが `max_memory_mb` を省略したプラグインのメモリ上限（MiB） |
| `default_max_fuel_units` | integer (u64) | `100000000` | マニフェストが `max_fuel_units` を省略したプラグインの Fuel 上限 |
| `default_timeout_ms` | integer (u64) | `250` | マニフェストが `execution_timeout_ms` を省略したプラグインのタイムアウト（ミリ秒） |
| `instance_pool_size` | integer (u32) | `64` | Wasm インスタンスのプール（Wasmtime のプーリングアロケータ）の枠数 = 同時に実行できるフック呼び出しの数。超えた呼び出しは空きを待つ。`0` で無効（呼び出しごとにメモリを確保・解放する） |
| `pool_max_memory_mb` | integer (u32) | `64` | プール 1 枠あたりの線形メモリの上限（MiB）。マニフェストの `max_memory_mb` がこれを超えるプラグインは読み込めない（起動しない） |

> [!NOTE]
> `default_*` は既定値であり上限ではありません。プラグインの `plugin.manifest.json` の `resource_limits` に値があればそちらが優先され、省略した項目（または `resource_limits` 全体）だけがこの設定で補われます（[プラグインフックリファレンス](./plugin_hooks.md#4-リソース制限-resource_limits) を参照）。読み込んだプラグインごとの実際の値は起動時の `INFO` ログに出力されます。

> [!NOTE]
> **インスタンスのプール**: フックは今までどおり呼び出しごとに新しいインスタンスで実行します（前の呼び出しの状態は残りません。返却された枠は Wasmtime がゼロに戻します）。プールは線形メモリ・テーブル・非同期スタックの確保を起動時に済ませておき、呼び出しごとの `mmap` / `munmap` を無くすためのものです。同時実行時のカーネル内の競合が無くなり、`official-code-enhancer` を有効にした記事ページは約 1.7 倍速くなりました。
> 枠ごとに約 6 GiB の**仮想**アドレス空間を予約します（64 枠で約 390 GiB。境界チェックを省いた高速なコードにするため。実メモリは使った分だけ）。`ulimit -v` などで仮想メモリを制限している環境では予約に失敗し、警告を出して従来の方式で動きます。枠数を減らすか `0` にしてください。

#### `[plugins]` — プラグインの有効化

`plugins_dir` にインストールしたプラグインのうち、読み込むものを指定します（テーマの `[theme].active` と同じく、インストールと有効化を分けています）。

| キー | 型 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `enabled` | array of string | `[]` | 有効にするプラグイン名（マニフェストの `name` = `plugins_dir` 直下のディレクトリ名）。フックはこの順に実行される |
| `settings.<name>` | table | なし | プラグインごとの設定値。プラグインは `host.config_get`（SDK の `config::get`）で読む。文字列の値 `env:NAME` は環境変数 `NAME` の値になる（API キーなどを設定ファイルに書かないため） |

```toml
[plugins]
enabled = ["official-shop-rules"]

[plugins.settings.official-shop-rules]
shipping_fee = 600
coupons = "WELCOME10:10%"
```

- 列挙したプラグインが未インストール・読み込めない（マニフェストの不備、ホストに無い関数のインポートなど）場合、名前の形式が不正・重複している場合は**サーバーが起動しません**。
- `[runtime].enable_wasm_plugins = false` の場合は読み込みません。
- Admin Studio（`PUT /api/v1/plugins/{name}/enabled`・`PUT /api/v1/plugins/order`）で有効・無効や順序を変えると、その一覧が DB に保存され、以後は `enabled` より優先されます（[プラグインフックリファレンス 5.1](./plugin_hooks.md#51-admin-studio-からの有効無効の切り替え)）。Admin Studio の「設定ファイルの値に戻す」（`DELETE /api/v1/plugins/order`）で `enabled` に戻ります。
- 状態は `aegis plugin list` で確認できます（[プラグインフックリファレンス](./plugin_hooks.md#5-インストールと有効化)）。

### 2.7 `[security]` — HTML 出力のセキュリティ / 最適化

コンテンツ作成時に、レンダリング済み HTML へストリーミングリライター (`lol-html`) で適用されます。
「外部リンク」は `http://` / `https://` / `//` で始まる絶対 URL のうち、`internal_domains` に一致しないものです（相対パス、`#`, `mailto:`, `tel:` などは対象外）。

| キー | 型 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `enforce_external_link_rel` | bool | `true` | 外部リンクの `<a>` に `rel="noopener noreferrer"` を付与（既存の `rel` とマージ） |
| `open_external_links_in_new_tab` | bool | `false` | `target` 未指定の外部リンクに `target="_blank"` を付与 |
| `internal_domains` | string[] | `[]` | 内部扱いするドメイン（サブドメインも一致、大文字・小文字は区別しない）。これらへのリンクは上記の対象外 |
| `enforce_image_lazy_loading` | bool | `true` | `loading` 未指定の `<img>` に `loading="lazy"` を付与 |
| `enforce_image_async_decoding` | bool | `true` | `decoding` 未指定の `<img>` に `decoding="async"` を付与 |

### 2.8 `[audit]` — 監査ログ

操作の監査イベントを記録します。記録対象・出力形式・参照 API の詳細は [監査ログリファレンス](./audit.md) を参照してください。

| キー | 型 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `enabled` | bool | `true` | `false` で監査ログの記録を無効化 |
| `sinks` | (`"database"` \| `"log"` \| `"prometheus"`)[] | `["database"]` | 出力先（複数指定可）。`database`: DB テーブル + 参照 API / `log`: 構造化ログ / `prometheus`: `/metrics` のカウンタ |
| `trust_forwarded_for` | bool | `false` | `true` で `X-Forwarded-For` の先頭アドレスをクライアント IP として扱う（監査ログの記録と、`[auth.login_rate_limit]` の IP 単位の制限に使用）。信頼できるリバースプロキシ配下でのみ有効化 |
| `retention_days` | integer (u32) | `0` | `database` 出力先の監査ログを保持する日数。これより古いログを定期的に削除（`0` で無期限） |
| `purge_interval_secs` | integer (u64) | `3600` | 保持期間切れのログを削除する間隔（秒、最小 60）。起動直後にも 1 回実行 |

### 2.9 `[auth.login_rate_limit]` — ログイン試行のレート制限

`POST /api/v1/auth/session` の認証失敗をユーザー名・クライアント IP 単位で数え、上限に達すると一定時間 `429 Too Many Requests` で拒否します（詳細は [認証ドキュメント](./authentication.md#5-セキュリティ上の挙動)）。失敗回数は `[cache]` のバックエンドに保存するため、複数インスタンスで制限を共有するには `backend = "redis"` にしてください。

| キー | 型 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `enabled` | bool | `true` | `false` でレート制限を無効化 |
| `max_failures_per_user` | integer (u64) | `5` | 同じユーザー名（大文字・小文字を区別しない）で `window_secs` 内に失敗できる回数。到達でそのユーザー名をロック（`0` で無制限） |
| `max_failures_per_ip` | integer (u64) | `20` | 同じクライアント IP から `window_secs` 内に失敗できる回数。到達でその IP をロック（`0` で無制限） |
| `window_secs` | integer (u64) | `900` | 失敗回数を数える期間（秒）。最初の失敗から数え、過ぎると 0 に戻る |
| `lockout_secs` | integer (u64) | `900` | ロックの継続時間（秒）。ロック中は正しいパスワードでもログインできない |

### 2.10 `[auth.oidc]` — シングルサインオン (OpenID Connect)

外部 IdP によるログイン（認可コードフロー + PKCE）。使い方は [認証ドキュメント](./authentication.md#6-シングルサインオン-openid-connect) を参照してください。`enabled = true` で必須項目が欠けている、またはロール名が不正な場合はサーバーが起動しません。

| キー | 型 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `enabled` | bool | `false` | `true` で SSO を有効化 |
| `display_name` | string | `"SSO"` | ログイン画面のボタン名（`Sign in with ...`） |
| `issuer_url` | string | `""` | IdP の Issuer（必須）。`{issuer_url}/.well-known/openid-configuration` からエンドポイントを取得する |
| `client_id` | string | `""` | クライアント ID（必須） |
| `client_secret` | string | `""` | クライアントシークレット |
| `redirect_url` | string | `""` | IdP に登録したコールバック URL（必須）。`https://<ホスト>/api/v1/auth/oidc/callback` |
| `scopes` | array of string | `["openid", "profile", "email"]` | 要求するスコープ（`openid` は常に付ける） |
| `username_claim` | string | `"preferred_username"` | 新規ユーザーのユーザー名にするクレーム（無ければ `email`） |
| `auto_create_users` | bool | `true` | 未登録のユーザーを初回ログイン時に作成する。`false` なら `aegis user link-oidc` で紐付けたユーザーのみ |
| `default_role` | string | `"viewer"` | 新規ユーザーのロール。`role_claim` 使用時はどのグループにも一致しない場合のロール |
| `role_claim` | string | `""` | ロールを決めるクレーム（例: `groups`）。空ならロールは AegisCMS のユーザーアカウントで決まる |
| `role_mapping` | table | `{}` | `role_claim` の値 → ロール（`admin` / `editor` / `viewer`）。複数一致した場合は最も強いロール |
| `post_login_redirect` | string | `"/admin/"` | ログイン後に戻る Admin Studio の URL（`#session=` / `#login_error=` を付ける）。`/` で始まる場合は `[server].base_path` が付く。Admin Studio を別オリジン（開発サーバーなど）で動かす場合は絶対 URL |

> [!NOTE]
> `client_secret` は設定ファイルに平文で書くため、ファイルの権限を制限してください。

### 2.11 `[auth.registration]` — アカウント登録

Admin Studio のログイン画面からのアカウント登録（メールアドレスの確認つき）。使い方は [認証ドキュメント](./authentication.md#7-アカウント登録) を参照してください。`enabled = true` のとき、通知（`[notifications]`）が無効、確認リンクの開き先が決まらない、`default_role` が `admin` または不明なロールの場合はサーバーが起動しません。

| キー | 型 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `enabled` | bool | `false` | `true` でアカウント登録を有効化 |
| `admin_url` | string | `""` | 確認リンクの開き先の Admin Studio の URL（例: `"https://cms.example.com/admin/"`。`#verify-email=<トークン>` を付ける）。空なら `[commerce].public_url` + `/admin/`。リクエストの Host からは作らない |
| `default_role` | string | `"viewer"` | 登録したユーザーのロール（`editor` / `viewer`） |
| `require_approval` | bool | `true` | `true` なら確認後のアカウントを無効の状態で作り、admin が有効にするまでログインできない |
| `allowed_email_domains` | array of string | `[]` | 登録できるメールアドレスのドメイン（`"example.com"` ならサブドメインも可）。空ならどのドメインでも登録できる |
| `token_ttl_secs` | integer (u64) | `86400` | 確認リンクの有効期間（秒） |
| `max_per_ip_per_hour` | integer (u64) | `5` | 同じクライアント IP から 1 時間に受け付ける申し込みの数（`0` で無制限） |

### 2.12 `[commerce]` — EC

カート・注文・在庫・決済（[EC ドキュメント](./commerce.md)）。`enabled = true` で設定が不正な場合（通貨コードの形式、Stripe のキーが無いなど）や商品スキーマに `price`（integer）が無い場合はサーバーが起動しません。

| キー | 型 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `enabled` | bool | `false` | `true` でカート・注文を有効化（無効でも商品一覧・商品ページは表示される） |
| `product_type` | string | `"product"` | 商品のコンテンツタイプ |
| `currency` | string | `"jpy"` | ISO 4217 の通貨コード（小文字）。金額はすべてこの通貨の最小単位の整数 |
| `public_url` | string | `""` | 公開サイトの URL（決済後の戻り先。サブパス運用なら `base_path` 込み）。空ならリクエストの Host（`trust_forwarded_for` なら `X-Forwarded-Host` / `-Proto`）から |
| `provider` | string | `"manual"` | `manual`（銀行振込・代引きなど。管理者が入金を確認）または `stripe` |
| `reservation_minutes` | integer | `60` | Stripe の決済ページの有効期限 = 在庫を確保しておく時間（Stripe では 30〜1440） |
| `max_quantity_per_item` | integer | `99` | 1 商品あたりの最大数量 |
| `max_items` | integer | `50` | 1 注文の最大商品数 |
| `max_orders_per_ip_per_hour` | integer | `20` | 同じ IP からの 1 時間あたりの注文数（0 で無制限。`[cache]` で数える） |

#### `[commerce.manual]`

| キー | 型 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `instructions` | string | 案内文 | 注文確認ページに表示する支払い方法の案内（振込先など） |
| `payment_due_hours` | integer | `72` | 未払いの注文を取り消して在庫を戻すまでの時間 |

#### `[commerce.stripe]`

| キー | 型 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `secret_key` | string | `""` | シークレットキー。空なら環境変数 `AEGIS_STRIPE_SECRET_KEY` |
| `webhook_secret` | string | `""` | Webhook の署名シークレット（`whsec_...`）。空なら環境変数 `AEGIS_STRIPE_WEBHOOK_SECRET` |
| `api_base` | string | `"https://api.stripe.com"` | API の URL（テスト用） |
| `async_payment_due_hours` | integer | `72` | コンビニ払いなど後払いの支払い期限（その間は在庫を確保したまま） |

### 2.13 `[notifications]` — 通知（メール）

注文の受付・入金確認・発送・取り消しを顧客と運営者に知らせます（[通知ドキュメント](./notifications.md)）。`enabled = true` で設定が不正な場合（`email` チャネルで `smtp_host` / `from` が無い、アドレスの形式が不正など）はサーバーが起動しません。

| キー | 型 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `enabled` | bool | `false` | `true` で通知を送る |
| `channels` | string[] | `["email"]` | 配信チャネル。`email`（SMTP）/ `log`（サーバーのログ。動作確認用）/ `plugin:<名前>`（`send-notification` フックを持つ Wasm プラグイン。`[plugins].enabled` にあること） |
| `locale` | string | `"en"` | 組み込みテンプレートの言語（`en` / `ja`） |
| `templates_dir` | string | なし | テンプレートの置き場所（`<種類>.subject` / `<種類>.txt` / `<種類>.html`）。無いファイルは組み込みを使う。Admin Studio で編集したものはこれより優先する |
| `disabled_events` | string[] | `[]` | 送らない通知の種類（`order.shipped` など） |
| `max_attempts` | integer | `3` | 一時的な失敗（SMTP の 4xx・接続失敗）の試行回数（1〜10。間隔は 2 秒・8 秒・32 秒…） |
| `html` | bool | `true` | HTML の本文も作る（メールはプレーンテキストと HTML の両方を送る。[通知 3.1](./notifications.md#31-html-メール)） |

#### `[notifications.queue]`

送信待ち（[通知 4](./notifications.md#4-配信と信頼性)）。

| キー | 型 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `persistent` | bool | `true` | 送信待ちを DB（`_aegis_notification_queue`）に保存する。再起動しても失われず、複数台では 1 台だけが送る。`false` ならメモリ上（再起動で失われる） |
| `poll_interval_secs` | integer | `10` | 再試行の時刻を迎えた通知を確かめる間隔（秒、1〜3600）。新しい通知は待たずに送る |
| `retention_days` | integer | `7` | 送れなかった通知を残す日数（Admin Studio から再送できる期間）。0 なら消さない |

#### `[notifications.email]`

| キー | 型 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `smtp_host` | string | `""` | SMTP サーバー（必須） |
| `smtp_port` | integer | `0` | ポート。0 なら `tls` に応じて 587 / 465 / 25 |
| `tls` | string | `"starttls"` | `starttls`（STARTTLS 必須）/ `tls`（最初から TLS）/ `none`（暗号化しない。ローカルのリレー用） |
| `username` | string | `""` | SMTP 認証のユーザー名（空なら認証しない） |
| `password` | string | `""` | SMTP 認証のパスワード。空なら環境変数 `AEGIS_SMTP_PASSWORD` |
| `from` | string | `""` | 差出人（`"Shop <shop@example.com>"`。必須） |
| `reply_to` | string | `""` | 返信先（空なら差出人） |
| `staff_recipients` | string[] | `[]` | 運営者向けの通知（新しい注文・取り消し）の宛先。空なら運営者には送らない |
| `timeout_secs` | integer | `15` | 接続・送信のタイムアウト（秒） |

#### `[[notifications.audit.rules]]`

監査イベント（管理者のログイン・ログインのロックなど）を通知する条件（[通知ドキュメント 2.1](./notifications.md#21-監査イベントの通知)）。`[audit].enabled = false` のままルールを書くとサーバーが起動しません。

| キー | 型 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `action` | string | （必須） | 監査ログの操作（`session.create` など。`notification.send` は不可） |
| `outcomes` | string[] | `[]` | `success` / `failure` / `denied`（空ならすべて） |
| `roles` | string[] | `[]` | 操作者のロール（空ならすべて） |
| `details` | table | `{}` | `details` の値（`"*"` はキーがあれば何でも） |
| `title` | string | `""` | 件名・本文の見出し |
| `recipients` | string[] | `[]` | 宛先（空なら `staff_recipients`） |
| `max_per_hour` | integer | `20` | 1 時間あたりの上限（0 で無制限） |

---

### 2.14 `[auth.saml]` — シングルサインオン (SAML 2.0)

SAML 2.0 の IdP によるログイン（SP-initiated。AuthnRequest は HTTP-Redirect、応答は HTTP-POST）。使い方は [認証ドキュメント](./authentication.md#8-シングルサインオン-saml-20) を参照してください。`enabled = true` で必須項目が欠けている、証明書を読めない、またはロール名が不正な場合はサーバーが起動しません。

| キー | 型 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `enabled` | bool | `false` | `true` で SAML ログインを有効化 |
| `display_name` | string | `"SSO"` | ログイン画面のボタン名（`Sign in with ...`） |
| `entity_id` | string | `""` | AegisCMS（SP）の Entity ID（必須）。AuthnRequest の Issuer、アサーションの Audience。慣例でメタデータの URL（`https://<ホスト>/api/v1/auth/saml/metadata`） |
| `acs_url` | string | `""` | Assertion Consumer Service の URL（必須）。`https://<ホスト>/api/v1/auth/saml/acs` |
| `idp_entity_id` | string | `""` | IdP の Entity ID（必須）。応答・アサーションの Issuer と一致しなければ拒否 |
| `idp_sso_url` | string | `""` | IdP の Single Sign-On サービスの URL（必須。HTTP-Redirect バインディング） |
| `idp_certificates` | array of string | `[]` | IdP の署名用証明書（必須）。PEM（`-----BEGIN CERTIFICATE-----`）、PEM の公開鍵、または IdP のメタデータの `<X509Certificate>` の Base64。鍵の切り替え中は新旧を並べる。RSA 2048 ビット以上 |
| `name_id_format` | string | `""` | AuthnRequest で求める NameID の形式（例: `urn:oasis:names:tc:SAML:2.0:nameid-format:persistent`）。空なら IdP に任せる。`transient` は使えない |
| `username_attribute` | string | `""` | 新規ユーザーのユーザー名にする属性。空なら NameID |
| `email_attribute` | string | `"email"` | メールアドレスの属性（無ければ、NameID の形式が `emailAddress` のとき NameID） |
| `display_name_attribute` | string | `"displayName"` | 表示名の属性 |
| `auto_create_users` | bool | `true` | 未登録のユーザーを初回ログイン時に作成する。`false` なら `aegis user link-saml` で紐付けたユーザーのみ |
| `default_role` | string | `"viewer"` | 新規ユーザーのロール。`role_attribute` 使用時はどの値にも一致しない場合のロール |
| `role_attribute` | string | `""` | ロールを決める属性（例: `groups`）。空ならロールは AegisCMS のユーザーアカウントで決まる |
| `role_mapping` | table | `{}` | `role_attribute` の値 → ロール（`admin` / `editor` / `viewer`）。複数一致した場合は最も強いロール |
| `post_login_redirect` | string | `"/admin/"` | ログイン後に戻る Admin Studio の URL（`[auth.oidc]` と同じ） |

属性は `Name` で探し、一致するものが無ければ `FriendlyName` で探します（Entra ID の `http://schemas.xmlsoap.org/ws/2005/05/identity/claims/emailaddress` のような URI 形式の名前もそのまま書けます）。

### 2.15 `[comments]` — コメント

公開ページへの読者のコメント（[コメントのドキュメント](./comments.md)）。`enabled = true` で値が不正な場合（`content_types` が空、`max_length` が範囲外など）や `content_types` のスキーマが無い場合はサーバーが起動しません。

| キー | 型 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `enabled` | bool | `false` | `true` でコメントを受け付ける |
| `content_types` | string[] | `["article"]` | コメントを受け付けるコンテンツの種類 |
| `max_depth` | integer | `3` | 返信できる深さ（0 で返信なし。最大 10） |
| `max_length` | integer | `5000` | 本文の最大文字数（最大 100,000） |
| `close_after_days` | integer | `0` | 公開からこの日数を過ぎたら新しいコメントを受け付けない（0 で閉じない） |
| `notify_author` | bool | `true` | コメントが付いたこと（承認待ちを含む）をコンテンツの著者に知らせる（`comment.posted` / `comment.pending`） |
| `notify_staff` | bool | `true` | 承認待ちのコメントを運営者に知らせる（`comment.pending`） |

#### `[comments.moderation]`

| キー | 型 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `require_approval` | bool | `true` | 承認してから公開する。Admin Studio から切り替えられる（そちらが優先） |
| `auto_approve_known` | bool | `true` | 承認制のとき、同じ名前とメールアドレスの承認済みコメントがある人は承認を待たずに公開する。Admin Studio から切り替えられる |
| `max_links` | integer | `2` | 本文のリンクがこれより多いと承認待ち |
| `hold_words` | string[] | `[]` | 含むと承認待ちにする語（大文字・小文字を区別しない） |
| `spam_words` | string[] | `[]` | 含むとスパムにする語 |
| `max_per_ip_per_hour` | integer | `10` | 同じ IP からの 1 時間あたりの投稿数（0 で無制限。`[cache]` で数える） |

## 3. 完全な設定例

すべての項目を明示した、そのままパース可能な設定例です。

```toml
[server]
host = "127.0.0.1"
port = 8080
base_path = ""

[database]
backend = "sqlite"
url = "sqlite://aegis.db?mode=rwc"
max_connections = 10

[storage]
backend = "local"
max_upload_mb = 10

[storage.local]
upload_dir = "data/uploads"
public_url_prefix = "/uploads"

[storage.s3]
bucket = "aegis-bucket"
region = "auto"
# endpoint = "https://<account_id>.r2.cloudflarestorage.com"
access_key_id = ""
secret_access_key = ""
force_path_style = false
# public_url_prefix = "https://cdn.example.com"

[cache]
backend = "memory"
default_ttl_secs = 3600

[cache.redis]
url = "redis://127.0.0.1:6379"
timeout_ms = 2000

[paths]
schemas_dir = "schemas"
plugins_dir = "plugins"
admin_dist_dir = "admin/dist"

[runtime]
enable_wasm_plugins = true
default_max_memory_mb = 16
default_max_fuel_units = 100000000
default_timeout_ms = 250
instance_pool_size = 64
pool_max_memory_mb = 64

[plugins]
enabled = []

[security]
enforce_external_link_rel = true
open_external_links_in_new_tab = false
internal_domains = []
enforce_image_lazy_loading = true
enforce_image_async_decoding = true

[audit]
enabled = true
sinks = ["database"]
trust_forwarded_for = false
retention_days = 0
purge_interval_secs = 3600

[auth.login_rate_limit]
enabled = true
max_failures_per_user = 5
max_failures_per_ip = 20
window_secs = 900
lockout_secs = 900

[auth.oidc]
enabled = false
display_name = "SSO"
issuer_url = ""
client_id = ""
client_secret = ""
redirect_url = ""
scopes = ["openid", "profile", "email"]
username_claim = "preferred_username"
auto_create_users = true
default_role = "viewer"
role_claim = ""
post_login_redirect = "/admin/"

[auth.oidc.role_mapping]

[auth.saml]
enabled = false
display_name = "SSO"
entity_id = ""
acs_url = ""
idp_entity_id = ""
idp_sso_url = ""
idp_certificates = []
name_id_format = ""
username_attribute = ""
email_attribute = "email"
display_name_attribute = "displayName"
auto_create_users = true
default_role = "viewer"
role_attribute = ""
post_login_redirect = "/admin/"

[auth.saml.role_mapping]

[auth.registration]
enabled = false
admin_url = ""
default_role = "viewer"
require_approval = true
allowed_email_domains = []
token_ttl_secs = 86400
max_per_ip_per_hour = 5

[commerce]
enabled = false
product_type = "product"
currency = "jpy"
public_url = ""
provider = "manual"
reservation_minutes = 60
max_quantity_per_item = 99
max_items = 50
max_orders_per_ip_per_hour = 20

[commerce.manual]
instructions = "ご注文ありがとうございます。お支払い方法はメールでご案内します。"
payment_due_hours = 72

[commerce.stripe]
secret_key = ""
webhook_secret = ""
api_base = "https://api.stripe.com"
async_payment_due_hours = 72

[notifications]
enabled = false
channels = ["email"]
locale = "en"
# templates_dir = "notifications"
disabled_events = []
max_attempts = 3
html = true

[notifications.queue]
persistent = true
poll_interval_secs = 10
retention_days = 7

[notifications.email]
smtp_host = ""
smtp_port = 0
tls = "starttls"
username = ""
password = ""
from = ""
reply_to = ""
staff_recipients = []
timeout_secs = 15

# [[notifications.audit.rules]]
# action = "session.create"
# outcomes = ["success"]
# roles = ["admin"]

[comments]
enabled = false
content_types = ["article"]
max_depth = 3
max_length = 5000
close_after_days = 0
notify_author = true
notify_staff = true

[comments.moderation]
require_approval = true
auto_approve_known = true
max_links = 2
hold_words = []
spam_words = []
max_per_ip_per_hour = 10
```

---

## 4. リバースプロキシ配下での運用

nginx / Caddy / ALB などのリバースプロキシの後ろで、サブドメイン（`cms.example.com`）のルートにも、サブパス（`example.com/cms/`）にも公開できます。公開サイト（`/`）、Admin Studio（`/admin/`）、API（`/api/v1`）、アップロード（`/uploads`）、テーマアセット（`/theme/assets`）は同じオリジン・同じパスの配下で動きます。

### 4.1 サブドメインで公開する

例: `https://cms.example.com/`

`[server]` の設定は不要です。プロキシから AegisCMS にそのまま転送してください（下の nginx の例）。

### 4.2 サブパスで公開する

例: `https://example.com/cms/`

`[server].base_path` に公開するパスを指定します。

```toml
[server]
base_path = "/cms"
```

- サーバーが生成する URL（テーマのリンク、アップロードしたファイルの URL、`/admin` のリダイレクト、SSO ログイン後の戻り先）にベースパスが付きます。
- プロキシはパスを**そのまま**（`/cms/...` のまま）転送してください。ベースパスを取り除いて転送する設定でも動きますが、ベースパスと同じ名前のスラッグ（`/cms/cms`）が開けなくなります。ベースパスの無いリクエスト（`/api/v1/health` など）もそのまま受け付けるため、コンテナのヘルスチェックは変更不要です。
- 独自テーマのテンプレートでは、リンクにグローバル変数 `base_path` を付けてください（`<a href="{{ base_path }}/articles/{{ item.slug }}">`。[テーマガイド](./theme_and_publishing_guide.md#テンプレートで使えるグローバル変数)）。
- 本文に手で書いたルート起点のリンク（`/about` など）は書き換えません。`/cms/about` のようにベースパスを含めて書いてください。
- アップロードしたファイルの URL はベースパス込みで本文に保存されます。**後から `base_path` を変えると、保存済みのコンテンツ内の画像・リンクは古いパスのまま**になるため、運用開始前に決めてください。

### 4.3 Admin Studio の接続先

本番ビルドは、自分を配信したサーバー（`{サーバー}/admin/assets/*.js` から逆算したオリジンとベースパス）の `/api/v1` に接続するため、ドメインやベースパスごとのビルドは不要です。

Admin Studio だけを別の場所で配信する場合（例: `admin.example.com` から `example.com/cms` の API を使う）は、ビルド時に `VITE_AEGIS_SERVER_URL=https://example.com/cms` を指定してください（Docker では `docker build --build-arg VITE_AEGIS_SERVER_URL=...`）。API は Bearer トークンで認証し CORS を全オリジンに許可しているため、サーバー側の追加設定は不要です。プレビューのテーマアセット・アップロード画像や SSO のログイン URL もこの URL を使います。

### 4.4 あわせて確認する設定

| 設定 | 内容 |
| --- | --- |
| `[audit].trust_forwarded_for = true` | 監査ログ・ログインのレート制限のクライアント IP を `X-Forwarded-For` から取る。無効のままだと全員がプロキシの IP として数えられ、IP 単位のロックが全ユーザーに効く |
| `[auth.oidc].redirect_url` | 公開 URL（`https://cms.example.com/api/v1/auth/oidc/callback`、サブパスなら `https://example.com/cms/api/v1/auth/oidc/callback`）にする。既定の `127.0.0.1` のままでは IdP から戻れない |
| `[auth.saml].entity_id` / `acs_url` | 公開 URL にする（`https://cms.example.com/api/v1/auth/saml/acs`、サブパスなら `https://example.com/cms/api/v1/auth/saml/acs`）。応答の `Destination`・`Recipient` と一致しないと拒否される |
| `[auth.oidc].post_login_redirect` / `[auth.saml].post_login_redirect` | `/` で始まる値（既定の `/admin/`）にはベースパスが自動で付く。Admin Studio を別の場所に置く場合は絶対 URL（`https://admin.example.com/admin/`） |
| `[security].internal_domains` | 本文中の自サイトへの絶対 URL を外部リンク扱い（`rel="noopener ..."` 付与）にしないよう、`["example.com"]` のように指定（サブドメインも含めて内部扱い） |
| `[storage].max_upload_mb` | プロキシ側の上限（nginx の `client_max_body_size` など）も同じ値以上にする |

nginx の例（サブドメイン）:

```nginx
server {
    listen 443 ssl;
    server_name cms.example.com;

    client_max_body_size 10m;  # [storage].max_upload_mb に合わせる

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        # AegisCMS は先頭のアドレスを使うため、クライアントが送った値は引き継がない
        proxy_set_header X-Forwarded-For $remote_addr;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

nginx の例（サブパス、`base_path = "/cms"`）:

```nginx
server {
    listen 443 ssl;
    server_name example.com;

    client_max_body_size 10m;

    # proxy_pass に URI（末尾の / など）を付けないことで、/cms/... のまま転送する
    location /cms/ {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $remote_addr;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
    location = /cms {
        return 308 /cms/;
    }
}
```

> [!NOTE]
> `X-Forwarded-For` はクライアントが自由に設定できます。`trust_forwarded_for` を有効にする場合は、AegisCMS に直接アクセスできないようにし（`[server].host = "127.0.0.1"` やファイアウォール）、上の例のようにプロキシ側で受け取った `X-Forwarded-For` を上書きしてください（`$proxy_add_x_forwarded_for` は受け取った値の後ろに追記するため、先頭を偽装できます）。プロキシを多段にする場合は、最も外側のプロキシで上書きしてください。

---

## 5. 管理画面から設定する動的サイト・SEO設定

サーバーの再起動を伴わずに即時反映できるよう、サイトの基本情報および検索エンジン・Bot制御はデータベースの `_aegis_settings` テーブルに保存され、Admin Studio の「設定」画面からリアルタイムに変更できます。

### 5.1 一般設定 (`site.*`)

| 項目名 | DB キー | 説明 |
| --- | --- | --- |
| サイト名 | `site.title` | 公開サイトおよび管理画面のタイトル表示 |
| ファビコン | `site.favicon` | `<link rel="icon">` で読み込むアイコン画像 URL（メディアライブラリから選択またはアップロード） |

### 5.2 SEO & Bot 対策設定 (`seo.*`)

| 項目名 | DB キー | 型 / 値の形式 | 既定値 | 説明 |
| --- | --- | --- | --- | --- |
| インデックス全体除外 | `seo.noindex_all` | `"true"` \| `"false"` | `"false"` | 有効時、サイト全体の `<head>` に `noindex, nofollow` を強制注入（開発環境や非公開環境向け） |
| Canonical Base URL | `seo.canonical_base_url` | URL 文字列 | `""` | サイトの正規ドメイン（例: `https://example.com`）。設定時、全ページに `<link rel="canonical">` を出力 |
| デフォルト Meta 説明文 | `seo.meta_description` | 文字列（最大 500 文字） | `""` | ページ個別の要約がない場合にフォールバック出力される `<meta name="description">` |
| デフォルト Meta キーワード | `seo.meta_keywords` | 文字列（カンマ区切り） | `""` | `<meta name="keywords">` の出力 |
| デフォルト OGP 画像 | `seo.og_image` | 画像 URL | `""` | SNS シェア時のデフォルトアイキャッチ画像（メディアライブラリから選択可能） |
| Twitter Card 種別 | `seo.twitter_card` | `"summary"` \| `"summary_large_image"` | `"summary_large_image"` | Twitter / X 共有時のカード表示形式 |
| カスタム robots.txt | `seo.robots_txt` | テキスト（最大 64 KB） | 空（自動生成） | クローラー巡回制御。空の場合は安全なデフォルトルールおよび動的 Sitemap 参照を出力 |

> [!TIP]
> Admin Studio の SEO 設定画面では「標準設定」「開発環境（全ブロック）」「生成AIクローラー拒否」のワンクリックプリセットが用意されており、安全かつ迅速に `robots.txt` を構成できます。

