[← Back to the documentation index](README.md)

**English** | [日本語](development.ja.md)

# Development Guide

Collects AegisCMS's development environment, verification, builds, CI, and Docker image. For branch, commit, and PR conventions, see [AGENTS.md](../AGENTS.md); for the internal design, see [architecture.md](architecture.md).

---

## 1. Tasks (`Taskfile.yml`)

At the root, use [go-task](https://taskfile.dev/)'s `task` (`task` alone lists them).

| Task                                                               | Description                                                                                                                                                                                                      |
| ------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `task dev` / `task dev:admin`                                      | The server (port 8080) / Admin Studio's dev server                                                                                                                                                               |
| `task check`                                                       | The full set of pre-PR verification: `fmt:check`, `fmt:admin:check`, `lint`, `lint:admin`, `test`, `build:admin`                                                                                                 |
| `task migrate`                                                     | Schema migration                                                                                                                                                                                                 |
| `task dump` / `task restore`                                       | Create / restore a one-shot backup ZIP of the database and upload storage                                                                                                                                        |
| `task codegen` (`codegen:ts` / `codegen:openapi`)                  | Generate `admin/src/types/schemas.ts` and `docs/openapi.json` from `schemas/*.yaml`. Run it when you change schemas                                                                                              |
| `task build` / `build:server` / `build:admin`                      | Production builds                                                                                                                                                                                                |
| `task test:admin`                                                  | Admin Studio unit tests (Vitest; [8.1](#81-unit-tests-vitest-adminsrctestts))                                                                                                                                    |
| `task test:e2e`                                                    | Admin Studio E2E tests (Playwright; [8.2](#82-e2e-tests-playwright-admine2e))                                                                                                                                    |
| `task storybook` / `build:storybook`                               | Admin Studio's Storybook (port 6006) / static build                                                                                                                                                              |
| `task plugin:build*`                                               | Wasm builds of plugins ([4](#4-building-plugins))                                                                                                                                                                |
| `task bench:plugins`                                               | Time per plugin hook call and throughput under concurrency (`crates/aegis-runtime/examples/plugin_profile.rs`)                                                                                                   |
| `task bench:http` (`bench`)                                        | The server's HTTP performance benchmark (top page, article, API, preview, quote; `scripts/benchmark.mjs`)                                                                                                        |
| `task bench:all`                                                   | Run plugin performance measurement and the HTTP benchmark together                                                                                                                                               |
| `task bench:dbs`                                                   | Run a benchmark comparison across the 3 engines SQLite / PostgreSQL / MySQL ([6.3](#63-per-database-performance-comparison-task-benchdbs--scriptscompare-dbsmjs))                                                |
| `task demo:shop` / `demo:shop:seed`                                | Start the sample shop / load data ([examples/coffee-shop](../examples/coffee-shop))                                                                                                                              |
| `task demo:forum` / `demo:forum:seed`                              | Start the sample forum / load data ([examples/hiking-forum](../examples/hiking-forum))                                                                                                                           |
| `task docs:screenshots`                                            | Retake the screenshots in `docs/images/` (starts a disposable server with sample data and takes them with Playwright; `scripts/capture-docs-screenshots.mjs`. Run it after changing screens and commit the diff) |
| `task docker:build` / `docker:run` / `docker:stop` / `docker:logs` | Docker image ([5](#5-docker-image))                                                                                                                                                                              |
| `task docker:dbs:up` / `docker:dbs:down`                           | Start / stop the real-database verification containers (PostgreSQL 16 / MySQL 8.0 / Redis 7) ([7](#7-real-database-verification-postgresql--mysql))                                                              |
| `task test:dbs`                                                    | Run the order and inventory integration tests in the real-container environment together                                                                                                                         |

---

## 2. Verification and Linting

- **Rust**: `cargo fmt --check`, `cargo clippy --workspace -- -D warnings` (zero warnings; `[workspace.lints]`), `cargo test --workspace`.
    - With `--all-targets`, `default_constructed_unit_structs` appears in the `sdk/templates/rust` tests (CI / `task check` run without `--all-targets`, so it is not addressed).
- **Admin Studio**: Prettier (`format:check`), oxlint (`--deny-warnings`; the missing-translation check `scripts/check-locales.mjs` also runs in `npm run lint`), `npm run build` (including type checking).
- **Prettier and VS Code**:
    - Prettier reads `.editorconfig` indentation. So that the 4 spaces of `[*]` do not apply to CSS / HTML, `[*.{ts,tsx,js,jsx,css,html}]` is set to 2.
    - The VS Code Prettier extension by default looks for `.prettierignore` directly under the workspace, so `.vscode/settings.json` points `prettier.ignorePath` at `admin/.prettierignore`. `prettier.requireConfig` leaves JSON and the like outside `admin/` unformatted.
    - `src/types/schemas.ts` (generated) and Markdown are not formatted.
- **Verifying refactors that do not change appearance**: with Playwright, take `getComputedStyle` of all screens (about 90 properties × all elements) and compare before and after (used for the split into CSS Modules).
- **The public site's smartphone display**: when you change a theme, check at widths 320 / 360 / 390px whether the main pages scroll horizontally (for the steps, see [Theme authoring guide 6](theme_authoring_guide.md#7-development-flow)).

---

## 3. CI (`.github/workflows/ci.yml`)

On pushes and PRs to `main`, it runs the following.

| Job           | Description                                                                                                        |
| ------------- | ------------------------------------------------------------------------------------------------------------------ |
| `rust-ci`     | `cargo fmt --check` → `cargo clippy --workspace -- -D warnings` → `cargo test --workspace`                         |
| `frontend-ci` | `npm ci` → Prettier → oxlint → type check and build → Storybook build                                              |
| `admin-e2e`   | Build the server and Admin Studio and run the Playwright E2E tests (on failure, the report is kept as an artifact) |
| `docker-ci`   | Build the Docker image (in parallel with the above two; GitHub Actions cache `type=gha`, profile `ci-release`)     |

- `docker-ci` verifies that it can build, so it builds with the `ci-release` profile that omits LTO (`--build-arg CARGO_PROFILE=ci-release`; the optimization level is the same as release). The production release profile uses `lto = "thin"` / `codegen-units = 16` (Thin LTO), which greatly shortens link and optimization time compared with Fat LTO while keeping production-equivalent execution speed.
- Debug info for the dev / test profiles is `line-tables-only` (line numbers in backtraces remain). With full debug info, the PDB of the `aegis-server` test binary is about 500 MB (about 200 MB with `line-tables-only`; the rebuild when changing `aegis-server` on Windows goes from about 10 seconds to 6.5 seconds). When you want to see variables in a debugger, override with `CARGO_PROFILE_DEV_DEBUG=true` (dependencies are rebuilt too).

### 3.1 Memory Safety and Dependency Audits

| Workflow             | Runs                                                     | What it checks                                                                                                                                                                                         |
| -------------------- | -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `security-audit.yml` | On `Cargo.lock` / `Cargo.toml` changes, and weekly       | `cargo audit --deny warnings`: known vulnerabilities and unmaintained crates in dependencies (RustSec). Exceptions, each with a reason, are in `.cargo/audit.toml`; open ones are tracked in `TODO.md` |
| `miri.yml`           | On changes to `aegis-plugin-sdk` / `aegis-plugin-macros` | `cargo miri test -p aegis-plugin-sdk`: undefined behavior in the SDK's `unsafe` (out-of-bounds reads, double frees, uninitialized memory)                                                              |

- **`unsafe` is forbidden in the workspace**: `[workspace.lints.rust] unsafe_code = "forbid"` in `Cargo.toml` makes any `unsafe` fail the build, and `#[allow(unsafe_code)]` cannot override `forbid`. The only exception is `aegis-plugin-sdk`, which needs `unsafe` for the Wasm ABI. It has its own `[lints]` table with `unsafe_code = "deny"` (copy the workspace table there when you change it) and allows `unsafe` only in `abi.rs`, `host.rs`, and `tests/`. Write a `// SAFETY:` comment on every `unsafe` block.
- **Locally**: `task audit` (`cargo install cargo-audit --locked`) and `task miri` (`rustup +nightly component add miri rust-src`).
- **Where the host trusts the plugin least**: the host never trusts a `ptr` / `len` that a Wasm plugin passes or returns. It checks them against the limit and the guest's linear memory before allocating (`crates/aegis-runtime/src/wasm/engine.rs`, `instance.rs`). The SDK's `deallocate` / `read_bytes_from_host` use a registry of what `allocate` handed out, so a wrong pointer, a wrong length, or a second free does nothing.

---

## 4. Building Plugins

| Task                              | Target                                                                                                                      | What you need                              |
| --------------------------------- | --------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------ |
| `task plugin:build`               | `sdk/templates/rust/plugin.wasm`                                                                                            | `rustup target add wasm32-unknown-unknown` |
| `task plugin:build:as`            | `sdk/templates/assemblyscript/plugin.wasm`                                                                                  | Node.js (`npm install`)                    |
| `task plugin:build:tinygo`        | `sdk/templates/tinygo/plugin.wasm`                                                                                          | TinyGo 0.31+ and `wasm-opt` (below)        |
| `task plugin:build:tinygo-hooks`  | `sdk/tinygo/examples/hooks/plugin.wasm` (the TinyGo version of the same; `sdk_hook_examples.rs` verifies it on a real host) | TinyGo 0.31+ and `wasm-opt` (below)        |
| `task plugin:build:as-hooks`      | `sdk/examples/assemblyscript-hooks/plugin.wasm` (examples of the order, notification, comment, and block hooks)             | Node.js (`npm install`)                    |
| `task plugin:build:code-enhancer` | `plugins/official-code-enhancer/plugin.wasm`                                                                                | Same (Rust)                                |
| `task plugin:build:mermaid`       | `plugins/official-mermaid/plugin.wasm`                                                                                      | Same (Rust)                                |
| `task plugin:build:plantuml`      | `plugins/official-plantuml/plugin.wasm`                                                                                     | Same (Rust)                                |
| `task plugin:build:shop`          | `plugins/official-shop-rules` / `official-order-notifier`                                                                   | Same (Rust)                                |
| `task build:wasm`                 | `sdk/examples/slack-notifier/plugin.wasm` (from hand-written WAT)                                                           | —                                          |

Building with TinyGo needs binaryen's `wasm-opt` in addition to TinyGo itself (without it, it fails with `could not find wasm-opt, set the WASMOPT environment variable to override`). Install binaryen and put it on PATH, or specify the path of `wasm-opt` in the `WASMOPT` environment variable. On Windows with no executable, you can also make a `.cmd` that calls, via `node`, the npm version's `node_modules/binaryen/bin/wasm-opt` (a Node.js script) that comes with `npm ci` in `sdk/assemblyscript`, and specify that path in `WASMOPT`.

Built `plugin.wasm` files are committed. `crates/aegis-runtime/tests/sdk_plugins.rs` runs all templates on a real host, so **when you change the SDKs, templates, or official plugins, rebuild and commit** (slack-notifier is unrelated to the SDKs).

`sdk/` is for templates and samples, and `plugins/` is the installation destination. Placing something in `plugins/` alone does not load it, and only those written in `[plugins].enabled` are enabled ([architecture.md 10.2](architecture.md#102-installation-and-enabling-wasmpluginloader)).

---

## 5. Docker Image

- Multi-stage build: `node:22.22-alpine` (Admin Studio) → `rust:1.99.1-slim-bookworm` (server) → `debian:bookworm-slim` (runtime). About 163 MB.
- Base image versions are pinned (`latest` changes with every Rust release and invalidates the layer cache, and the OS also drifts from the runtime's bookworm). When updating Rust, raise the tag in the `Dockerfile`.
- Rust uses cargo-chef to separate "building dependency crates (`cargo chef cook`)" from "building the workspace", and unless `Cargo.toml` / `Cargo.lock` change, the dependency layer (about 1.7 GB) is reused from the CI cache. The production release profile is built with Thin LTO (`lto = "thin"` / `codegen-units = 16`).
- The build profile can be switched with `ARG CARGO_PROFILE` (default `release`). CI verification passes `ci-release` ([3](#3-ci-githubworkflowsciyml)).
- Workspace members (`crates/`, `sdk/`, `plugins/`) are needed in the context at the time of `cargo chef prepare` and `cargo build` (`COPY plugins/ ./plugins/`, etc.). `themes/` is also placed in the runtime.
- It runs as the non-root user `aegis` with UID 10001 and has a HEALTHCHECK. Data uses `/app/data` as a volume (e.g. `-v aegis-data:/app/data`).
- Production publishing (HTTPS with Caddy, multiple sites) is [`deploy/`](../deploy). `aegis-server deploy generate` (`crates/aegis-server/src/deploy.rs`) creates `compose.yaml`, `Caddyfile`, and a per-site `aegis.toml` from `deploy/sites.toml`. Locally, `task deploy:generate` can generate from `deploy/sites.toml` (check built-in themes and plugins with the repository's `themes/` and `plugins/`).
- The configuration is `config/docker.toml` (`host = "0.0.0.0"`, `port = 8080`). It can be overridden with `AEGIS_CONFIG`, `HOST`, and `PORT`. To use a different API target for Admin Studio, use `--build-arg VITE_AEGIS_SERVER_URL=...` ([configuration.md 4.3](configuration.md#43-admin-studios-connection-target)).

---

## 6. Performance Benchmarks

AegisCMS provides benchmark environments that verify both the load tolerance of the whole HTTP server and the overhead of Wasm plugins alone.

### 6.1 HTTP Endpoint Benchmark (`scripts/benchmark.mjs`)

It runs on Node.js 18+ standard features (no external dependencies) and measures the throughput (req/s) and latency (p50, p90, p99, max) of the main endpoints such as the top page, article details, the content list API, preview, and the e-commerce quote.

```bash
# 1. Start the server (in another terminal)
task dev
# Or start with an optimized release binary
cargo run --release -p aegis-server -- run

# 2. Run the benchmark (default: 50 concurrent, 5 seconds per endpoint)
task bench:http

# 3. Options (quick run, high load, specifying targets, etc.)
task bench:http:quick                    # 20 concurrent, 2 seconds
task bench:http:stress                   # 100 concurrent, 10 seconds
node scripts/benchmark.mjs -t index      # top page only
node scripts/benchmark.mjs -t api        # list API only
node scripts/benchmark.mjs --auto-server # start the server automatically, measure, then exit
node scripts/benchmark.mjs --json        # output the result as JSON

# Also measure the list API and preview (log in as a user of Viewer or above)
AEGIS_BENCH_PASSWORD=... node scripts/benchmark.mjs --user admin
node scripts/benchmark.mjs --token <session ID>   # the environment variable AEGIS_BENCH_TOKEN works too
```

- **The list API (`api`) and preview (`preview`) need a session of Viewer or above.** Without specifying `--user` / `--token`, these two are not measured (specifying them individually, such as `-t api`, is an error). This is because running without a session would measure 401 responses and the audit log writes (`access.denied`) that record the refusals, not the real processing. The same session is also used to find the slug used for the article page.
- **The cart quote (`quote`) needs `[commerce].enabled = true` and a published product.** If the shop is disabled or there are no products, it is not measured (if explicitly specified with `-t quote`, it shows the reason and exits). This is because running with the shop disabled (404) or no products (404) would measure error responses, not the real quote and plugin hook processing. The product is found automatically from the API or the public `/products`.
- To measure with many articles, first load data with `scripts/seed-benchmark.mjs` (`task bench:seed`). It creates 20 categories and 10,000 articles (about 10 blocks, about 4KB, with a relation to the category, 1 in 10 a draft) through the API, and also shows the loading speed (items/second). Running it twice with the same prefix (`--prefix`, default `bench`) fails because of duplicate slugs, so recreate the DB when measuring again.

    ```bash
    export AEGIS_BENCH_PASSWORD=...
    task bench:seed -- --user admin            # 10,000 articles (-n for the count, -c for concurrency)
    task bench:http -- --user admin
    ```

- Scenarios mixed with 4xx / 5xx responses show ⚠️ and a breakdown of responses (e.g. `HTTP 422 x 32106`). Those numbers are not the endpoint's real performance.

### 6.2 Profiling Wasm Plugins (`task bench:plugins`)

For Wasm plugins (modules in `sdk/` and `plugins/`), it measures the per-stage time per hook call (Store creation, instantiation, input writing, call, output reading) and the throughput under concurrency ([architecture.md 12](architecture.md#12-performance-records)).

```bash
task bench:plugins
```

How `on-render-page` changes with body size (1KB–1MB) is measured for the official plugins in `plugins/` with the following command. It shows the host's JSON conversion, writing to Wasm, call, reading, and the time of the whole `on_render_page` (`e2e`; without the result cache), and the time when the cache is used (`cached`) (`crates/aegis-runtime/examples/plugin_payload_profile.rs`; for how to read the results, see [architecture.md 10.7](architecture.md#107-lightening-the-input-of-on-render-page-aegis-runtimesrcpluginrs-wasminstancers)).

```bash
cargo run --release -p aegis-runtime --example plugin_payload_profile
```

To run both together, use `task bench:all`.

### 6.3 Per-Database Performance Comparison (`task bench:dbs` / `scripts/compare-dbs.mjs`)

Using the same release binary (LTO enabled, optimized), it measures and compares the load tolerance and throughput of the main endpoints across 3 database engines: the embedded **SQLite (WAL mode)**, **PostgreSQL 16** on Docker, and **MySQL 8.0**.

```bash
# Start the test DB containers beforehand (with health check waiting)
task docker:dbs:up

# Run the automatic comparison benchmark against the 3 DBs (build, migration, data loading, measurement, comparison report output) together
task bench:dbs
```

#### Measured Benchmark Results (Windows 11 / AMD Ryzen 7 / Docker Desktop, concurrency 50)

##### 1. Excluding audit log DB writes (recommended operating setup: `[audit] sinks = []` or `sinks = ["log"]`)

Pure web / DB performance when audit logs are not written synchronously to the DB but sent to standard output / log forwarding.

| Endpoint / workload                                                                                               | SQLite (WAL)                                   | PostgreSQL 16 (Docker)                         | MySQL 8.0 (Docker)                             | Characteristics and analysis                                                                                                                                                                                                   |
| :---------------------------------------------------------------------------------------------------------------- | :--------------------------------------------- | :--------------------------------------------- | :--------------------------------------------- | :----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Public top page** (`GET /`)<br>_(read-centric, cache, template rendering)_                                      | **1,178 req/s**<br>(p50: 41.0ms / p99: 63.1ms) | **510 req/s**<br>(p50: 94.8ms / p99: 174.0ms)  | **514 req/s**<br>(p50: 92.0ms / p99: 148.2ms)  | SQLite is about 2.3 times faster thanks to in-process, zero-copy access. For external RDBMSs, the overhead of TCP loopback and the connection pool (default 10) dominates.                                                     |
| **Unauthenticated access** (`GET /api/v1/contents/article`)<br>_(401 Unauthorized / no audit log DB write)_       | **1,744 req/s**<br>(p50: 28.1ms / p99: 43.4ms) | **2,025 req/s**<br>(p50: 24.1ms / p99: 37.4ms) | **2,047 req/s**<br>(p50: 23.8ms / p99: 32.2ms) | Excluding the audit log's synchronous DB INSERT, MySQL's 401 responses jump from 230 req/s to about 2,050 req/s (about 8.9 times), and all 3 engines line up at extremely fast HTTP rejection performance (1,700–2,050 req/s). |
| **Content list API** (`GET /api/v1/contents/article`)<br>_(authenticated 200 OK: DB query + relation resolution)_ | **1,590 req/s**<br>(p50: 30.6ms / p99: 46.1ms) | **887 req/s**<br>(p50: 53.2ms / p99: 85.9ms)   | **809 req/s**<br>(p50: 58.3ms / p99: 128.4ms)  | For a pure DB-reading API, MySQL and PostgreSQL both steadily deliver 800–890 req/s. SQLite is about 1.8–2.0 times faster at 1,590 req/s.                                                                                      |
| **Session login** (`POST /api/v1/auth/session`)<br>_(Argon2id verification + session issuance)_                   | **106 req/s**<br>(p50: 185.3ms / p99: 243.5ms) | **102 req/s**<br>(p50: 192.7ms / p99: 256.6ms) | **99 req/s**<br>(p50: 197.3ms / p99: 253.1ms)  | The computational cost of Argon2id (memory 64MB / 3 iterations) is the bottleneck, so all DB engines are even at about 100 req/s (p50 about 190ms).                                                                            |

##### 2. Reference: With Synchronous Audit Log INSERT Enabled (`[audit] sinks = ["database"]`)

Measurements for a setup where each unauthenticated access (401) triggers a synchronous INSERT into `_aegis_audit_logs`.

| Endpoint / workload                                             | SQLite (WAL)                                   | PostgreSQL 16 (Docker)                        | MySQL 8.0 (Docker)                             | Characteristics and analysis                                                                                                                                                                       |
| :-------------------------------------------------------------- | :--------------------------------------------- | :-------------------------------------------- | :--------------------------------------------- | :------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Unauthenticated access (401 + synchronous audit log INSERT)** | **2,014 req/s**<br>(p50: 16.4ms / p99: 89.8ms) | **992 req/s**<br>(p50: 45.2ms / p99: 138.7ms) | **230 req/s**<br>(p50: 178.7ms / p99: 675.1ms) | SQLite is fast thanks to `synchronous = NORMAL`. PostgreSQL's MVCC/WAL is also excellent. MySQL's throughput drops sharply because of the fsync of InnoDB transaction commits and pool exhaustion. |

#### Design Observations and Implications

1. **Read performance**: SQLite has overwhelming performance (1,100–1,500+ req/s) because it connects directly to in-process memory and local files. With external DBs, unless you tune the connection pool (such as raising `max_connections`), latency from waiting for connections increases, but even so, pure API reads steadily maintain over 800 req/s.
2. **Removing the bottleneck of the synchronous audit log INSERT**: with the `database` sink enabled, MySQL's 401 responses were limited to 230 req/s by the wait of synchronous transaction writes. But by excluding audit log DB writes and sending them to the recommended `log` sink (or excluding DB writes), MySQL also delivers its true high performance: about 1,965 req/s for 401 responses and about 858 req/s for the content list API. It was demonstrated again that for high-traffic operation you should configure audit logs not as `database` but as the `log` sink (file/stdout output + Fluentd/Logstash forwarding) ([audit.md 1.1](audit.md#11-choosing-a-destination)).

### 6.4 Forum Benchmark (`task bench:forum` / `scripts/benchmark-forum.mjs`)

Measures "how many requests the forum ([forum](forum.md)) can withstand". It starts the server with the dedicated settings `config/bench-forum.toml` (SQLite, no audit log, no rate limit on consecutive posts, no auto-lock, no operator notifications), loads 200 threads (one popular thread with 1,000 replies) through the API, and then measures each scenario while raising concurrency from 10 to 800. It treats **the highest req/s handled with p99 within the target (default 500ms) and no errors** as "withstood", and tabulates it at the end.

```bash
cargo build --release -p aegis-server   # build the release binary first
task bench:forum                        # default: concurrency 10,50,100,200,400,800 × 5 seconds each
task bench:forum -- --levels 50,200 -d 3 --slo-p99 200
```

#### Measured Results (Windows 11 / AMD Ryzen 7, SQLite (WAL), with the load-generating Node.js on the same machine)

| Scenario                                         |   Max req/s | p50 / p99 at that point |
| :----------------------------------------------- | ----------: | ----------------------: |
| Forum top (`GET /forum`)                         | about 1,900 |           5.1ms / 9.9ms |
| Thread list (`GET /forum/{slug}`)                | about 1,700 |          5.6ms / 11.3ms |
| Viewing a thread (a few replies)                 | about 3,300 |            57ms / 111ms |
| Popular thread (1,000 replies, random pages)     | about 1,100 |             43ms / 70ms |
| Posting a reply (anonymous, spread over threads) |   about 880 |           206ms / 380ms |
| Posting a reply (concentrated on one thread)     |   about 940 |            94ms / 196ms |
| 95% viewing + 5% posting                         |   about 900 |             11ms / 27ms |

- Viewing in every scenario has **no errors and throughput roughly flat** up to concurrency 800 (it does not fall). As concurrency increases the queue grows and only latency increases (p99 is 230–560ms at concurrency 400 and 670–1,200ms at 800), so the real limit is decided by how much p99 you allow.
- Writes are capped by SQLite's single writer, topping out at about 900–1,000 req/s (zero failures even at concurrency 800; only p99 exceeds 1 second).
- Thread and category pages read only what is displayed from the DB (`LIMIT / OFFSET`). The total (for computing the page count) is counted with `COUNT(*)`. With a design that reads from the start up to the page number and slices, later pages of threads with many replies get slower, and a 1,000-reply thread tops out at about 100 req/s.
- Numbers vary by about 10% between runs (because the load is generated on the same machine).

---

## 7. Real-Database Verification (PostgreSQL / MySQL)

To check the behavior of order and inventory transactions and schema migrations on real RDBMS environments other than SQLite (`sqlite::memory:`) (PostgreSQL 16 / MySQL 8.0), a dedicated Docker Compose setup ([`docker-compose.test-dbs.yml`](../docker-compose.test-dbs.yml)) and test suite are provided.

```bash
# 1. Start the test DB containers (with health check waiting)
task docker:dbs:up

# 2. Run the integration tests together (aegis-db standalone transaction verification + aegis-server E2E API verification)
task test:dbs

# 3. Discard the containers and volumes after the tests finish
task docker:dbs:down
```

- **Automatic skipping**: in ordinary development where the containers are not running (`task test` and `task check`), a TCP port probe (timeout 150ms) skips the real-database tests immediately and does not hurt overall test execution speed.
- **Not run in CI**: CI does not start PostgreSQL / MySQL, so all real-database tests are skipped. Verification on real databases is done in local quality checks (`task test` after `task docker:dbs:up`).
- **Tests use the shared DB serially**: the real-database tests recreate the same `aegis_test` tables, so if tests using the same DB run in parallel they break each other's tables. aegis-server's tests hold `test_support::POSTGRES_TEST_LOCK` / `MYSQL_TEST_LOCK` and run one at a time. When adding a test that uses a real DB, take the same lock.
- **Verification items**:
    - Initialization of the `_aegis_orders` table and indexes
    - Atomic inventory decrement at order creation (`stock >= quantity`)
    - Automatic transaction rollback on partial inventory shortage (if even one of multiple lines is short, everything is discarded)
    - Handling of unlimited-inventory (`stock IS NULL`) products
    - Inventory return (`restock`) on order cancellation
    - E2E behavior of the order confirmation page, the staff order list/detail API, and the status change API
    - Round trips of backup → restore across database kinds (`cross_db_roundtrip` in `aegis-backup`. It passes SQLite → PostgreSQL → MySQL → SQLite, and verifies that Japanese, emoji, long text, boolean and numeric custom fields, comments, tags, forum, and uploaded files come back with the same values. It also runs with only one of PostgreSQL and MySQL)
    - The inter-server event bus over Redis Pub/Sub (send and receive with two servers' connections, reconnection after dropping the subscription, and `Resync`. The target can be changed with `REDIS_URL`)

### 7.1 When Changing System Table Definitions

The definitions of system tables (`_aegis_*`) are changed with SQL files in `crates/aegis-db/migrations/` (for the policy, see [versioning.md 6](versioning.md#6-policy-for-updating-table-definitions)).

1. Add a new step with the same number and the same name (`NNNN_description.sql`; the next after the previous number) to each of `sqlite/`, `postgres/`, and `mysql/`. **Do not rewrite the file of a published step.**
2. Mind the dialect differences (types, the equivalent of `AUTOINCREMENT`, index syntax. MySQL has no `CREATE INDEX IF NOT EXISTS` and cannot roll back DDL partway).
3. Run `cargo test -p aegis-db` to verify the steps of the 3 dialects line up. Then **run them on real PostgreSQL and MySQL too, with `task docker:dbs:up` → `task test:dbs`** (CI does not start real databases, so this is the only place real-database verification can be done).
4. When an integration test initializes a real DB and recreates it with `init`, run `aegis_db::test_support::cleanup_postgres` / `cleanup_mysql` (dropping not only `_sqlx_migrations` but all migration-managed system tables at once prevents duplicate column errors when migrations 2 and later are rerun).

---

## 8. Admin Studio Tests and Storybook

### 8.1 Unit Tests (Vitest, `admin/src/**/*.test.ts`)

```bash
task test:admin                 # = npm --prefix admin run test:unit
npm --prefix admin run test:unit:watch # file watch mode
```

- **Fast verification of utilities and logic**: runs with Vitest linked to the Vite config, targeting `src/utils/` (the Markdown parser and serializer, currency formatter, permission checks, date formatting, SSO / authentication redirects, etc.) and the helpers and classes in `src/api/`.
- **Integration with CI / task check**: runs automatically in `task check` and in the GitHub Actions `frontend-ci` job.

### 8.2 E2E Tests (Playwright, `admin/e2e/`)

```bash
task test:e2e                      # = npm --prefix admin run test:e2e
npm --prefix admin run test:e2e -- --ui   # Playwright's UI mode
```

- **Run against a real server** (the API is not mocked). The `webServer` in `playwright.config.ts` starts `e2e/start-server.mjs`, which builds Admin Studio and `aegis-server`, clears `target/e2e/`, creates the admin user (`e2e/credentials.mjs`), and brings the server up on port 18080 with `e2e/aegis.e2e.toml`. This is because mismatches between the API's shape and the screens' types (which actually happened on the plugin screen) cannot be found with mocks.
- **A comprehensive test suite (17 tests in all)**:
    - `auth.spec.ts`: successful login, sign-out (session discard and redirect), the error display on a wrong password, and temporary account lockout by rate limiting (a 429 response after 3 consecutive failures).
    - `content.spec.ts`: creating a new article, saving a draft (Draft) and its reflection in the sidebar list, switching to published (Published), persistence verification on reload, and mutual conversion between Markdown mode and Blocks mode.
    - `themes.spec.ts`: displaying the installed theme list, switching the active theme (Aegis Horizon ↔ Nordic Minimal Editorial), and toast notification and reload persistence verification.
    - `users.spec.ts`: displaying the existing administrator, creating a new user (adding an editor role), and email invitation (issuing an invitation link with `InviteUserModal`).
    - `commerce.spec.ts`: creating a product (setting price and inventory) and the catalog list in an e-commerce-enabled environment (`[commerce].enabled = true`), and verifying the behavior of the order list and status filter screens.
    - `responsive.spec.ts`: opening and closing the hamburger menu and view transitions on a mobile viewport (375x667), and switching the editor screen's 3-pane (article list / edit / preview) tabs.
    - `plugins.spec.ts`: the plugin list and hook display, enabling and disabling without downtime, saving settings values, changing hook execution order, and returning to the config file.
- The first time takes several minutes for compiling the server. If already built, `E2E_SKIP_BUILD=1` skips it. Locally, an already running server (18080) is reused (`reuseExistingServer`). The server's log is `RUST_LOG=warn` (`RUST_LOG=debug` for all requests).
- Tests share the server's DB, so they run one at a time (`workers: 1`). Tests that changed state either restore it or leave a state that other tests do not depend on (making use of random suffixes, etc.).
- Plugins use the repository's `plugins/` (the built Wasm of the official plugins), starting with all of them disabled.
- Find screen elements by role and name (`getByRole`). Run in English (`locale: 'en-US'`). A plugin's card is an `article` named by the display name.

### 8.3 Storybook (`admin/.storybook/`, `src/**/*.stories.tsx`)

```bash
task storybook          # http://localhost:6006
task build:storybook    # admin/storybook-static/ (also built in CI)
```

- Screens that load their own data (such as `PluginManager`) are shown with `fetch` to `/api/v1/*` replaced by `mockApi` in `.storybook/mockApi.ts` (call it in the story's `beforeEach` and restore with the returned function). Make it the same response shape as the server.
- You can switch between English and Japanese with Language in the toolbar.
- **Component & interaction tests (`task test:storybook`)**: with `@storybook/addon-vitest` (Vitest Browser Mode + Playwright Chromium), all stories and `play` functions are verified automatically in a headless browser. It runs automatically in `task check` and in CI (`frontend-ci`).
- Versions are pinned (`--save-exact`). Choose ones published at least 2 weeks ago (except vulnerability fixes; 10.6.2 is the fix for GHSA-f2ph-cmg5-wxc8).

### 8.4 When Adding Dependencies (`package-lock.json`)

When you `npm install` with npm 11 on Windows, optional dependencies for other platforms (the `@emnapi/*` used by `@oxc-parser/binding-wasm32-wasi` and the like) may not be written to the lock file. Even if it works locally, `npm ci` in CI and Docker (Linux, npm 10 on Node 22) fails with `package.json and package-lock.json ... are in sync` / `Missing: @emnapi/core@... from lock file` (this happened in #74).

- After adding a dependency, check that `npm ci` passes with the same npm 10 as CI (local `npm ci --dry-run` uses npm 11's judgment and is not enough):

    ```bash
    # At the repository root. Check by placing only admin/'s package.json and package-lock.json in an empty directory
    docker run --rm -v "$PWD/admin:/src:ro" node:22.22-alpine sh -c \
      "mkdir /app && cp /src/package.json /src/package-lock.json /app/ && cd /app && npm ci --ignore-scripts && echo NPM_CI_OK"
    # In Git Bash on Windows paths get converted, so: MSYS_NO_PATHCONV=1 docker run ... -v "$(pwd -W)/admin:/src:ro" ...
    ```

- If it fails, use the `package-lock.json` produced by `npm install --ignore-scripts` in the same image (only the missing records are filled in). Compare with `main`'s `package-lock.json` and confirm that the versions of existing packages have not changed.
- Do not throw away `npm ci`'s output (with `>/dev/null` you miss the failure and verify with an old `node_modules`).

---

## 9. CLI Subcommand Reference (`aegis`)

The main subcommands available in the `aegis-server` binary (`cargo run -p aegis-server -- <command>` or the installed `aegis`).

```bash
# Overall help
aegis-server --help

# Start the server (default: port 8080)
aegis-server run [--port <PORT>] [--host <HOST>]

# Initial setup (runs migration, administrator creation, and loading of initial sample data together)
aegis-server init
# Non-interactive run (for CI / automated build environments)
aegis-server init --site-title "My Site" --admin-user admin --admin-password <PASSWORD> --non-interactive

# Run the database schema migration
aegis-server migrate

# Create a backup archive (DB + upload storage)
aegis-server dump --output backup.zip
# DB-only backup (excluding storage)
aegis-server dump --output db_only.zip --no-storage
# Storage-only backup (excluding the DB)
aegis-server dump --output storage_only.zip --no-database

# Pre-verify a backup's integrity and checksums (does not restore)
aegis-server restore --input backup.zip --verify-only

# Restore from a backup archive
aegis-server restore --input backup.zip
# Restore by overwriting when it conflicts with existing records
aegis-server restore --input backup.zip --overwrite

# Generate TypeScript type definitions / the OpenAPI 3.1 specification
aegis-server codegen --target typescript --output admin/src/types/schemas.ts
aegis-server codegen --target openapi --output docs/openapi.json

# User management
aegis-server user add <username> --role admin [--email <email>]
aegis-server user passwd <username>
aegis-server user list
aegis-server user role <username> <editor|admin|viewer>
aegis-server user disable <username>
aegis-server user enable <username>
aegis-server user link-oidc <username> <sub_claim>
aegis-server user link-saml <username> <name_id>

# Check the list of plugins and their operating status
aegis-server plugin list

# Test notification settings and delivery channels
aegis-server notify test --to <email> [--staff]
```

---

## 10. Troubleshooting & Development Tips

### 10.1 Dealing with Disk Space During Builds

When disk space gets tight in cloud development environments, CI containers, and the like (the incremental compilation artifacts of `target/debug` can swell to nearly 30GB), do the following:

```bash
# Build with incremental compilation disabled
CARGO_INCREMENTAL=0 cargo build

# Or delete the incremental cache that is no longer needed
rm -rf target/debug/incremental
```

### 10.2 Notes on the Storybook / Playwright Environment

- **Browser shell compatibility**: on Linux / cloud environments, the browser may fail to start if the Playwright version and the version of the headless shell in `/opt/pw-browsers` do not match. In that case, alias to an available version with a symbolic link or the like.
- **204 responses in Storybook mockApi**: when mocking `204 No Content` in `.storybook/mockApi.ts`, always return the response with no body (a 204 with a body makes the browser's `Response` object throw an exception).
