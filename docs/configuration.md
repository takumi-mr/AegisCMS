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

> [!WARNING]
> 設定ファイルが **存在しない、またはパースに失敗した場合、エラーにならず全項目が組み込みの既定値で起動します**（`AegisConfig::load_or_default`）。警告ログも出ないため、設定が反映されていないと感じたらまず以下を確認してください。

- **トップレベルのセクション（`[server]`, `[database]` など）は省略可能**です。省略したセクションは丸ごと既定値になります。
- **セクションを書いた場合、そのセクション内の項目は原則すべて必須**です。下表の「省略可」列が「✅」の項目だけが省略できます。
  - 例: `[server]` に `port = 9000` だけを書くと `missing field 'host'` でパースに失敗し、ファイル全体が既定値にフォールバックします。
- バックエンド種別（`backend`）は TOML 上では下表に記載した値のみ受け付けます（大文字・小文字も区別）。`"r2"` や `"minio"` などの別名は使えません。

> [!CAUTION]
> **既知の問題**: 現在同梱している `config/default.toml` と `config/docker.toml` は、`[storage.s3]` セクションに必須項目 `access_key_id` / `secret_access_key` / `force_path_style` が含まれていない（コメントアウトされている）ため、パースに失敗して既定値で起動しています。
> `config/default.toml` は既定値とほぼ同じ内容のため実害は小さいですが、`config/docker.toml` の `host = "0.0.0.0"` や `/app/data/...` 配下のパスは現状反映されていません。
> 回避策としては、`[storage.s3]` に上記 3 項目を明示的に記述してください（例: `access_key_id = ""`, `secret_access_key = ""`, `force_path_style = false`）。

---

## 2. 設定項目一覧

### 2.1 `[server]` — HTTP サーバー

| キー | 型 | 既定値 | 省略可 | 説明 |
| --- | --- | --- | --- | --- |
| `host` | string | `"127.0.0.1"` | | リッスンするアドレス。コンテナでは `"0.0.0.0"` を指定 |
| `port` | integer (u16) | `8080` | | リッスンするポート |

### 2.2 `[database]` — データベース

| キー | 型 | 既定値 | 省略可 | 説明 |
| --- | --- | --- | --- | --- |
| `backend` | `"sqlite"` \| `"postgres"` \| `"mysql"` | `"sqlite"` | ✅ | 使用する RDBMS |
| `url` | string | `"sqlite://aegis.db?mode=rwc"` | | 接続 URL |
| `max_connections` | integer (u32) | `10` | | 接続プールの最大接続数 |

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

| キー | 型 | 既定値 | 省略可 | 説明 |
| --- | --- | --- | --- | --- |
| `backend` | `"local"` \| `"s3"` | `"local"` | ✅ | アップロードファイルの保存先 |

#### `[storage.local]` — ローカルディスク

`backend = "local"` のとき使用されます。サーバー起動時に `upload_dir` が作成され、`public_url_prefix` のパスで静的配信されます。

| キー | 型 | 既定値 | 省略可 | 説明 |
| --- | --- | --- | --- | --- |
| `upload_dir` | path | `"data/uploads"` | | 保存先ディレクトリ |
| `public_url_prefix` | string | `"/uploads"` | | 配信 URL のプレフィックス（ルーティングにも使用） |

#### `[storage.s3]` — S3 互換オブジェクトストレージ

`backend = "s3"` のとき使用されます。AWS S3 / Cloudflare R2 / MinIO / Wasabi などに対応しています。

| キー | 型 | 既定値 | 省略可 | 説明 |
| --- | --- | --- | --- | --- |
| `bucket` | string | `"aegis-bucket"` | | バケット名 |
| `region` | string | `"auto"` | | リージョン（SigV4 署名に使用。R2 は `"auto"`） |
| `endpoint` | string | なし | ✅ | カスタムエンドポイント。R2 / MinIO では必須。未指定時は `https://{bucket}.s3.{region}.amazonaws.com` |
| `access_key_id` | string | `""` | | アクセスキー ID |
| `secret_access_key` | string | `""` | | シークレットアクセスキー |
| `force_path_style` | bool | `false` | | `true` でパススタイル URL (`{endpoint}/{bucket}/{key}`) を使用。MinIO では `true` |
| `public_url_prefix` | string | なし | ✅ | 公開 URL のプレフィックス（CDN / カスタムドメイン）。未指定時はエンドポイント URL を返す |

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

> [!NOTE]
> `[storage]` を書く場合、`backend = "local"` であっても `[storage.s3]` セクションを書くなら全必須項目が必要です。S3 を使わない場合は `[storage.s3]` セクションごと省略するのが安全です。

### 2.4 `[cache]` — キャッシュ & セッション

| キー | 型 | 既定値 | 省略可 | 説明 |
| --- | --- | --- | --- | --- |
| `backend` | `"memory"` \| `"redis"` | `"memory"` | ✅ | `memory`: プロセス内 TTL キャッシュ / `redis`: 複数インスタンスでセッション共有 |
| `default_ttl_secs` | integer (u64) | `3600` | ✅ | キャッシュ・セッションの既定 TTL（秒） |

#### `[cache.redis]`

`backend = "redis"` のとき使用されます。

| キー | 型 | 既定値 | 省略可 | 説明 |
| --- | --- | --- | --- | --- |
| `url` | string | `"redis://127.0.0.1:6379"` | | Redis 接続 URL |
| `timeout_ms` | integer (u64) | `2000` | ✅ | （予約）接続タイムアウト（ミリ秒）。現時点では未使用 |

### 2.5 `[paths]` — ディレクトリ

| キー | 型 | 既定値 | 省略可 | 説明 |
| --- | --- | --- | --- | --- |
| `schemas_dir` | path | `"schemas"` | | 宣言的スキーマ (`*.yaml`) の読み込み元 |
| `plugins_dir` | path | `"plugins"` | | Wasm プラグインの走査ディレクトリ（直下の各サブディレクトリを 1 プラグインとして読み込む） |
| `admin_dist_dir` | path | `"admin/dist"` | | Admin Studio ビルド成果物。存在すれば `/` で SPA として配信 |

### 2.6 `[runtime]` — Wasm プラグインランタイム

| キー | 型 | 既定値 | 省略可 | 説明 |
| --- | --- | --- | --- | --- |
| `enable_wasm_plugins` | bool | `true` | | `false` でプラグインの読み込みを無効化 |
| `default_max_memory_mb` | integer (u32) | `16` | | （予約）プラグインのメモリ上限の既定値 |
| `default_max_fuel_units` | integer (u64) | `100000000` | | （予約）プラグインの Fuel 上限の既定値 |
| `default_timeout_ms` | integer (u64) | `250` | | （予約）プラグインのタイムアウトの既定値 |

> [!NOTE]
> `default_*` の 3 項目は現時点ではランタイムから参照されていません。実際のリソース制限は各プラグインの `plugin.manifest.json` の `resource_limits` で決まります（[プラグインフックリファレンス](./plugin_hooks.md#4-リソース制限-resource_limits) を参照）。

### 2.7 `[security]` — HTML 出力のセキュリティ / 最適化

コンテンツ作成時に、レンダリング済み HTML へストリーミングリライター (`lol-html`) で適用されます。
「外部リンク」は `http://` / `https://` / `//` で始まる絶対 URL のうち、`internal_domains` に一致しないものです（相対パス、`#`, `mailto:`, `tel:` などは対象外）。

| キー | 型 | 既定値 | 省略可 | 説明 |
| --- | --- | --- | --- | --- |
| `enforce_external_link_rel` | bool | `true` | | 外部リンクの `<a>` に `rel="noopener noreferrer"` を付与（既存の `rel` とマージ） |
| `open_external_links_in_new_tab` | bool | `false` | | `target` 未指定の外部リンクに `target="_blank"` を付与 |
| `internal_domains` | string[] | `[]` | | 内部扱いするドメイン（サブドメインも一致、大文字・小文字は区別しない）。これらへのリンクは上記の対象外 |
| `enforce_image_lazy_loading` | bool | `true` | | `loading` 未指定の `<img>` に `loading="lazy"` を付与 |
| `enforce_image_async_decoding` | bool | `true` | | `decoding` 未指定の `<img>` に `decoding="async"` を付与 |

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
```
