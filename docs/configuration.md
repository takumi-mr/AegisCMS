[← Back to the documentation index](README.md)

**English** | [日本語](configuration.ja.md)

# AegisCMS Configuration Reference

A list of the items that can be specified in the AegisCMS core server (`aegis-server`) config files (`config/*.toml`).
The definition source of the settings is `AegisConfig` in [`crates/aegis-server/src/config.rs`](../crates/aegis-server/src/config.rs). The type of each section is in the `config` module of the crate that uses it (for example, `[database]` is in `aegis-db`, `[storage]` in `aegis-storage`, and `[commerce]` in `aegis-commerce`).

---

## 1. Loading the Config File

### 1.1 Deciding Which File to Load

| Priority | How to specify | Example |
| --- | --- | --- |
| 1 | The CLI argument `-c` / `--config` | `aegis-server --config config/prod.toml run` |
| 2 | The environment variable `AEGIS_CONFIG` | `AEGIS_CONFIG=/app/config/docker.toml` |
| 3 | The default | `config/default.toml` |

Bundled config files:

- [`config/default.toml`](../config/default.toml): for local development
- [`config/docker.toml`](../config/docker.toml): for containers (the `Dockerfile` sets it in `AEGIS_CONFIG`)

### 1.2 Overriding with the CLI / Environment Variables

Only for the `run` subcommand, the values of `[server]` can be overridden with the following.

| CLI argument | Environment variable | What it overrides |
| --- | --- | --- |
| `-p`, `--port <PORT>` | `PORT` | `server.port` |
| `-H`, `--host <HOST>` | `HOST` | `server.host` |

The log level is controlled with the standard `RUST_LOG` environment variable (when unspecified: `info,aegis_server=debug,tower_http=debug`).

### 1.3 Notes on Parsing

- **Every section and item is optional.** Omitted items use the defaults in the tables below.
  - Example: if you write only `port = 9000` in `[server]`, `host` becomes the default `"127.0.0.1"`.
- **If the file does not exist**, a warning log (`Config file ... not found`) is written and it starts with the built-in defaults.
- **`"env:NAME"` is replaced by the value of environment variable `NAME`** when the file is loaded. It works on every string value in the file (including array elements; not on table keys), for example `port = "env:PORT"` or `url = "env:DATABASE_URL"`. Use it to keep secrets out of the file.
  - If the variable is **not set**, a warning log is written and the item is treated as not written, so it takes its default value (an array element is dropped). It is not an error. A variable that is set to an empty string is used as the empty string.
  - Environment variables are strings, so an item whose default is a number or a boolean converts the value to that type (`true` / `false` for a boolean). If it cannot be converted, a warning is written and the default is used.
  - The value is not logged. A value that must start with `env:` literally is not supported.
- **If reading or parsing the file fails** (a wrong type, an unknown `backend` value, etc.), an error message is shown and startup is aborted (exit code 1).
- Backend kinds (`backend`) in TOML accept only the values listed in the tables below (case-sensitive too). Aliases such as `"r2"` and `"minio"` cannot be used.

---

## 2. Settings List

### 2.1 `[server]` — HTTP Server

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `host` | string | `"127.0.0.1"` | The address to listen on. In containers specify `"0.0.0.0"` |
| `port` | integer (u16) | `8080` | The port to listen on |
| `base_path` | string | `""` | The path when publishing under a subpath (e.g. `"/cms"`). If empty, published at the root `/`. Usable characters are alphanumerics and `- . _ ~` (an invalid value stops the server from starting). See [4.2](#42-publishing-under-a-subpath) |
| `admin_path` | string | `"/admin"` | The path where Admin Studio is served. One level only, with the same usable characters as `base_path`. It cannot overlap the paths of the public site and API (`/api`, `/uploads`, `/theme`, `/articles`, etc.) (an invalid value stops the server from starting). See [4.6](#46-making-it-harder-to-identify-the-cms-in-use) |
| `namespace` | string | `"cms"` | The prefix of identifiers that appear on public pages (CSS class names, `id`s, `data-*` attributes, cookie and localStorage keys, JS global names, and the `/_<namespace>/` serving path). It starts with a lowercase letter and is only lowercase letters and digits, within 16 characters (an invalid value stops the server from starting). `admin_path` and `/_<namespace>` cannot be the same. See [4.6](#46-making-it-harder-to-identify-the-cms-in-use) |
| `cors_origins` | array of strings | `[]` | Origins that are allowed to call authenticated APIs from another origin (e.g. `["https://admin.example.com"]`). The default (empty) is: only public reads are open to all origins (`*`), and APIs requiring authentication and writes are same-origin only. `"*"` gives the earlier behavior (all methods to all origins). An invalid value stops the server from starting. See [4.3.1](#431-cors-servercors_origins) |

### 2.2 `[database]` — Database

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `backend` | `"sqlite"` \| `"postgres"` \| `"mysql"` | `"sqlite"` | The RDBMS to use |
| `url` | string | `"sqlite://aegis.db?mode=rwc"` | The connection URL |
| `max_connections` | integer (u32) | `10` | The maximum connections of the connection pool |

When `backend` is `"sqlite"` (the default), the actual backend is inferred from the `url` scheme.

SQLite files are opened in WAL mode (`journal_mode = WAL`, `synchronous = NORMAL`). Reads do not wait for writes, and disk syncs per commit are also reduced (processing that writes on every request, like the audit log, is less likely to jam). `<name>-wal` / `<name>-shm` are created beside the DB file, so handle all 3 together in backups and volumes. WAL cannot be used on network file systems such as NFS, so place the DB on a local disk (a volume for containers).

| `url` scheme | Inferred backend |
| --- | --- |
| `postgres://`, `postgresql://` | PostgreSQL |
| `mysql://`, `mariadb://` | MySQL |
| Anything else | SQLite |

```toml
[database]
url = "postgres://user:password@localhost:5432/aegis"  # it becomes postgres even if backend is omitted
max_connections = 10
```

### 2.3 `[storage]` — Asset Storage

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `backend` | `"local"` \| `"s3"` | `"local"` | Where uploaded files are stored |
| `max_upload_mb` | integer (u64) | `10` | The maximum size per upload (MB). Exceeding it gives `413 Payload Too Large`. When putting a reverse proxy in front, match its limit too (such as nginx's `client_max_body_size`) |

#### `[storage.local]` — Local Disk

Used when `backend = "local"`. At server startup `upload_dir` is created, and files are served statically at the `public_url_prefix` path.

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `upload_dir` | path | `"data/uploads"` | The storage directory |
| `public_url_prefix` | string | `"/uploads"` | The prefix of serving URLs (also used for routing) |

#### `[storage.s3]` — S3-Compatible Object Storage

Used when `backend = "s3"`. Supports AWS S3 / Cloudflare R2 / MinIO / Wasabi, and others.

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `bucket` | string | `"aegis-bucket"` | Bucket name |
| `region` | string | `"auto"` | Region (used for SigV4 signing. `"auto"` for R2) |
| `endpoint` | string | None | A custom endpoint. Required for R2 / MinIO. When unspecified, `https://{bucket}.s3.{region}.amazonaws.com` |
| `access_key_id` | string | `""` | Access key ID |
| `secret_access_key` | string | `""` | Secret access key |
| `force_path_style` | bool | `false` | `true` uses path-style URLs (`{endpoint}/{bucket}/{key}`). `true` for MinIO |
| `public_url_prefix` | string | None | The prefix of public URLs (CDN / custom domain). When unspecified, the endpoint URL is returned |

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

### 2.4 `[cache]` — Cache & Sessions

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `backend` | `"memory"` \| `"redis"` | `"memory"` | `memory`: an in-process TTL cache (for single-server setups only) / `redis`: shares sessions and change notices across multiple instances |
| `default_ttl_secs` | integer (u64) | `3600` | The default TTL (seconds) for cache and sessions. Also serves as the upper limit of `ttl_secs` specified at login |

- With the same backend as the cache, changes in Admin Studio (toggling, reordering, and settings values of plugins) are announced between servers ([Plugin hooks reference 7.3](./plugin_hooks.md#73-reflection-across-multiple-servers)). With `redis`, the Pub/Sub channel `aegis:events` is used.
- **`memory` is for single-server setups only.** At startup it writes a mark to the DB (`server.memory_cache_instance` in `_aegis_settings`) and rewrites it every 10 seconds. If a `memory` server is already running on the same DB, the second one does not start (`Another AegisCMS server with [cache].backend = "memory" is running on this database`). When running multiple servers, use `redis`.
  - A server that was stopped normally (Ctrl+C, SIGTERM) removes the mark, so it can be restarted immediately. Restarting right after a forced kill (within 30 seconds) waits about 20 seconds before starting, to confirm the previous server has stopped.

#### `[cache.redis]`

Used when `backend = "redis"`.

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `url` | string | `"redis://127.0.0.1:6379"` | The Redis connection URL |
| `timeout_ms` | integer (u64) | `2000` | The timeout per connection to Redis and each command's response timeout (milliseconds). If connecting fails, it retries up to 2 times (waiting at most 1 second), and if it still fails it becomes a startup error or a command error |

### 2.5 `[paths]` — Directories

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `schemas_dir` | path | `"schemas"` | Where declarative schemas (`*.yaml`) are loaded from |
| `queries_dir` | path | `"queries"` | Where declarative query definitions (`*.yaml`) are loaded from ([queries.md](queries.md)) |
| `plugins_dir` | path | `"plugins"` | Where Wasm plugins are installed (one plugin in `<plugins_dir>/<name>/`). Only those in the enabled list (`[plugins].enabled`, or the list toggled in Admin Studio) are loaded |
| `admin_dist_dir` | path | `"admin/dist"` | Admin Studio build output. If it exists, it is served as the SPA at `/` |

### 2.6 `[runtime]` — Wasm Plugin Runtime

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `enable_wasm_plugins` | bool | `true` | `false` disables loading plugins |
| `default_max_memory_mb` | integer (u32) | `16` | The memory limit (MiB) for plugins whose manifest omits `max_memory_mb` |
| `default_max_fuel_units` | integer (u64) | `100000000` | The fuel limit for plugins whose manifest omits `max_fuel_units` |
| `default_timeout_ms` | integer (u64) | `250` | The timeout (milliseconds) for plugins whose manifest omits `execution_timeout_ms` |
| `fuel_per_input_byte` | integer (u64) | `150` | The fuel limit added per byte of hook input (dynamic scaling). `0` disables (fixed limit). Prevents JSON serialization and body scanning of huge articles from trapping with `all fuel consumed` |
| `instance_pool_size` | integer (u32) | `64` | The number of slots in the Wasm instance pool (Wasmtime's pooling allocator) = the number of hook calls that can run simultaneously. Calls beyond it wait for a free slot. `0` disables (memory is allocated and freed on each call) |
| `pool_max_memory_mb` | integer (u32) | `64` | The linear memory limit per pool slot (MiB). A plugin whose manifest `max_memory_mb` exceeds this cannot be loaded (the server does not start) |
| `render_cache_mb` | integer (u32) | `32` | The budget (MiB) of the `on-render-page` result cache. `0` disables. It remembers calls of plugins whose output is decided only by the page content passed in, and does not call Wasm for the same input ([architecture.md 10.8](./architecture.md#108-caching-the-result-of-on-render-page-aegis-runtimesrcwasmresult_cachers)) |

> [!NOTE]
> `default_*` are defaults, not limits. If the plugin's `plugin.manifest.json` `resource_limits` has a value, that takes precedence, and only omitted items (or all of `resource_limits`) are filled in by these settings (see the [Plugin hooks reference](./plugin_hooks.md#4-resource-limits-resource_limits)). The actual values per loaded plugin are output to the `INFO` log at startup.

> [!NOTE]
> **Instance pool**: hooks still run in a new instance on every call (state from the previous call does not remain; Wasmtime zeroes a returned slot). The pool does the allocation of linear memory, tables, and async stacks at startup, to eliminate `mmap` / `munmap` on each call. In-kernel contention under concurrent execution disappears, and article pages with `official-code-enhancer` enabled became about 1.7 times faster.
> About 6 GiB of **virtual** address space is reserved per slot (about 390 GiB for 64 slots; this is to make fast code that omits bounds checks, and real memory is used only as much as needed). In environments that limit virtual memory with `ulimit -v` or the like, the reservation fails, a warning is shown, and it runs the conventional way. Reduce the slot count or set `0`.

#### `[plugins]` — Enabling Plugins

Of the plugins installed in `plugins_dir`, specifies which to load (like the theme's `[theme].active`, installation and enabling are separate).

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `enabled` | array of string | `[]` | The plugin names to enable (the manifest's `name` = the directory name directly under `plugins_dir`). Hooks run in this order |
| `settings.<name>` | table | None | Per-plugin settings values. Plugins read them with `host.config_get` (the SDK's `config::get`). A string value `env:NAME` becomes the value of the environment variable `NAME` (so API keys and the like are not written in the config file) |
| `require_signatures` | bool | `false` | Require a signature (`signature.json`) for ZIP installs ([Installing packages](./plugin_hooks.md#74-installing-from-a-zip-package-and-signatures)) |
| `trusted_public_keys` | string[] | `[]` | The Ed25519 public keys to accept (32-byte hex). With `require_signatures = true`, at least 1 is required (if empty, it does not start). With `require_signatures = false` and empty, any key is accepted if the signature is valid |

```toml
[plugins]
enabled = ["official-shop-rules"]

[plugins.settings.official-shop-rules]
shipping_fee = 600
coupons = "WELCOME10:10%"
```

- You can add domains in the site's settings to the allowed domains (`capabilities.network.allowed_domains`) for when a plugin communicates externally: `extra_allowed_domains = "qiita.com, zenn.dev"` in `[plugins.settings.<plugin name>]` (including subdomains). Communication is only to public addresses over `http` (80, 8080) and `https` (443), and it never connects to internal IP addresses ([Plugin hooks reference 5.4](./plugin_hooks.md#54-http-requests-hosthttp_fetch)).
- If a listed plugin is not installed or cannot be loaded (a manifest defect, importing a function the host lacks, etc.), or a name is malformed or duplicated, **the server does not start**.
- With `[runtime].enable_wasm_plugins = false`, they are not loaded.
- When you change enabling or order in Admin Studio (`PUT /api/v1/plugins/{name}/enabled`, `PUT /api/v1/plugins/order`), that list is saved in the DB and from then on takes precedence over `enabled` ([Plugin hooks reference 7.1](./plugin_hooks.md#71-toggling-on-and-off-from-admin-studio)). Admin Studio's "Return to config file values" (`DELETE /api/v1/plugins/order`) returns to `enabled`.
- The status can be checked with `aegis-server plugin list` ([Plugin hooks reference](./plugin_hooks.md#7-installation-and-enabling)).

### 2.7 `[security]` — HTML Output Security / Optimization

At content creation, these are applied to the rendered HTML by the streaming rewriter (`lol-html`).
An "external link" is an absolute URL starting with `http://` / `https://` / `//` that does not match `internal_domains` (relative paths, `#`, `mailto:`, `tel:`, etc. are not covered).

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `strict_csp` | string | `"on"` | The public pages' Content-Security-Policy. `"on"` (default) enforces Strict (removes `'unsafe-inline'` from `script-src` and `style-src`, and allows inline scripts and `<style>` that plugins and templates output by body hash; `style=""` attributes are stopped too). `"report-only"` keeps the previous CSP (`script-src 'self' 'unsafe-inline'`) and also outputs the Strict CSP as `Content-Security-Policy-Report-Only`, so you can check what would be blocked in the browser console. `"off"` returns to the previous CSP. **A theme's own inline `<script>`, `<style>`, `style=""`, and `on*` attributes do not run with `on`** ([How to write themes](./theme_authoring_guide.md#6a-how-to-write-scripts-do-not-write-inline)). When the CSP is overridden in the admin screen's "Settings › Headers", that value takes priority. For details see [csp.md](./csp.md) |
| `enforce_external_link_rel` | bool | `true` | Add `rel="noopener noreferrer"` to external links' `<a>` (merged with any existing `rel`) |
| `open_external_links_in_new_tab` | bool | `false` | Add `target="_blank"` to external links with no `target` |
| `internal_domains` | string[] | `[]` | Domains to treat as internal (subdomains match too; case-insensitive). Links to these are excluded from the above |
| `enforce_image_lazy_loading` | bool | `true` | Add `loading="lazy"` to `<img>` with no `loading` |
| `enforce_image_async_decoding` | bool | `true` | Add `decoding="async"` to `<img>` with no `decoding` |

### 2.8 `[audit]` — Audit Log

Records audit events of operations. For the recorded targets, output formats, and the query API, see the [Audit log reference](./audit.md).

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `enabled` | bool | `true` | `false` disables recording audit logs |
| `sinks` | (`"database"` \| `"log"` \| `"prometheus"`)[] | `["database"]` | Destinations (multiple allowed). `database`: a DB table + query API / `log`: structured logs / `prometheus`: counters at `/metrics` |
| `forwarded_for_hops` | integer (u32) | `1` | When `trust_forwarded_for = true`, the number of trusted proxies that append to `X-Forwarded-For`. The address this many entries from the right end is the client IP (the first is not used because it can be spoofed) |
| `trust_forwarded_for` | bool | `false` | `true` treats `X-Forwarded-For` (the `forwarded_for_hops`-th from the right end) as the client IP (used for audit log records and the per-IP limit of `[auth.login_rate_limit]`). Enable only behind a trusted reverse proxy |
| `retention_days` | integer (u32) | `0` | Days to keep the `database` destination's audit logs. Older logs are deleted periodically (`0` is unlimited) |
| `purge_interval_secs` | integer (u64) | `3600` | The interval for deleting logs past retention (seconds, minimum 60). Also runs once right after startup |

### 2.9 `[auth.login_rate_limit]` — Rate Limiting of Login Attempts

Authentication failures of `POST /api/v1/auth/session` are counted per username and client IP, and on reaching the limit are refused with `429 Too Many Requests` for a set time (for details see the [Authentication document](./authentication.md#5-security-behavior)). Failure counts are stored in the `[cache]` backend, so to share the limit across multiple instances, set `backend = "redis"`.

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `enabled` | bool | `true` | `false` disables the rate limit |
| `max_failures_per_user` | integer (u64) | `5` | The number of failures allowed within `window_secs` with the same username (case-insensitive). On reaching it, that username is locked (`0` is unlimited) |
| `max_failures_per_ip` | integer (u64) | `20` | The number of failures allowed within `window_secs` from the same client IP. On reaching it, that IP is locked (`0` is unlimited) |
| `window_secs` | integer (u64) | `900` | The period over which failures are counted (seconds). Counted from the first failure, and returns to 0 once it passes |
| `lockout_secs` | integer (u64) | `900` | The duration of the lock (seconds). While locked, login is impossible even with the correct password |

### 2.10 `[auth.oidc]` — Single Sign-On (OpenID Connect)

Login with an external IdP (authorization code flow + PKCE). For usage, see the [Authentication document](./authentication.md#6-single-sign-on-openid-connect). With `enabled = true`, if a required item is missing or a role name is invalid, the server does not start.

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `enabled` | bool | `false` | `true` enables SSO |
| `display_name` | string | `"SSO"` | The login screen's button name (`Sign in with ...`) |
| `issuer_url` | string | `""` | The IdP's Issuer (required). Endpoints are fetched from `{issuer_url}/.well-known/openid-configuration` |
| `client_id` | string | `""` | The client ID (required) |
| `client_secret` | string | `""` | The client secret |
| `redirect_url` | string | `""` | The callback URL registered with the IdP (required). `https://<host>/api/v1/auth/oidc/callback` |
| `scopes` | array of string | `["openid", "profile", "email"]` | The scopes to request (`openid` is always included) |
| `username_claim` | string | `"preferred_username"` | The claim to use as a new user's username (`email` if absent) |
| `auto_create_users` | bool | `true` | Create unregistered users at first login. With `false`, only users linked with `aegis-server user link-oidc` |
| `default_role` | string | `"viewer"` | A new user's role. When using `role_claim`, the role when no group matches |
| `role_claim` | string | `""` | The claim that decides the role (e.g. `groups`). If empty, the role is decided by the AegisCMS user account |
| `role_mapping` | table | `{}` | `role_claim` value → role (`admin` / `editor` / `viewer`). If several match, the strongest role |
| `post_login_redirect` | string | `"/admin/"` | The Admin Studio URL to return to after login (`#session=` / `#login_error=` is appended). If it starts with `/`, `[server].base_path` is prepended. When running Admin Studio on a different origin (such as a dev server), an absolute URL |

> [!NOTE]
> `client_secret` is written in plain text in the config file, so restrict the file's permissions.

### 2.11 `[auth.registration]` — Account Registration

Account registration from Admin Studio's login screen (with email address confirmation). For usage, see the [Authentication document](./authentication.md#7-account-registration). With `enabled = true`, the server does not start if notifications (`[notifications]`) are disabled, the destination of the confirmation link cannot be determined, or `default_role` is `admin` or an unknown role.

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `enabled` | bool | `false` | `true` enables account registration |
| `admin_url` | string | `""` | The Admin Studio URL where the confirmation link opens (e.g. `"https://cms.example.com/admin/"`; `#verify-email=<token>` is appended). If empty, `[commerce].public_url` + `/admin/`. It is not built from the request's Host |
| `default_role` | string | `"viewer"` | The registered user's role (`editor` / `contributor` / `viewer`) |
| `require_approval` | bool | `true` | With `true`, the account after confirmation is created disabled and cannot log in until an admin enables it |
| `allowed_email_domains` | array of string | `[]` | Email address domains that may register (`"example.com"` allows subdomains too). If empty, any domain may register |
| `token_ttl_secs` | integer (u64) | `86400` | The validity of the confirmation link (seconds) |
| `max_per_ip_per_hour` | integer (u64) | `5` | The number of applications accepted per hour from the same client IP (`0` is unlimited) |

### 2.12 `[commerce]` — E-Commerce

Cart, orders, inventory, and payment ([E-commerce document](./commerce.md)). With `enabled = true`, if the settings are invalid (the currency code format, no Stripe key, etc.) or the product schema lacks `price` (integer), the server does not start.

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `enabled` | bool | `false` | `true` enables the cart and orders (the product list and product pages are shown even when disabled) |
| `product_type` | string | `"product"` | The product's content type |
| `currency` | string | `"jpy"` | The ISO 4217 currency code (lowercase). All amounts are integers in this currency's smallest unit |
| `supported_currencies` | string[] | `[]` | Currencies customers can choose (3 lowercase letters). If empty, only `currency`. `currency` is always included ([Multiple currencies](./commerce.md#22-multiple-currencies)) |
| `currency_rates` | table | `{}` | The rate of each currency per 1 unit of `currency` (e.g. `{ usd = 0.0067 }`). Positive numbers only |
| `public_url` | string | `""` | The public site's URL (the return destination after payment; includes `base_path` for subpath operation). If empty, from the request's Host (`X-Forwarded-Host` / `-Proto` if `trust_forwarded_for`) |
| `provider` | string | `"manual"` | `manual` (bank transfer, cash on delivery, etc.; an administrator confirms payment) or `stripe` |
| `reservation_minutes` | integer | `60` | The expiry of Stripe's payment page = how long inventory is held (30–1440 for Stripe) |
| `max_quantity_per_item` | integer | `99` | The maximum quantity per product |
| `max_items` | integer | `50` | The maximum number of products per order |
| `max_orders_per_ip_per_hour` | integer | `20` | Orders per hour from the same IP (0 is unlimited; counted in `[cache]`) |
| `low_stock_threshold` | integer or `null` | `5` | Notify staff when inventory reaches the threshold. `null` disables |

#### `[commerce.manual]`

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `instructions` | string | A guidance text | Payment instructions (such as the transfer destination) shown on the order confirmation page |
| `payment_due_hours` | integer | `72` | Time until unpaid orders are cancelled and inventory restored |

#### `[commerce.stripe]`

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `secret_key` | string | `""` | The secret key. If empty, the environment variable `AEGIS_STRIPE_SECRET_KEY` |
| `webhook_secret` | string | `""` | The webhook's signing secret (`whsec_...`). If empty, the environment variable `AEGIS_STRIPE_WEBHOOK_SECRET` |
| `api_base` | string | `"https://api.stripe.com"` | The API URL (for testing) |
| `async_payment_due_hours` | integer | `72` | The payment deadline for deferred payments such as convenience-store payment (inventory stays reserved during that time) |

### 2.12a `[members]` — Member Accounts

Visitor accounts (member sign-up and login; shared by the shop's order history and reviews and the forum; [Member accounts](./members.md)). They are separate from admin accounts. The former `[commerce.customers]` moved here (if the old form remains, startup fails with an error).

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `enabled` | bool | `false` | `true` uses member accounts. With `false`, the member API and screens are 404 (guest purchases can be used) |
| `registration` | string | `"open"` | `"open"` lets anyone register. `"closed"` stops only new registrations (403; registered members can log in) |
| `session_days` | integer | `30` | The login validity (days; 1–365) |
| `require_email_verification` | bool | `false` | With `true`, after sign-up you cannot log in until you open the confirmation email's link. It requires that confirmation emails can be sent (`[notifications]` is enabled and there is a `public_url`). If not satisfied, it does not start |
| `password_reset` | bool | `true` | Use password reset (email link). It works only when `[notifications]` is enabled and `public_url` is set (email links are built from `public_url`, not the `Host` header) |
| `public_url` | string | `""` | The site URL that is the base of member emails' (reset, confirmation) links (e.g. `https://example.com`). Separate from `[commerce].public_url` for order confirmation links |
| `levels` | array of strings | `["free", "paid"]` | Members' permission levels (low → high). The first is a new member's level. A higher level can also view the content of lower levels. 1–32 characters of lowercase letters, digits, `-`, `_`, no duplicates, and `public` cannot be used (checked at startup). The required level is specified with the article/page `access` ([members.md 15](./members.md#15-viewing-restrictions-member-levels)) |
| `upgrade_url` | string | `""` | The link target of the "upgrade notice" shown to members of an insufficient level. A path within the site starting with `/`, or an `https://` URL (anything else is refused at startup). If empty, no link is shown |

#### `[members.purchase]` — Article Purchases

Settings used for single purchases of articles and pages ([Members 16](./members.md#16-purchases-single-article-purchases-and-subscriptions)). Payment is by Stripe Checkout. To use it, `[members].enabled = true` and `[members].public_url` (the base of the post-payment return destination) are required.

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `enabled` | bool | `false` | `true` uses purchases. With `false`, the purchase API and screens are 404 and the notice shows no purchase link |
| `currency` | string | `"jpy"` | The currency used for article purchases (3 lowercase letters of ISO 4217). The article's `purchase_price` is in this currency's smallest unit |
| `stripe.secret_key` | string | `""` | Stripe's secret key. If empty, the environment variable `AEGIS_STRIPE_SECRET_KEY` |
| `stripe.webhook_secret` | string | `""` | The webhook's signing secret (`whsec_...`). If empty, the environment variable `AEGIS_MEMBERS_STRIPE_WEBHOOK_SECRET`. The endpoint is separate from the shop's webhook, so the secret is separate too |
| `stripe.api_base` | string | `"https://api.stripe.com"` | Change only when pointing at a mock server for testing |

With `enabled = true`, it is a startup error if member accounts are disabled, there is no `public_url`, the currency is invalid, or there is no key.

##### `[[members.purchase.plans]]` — Subscription Plans

Auto-renewing subscriptions ([Members 16](./members.md#16-purchases-single-article-purchases-and-subscriptions)). Writing one or more plans shows a subscription notice on My Page. The unit of payment is decided by `interval` × `interval_count`. The currency of the amount is `[members.purchase].currency`.

```toml
[[members.purchase.plans]]
id = "monthly"
label = "Monthly"
level = "paid"
amount = 800
interval = "month"
interval_count = 1

[[members.purchase.plans]]
id = "quarterly"       # every 3 months
level = "paid"
amount = 2200
interval = "month"
interval_count = 3
```

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `id` | string | Required | The plan's ID (1–32 characters of lowercase letters, digits, `-`, `_`; no duplicates) |
| `label` | string | `id` | The name shown on the payment page and My Page (up to 100 characters) |
| `level` | string | Required | The level gained while subscribed (one of `[members].levels`) |
| `amount` | integer | Required | One payment (in the currency's smallest unit; 1–99,999,999) |
| `interval` | string | Required | `"day"` / `"week"` / `"month"` / `"year"` |
| `interval_count` | integer | `1` | How many `interval`s make one payment. The limits are day 365, week 52, month 12, year 1 (Stripe's limits) |

Invalid plans (a duplicate ID, an unknown level, an out-of-range value) are a startup error. For Stripe's webhook, in addition to the above events, choose `customer.subscription.updated` and `customer.subscription.deleted`. Enable the Customer Portal used for cancelling and changing cards in the Stripe dashboard. In Stripe, create the webhook endpoint `https://example.com/api/v1/members/webhooks/stripe` and choose `checkout.session.completed`, `checkout.session.async_payment_succeeded`, `checkout.session.async_payment_failed`, `checkout.session.expired`, and `charge.refunded` (plus `customer.subscription.updated` and `customer.subscription.deleted` if using subscriptions).

### 2.13 `[notifications]` — Notifications (Email)

Notifies customers and operators of order receipt, payment confirmation, shipping, and cancellation ([Notifications document](./notifications.md)). With `enabled = true`, if the settings are invalid (no `smtp_host` / `from` for the `email` channel, an invalid address format, etc.), the server does not start.

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `enabled` | bool | `false` | `true` sends notifications |
| `channels` | string[] | `["email"]` | Delivery channels. `email` (SMTP) / `log` (the server log; for checking operation) / `plugin:<name>` (a Wasm plugin that has the `send-notification` hook; it must be in `[plugins].enabled`) |
| `locale` | string | `"en"` | The language of the built-in templates (`en` / `ja`) |
| `templates_dir` | string | None | Where templates are placed (`<kind>.subject` / `<kind>.txt` / `<kind>.html`). Missing files use the built-in ones. Ones edited in Admin Studio take precedence over this |
| `disabled_events` | string[] | `[]` | Notification kinds not to send (such as `order.shipped`) |
| `max_attempts` | integer | `3` | The number of attempts for transient failures (SMTP 4xx, connection failures) (1–10; intervals of 2 seconds, 8 seconds, 32 seconds, ...) |
| `html` | bool | `true` | Also build an HTML body (email is sent as both plain text and HTML; [Notifications 3.1](./notifications.md#31-html-email)) |

#### `[notifications.queue]`

The outbox ([Notifications 4](./notifications.md#4-delivery-and-reliability)).

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `persistent` | bool | `true` | Store the outbox in the DB (`_aegis_notification_queue`). It is not lost on restart, and with multiple servers only one sends. With `false`, in memory (lost on restart) |
| `poll_interval_secs` | integer | `10` | The interval (seconds, 1–3600) at which to check notifications whose retry time has come. New notifications are sent without waiting |
| `retention_days` | integer | `7` | Days to keep notifications that could not be sent (the period they can be resent from Admin Studio). 0 never deletes |

#### `[notifications.email]`

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `smtp_host` | string | `""` | The SMTP server (required) |
| `smtp_port` | integer | `0` | The port. If 0, 587 / 465 / 25 depending on `tls` |
| `tls` | string | `"starttls"` | `starttls` (STARTTLS required) / `tls` (TLS from the start) / `none` (no encryption; for a local relay) |
| `username` | string | `""` | The SMTP authentication username (no authentication if empty) |
| `password` | string | `""` | The SMTP authentication password. If empty, the environment variable `AEGIS_SMTP_PASSWORD` |
| `from` | string | `""` | The sender (`"Shop <shop@example.com>"`; required) |
| `reply_to` | string | `""` | The reply-to (the sender if empty) |
| `staff_recipients` | string[] | `[]` | The recipients of notifications for operators (new orders, cancellations). If empty, nothing is sent to operators |
| `timeout_secs` | integer | `15` | The timeout for connecting and sending (seconds) |

#### `[[notifications.audit.rules]]`

Conditions for notifying audit events (administrator logins, login lockouts, etc.) ([Notifications document 2.1](./notifications.md#21-notifying-audit-events)). Writing rules while `[audit].enabled = false` keeps the server from starting.

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `action` | string | (required) | The audit log operation (such as `session.create`; `notification.send` is not allowed) |
| `outcomes` | string[] | `[]` | `success` / `failure` / `denied` (all if empty) |
| `roles` | string[] | `[]` | The actor's role (all if empty) |
| `details` | table | `{}` | Values of `details` (`"*"` means anything as long as the key exists) |
| `title` | string | `""` | The heading of the subject and body |
| `recipients` | string[] | `[]` | Recipients (`staff_recipients` if empty) |
| `max_per_hour` | integer | `20` | The upper limit per hour (0 is unlimited) |

---

### 2.14 `[auth.saml]` — Single Sign-On (SAML 2.0)

Login with a SAML 2.0 IdP (SP-initiated; the AuthnRequest by HTTP-Redirect and the response by HTTP-POST). For usage, see the [Authentication document](./authentication.md#8-single-sign-on-saml-20). With `enabled = true`, if a required item is missing, a certificate cannot be read, or a role name is invalid, the server does not start.

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `enabled` | bool | `false` | `true` enables SAML login |
| `display_name` | string | `"SSO"` | The login screen's button name (`Sign in with ...`) |
| `entity_id` | string | `""` | AegisCMS's (the SP's) Entity ID (required). The AuthnRequest's Issuer and the assertion's Audience. By convention the metadata URL (`https://<host>/api/v1/auth/saml/metadata`) |
| `acs_url` | string | `""` | The Assertion Consumer Service URL (required). `https://<host>/api/v1/auth/saml/acs` |
| `idp_entity_id` | string | `""` | The IdP's Entity ID (required). Refused if it does not match the Issuer of the response and assertion |
| `idp_sso_url` | string | `""` | The URL of the IdP's Single Sign-On service (required; HTTP-Redirect binding) |
| `idp_certificates` | array of string | `[]` | The IdP's signing certificates (required). PEM (`-----BEGIN CERTIFICATE-----`), a PEM public key, or the Base64 of `<X509Certificate>` in the IdP's metadata. Line up old and new during a key rotation. RSA 2048 bits or more |
| `name_id_format` | string | `""` | The NameID format requested in the AuthnRequest (e.g. `urn:oasis:names:tc:SAML:2.0:nameid-format:persistent`). If empty, left to the IdP. `transient` cannot be used |
| `username_attribute` | string | `""` | The attribute to use as a new user's username. If empty, the NameID |
| `email_attribute` | string | `"email"` | The email address attribute (if absent, the NameID when its format is `emailAddress`) |
| `display_name_attribute` | string | `"displayName"` | The display name attribute |
| `auto_create_users` | bool | `true` | Create unregistered users at first login. With `false`, only users linked with `aegis-server user link-saml` |
| `default_role` | string | `"viewer"` | A new user's role. When using `role_attribute`, the role when no value matches |
| `role_attribute` | string | `""` | The attribute that decides the role (e.g. `groups`). If empty, the role is decided by the AegisCMS user account |
| `role_mapping` | table | `{}` | `role_attribute` value → role (`admin` / `editor` / `viewer`). If several match, the strongest role |
| `post_login_redirect` | string | `"/admin/"` | The Admin Studio URL to return to after login (same as `[auth.oidc]`) |

Attributes are looked up by `Name`, and if none match, by `FriendlyName` (URI-form names such as Entra ID's `http://schemas.xmlsoap.org/ws/2005/05/identity/claims/emailaddress` can be written as they are).

### 2.15 `[comments]` — Comments

Reader comments on public pages ([Comments document](./comments.md)). With `enabled = true`, if values are invalid (empty `content_types`, `max_length` out of range, etc.) or the schema for `content_types` is missing, the server does not start.

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `enabled` | bool | `false` | `true` accepts comments |
| `content_types` | string[] | `["article"]` | The content kinds that accept comments |
| `max_depth` | integer | `3` | The depth replies can go (0 for no replies; up to 10) |
| `max_length` | integer | `5000` | The maximum number of characters in the body (up to 100,000) |
| `close_after_days` | integer | `0` | Do not accept new comments after this many days from publication (0 never closes) |
| `notify_author` | bool | `true` | Notify the content's author that a comment was posted (including awaiting approval) (`comment.posted` / `comment.pending`) |
| `notify_staff` | bool | `true` | Notify operators of comments awaiting approval (`comment.pending`) |
| `avatars` | string | `"none"` | The poster's avatar image. `none` (not shown) / `gravatar` ([Comments 3.1](./comments.md#31-avatars)) |
| `avatar_default` | string | `"identicon"` | The image for people with no image on Gravatar (`mp` / `identicon` / `monsterid` / `wavatar` / `retro` / `robohash` / `blank` / `404`) |
| `reject_duplicates` | bool | `true` | Refuse (`409`) when the same person (same email address or IP address) posts the same body on the same content |

#### `[comments.moderation]`

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `require_approval` | bool | `true` | Publish after approval. Can be switched from Admin Studio (that takes precedence) |
| `auto_approve_known` | bool | `true` | With the approval workflow, people with an approved comment under the same name and email address are published without waiting for approval. Can be switched from Admin Studio |
| `max_links` | integer | `2` | More links in the body than this awaits approval |
| `hold_words` | string[] | `[]` | Words that put a comment awaiting approval (case-insensitive) |
| `spam_words` | string[] | `[]` | Words that make a comment spam |
| `max_per_ip_per_hour` | integer | `10` | Posts per hour from the same IP (0 is unlimited; counted in `[cache]`) |

### 2.16 `[forum]` — Forum

A forum of categories, threads, and replies ([Forum document](./forum.md)). With `enabled = true`, if values are invalid (contradictory minimum and maximum character counts, `default_page_size` out of range, etc.), the server does not start.

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `enabled` | bool | `false` | `true` enables `/forum` and the API |
| `database_url` | string | None | A DB dedicated to the forum (e.g. `sqlite:data/forum.db?mode=rwc`). If omitted, shares `[database].url` |
| `title_min_length` / `title_max_length` | integer | `2` / `200` | The thread title's character count (up to 1,000) |
| `body_min_length` / `body_max_length` | integer | `1` / `10000` | The body's character count (up to 100,000) |
| `rate_limit_secs` | integer | `5` | The interval between posts from the same IP/user (seconds; counted in `[cache]`) |
| `posting` | string | `"anyone"` | Who can post. `"anyone"` is everyone (members under their public name, anonymous people by entering a name), `"members"` is only members and staff (requires `[members].enabled = true`). The former `allow_anonymous` is removed (if it remains, it does not start). For category visibility see [Forum 4.0](./forum.md) |
| `auto_lock_post_count` | integer | `1000` | Automatically lock a thread at this many replies (0 is unlimited) |
| `default_page_size` | integer | `50` | Items per page (1–200) |

### 2.17 `[cdn]` — On-demand Cache-Tag Purge (CDN Edge Cache Integration)

Without holding an HTML page cache on the backend, it adds `Cache-Tag` (comma-separated: Cloudflare / BunnyCDN / Netlify, etc.) and `Surrogate-Key` (space-separated: Fastly / Varnish / Google Cloud CDN, etc.) headers to public pages' HTTP responses, and purges the CDN cache by tag when content is published, updated, deleted, scheduled published/unpublished, or site settings change. In environments without a CDN, leaving `enabled = false` (the default) runs with zero overhead.

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `enabled` | bool | `false` | `true` enables adding the `Cache-Tag` / `Surrogate-Key` headers and purging |
| `provider` | `"none"` \| `"log"` \| `"cloudflare"` \| `"fastly"` \| `"webhook"` | `"none"` | The CDN provider to purge. `"none"` only sends response headers and calls no external API. `"log"` outputs the tags to purge to the structured log (`aegis::cdn`) |
| `emit_headers` | bool | `true` | Add the tag headers to `200 OK` public page responses. `provider = "cloudflare"` adds `Cache-Tag` only, `"fastly"` adds `Surrogate-Key` only, and `"none"` / `"log"` / `"webhook"` add both (the same tags appear in 2 forms, so on list pages the headers are about 3 KB. When putting nginx in front, use `proxy_buffer_size 16k;` and the like; [benchmark.md](benchmark.md)). Use `false` if you use neither the tags nor API purging |
| `cache_control` | string | `"public, max-age=0, s-maxage=86400, stale-while-revalidate=60"` | The value to add when a public page response has no `Cache-Control` (nothing is added for an empty string) |
| `timeout_ms` | integer (u64) | `5000` | The HTTP timeout of CDN purge API calls (milliseconds, 100–60000) |

#### `[cdn.cloudflare]` (`provider = "cloudflare"`)

Calls Cloudflare's `POST /zones/{zone_id}/purge_cache` (`{"tags": [...]}`) (split automatically 30 tags at a time).

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `zone_id` | string | `""` | Cloudflare Zone ID (required) |
| `api_token` | string | `""` | An API token with Cache Purge permission. Can also be specified as `"env:VAR_NAME"` or with the environment variable `AEGIS_CLOUDFLARE_API_TOKEN` |
| `api_base` | string | `"https://api.cloudflare.com/client/v4"` | The Cloudflare API base URL |

#### `[cdn.fastly]` (`provider = "fastly"`)

Calls Fastly's `POST /service/{service_id}/purge` (the `Surrogate-Key` header, split automatically 256 keys at a time).

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `service_id` | string | `""` | Fastly Service ID (required) |
| `api_token` | string | `""` | Fastly API token (`Fastly-Key`). Can also be specified as `"env:VAR_NAME"` or with the environment variable `AEGIS_FASTLY_API_TOKEN` |
| `soft_purge` | bool | `true` | `true` adds `Fastly-Soft-Purge: 1` (marks as stale rather than deleting immediately, and revalidates in the background) |
| `api_base` | string | `"https://api.fastly.com"` | The Fastly API base URL |

#### `[cdn.webhook]` (`provider = "webhook"`)

Sends a JSON payload of `{"action": "purge_tags" | "purge_all", "tags": [...], "surrogate_key": "..."}` as an HTTP request to Varnish, Nginx, BunnyCDN, an AWS CloudFront Lambda / Cloudflare Workers adapter, or your own reverse proxy.

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `url` | string | `""` | The destination endpoint URL (`http://` or `https://`; required) |
| `method` | string | `"POST"` | The HTTP method (`"POST"`, `"PURGE"`, `"DELETE"`, etc.) |
| `bearer_token` | string | `""` | The token to put in `Authorization: Bearer <token>`. Can also be specified as `"env:VAR_NAME"` or with the environment variable `AEGIS_CDN_WEBHOOK_TOKEN` |
| `headers` | table | `{}` | Arbitrary custom HTTP headers to add |

### 2.18 `[search]` — Site Search

Full-text search of published content ([Site search document](./search.md)). It is built in with the server's Cargo feature `search` (enabled by default).

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `enabled` | bool | `true` | `true` enables `/search` and `GET /api/v1/search` |
| `index_dir` | string | `"data/search"` | The directory for the index (in memory only if empty; recreated at every startup) |
| `content_types` | string[] | `["article", "page"]` | The content kinds to search |

### 2.19 `[analytics]` — Analytics

Cookie-less analytics ([Analytics document](./analytics.md)). It is built in with the server's Cargo feature `analytics` (built in by default). If `exclude_paths` does not start with `/`, the server does not start.

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `enabled` | bool | `false` | `true` puts the beacon into public pages, aggregates, and shows the Analytics tab in Admin Studio |
| `retention_days` | integer | `400` | Days to keep aggregates (0 never deletes) |
| `flush_interval_secs` | integer | `30` | The interval for writing aggregates accumulated in memory to the DB (seconds) |
| `exclude_paths` | string[] | `[]` | Paths not to count (prefix match. `"/preview"` covers `/preview` and `/preview/...`) |

### 2.20 `[contact]` — Contact Form

The public site's contact form and Admin Studio's inbox ([Contact form document](./contact.md)). With `enabled = true`, if values are invalid (duplicate form or field names, a `select` with no options, an `auto_reply` with no required email address field, etc.), the server does not start.

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `enabled` | bool | `false` | `true` enables `/contact`, the API, and the inbox |
| `retention_days` | integer | `365` | Days to keep received inquiries (0 never deletes) |
| `max_per_ip_per_hour` | integer | `5` | Submissions per hour from the same IP (0 is unlimited; counted in `[cache]`) |
| `max_links` | integer | `3` | More links in the input than this is spam |
| `spam_words` | string[] | `[]` | Words that make a submission spam |
| `notify_staff` | bool | `true` | Notify operators that one arrived (`contact.received`) |
| `locale` | string | `"en"` | The language of built-in text (input errors, default headings and field names). `en` / `ja` |
| `forms` | table[] | One `default` form | `[[contact.forms]]` (`name`, `title`, `description`, `submit_label`, `success_message`, `auto_reply`, `subject_field`, `fields`). Fields are `[[contact.forms.fields]]` ([contact.md 1.1](./contact.md#11-fields)) |

### 2.21 `[tags]` — Tags

Tags that can be attached to any kind of content, such as articles, static pages, and products ([Tags document](./tags.md)). Enabled by default. If `max_per_content` is out of range, the server does not start.

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `enabled` | bool | `true` | `false` stops using the tag API and Admin Studio's screens (`404`). Attached tags remain in the DB |
| `max_per_content` | integer | `20` | The number of tags that can be attached to one content item (1–100) |

### 2.22 `[newsletter]` — Newsletter

Email address sign-up (double opt-in with a confirmation email) and bulk delivery of published articles ([Newsletter document](./newsletter.md)). Emails are sent through `[notifications]`, so `[notifications].enabled = true` and the SMTP settings are needed first. With `enabled = true`, if `[commerce].public_url` is empty or invalid, the server does not start.

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `enabled` | bool | `false` | `true` enables `/newsletter`, the API, and Admin Studio's screen |
| `max_per_ip_per_hour` | integer | `10` | Sign-ups per hour from the same IP (0 is unlimited; counted in `[cache]`) |
| `confirm_ttl_hours` | integer | `48` | Validity of the confirmation link (hours; 1–336) |
| `locale` | string | `"en"` | The language of the text on the sign-up, confirmation, and unsubscribe pages. `en` / `ja` |

### 2.23 `[consent]` — Cookie Consent Banner

A consent banner for not running ads, analytics, and external embeds that require consent until consent is given ([Cookie consent banner document](./consent.md)). The banner's HTML and wording are the theme's `consent.html` (or the built-in one), and the wording's language is `[theme].locale`. With `enabled = true`, if values are invalid, the server does not start.

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `enabled` | bool | `false` | `true` shows the banner on public pages |
| `layout` | string | `"banner"` | The banner's shape. `banner` (a strip at the screen edge) / `box` (a card in a corner) / `modal` (center) |
| `position` | string | The shape's default | Position. `banner`: `bottom` / `top`; `box`: `bottom-right` / `bottom-left` / `top-right` / `top-left`; `modal`: `center` |
| `backdrop` | bool | `true` for `modal` only | Dim the background and make the page non-interactive |
| `delay_ms` | integer | `0` | Time to delay the display (0–10000) |
| `cookie_days` | integer | `180` | Days to keep the choice (1–395) |
| `policy_url` | string | `""` | The privacy policy URL (a path starting with `/` or `http(s)://`) |
| `message` | string | `""` | The description (up to 600 characters). If empty, the theme's default text |
| `reopen_button` | bool | `true` | Show a settings button after a choice is made |
| `revision` | integer | `1` | Increasing it asks everyone again |
| `categories` | table[] | `analytics` and `marketing` | `[[consent.categories]]` (`id`, `label`, `description`; up to 10) |

### 2.24 `[captcha]` — Shared CAPTCHA

Applies a CAPTCHA such as Cloudflare Turnstile, hCaptcha, or reCAPTCHA together to the destinations listed in `targets` ([Contact form 1.2](./contact.md#12-captcha-optional)). Write the keys in one place and switch where it applies with `targets`. If `targets` is empty or duplicated, or the keys are invalid, the server does not start.

```toml
[captcha]
targets = ["contact", "newsletter", "comments", "forum"]   # the destinations to apply it to
provider = "turnstile"
site_key = "0x4AAAA..."
secret = "env:TURNSTILE_SECRET"
```

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `targets` | string[] | (required) | `contact` (contact), `newsletter` (newsletter sign-up), `comments` (comments), `forum` (forum thread creation and replies). Does nothing if that feature is disabled |
| `provider` | string | (required) | `turnstile` / `hcaptcha` / `recaptcha` / `custom` |
| `site_key` | string | (required) | The site key (may be public) |
| `secret` | string | (required) | The secret key. Can be read from an environment variable with `"env:VAR_NAME"` |
| `fail_open` | bool | `false` | Let requests through when the verification service cannot be reached (the default is to refuse) |
| `timeout_secs` | integer | `5` | Seconds to wait for the verification service's response (1–30) |
| `verify_url`, `script_url`, `widget_class`, `response_field` | string | The provider's defaults | Required for `custom`. For other providers, writing them replaces the defaults |

### 2.25 `[headless]` — Headless Delivery API

A read-only API that returns only published content as JSON without authentication (`GET /api/v1/public/contents/{type}`, `.../{type}/{slug}`; [Headless delivery API design](./headless.md)). It is built in with the server's Cargo feature `headless` (built in by default). **It is disabled by default**, and returns 404 when disabled.

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `enabled` | bool | `false` | `true` enables the delivery API |
| `content_types` | string[] | `["article", "page"]` | The kinds to deliver. Kinds not here return 404 as if nonexistent (kinds with internal values, such as products, are not returned unless explicitly specified) |
| `max_page_size` | integer | `100` | The upper limit of items per list page (`limit` is clamped to this value) |
| `cache_max_age` | integer | `60` | The response's `Cache-Control: public, max-age=...` (seconds). Takes precedence over `[cdn].cache_control` (`no-store` if caching is disabled overall) |
| `rate_limit_per_minute` | integer | `600` | The per-IP limit per minute (429 when exceeded). 0 means no limit. Note that if `[audit].trust_forwarded_for` is disabled, everyone behind a reverse proxy is the same IP |

### 2.26 `[queries]` — Declarative Queries

Aggregations and conditional lists are declared in YAML (`*.yaml` in `[paths].queries_dir`), and the results are passed to the theme's `queries` and to plugins. For how to write definitions, behavior, and notes, see [queries.md](queries.md).

| Key | Default | Description |
| --- | --- | --- |
| `max_limit` | `100` | The maximum of a query's `limit` (rows; 1–1000) |
| `timeout_ms` | `2000` | The timeout of one execution (milliseconds; 100–60000). A query that exceeds it keeps its previous result |
| `max_concurrency` | `8` | The number of queries that can run at the same time (1–64) |
| `max_in_items` | `100` | The maximum length of an `in` list (1–1000) |
| `timezone` | `"UTC"` | The time zone used for date splits (`bucket`) and datetime conditions (an IANA name, e.g. `"Asia/Tokyo"`). Changing it recreates the public-side columns at startup |

Wrong definitions and mistakes in these settings are a startup error that stops the server.

## 3. A Complete Example

A parseable-as-is example with every item written out.

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
low_stock_threshold = 5

[commerce.manual]
instructions = "Thank you for your order. We will email you the payment instructions."
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

[tags]
enabled = true
max_per_content = 20

[newsletter]
enabled = false
max_per_ip_per_hour = 10
confirm_ttl_hours = 48
locale = "en"

[consent]
enabled = false
layout = "banner"
position = ""
delay_ms = 0
cookie_days = 180
policy_url = ""
message = ""
reopen_button = true
revision = 1

[comments]
enabled = false
content_types = ["article"]
max_depth = 3
max_length = 5000
close_after_days = 0
notify_author = true
notify_staff = true
avatars = "none"
avatar_default = "identicon"
reject_duplicates = true

[comments.moderation]
require_approval = true
auto_approve_known = true
max_links = 2
hold_words = []
spam_words = []
max_per_ip_per_hour = 10

[search]
enabled = true
index_dir = "data/search"
content_types = ["article", "page"]

[headless]
enabled = false
content_types = ["article", "page"]
max_page_size = 100
cache_max_age = 60
rate_limit_per_minute = 600

[analytics]
enabled = false
retention_days = 400
flush_interval_secs = 30
exclude_paths = []

[contact]
enabled = false
retention_days = 365
max_per_ip_per_hour = 5
max_links = 3
spam_words = []
notify_staff = true
locale = "en"

[[contact.forms]]
name = "default"
title = "Contact"
description = ""
submit_label = "Send"
success_message = "Thank you. We have received your message."
auto_reply = false
subject_field = "subject"

[[contact.forms.fields]]
name = "name"
label = "Name"
type = "text"
required = true

[[contact.forms.fields]]
name = "email"
label = "Email"
type = "email"
required = true

[[contact.forms.fields]]
name = "subject"
label = "Subject"
type = "text"
required = false

[[contact.forms.fields]]
name = "message"
label = "Message"
type = "textarea"
required = true
```

---

## 4. Operating Behind a Reverse Proxy

Behind a reverse proxy such as nginx / Caddy / ALB, you can publish at the root of a subdomain (`cms.example.com`) or under a subpath (`example.com/cms/`). The public site (`/`), Admin Studio (`/admin/`), the API (`/api/v1`), uploads (`/uploads`), and theme assets (`/theme/assets`) work under the same origin and the same path.

### 4.1 Publishing on a Subdomain

Example: `https://cms.example.com/`

No `[server]` settings are needed. Just forward from the proxy to AegisCMS as is (see the nginx example below).

### 4.2 Publishing Under a Subpath

Example: `https://example.com/cms/`

Specify the path to publish at in `[server].base_path`.

```toml
[server]
base_path = "/cms"
```

- The base path is prepended to URLs the server generates (theme links, URLs of uploaded files, the `/admin` redirect, and the return destination after SSO login).
- Have the proxy forward the path **as is** (keeping `/cms/...`). It also works if you configure it to strip the base path before forwarding, but then a slug with the same name as the base path (`/cms/cms`) can no longer be opened. Requests without the base path (such as `/api/v1/health`) are also accepted as they are, so container health checks need no change.
- In your own theme's templates, prefix links with the global variable `base_path` (`<a href="{{ base_path }}/articles/{{ item.slug }}">`; [Theme guide](./theme_and_publishing_guide.md#global-variables-available-in-templates)).
- Root-relative links written by hand in the body (such as `/about`) are not rewritten. Write them including the base path, like `/cms/about`.
- URLs of uploaded files are stored in the body with the base path included. **If you change `base_path` later, images and links in already saved content keep the old path**, so decide before going live.

### 4.3 Admin Studio's Connection Target

The production build connects to the `/api/v1` of the server that served it (the origin and base path derived from `{server}/admin/assets/*.js`), so no build per domain or base path is needed.

When serving Admin Studio alone from a different place (for example, using the API of `example.com/cms` from `admin.example.com`), specify `VITE_AEGIS_SERVER_URL=https://example.com/cms` at build time (in Docker, `docker build --build-arg VITE_AEGIS_SERVER_URL=...`). The API authenticates with a Bearer token, but **APIs requiring authentication are same-origin only by default** (no CORS is returned), so write Admin Studio's origin in `[server].cors_origins = ["https://admin.example.com"]` (4.3.1). The preview's theme assets and uploaded images, and the SSO login URL, use this URL too.

#### 4.3.1 CORS (`[server].cors_origins`)

| Target | Default (`cors_origins = []`) | When origins are written |
| --- | --- | --- |
| Public reads (`GET` / `HEAD`. Public pages, `/uploads/`, theme static files and others outside `/api/`, and `/api/v1/public/`, `/api/v1/search`, `/api/v1/health`) | `Access-Control-Allow-Origin: *` to all origins | The same |
| APIs requiring authentication and writes (`POST` / `PUT` / `PATCH` / `DELETE`) | No CORS (same-origin only; browsers of other origins cannot read the response) | Returns that origin's value only to the origins written (`Vary: Origin`), and allows the methods and the headers the preflight asked for |

- The value is an origin such as `https://admin.example.com` (scheme, host, and an optional port; no path). Writing `"*"` returns to the earlier behavior (all methods to all origins). An invalid value stops startup.
- `Access-Control-Allow-Credentials` is not returned (authentication is by Bearer token, not cookies).
- When Admin Studio served from a different domain or a headless client calls authenticated APIs, add that origin.

### 4.4 Other Settings to Check

| Setting | Description |
| --- | --- |
| `[audit].trust_forwarded_for = true` | Takes the client IP for audit logs and login rate limiting from `X-Forwarded-For`. If left disabled, everyone is counted as the proxy's IP, and the per-IP lock applies to all users |
| `[auth.oidc].redirect_url` | Make it the public URL (`https://cms.example.com/api/v1/auth/oidc/callback`, or `https://example.com/cms/api/v1/auth/oidc/callback` for a subpath). The default `127.0.0.1` cannot be returned to from the IdP |
| `[auth.saml].entity_id` / `acs_url` | Make them public URLs (`https://cms.example.com/api/v1/auth/saml/acs`, or `https://example.com/cms/api/v1/auth/saml/acs` for a subpath). They are refused if they do not match the response's `Destination` and `Recipient` |
| `[auth.oidc].post_login_redirect` / `[auth.saml].post_login_redirect` | Values starting with `/` (the default `/admin/`) automatically get the base path prepended. If Admin Studio is placed elsewhere, an absolute URL (`https://admin.example.com/admin/`) |
| `[security].internal_domains` | So absolute URLs to your own site in the body are not treated as external links (given `rel="noopener ..."`), specify something like `["example.com"]` (subdomains included as internal) |
| `[storage].max_upload_mb` | Make the proxy-side limit (such as nginx's `client_max_body_size`) the same value or more |

nginx example (subdomain):

```nginx
server {
    listen 443 ssl;
    server_name cms.example.com;

    client_max_body_size 10m;  # match [storage].max_upload_mb

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        # AegisCMS uses the first address, so do not carry over what the client sent
        proxy_set_header X-Forwarded-For $remote_addr;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

nginx example (subpath, `base_path = "/cms"`):

```nginx
server {
    listen 443 ssl;
    server_name example.com;

    client_max_body_size 10m;

    # By not adding a URI (such as a trailing /) to proxy_pass, /cms/... is forwarded as is
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
> `X-Forwarded-For` can be set freely by clients. When enabling `trust_forwarded_for`, make AegisCMS unreachable directly (`[server].host = "127.0.0.1"` or a firewall), and overwrite the `X-Forwarded-For` received on the proxy side as in the examples above (`$proxy_add_x_forwarded_for` appends after the received value, so the first entry can be spoofed). With multiple proxy stages, overwrite at the outermost proxy.

### 4.5 Publishing Together with Caddy (`deploy/`)

[`deploy/`](../deploy) has a setup that starts AegisCMS and Caddy (which obtains and renews Let's Encrypt certificates automatically) with Docker Compose. Write the domains and site settings in `deploy/sites.toml` and start with `./deploy.sh up`. Each site gets its own AegisCMS container, so just adding a `[[sites]]` lets you publish multiple sites (domains) on the same server. Each site's `aegis.toml` is generated by `aegis-server deploy generate` on top of `config/docker.toml`, with `[audit].trust_forwarded_for = true` set. For the steps, see [deploy/README.md](../deploy/README.md).

---

### 4.6 Making It Harder to Identify the CMS in Use

Attackers' automatic scans identify the CMS from the product name, version, and fixed URLs and try known vulnerabilities. The following settings reduce the clues (it only hides things, so it does not replace updates, two-factor authentication, or restricting the route to the admin screen).

| Setting | Effect |
| --- | --- |
| `[server].admin_path = "/<a hard-to-guess name>"` | Serves Admin Studio at a different path. The default `/admin` stops being Admin Studio (a 404 on the public site). A static page whose slug has the same name can no longer be opened |
| `[server].namespace = "<any name>"` | Changes the prefix of identifiers that appear in public pages' HTML, CSS, and JS (`aegis-consent` → `<name>-consent`, `/_aegis/consent.js` → `/_<name>/consent.js`, `window.AegisConsent` → `window.<name>Consent`, etc.). The default is `cms` (specifying `aegis` keeps the original names). The replacement is done right before HTML, CSS, and JS responses are returned, so templates and your own themes can keep writing `aegis-` |
| `[theme].show_powered_by = false` | Removes notices such as "Powered by AegisCMS" from the footers of the built-in templates and bundled themes |
| `[theme].show_admin_link` | Whether to show a link to Admin Studio on public pages (the headers and footers of the built-in templates and bundled themes). **If unspecified, it is shown only when `admin_path` is the default `/admin`** (changing the path was to hide it, so once changed the link is not shown either). `true` shows a link to the changed path, and `false` does not show it even when the path is the default |

- What `namespace` replaces, in `text/html`, `text/css`, and JavaScript responses (excluding `/api/`, Admin Studio, and uploads), is `aegis-`, `aegis_`, `aegis:`, `/_aegis/`, names like `AegisConsent`, and constant names starting with `AEGIS_`. If the body of an article has a string of the same form (for example, `aegis-server` in a code example), it is replaced too. Parts of URLs (`/aegis-…`) and the product name `AegisCMS` are not replaced. When you change it, saved cookies and localStorage (consent choices, the cart) are not carried over, and if you hard-coded names other than `aegis-` in the theme's CSS, fix them to match.
- Do not write the changed path in `robots.txt` (writing it in `Disallow` makes the path public). The default `robots.txt`'s `Disallow: /admin/` can stay as is.
- If `post_login_redirect` of `[auth.oidc]` / `[auth.saml]` is left at the default (`/admin/`), it follows the new path automatically. If you wrote `[auth.registration].admin_url`, fix it to the new path.
- When you change `admin_path`, also update bookmarks and the return URLs registered with the IdP.
- `GET /api/v1/health` returns the product name and version only to an `admin` session. In addition, block `/health`, `/api/v1/health`, and `/metrics` from outside at the reverse proxy (blocked by default in the `deploy/` Caddy setup; [deploy/README.md](../deploy/README.md#5-operations)), and protecting Admin Studio's path with IP restrictions or a VPN is more effective.

## 5. Dynamic Site & SEO Settings from the Admin Screen

So they can take effect immediately without restarting the server, the site's basic information and search engine / bot control are stored in the database's `_aegis_settings` table and can be changed in real time from Admin Studio's "Settings" screen.

### 5.1 General Settings (`site.*`)

| Item | DB key | Description |
| --- | --- | --- |
| Site name | `site.title` | Title display of the public site and the admin screen |
| Favicon | `site.favicon` | The icon image URL loaded with `<link rel="icon">` (chosen from the media library or uploaded) |

### 5.2 SEO & Bot Settings (`seo.*`)

| Item | DB key | Type / value form | Default | Description |
| --- | --- | --- | --- | --- |
| Exclude whole site from indexing | `seo.noindex_all` | `"true"` | `"false"` | When enabled, `noindex, nofollow` is forcibly injected into the whole site's `<head>` (for development and private environments) |
| Canonical Base URL | `seo.canonical_base_url` | URL string | `""` | The site's canonical domain (e.g. `https://example.com`). When set, `<link rel="canonical">` is output on all pages |
| Default Meta description | `seo.meta_description` | String (up to 500 characters) | `""` | The `<meta name="description">` output as a fallback when a page has no individual summary |
| Default Meta keywords | `seo.meta_keywords` | String (comma-separated) | `""` | Output of `<meta name="keywords">` |
| Default OGP image | `seo.og_image` | Image URL | `""` | The default eye-catch image when sharing on SNS (can be chosen from the media library) |
| Twitter Card type | `seo.twitter_card` | `"summary"` \| `"summary_large_image"` | `"summary_large_image"` | The card display format when sharing on Twitter / X |
| Custom robots.txt | `seo.robots_txt` | Text (up to 64 KB) | Empty (auto-generated) | Crawler control. If empty, safe default rules and a dynamic Sitemap reference are output |

> [!TIP]
> Admin Studio's SEO settings screen has one-click presets "Standard", "Development (block all)", and "Reject generative AI crawlers", so you can configure `robots.txt` safely and quickly.

### 5.3 System Operation Settings (`system.*`)

These take effect immediately from Admin Studio's "Settings › System" without restarting the server process. Values are stored in the database's `_aegis_settings` and override the initial values in the config file (TOML).

| Item | DB key | Type / value form | Default | Description |
| --- | --- | --- | --- | --- |
| Maintenance mode | `system.maintenance_mode` | `"true"` \| `"false"` | `"false"` | When enabled, ordinary requests get `503 Service Unavailable` and a maintenance screen. The admin screen and administrators can access as usual |
| Maintenance message | `system.maintenance_message` | String (up to 1000 characters) | `"Site is currently under maintenance..."` | The notice shown on the maintenance screen |
| Maximum upload size | `system.max_upload_mb` | Integer (1–1024) | `[storage].max_upload_mb` | The maximum file size (MB) when uploading assets |
| Enable caching | `system.cache_enabled` | `"true"` \| `"false"` | `[cdn].enabled` | Enable or disable response caching of public pages |
| Cache TTL | `system.cache_ttl_secs` | Integer (seconds) | `[cache].default_ttl_secs` | The seconds of public pages' `Cache-Control: max-age={ttl}` |
| Accept new user registration | `system.registration_enabled` | `"true"` \| `"false"` | `[auth.registration].enabled` | Whether to accept self-service new account registration from Admin Studio's login screen |
| Login attempt limit | `system.max_login_attempts` | Integer (0 is unlimited) | `[auth.login_rate_limit].max_failures_per_ip` | The maximum allowed login failures from the same IP address |
| Accept comment posts | `system.comments_enabled` | `"true"` \| `"false"` | `[comments].enabled` | Temporarily stop / resume accepting new comment posts from the public site |
| Accept forum posts | `system.forum_enabled` | `"true"` \| `"false"` | `[forum].enabled` | Temporarily stop / resume accepting new thread and reply posts in the forum |
