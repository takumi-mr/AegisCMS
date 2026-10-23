# AegisCMS WebAssembly プラグイン 共通 ABI 仕様書 (ABI Specification)

本ドキュメントは、AegisCMS のゼロ・トラスト WebAssembly プラグインとホストランタイム (`aegis-runtime`) 間で取り交わされる低レイヤー ABI (Application Binary Interface)、メモリモデル、ホスト API、ライフサイクルフック、およびデータ交換規約を定義する言語非依存の標準仕様書です。

Rust のみならず、**AssemblyScript**、**TinyGo**、**C/C++**、**Zig** などの任意の WebAssembly 対応言語でプラグインや SDK を実装する際の準拠標準となります。

---

## 1. アーキテクチャと実行モデル

AegisCMS のプラグインシステムは、Bytecode Alliance 製 WebAssembly ランタイム `Wasmtime` をホストエンジンとして採用しています。

- **サンドボックス隔離**: プラグインはゲストの 32bit リニアメモリ空間内に完全に隔離され、ホストの物理メモリや OS リソースへの直接アクセスは不可能です。
- **インスタンス完全分離**: フック呼び出しごとに独立した `Store` および `Instance` が生成され、状態の汚染やプラグイン間メモリ共有を防ぎます（永続状態はホスト提供の KVS を使用）。
- **ゼロ・トラスト実行制御**:
  - **Fuel（CPU クォータ）**: 各フック呼び出しで使用可能な命令数を厳格に制限（無限ループや DoS 対策）。
  - **非同期タイムアウト**: 指定ミリ秒（例: 250ms）を超過したフック呼び出しはホストによって強制終了。
  - **ケイパビリティガード**: `plugin.manifest.json` で宣言されていないネットワークやストレージアクセスは、ホスト API 呼び出し時に即座に遮断。

---

## 2. メモリ管理 & パスバイバリュー規約

### 2.1 WebAssembly メモリエクスポート

プラグインモジュールは、自身のリニアメモリを `"memory"` という名前で必ずエクスポートする必要があります。

```wat
(export "memory" (memory 0))
```

### 2.2 メモリアロケータエクスポート (`allocate`)

ホストがプラグインへ文字列や JSON データを書き込む領域を確保するため、プラグインは以下のシグネチャを持つアロケーション関数を必ずエクスポートしなければなりません。

```text
allocate(len: i32) -> i32
```

- **引数**:
  - `len` (i32): 確保したいバイトサイズ。
- **戻り値**:
  - 確保されたゲストリニアメモリ上の先頭オフセットアドレス (`ptr`: i32)。
  - メモリ枯渇等で確保に失敗した場合は `0` を返します。

### 2.3 ホストからゲストへのデータ渡し手順

1. ホストはプラグインのエクスポート関数 `allocate(len)` を呼び出し、必要なバイト長 `len` 分のバッファをゲストメモリ内に確保させます。
2. ホストは返却された先頭アドレス `ptr` を起点として、UTF-8 エンコードされた JSON バイト列をゲストメモリに直接書き込みます。
3. ホストは対象のライフサイクルフック関数（例: `on_before_save(ptr, len)`）を呼び出します。
4. ゲスト側は `ptr` から `len` バイトを読み取り、JSON をデシリアライズします。使用後の入力バッファはゲスト側の責任でメモリ解放または GC に委ねます。

### 2.4 ゲストからホストへの結果返却 (Fat Pointer パッキング)

ライフサイクルフック関数は、変換後のデータや実行結果を単一の **64bit 符号付き整数 (`i64`)** にパックしてホストへ返却します。

#### パッキング規則
- **上位 32bit**: 結果データが格納されたゲストメモリの先頭ポインタ (`ptr` as i32)
- **下位 32bit**: 結果データのバイト長 (`len` as u32)

```text
packed_result (i64) = ((ptr as i64) << 32) | ((len as u32) as i64)
```

#### 各言語でのパッキング実装例
- **Rust**:
  ```rust
  ((ptr as i64) << 32) | ((len as u32) as i64)
  ```
- **AssemblyScript**:
  ```typescript
  ((ptr as i64) << 32) | ((len as i64) & 0xFFFFFFFF)
  ```
- **TinyGo / Go**:
  ```go
  (int64(ptr) << 32) | (int64(uint32(length)) & 0xFFFFFFFF)
  ```

#### ホスト側のアンパック処理
ホストは受け取った `i64` から `(packed >> 32) as i32` でポインタを、`packed as u32` でバイト長を復元し、ゲストメモリから結果バイト列を読み取ります。

---

## 3. ホスト API インポート仕様 (`host` モジュール)

プラグインは、ホスト環境から提供される機能を以下のインポートシグネチャを通じて利用できます。モジュール名はすべて `"host"` です。

**インポートできるのは以下の `host` モジュールの関数だけです。** それ以外（`env.abort`、`wasi_snapshot_preview1.*` など）をインポートするモジュールは読み込み時に拒否されます。

- AssemblyScript: 既定の `env.abort` を `asconfig.json` の `"use": ["abort=assembly/sdk/host/abort"]` で SDK の実装に差し替える。
- TinyGo: `-target=wasm-unknown` でビルドする（`wasi` ターゲットは WASI をインポートする）。

モジュールが `_initialize`（引数・戻り値なし）をエクスポートしている場合、ホストはインスタンス化の直後、他のエクスポートより先に呼び出します（WASI のリアクター形式の慣例。TinyGo の `wasm-unknown` はランタイムとグローバル変数をここで初期化する）。

### 3.1 `host.log`

ホストの構造化ログ機構（`tracing`）経由で安全にログを出力します。

```text
(import "host" "log" (func (param i32 i32 i32)))
```

- **引数**:
  - `level` (i32): ログレベル
    - `0`: DEBUG
    - `1`: INFO
    - `2`: WARN
    - `3` (またはその他): ERROR
  - `ptr` (i32): ログメッセージ文字列 (UTF-8) の先頭ポインタ
  - `len` (i32): ログメッセージ文字列のバイト長

### 3.2 `host.http_check_allowed`

アクセス先 URL がプラグインマニフェストの `capabilities.network.allowed_domains` で許可されているかをホスト側で検証します。

```text
(import "host" "http_check_allowed" (func (param i32 i32) (result i32)))
```

- **引数**:
  - `ptr` (i32): 検証対象 URL 文字列 (UTF-8) の先頭ポインタ
  - `len` (i32): URL 文字列のバイト長
- **戻り値** (i32):
  - `1`: 許可 (Allowed)
  - `0`: 拒否 (Denied / 不正な URL)

### 3.3 `host.kv_set`

プラグイン専用ネームスペースの Key-Value ストアへデータを永続化・更新します。マニフェストの `capabilities.storage.kv_namespace` で許可されている必要があります。

```text
(import "host" "kv_set" (func (param i32 i32 i32 i32) (result i32)))
```

- **引数**:
  - `k_ptr` (i32): キー文字列 (UTF-8) の先頭ポインタ
  - `k_len` (i32): キー文字列のバイト長
  - `v_ptr` (i32): 値文字列 (UTF-8) の先頭ポインタ
  - `v_len` (i32): 値文字列のバイト長
- **戻り値** (i32):
  - `0`: 成功 (Ok)
  - `1`: 権限不足 (Denied: マニフェストに storage 権限が未宣言)
  - `2`: エラー (Internal Error: メモリアクセス失敗やロック競合)
- 値はサーバーの `[cache]` バックエンドに保存されます（`redis` なら再起動後も残る）。

### 3.4 ゲストメモリへ結果を返すインポートの共通規約

`kv_get` / `http_fetch` / `config_get` は結果の長さが事前に分からないため、ホストがゲストの `allocate` エクスポートを呼んで領域を確保し、そこへ UTF-8 の結果を書き込んで `(ptr << 32) | len` の `i64` を返します。ゲストは読み終えたら領域を解放します（Rust SDK の `abi::read_bytes_from_host`）。`0` は「値なし」、負の値はエラーです。

### 3.5 `host.kv_get`

```text
(import "host" "kv_get" (func (param i32 i32) (result i64)))
```

- **引数**: キー文字列 (UTF-8) の `ptr`, `len`（ネームスペースはホストが付ける）
- **戻り値** (i64): 値の `(ptr << 32) | len`、未設定なら `0`、権限不足（storage 未宣言）は `-1`、エラーは `-2`

### 3.6 `host.http_fetch`

`allowed_domains` で許可された宛先へ HTTP リクエストを送ります。

```text
(import "host" "http_fetch" (func (param i32 i32) (result i64)))
```

- **引数**: リクエスト JSON の `ptr`, `len`
  ```json
  { "method": "POST", "url": "https://hooks.slack.com/services/…",
    "headers": { "Content-Type": "application/json" }, "body": "{\"text\":\"hi\"}" }
  ```
- **戻り値** (i64): 応答 JSON `{"status": 200, "headers": {"content-type": "…"}, "body": "…"}`、または失敗時 `{"error": "<理由>"}` の `(ptr << 32) | len`（書き込めなければ `0`）
- **制限**: メソッドは GET / POST / PUT / PATCH / DELETE、リダイレクトは追わない、本文は送受信とも 1 MiB、1 回 10 秒、1 回のフック呼び出しで 8 回まで。待ち時間はフックの `execution_timeout_ms` に含まれる

### 3.7 `host.config_get`

サーバーの `[plugins.settings.<プラグイン名>]` の値を読みます。

```text
(import "host" "config_get" (func (param i32 i32) (result i64)))
```

- **引数**: キー文字列の `ptr`, `len`
- **戻り値** (i64): 値（文字列はそのまま、それ以外は JSON）の `(ptr << 32) | len`、未設定なら `0`

---

## 4. ライフサイクルフック仕様 (ゲストエクスポート関数)

プラグインは、実行したいフックに対応する以下の関数をエクスポートします。マニフェストの `capabilities.hooks` に該当フック名（ケバブケース）が宣言されている場合のみホストから呼び出されます。

| マニフェストフック名 | Wasm エクスポート関数名 | 引数 | 戻り値 | 役割 |
|---|---|---|---|---|
| `on-before-save` | `on_before_save` | `(ptr: i32, len: i32)` | `i64` | 記事保存直前の検証・値補正・保存拒否 |
| `transform-content-ast` | `transform_content_ast` | `(ptr: i32, len: i32)` | `i64` | ブロック AST 配列の変換・装飾 |
| `on-render-page` | `on_render_page` | `(ptr: i32, len: i32)` | `i64` | 公開ページ描画コンテキストの動的変更 |
| `on-user-authenticated` | `on_user_authenticated` | `(ptr: i32, len: i32)` | `i64` | ログイン成功後のロールマッピング・属性の補完・ログイン拒否 |

### 4.1 `on_before_save`

記事がデータベースに保存される直前に呼び出されます。

- **入力**: UTF-8 JSON エンコードされた `PostContext`
- **戻り値**:
  - 成功時: 変更後の `PostContext` を UTF-8 JSON 化し、パックした `i64`
  - 保存拒否時: エラーエンベロープ `{"error": "<拒否理由メッセージ>"}` を UTF-8 JSON 化し、パックした `i64`
- **保存拒否の挙動**:
  - エラーエンベロープが返されると、ホストは記事の保存処理を即座に中断し、API は `422 Unprocessable Entity` を返します。

### 4.2 `transform_content_ast`

記事本文のブロック AST 配列を変換します。保存時および管理画面のリアルタイムプレビュー時に実行されます。

- **入力**: UTF-8 JSON エンコードされた `BlockNode[]` 配列
- **戻り値**: 変換後の `BlockNode[]` 配列を UTF-8 JSON 化し、パックした `i64`
- **注意事項**:
  - ブロック配列を変更しない場合、引数で渡された `(ptr, len)` をそのままパックして返却可能です。
  - プレビューのたびに実行されるため、外部副作用（HTTP リクエストや KVS 書き込み）を持たせないでください。

### 4.3 `on_render_page`

一般公開ページ (`GET /`, `GET /articles/{slug}`) またはプレビュー全体のレンダリング直前に呼び出されます。

- **入力**: UTF-8 JSON エンコードされた `PageRenderContext`
- **戻り値**: 変更後の `PageRenderContext` を UTF-8 JSON 化し、パックした `i64`
- **注意事項**:
  - サイトタイトル (`site_title`)、追加 CSS (`extra_css`)、追加ヘッダー (`extra_head`)、テンプレート変数 (`variables`) を動的に更新できます。

### 4.4 `on_user_authenticated`

ログイン（パスワード・OpenID Connect）で資格情報を確認した後、セッションを発行する前に呼び出されます。

- **入力**: UTF-8 JSON エンコードされた `UserAuthContext`（`provider`, `user_id`, `email`, `display_name`, `role`, `metadata`。詳細は [プラグインフック](plugin_hooks.md#23-userauthcontext)）
- **戻り値**:
  - 成功時: 変更後の `UserAuthContext` を UTF-8 JSON 化し、パックした `i64`（`role` / `display_name` / `email` / `metadata` がセッションに反映。`provider` / `user_id` の変更は無視）
  - ログイン拒否時: エラーエンベロープ `{"error": "<拒否理由メッセージ>"}`。API は `403 Forbidden` を返します
- **注意事項**:
  - `role` は `admin` / `editor` / `viewer` のいずれか。それ以外やトラップ・タイムアウトもログイン失敗になります（安全側に倒す）。

### 4.5 `calculate_order`

EC のカートの見積もり・注文の確定時に呼び出されます（[プラグインフック 1.5](plugin_hooks.md#15-calculate-order)）。

- **入力**: `OrderContext` の JSON（[プラグインフック 2.5](plugin_hooks.md#25-ordercontext--orderstatuschange)）
- **戻り値**: 変更後の `OrderContext`（反映されるのは `adjustments` と `metadata` のみ。`{"adjustments": [...]}` だけを返してもよい）、または注文を断るエラーエンベロープ `{"error": "<顧客に見せる理由>"}`

### 4.6 `on_order_status_changed`

注文の作成・状態の変化の後に呼び出されます。

- **入力**: `OrderStatusChange` の JSON
- **戻り値**: `0`（成功）、または `{"error": "..."}`（ログと監査ログに残るだけで、注文には影響しない）

---

## 5. データ構造仕様 (JSON Schemas)

全データ型は JSON 形式でシリアライズされ、リニアメモリ経由で受け渡されます。

### 5.1 `PostContext`
```json
{
  "id": "post_01hgbc3...",
  "title": "記事タイトル",
  "slug": "article-slug",
  "content_type": "article",
  "status": "draft",
  "author_id": "usr_01hgbc...",
  "blocks_json": "[{\"name\":\"core/paragraph\",\"attributes\":{},\"children\":[],\"text\":\"Hello\"}]",
  "created_at": 1729600000,
  "updated_at": 1729600000
}
```

### 5.2 `BlockNode`
```json
{
  "name": "core/paragraph",
  "attributes": {
    "level": 1
  },
  "children": [],
  "text": "テキスト内容"
}
```
- `attributes`: 属性のキー・バリューオブジェクト（未指定時は `{}` または `null`）。
- `children`: 入れ子の子 `BlockNode` 配列。
- `text`: ブロックのテキスト本文（未指定時は `""` または `null`）。

### 5.3 `PageRenderContext`
```json
{
  "site_title": "AegisCMS サイト",
  "site_description": "サイトの説明",
  "template": "article.html",
  "content_type": "article",
  "title": "記事タイトル",
  "slug": "hello-world",
  "content_html": "<p>本文</p>",
  "author": "admin",
  "published_at": "2026-10-22T00:00:00Z",
  "updated_at": null,
  "fields": {},
  "relations": {},
  "items": [],
  "extra_css": "body { background: #fafafa; }",
  "extra_head": "<meta name=\"author\" content=\"AegisCMS\">",
  "theme_vars": { "accent": "#10b981" },
  "pagination": {
    "page": 1, "per_page": 10, "total_items": 42, "total_pages": 5,
    "prev_url": null, "next_url": "/?page=2"
  }
}
```

### 5.3.1 オブジェクトを返すフックの部分更新

`on_before_save`・`on_render_page`・`on_user_authenticated` の返り値（JSON オブジェクト）は、**トップレベルのキー単位で入力に上書き**されます。変更したフィールドだけを返してもよく、返さなかったフィールドやプラグインが知らないフィールドは入力の値のまま保たれます（例: `{"extra_css": "..."}` だけを返す）。長さ 0 を返した場合は「変更なし」です。

### 5.4 エラーエンベロープ (Error Envelope)
```json
{
  "error": "タイトルは必須入力項目です"
}
```
フックが処理を拒否したい場合、この JSON をホストへ返却します。

---

## 6. プラグインマニフェスト仕様 (`plugin.manifest.json`)

```json
{
  "name": "sample-plugin",
  "version": "1.0.0",
  "entrypoint": "plugin.wasm",
  "capabilities": {
    "network": {
      "allowed_domains": ["api.example.com"]
    },
    "storage": {
      "kv_namespace": "sample_state"
    },
    "hooks": [
      "on-before-save",
      "transform-content-ast",
      "on-render-page"
    ]
  },
  "resource_limits": {
    "max_memory_mb": 16,
    "max_fuel_units": 100000000,
    "execution_timeout_ms": 250
  }
}
```

- **`capabilities.hooks`**: プラグインが受領するフックの一覧（ケバブケース）。ここに未列挙のフックは、Wasm 側に関数がエクスポートされていても呼び出されません。
- **`name`**: プラグインの識別子（英小文字・数字・`-`・`_`）。インストール先は `plugins/<name>/` で、`[plugins].enabled` に書くと有効になる。
- **`display_name` / `description` / `author` / `license` / `tags` / `homepage` / `repository` / `min_aegis_version`**: ストア・一覧表示用のメタデータ（任意）。
- **`resource_limits`**: メモリ上限（MB単位）、Fuel 上限、タイムアウト（ミリ秒単位）。

---

## 7. 言語別実装ガイドライン

| 項目 | Rust | AssemblyScript | TinyGo |
|---|---|---|---|
| **公式 SDK** | `crates/aegis-plugin-sdk` | `sdk/templates/assemblyscript/assembly/sdk` | `sdk/templates/tinygo/sdk` |
| **公式テンプレート** | `sdk/templates/rust` | `sdk/templates/assemblyscript` | `sdk/templates/tinygo` |
| **ビルドコマンド** | `cargo build --target wasm32-unknown-unknown --release` | `npm run asbuild` | `tinygo build -target=wasm-unknown -o plugin.wasm main.go` |
| **ホストインポート** | `#[link(wasm_import_module = "host")] extern "C"` | `@external("host", "...")` | `//go:wasmimport host ...` |
| **メモリアロケータ** | `std::alloc::alloc` | `heap.alloc` | `unsafe.Pointer(&buf[0])` / `malloc` |
| **JSON ハンドリング** | `serde_json` | 軽量 JSON ビルダー / パーサー | `encoding/json` |
