# AegisCMS プラグインフック & ホスト API リファレンス

Wasm プラグインから利用できるフック、マニフェスト項目、ホスト API の一覧です。
プラグイン開発の手順そのものは [Wasm プラグイン開発公式ガイド](./plugin_sdk_guide.md) を参照してください。

関連ソース:

- フックのディスパッチ: [`crates/aegis-runtime/src/plugin.rs`](../crates/aegis-runtime/src/plugin.rs) (`PluginSupervisor`)
- Wasm 呼び出し: [`crates/aegis-runtime/src/wasm/instance.rs`](../crates/aegis-runtime/src/wasm/instance.rs)
- ホスト API: [`crates/aegis-runtime/src/wasm/engine.rs`](../crates/aegis-runtime/src/wasm/engine.rs)
- 権限チェック: [`crates/aegis-runtime/src/guard.rs`](../crates/aegis-runtime/src/guard.rs)
- SDK: [`crates/aegis-plugin-sdk`](../crates/aegis-plugin-sdk)

---

## 1. フック一覧

| マニフェストのフック名 | Wasm エクスポート名 | SDK トレイトメソッド | 入力 | 出力 | 状態 |
| --- | --- | --- | --- | --- | --- |
| `transform-content-ast` | `transform_content_ast` | `Plugin::transform_content_ast` | `BlockNode[]` | `BlockNode[]` | ✅ 実装済み |
| `on-before-save` | `on_before_save` | `Plugin::on_before_save` | `PostContext` | `PostContext` | ✅ 実装済み（戻り値は保存に未反映、後述） |
| `on-user-authenticated` | — | — | — | — | 🔮 計画中（未実装） |

- フックを有効にするには、**マニフェストの `capabilities.hooks` にフック名（ケバブケース）を列挙**する必要があります。列挙されていないフックは、Wasm 側に関数がエクスポートされていても呼び出されません。
- 逆に、マニフェストに列挙されていても該当の関数がエクスポートされていない場合は、入力がそのまま次へ渡されます（エラーにはなりません）。
- SDK の `#[aegis_plugin]` マクロ（または `register_plugin!`）は両方のエクスポートを常に生成します。実装しなかったメソッドはトレイトの既定実装（入力をそのまま返す）になります。

### 1.1 実行タイミングと順序

`transform-content-ast` / `on-before-save` が呼ばれるのは **コンテンツ作成 API `POST /api/v1/contents/{content_type}`** と、`transform-content-ast` のみ **エディタのプレビュー API `POST /api/v1/preview`** です（一覧・取得 API や公開ページの表示では呼ばれず、保存済みのブロックを手順 4・5 だけでレンダリングします）。処理順は以下のとおりです。

```
リクエストの blocks
  │
  ├─ 1. 組み込み AST パイプライン (HeadingIdTransformer: 見出しに id を付与)
  ├─ 2. transform-content-ast  … 全プラグインを順に連鎖適用
  ├─ 3. on-before-save         … 全プラグインを順に連鎖適用（保存可否の判定）
  ├─ 4. HTML レンダリング (HtmlRenderer)
  ├─ 5. ストリーミング HTML リライター ([security] 設定)
  └─ 6. データベースへ保存
```

- プレビュー API は手順 1・2・4・5 を実行し（保存・`on-before-save` はなし）、保存して公開ページで表示した場合と同じ HTML を返します。テーマを適用したページ全体（`page_html`）のため、`on-render-page` フック（[テーマガイド](theme_and_publishing_guide.md)）もプレビューのたびに呼ばれます。
- 保存済みの記事を開いて再保存・プレビューすると、`transform-content-ast` は保存済み（変換済み）のブロックに再度適用されます。ブロックを追加するプラグインは、すでに追加済みなら追加しない（冪等な）実装にしてください。
- 複数プラグインがある場合、前のプラグインの出力が次のプラグインの入力になります。
- プラグインの実行順は `paths.plugins_dir` 配下のディレクトリ走査順（ファイルシステム依存）であり、**順序は保証されません**。順序に依存しない実装にしてください。

### 1.2 `transform-content-ast`

本文のブロック AST を変換します。見出しへのアンカー付与、ショートコード展開、ブロックの追加・削除などに使います。

```rust
fn transform_content_ast(&mut self, blocks: Vec<BlockNode>) -> Result<Vec<BlockNode>>;
```

- 入力: 組み込みパイプライン適用後のブロック配列（トップレベルのみの配列。子ブロックは `children` に入る）
- 出力: 変換後のブロック配列。**これがそのまま保存・HTML レンダリングされます。**
- エラー時の挙動: いずれかのプラグインでホスト側エラー（トラップ、タイムアウト、Fuel 枯渇、不正な JSON の返却など）が発生すると、**このフック全体の結果が破棄**され、手順 1 の結果のまま保存処理が続行されます（リクエストは失敗しません）。プレビューでは同じ場合に `warnings` として返し、エディタに表示します（監査ログには記録しません）。
- **副作用を持たせないでください**: エディタのプレビューは編集のたび（入力が 300ms 止まるごと）にこのフックを呼びます。KVS への書き込みや外部通知など、保存時に一度だけ行いたい処理は `on-before-save` に置いてください。

### 1.3 `on-before-save`

保存直前の記事情報を受け取ります。バリデーション、監査ログ、外部通知などに使います。

```rust
fn on_before_save(&mut self, post: PostContext) -> Result<PostContext>;
```

- 入力: `PostContext`（`blocks_json` は `transform-content-ast` 適用後の内容）
- 出力: `PostContext`。次のプラグインへの入力になります。
- エラー時の挙動: ホスト側エラーが発生すると保存は中止され、API は `400 Bad Request` と `{"error": "Plugin hook rejected save: ..."}` を返します。

> [!IMPORTANT]
> **現状の制限事項**
> - `on_before_save` が返した `PostContext` の変更（`title` の書き換えなど）は、**保存されるデータには反映されません**。保存されるのはリクエストの値と `transform-content-ast` の結果です。本文を変更したい場合は `transform-content-ast` を使ってください。
> - SDK でトレイトメソッドから `Err(...)` を返した場合、生成コードはエラーログを出したうえで**入力をそのまま返す**ため、保存は拒否されません。保存が中止されるのは、パニック（Wasm トラップ）、タイムアウト、Fuel 枯渇など、ホスト側でエラーとして検出される場合のみです。

### 1.4 `on-user-authenticated`（計画中）

認証成功後のユーザー属性同期、ロールマッピング、ログイン監査通知のためのフックとして計画されています（[TODO.md](../TODO.md) の「将来のロードマップ」参照）。認証判定そのものはプラグインには開放されません。

---

## 2. フックに渡されるデータ型

すべて JSON としてゲストのリニアメモリ経由で受け渡されます。

### 2.1 `PostContext`

| フィールド | 型 | 説明 |
| --- | --- | --- |
| `id` | string | 新規発行されたコンテンツ ID (UUID v4) |
| `title` | string | タイトル |
| `slug` | string | スラッグ |
| `author_id` | string | 著者 ID（未指定時は空文字列） |
| `blocks_json` | string | ブロック配列を JSON 文字列化したもの |

SDK ヘルパー:

| メソッド | 説明 |
| --- | --- |
| `PostContext::new(title, slug)` | テスト用コンストラクタ |
| `post.blocks()` | `blocks_json` を `Vec<BlockNode>` にパース |
| `post.set_blocks(&blocks)` | `Vec<BlockNode>` を `blocks_json` に書き戻す |

### 2.2 `BlockNode`

```json
{ "name": "core/heading", "attributes": { "level": 2 }, "text": "見出し", "children": [] }
```

| フィールド | 型 | 説明 |
| --- | --- | --- |
| `name` | string | ブロック種別（`core/heading`, `core/paragraph`, `core/code`, `core/image` など） |
| `attributes` | object | 任意の属性 |
| `text` | string \| null | テキスト内容 |
| `children` | `BlockNode[]` | 子ブロック |

SDK ヘルパー:

| メソッド | 説明 |
| --- | --- |
| `BlockNode::new(name)` | 空のブロックを作成 |
| `BlockNode::paragraph(text)` | `core/paragraph` |
| `BlockNode::heading(level, text)` | `core/heading`（`level` 属性付き） |
| `BlockNode::code(language, code)` | `core/code`（`language` 属性付き） |
| `block.attribute(key)` | 属性を取得 |
| `block.set_attribute(key, value)` | 属性を設定 |

### 2.3 `PluginError`（SDK）

| バリアント | 用途 |
| --- | --- |
| `PluginError::Execution(String)` | プラグインロジックのエラー |
| `PluginError::Serialization(String)` | JSON 変換エラー（`serde_json::Error` から自動変換） |
| `PluginError::Host(String)` | ホスト API 呼び出しのエラー |

---

## 3. マニフェスト (`plugin.manifest.json`)

各プラグインは `plugins_dir` 直下のサブディレクトリに配置し、`plugin.manifest.json` を置きます。

```json
{
  "name": "my-plugin",
  "version": "0.1.0",
  "entrypoint": "plugin.wasm",
  "capabilities": {
    "network": { "allowed_domains": ["hooks.slack.com"] },
    "storage": { "kv_namespace": "my_plugin_state" },
    "hooks": ["on-before-save", "transform-content-ast"]
  },
  "resource_limits": {
    "max_memory_mb": 16,
    "max_fuel_units": 100000000,
    "execution_timeout_ms": 250
  }
}
```

| キー | 型 | 必須 | 説明 |
| --- | --- | --- | --- |
| `name` | string | ✅ | プラグイン名。ログのタグ (`[Wasm:<name>]`) に使用 |
| `version` | string | ✅ | バージョン |
| `entrypoint` | string | ✅ | Wasm バイナリのパス（マニフェストからの相対） |
| `capabilities.network.allowed_domains` | string[] | | `net::is_allowed` で許可するドメイン。完全一致またはサブドメイン一致 |
| `capabilities.storage.kv_namespace` | string | | KVS のネームスペース。未宣言の場合 `kv::set` は権限エラー |
| `capabilities.hooks` | string[] | | 有効にするフック名（[1. フック一覧](#1-フック一覧)） |
| `resource_limits` | object | | 省略した項目（または全体）はサーバー設定 `[runtime].default_*` で補われる（下記） |

マニフェストのパースに失敗した場合や `entrypoint` のファイルが無い場合、そのプラグインは読み込まれずスキップされます（`debug` レベルでログ出力）。

## 4. リソース制限 (`resource_limits`)

| キー | 省略時の値 | 説明 |
| --- | --- | --- |
| `max_fuel_units` | `[runtime].default_max_fuel_units`（既定 `100000000`） | フック 1 回あたりの Wasm 命令実行量の上限。超過するとトラップ |
| `execution_timeout_ms` | `[runtime].default_timeout_ms`（既定 `250`） | フック 1 回あたりの実行時間の上限（最小 50ms に切り上げ） |
| `max_memory_mb` | `[runtime].default_max_memory_mb`（既定 `16`） | 線形メモリの上限（MiB、最小 1）。`memory.grow` で超えようとするとトラップし、初期メモリが上限を超えるモジュールはインスタンス化に失敗する。Rust の `wasm32-unknown-unknown` ビルドは初期メモリだけで約 1.1 MiB を使うため、2 以上を指定する |

- 項目ごとに補われるため、`{"max_memory_mb": 8}` のように一部だけ指定できます。
- 上限超過（トラップ）はプラグインのエラーとして扱われます（各フックのエラー時の挙動に従い、`plugin.error` を監査ログに記録）。

> [!NOTE]
> フックは呼び出しごとに新しい Wasm インスタンスで実行されます。プラグイン構造体も毎回 `Default::default()` で生成されるため、**フック呼び出し間でメモリ上の状態は保持されません**。

---

## 5. ホスト API

ホストは `host` モジュールとして以下の関数をインポート提供します。

| SDK API | Wasm インポート | シグネチャ | 必要な権限 |
| --- | --- | --- | --- |
| `log_debug!` / `log_info!` / `log_warn!` / `log_error!` | `host.log` | `(level: i32, ptr: i32, len: i32)` | なし |
| `net::is_allowed(url) -> bool` | `host.http_check_allowed` | `(ptr: i32, len: i32) -> i32` | `capabilities.network` |
| `kv::set(key, value) -> Result<()>` | `host.kv_set` | `(k_ptr, k_len, v_ptr, v_len) -> i32` | `capabilities.storage` |
| `kv::set_json(key, &value) -> Result<()>` | `host.kv_set` | 同上（値を JSON 文字列化） | `capabilities.storage` |

### 5.1 ログ (`host.log`)

| `level` | SDK マクロ | ホスト側 `tracing` レベル |
| --- | --- | --- |
| `0` | `log_debug!` | `debug` |
| `1` | `log_info!` | `info` |
| `2` | `log_warn!` | `warn` |
| `3` 以上 | `log_error!` | `error` |

### 5.2 ネットワーク許可チェック (`host.http_check_allowed`)

- `http://` または `https://` で始まる URL のホスト名が `allowed_domains` と完全一致、またはそのサブドメインであれば `1`、それ以外は `0` を返します（比較は大文字・小文字を区別）。
- `capabilities.network` が未宣言の場合は常に `0` です。
- **このAPIは許可判定のみを行います。** 現時点ではプラグインから実際に HTTP リクエストを送信するホスト API は提供されていません。

### 5.3 KVS 書き込み (`host.kv_set`)

| 戻り値 | 意味 | SDK の結果 |
| --- | --- | --- |
| `0` | 成功 | `Ok(())` |
| `1` | 権限なし（`capabilities.storage` 未宣言） | `Err(PluginError::Host(...))` |
| `2` | メモリ読み取り失敗などのエラー | `Err(PluginError::Host(...))` |

- キーは `{kv_namespace}:{key}` の形でホスト側に保存され、他プラグインのネームスペースとは分離されます。
- 現状のストアは**サーバープロセス内のメモリ**であり、再起動すると消えます。また**読み取り API (`kv_get`) は未提供**です。

### 5.4 ネイティブテスト時のモック

`cargo test` などネイティブターゲット (`not(target_arch = "wasm32")`) でビルドした場合、SDK のホスト API はモックに置き換わります。

| API | モックの挙動 |
| --- | --- |
| ログ | 標準出力に `[WasmMock:LEVEL] message` を出力 |
| `net::is_allowed` | 常に `true`（マニフェストは参照しない） |
| `kv::set` / `kv::set_json` | 何も保存せず常に `Ok(())` |

権限まわりの挙動は、Wasm にビルドしてランタイム上で確認してください。

---

## 6. 低水準 ABI（SDK を使わない場合）

| エクスポート | シグネチャ | 必須 | 説明 |
| --- | --- | --- | --- |
| `memory` | memory | ✅ | リニアメモリ |
| `allocate` | `(len: i32) -> i32` | ✅ | ホストが入力 JSON を書き込む領域を確保し、ポインタを返す |
| `transform_content_ast` | `(ptr: i32, len: i32) -> i64` | | 入力 `BlockNode[]` JSON、戻り値 `(ptr << 32) \| len` |
| `on_before_save` | `(ptr: i32, len: i32) -> i64` | | 入力 `PostContext` JSON、戻り値 `(ptr << 32) \| len` |

- 戻り値の `len` が `0` の場合、ホストは入力をそのまま採用します。
- 返却された JSON が期待する型としてパースできない場合はホスト側エラー（[1.2](#12-transform-content-ast) / [1.3](#13-on-before-save) のエラー時の挙動を参照）になります。
