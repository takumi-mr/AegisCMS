# AegisCMS 設定リファレンス (Configuration Reference)

AegisCMS コアサーバー (`aegis-server`) の設定ファイル (`config/*.toml`) で指定できる項目の一覧です。
設定値の定義元は [`crates/aegis-core/src/config.rs`](../crates/aegis-core/src/config.rs) の `AegisConfig` です。

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

### 2.2 `[database]` — データベース

| キー | 型 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `backend` | `"sqlite"` \| `"postgres"` \| `"mysql"` | `"sqlite"` | 使用する RDBMS |
| `url` | string | `"sqlite://aegis.db?mode=rwc"` | 接続 URL |
| `max_connections` | integer (u32) | `10` | 接続プールの最大接続数 |

`backend` が `"sqlite"`（既定値）の場合は、`url` のスキームから実際のバックエンドが推論されます。

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
| `backend` | `"memory"` \| `"redis"` | `"memory"` | `memory`: プロセス内 TTL キャッシュ / `redis`: 複数インスタンスでセッション共有 |
| `default_ttl_secs` | integer (u64) | `3600` | キャッシュ・セッションの既定 TTL（秒）。ログイン時に指定する `ttl_secs` の上限も兼ねる |

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
| `plugins_dir` | path | `"plugins"` | Wasm プラグインの走査ディレクトリ（直下の各サブディレクトリを 1 プラグインとして読み込む） |
| `admin_dist_dir` | path | `"admin/dist"` | Admin Studio ビルド成果物。存在すれば `/` で SPA として配信 |

### 2.6 `[runtime]` — Wasm プラグインランタイム

| キー | 型 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `enable_wasm_plugins` | bool | `true` | `false` でプラグインの読み込みを無効化 |
| `default_max_memory_mb` | integer (u32) | `16` | マニフェストが `max_memory_mb` を省略したプラグインのメモリ上限（MiB） |
| `default_max_fuel_units` | integer (u64) | `100000000` | マニフェストが `max_fuel_units` を省略したプラグインの Fuel 上限 |
| `default_timeout_ms` | integer (u64) | `250` | マニフェストが `execution_timeout_ms` を省略したプラグインのタイムアウト（ミリ秒） |

> [!NOTE]
> `default_*` は既定値であり上限ではありません。プラグインの `plugin.manifest.json` の `resource_limits` に値があればそちらが優先され、省略した項目（または `resource_limits` 全体）だけがこの設定で補われます（[プラグインフックリファレンス](./plugin_hooks.md#4-リソース制限-resource_limits) を参照）。読み込んだプラグインごとの実際の値は起動時の `INFO` ログに出力されます。

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

`POST /api/v1/auth/session` の認証失敗をユーザー名・クライアント IP 単位で数え、上限に達すると一定時間 `429 Too Many Requests` で拒否します（詳細は [認証ドキュメント](./authentication.md#4-セキュリティ上の挙動)）。失敗回数は `[cache]` のバックエンドに保存するため、複数インスタンスで制限を共有するには `backend = "redis"` にしてください。

| キー | 型 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `enabled` | bool | `true` | `false` でレート制限を無効化 |
| `max_failures_per_user` | integer (u64) | `5` | 同じユーザー名（大文字・小文字を区別しない）で `window_secs` 内に失敗できる回数。到達でそのユーザー名をロック（`0` で無制限） |
| `max_failures_per_ip` | integer (u64) | `20` | 同じクライアント IP から `window_secs` 内に失敗できる回数。到達でその IP をロック（`0` で無制限） |
| `window_secs` | integer (u64) | `900` | 失敗回数を数える期間（秒）。最初の失敗から数え、過ぎると 0 に戻る |
| `lockout_secs` | integer (u64) | `900` | ロックの継続時間（秒）。ロック中は正しいパスワードでもログインできない |

---

## 3. 完全な設定例

すべての項目を明示した、そのままパース可能な設定例です。

```toml
[server]
host = "127.0.0.1"
port = 8080

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
```
