[← ドキュメント一覧に戻る](README.md)

# 最初のブロックプラグインを作る（15 分）

本文に置ける**カスタムブロック**を足す Rust のプラグインを、テンプレートから作って、実サーバーの記事に表示するところまで進めます。ここでは「ノート」（補足や注意を目立たせる囲み）を `my-note` というプラグインとして作ります。

仕組みだけ先に: ブロックの**定義**（名前と入力欄）は `plugin.manifest.json`、**HTML** を作るのは Wasm の `render_blocks` です。記事に保存されるのはブロック名と属性（`attributes`）だけで、HTML は表示のたびにプラグインが作ります（[本文のブロック 4](blocks.md#4-プラグインのブロック)・[フック 1.10](plugin_hooks.md#110-render-blocks)）。

| 手順 | 所要 | 使うもの |
| --- | --- | --- |
| 1. 準備と、テンプレートのコピー | 2 分 | Rust・`wasm32-unknown-unknown` |
| 2. ブロックを書く | 5 分 | エディター |
| 3. テストして、ビルドする | 初回 1 分 | `cargo` |
| 4. 開発サーバーで確かめる | 初回 5〜7 分 | `aegis-server plugin dev` |
| 5. 実サーバーの記事に置く | 3 分 | `aegis-server`・`curl` |

> [!NOTE]
> 手順 4 の初回は、`aegis-server` 全体（wasmtime・sqlx・axum など）のビルドで 5〜7 分かかります。手順 3 まで進めてから、ビルドを走らせたまま手順 2 を直す、という進め方もできます。

---

## 1. 準備と、テンプレートのコピー

```bash
rustup target add wasm32-unknown-unknown
```

リポジトリのルートで、ブロック用のテンプレート [`sdk/templates/rust-block`](../sdk/templates/rust-block) を `plugins/my-note/` にコピーします。**フォルダー名はマニフェストの `name` と同じにします。**

```bash
cp -r sdk/templates/rust-block plugins/my-note
rm -rf plugins/my-note/plugin.wasm
```

コピーしたものに 3 か所、手を入れます。

**`plugins/my-note/Cargo.toml`**

```toml
[package]
name = "my-note"                 # ← 名前を変える（成果物は my_note.wasm になる）
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
# テンプレートのままだと 1 つ深い場所を指しているので、1 段浅くする
aegis-plugin-sdk = { path = "../../crates/aegis-plugin-sdk", version = "0.2.0" }

[workspace]                      # ← 末尾に空の [workspace] を足す（理由は下）
```

- **空の `[workspace]` が要る理由**: ルートの `Cargo.toml` は公式のプラグインだけをワークスペースのメンバーにしています。`plugins/my-note/` はメンバーではないので、書かないと `error: current package believes it's in a workspace when it's not` で `cargo` が止まります。公式プラグインとして取り込むときは、これを書かずに、ルートの `[workspace] members` に `"plugins/my-note"` を足します。
- リポジトリの外に置くなら、`aegis-plugin-sdk` は `{ git = "https://github.com/takumi-mr/AegisCMS", tag = "v0.2.0" }` にします（`tag` は [Releases](https://github.com/takumi-mr/AegisCMS/releases) の版）。
- `serde_json` は書きません（SDK が再エクスポートしています。コードでは `aegis_plugin_sdk::serde_json`）。

**`plugins/my-note/plugin.manifest.json`**: `"name"` を `"my-note"` にします。ほかの項目はそのままで動きます。

---

## 2. ブロックを書く

### 2.1 定義: `plugin.manifest.json` の `blocks`

テンプレートには `note` というブロックがあり、入力欄は本文（`text`）と種類（`tone`）です。

```json
"blocks": [
  {
    "name": "note",
    "label": { "en": "Note box", "ja": "ノート" },
    "fields": [
      { "name": "text", "type": "text", "label": { "en": "Text", "ja": "本文" }, "required": true },
      { "name": "tone", "type": "select", "label": { "en": "Tone", "ja": "種類" }, "default": "info",
        "options": [
          { "value": "info", "label": { "en": "Info", "ja": "補足" } },
          { "value": "warning", "label": { "en": "Warning", "ja": "注意" } }
        ] }
    ]
  }
]
```

- 本文に保存される名前は `<プラグイン名>/<name>`、つまり `my-note/note` です。
- `capabilities.hooks` に `"render-blocks"` が必要です（テンプレートに入っています）。
- 入力欄の `type` と、`attributes` に入る値の型は [プラグインフック 3.1](plugin_hooks.md#31-カスタムブロック-blocks) の表のとおりです。**`select` は数字の選択肢でも文字列で届きます**（`"3"`）。

### 2.2 描画: `src/lib.rs`

`render_blocks` は、このプラグインのブロックが**まとめて**渡されます。入力と同じ数・同じ順の HTML を返し、描かないブロックは `None` にします。

```rust
fn render_note(block: &BlockNode) -> Option<String> {
    let text = attr(block, "text");          // 前後の空白を除いた文字列（無ければ ""）
    if text.is_empty() {
        return None;                         // 入力が足りないブロックは何も表示しない
    }
    let tone = match attr(block, "tone") {
        "warning" => "warning",
        _ => "info",                         // 知らない値は既定にする
    };
    Some(format!(
        "<aside class=\"nb-note nb-note--{}\"><p>{}</p></aside>",
        tone,
        escape(text)                         // 利用者の入力は必ずエスケープする
    ))
}
```

守ること:

- **利用者の入力は必ず `escape` する。** 返した HTML は表示前にサニタイズされますが、それは `<script>` や `on*` 属性などを取り除くだけです。レイアウトを壊すタグは残ります。
- **属性の型を決めつけない。** API や Markdown から保存されたブロックは任意の JSON を持てます。テンプレートの `attr` は、文字列でなければ空として扱います。
- **CSS は最初のブロックにだけ付ける。** まとめて渡されるので、`<style>` を 1 回だけ出せます。`:where()` で詳細度を 0 にすると、テーマの CSS で上書きできます。
- 外部の `<iframe>` を出すなら、マニフェストの `capabilities.embed.frame_domains` で宣言します（[プラグインフック 3.2](plugin_hooks.md#32-外部のページの埋め込み-capabilitiesembed)）。

---

## 3. テストして、ビルドする

### 3.1 テスト（Wasm なしでネイティブ実行）

SDK にモックが入っているので、`cargo test` だけでロジックを確かめられます。ブロックは `BlockNode::new(名前)` に `set_attribute` で属性を足して作ります。

```bash
cd plugins/my-note
cargo test
```

テンプレートのテストを見て、同じ形で足してください（`json!({...})` をそのまま `attributes` に入れるとコンパイルエラーになります。SDK の `attributes` は `HashMap` です）。

### 3.2 Wasm にビルドして、`plugin.wasm` に置く

```bash
cargo build --target wasm32-unknown-unknown --release
cp target/wasm32-unknown-unknown/release/my_note.wasm plugin.wasm
```

`target/` は、独立した crate（空の `[workspace]` を書いたもの）ではそのフォルダーの中、ワークスペースのメンバーならリポジトリのルートにできます。`cargo metadata --format-version 1 --no-deps | grep -o '"target_directory":"[^"]*"'` で場所が分かります。

---

## 4. 開発サーバーで確かめる

DB やログインなしで、フックの入出力・Fuel（命令数）・実行時間を確かめられます（[開発サーバー](plugin_dev_server.md)）。

```bash
# リポジトリのルートで（task があれば: task dev:plugin -- plugins/my-note）
cargo run -p aegis-server -- plugin dev plugins/my-note
```

別のターミナルで、`render-blocks` を呼びます。入力は `{"blocks": [...]}` です。

```bash
curl -s -X POST http://127.0.0.1:8091/__dev/api/invoke \
  -H "x-aegis-dev: 1" -H "Content-Type: application/json" \
  -d '{"hook": "render-blocks",
       "input": {"blocks": [
         {"name": "my-note/note", "attributes": {"text": "5 < 6 & more", "tone": "warning"}},
         {"name": "my-note/note", "attributes": {"text": ""}}
       ]}}'
```

`output.html` に、1 つ目の HTML（`<style>` が付く）と、2 つ目の `null` が返れば成功です。`text` に入れた `<` や `&` がエスケープされていることも確かめてください。

- **ソースを保存すると自動で再ビルド**されます。Rust の `Cargo.toml` があるフォルダーでは、`cargo build --target wasm32-unknown-unknown --release` を走らせて、`plugin.wasm` に自動でコピーします（手順 3.2 のコピーは、開発サーバーを使わないときだけ要ります）。コマンドを変えたいときは、マニフェストの `"dev": { "build_command": "..." }` か `--build-command` です（[2.7](plugin_dev_server.md#27-ビルドトリガー-api-post-__devapibuild)）。
- 開発サーバーのプレビュー（`/__dev/api/preview`）は、**サニタイズも `aegis-plugin-block` の包みも通らない簡易表示**です。本当の出力は、次の手順の実サーバーで確かめます。

---

## 5. 実サーバーの記事に置く

### 5.1 サーバーを用意する

作業用の設定ファイル（データベースもその中に作る）を用意して、プラグインを有効にします。

```bash
mkdir -p /tmp/aegis-try && cp config/default.toml /tmp/aegis-try/config.toml
# /tmp/aegis-try/config.toml を編集:
#   [database] url = "sqlite:///tmp/aegis-try/aegis.db?mode=rwc"
#   [plugins]  enabled = ["my-note"]
export AEGIS_CONFIG=/tmp/aegis-try/config.toml
```

```bash
cargo run -p aegis-server -- plugin list     # my-note 0.1.0 enabled render-blocks と出る
cargo run -p aegis-server -- init --non-interactive --admin-user admin \
  --admin-password 'change-me-please' --sample-data false
cargo run -p aegis-server -- run             # http://localhost:8080
```

> [!NOTE]
> コマンドの `aegis-server` は、ビルド済みなら `target/debug/aegis-server` を直接呼んでも同じです（`cargo run -p aegis-server --` は、その分の確認で少し待ちます）。有効にしたプラグインが読み込めないと、サーバーは起動しません（原因は起動ログと `plugin list` に出ます）。

### 5.2 記事にブロックを置く

ログインして、`blocks` に `name` と `attributes` を書いた記事を作ります（HTML は書きません）。

```bash
TOKEN=$(curl -s -X POST http://localhost:8080/api/v1/auth/session \
  -H "Content-Type: application/json" \
  -d '{"username": "admin", "password": "change-me-please"}' | python3 -c 'import sys,json; print(json.load(sys.stdin)["id"])')

curl -s -X POST http://localhost:8080/api/v1/contents/article \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"title": "ノートのテスト", "slug": "note-test", "status": "published",
       "blocks": [
         {"name": "core/paragraph", "text": "本文です。"},
         {"name": "my-note/note", "attributes": {"text": "ここに注意 <b>", "tone": "warning"}}
       ]}' > /dev/null

curl -s http://localhost:8080/note-test | grep -o '<div class="cms-plugin-block".*</div>'
```

公開ページに `<div class="cms-plugin-block" data-block="my-note/note"><style>…</style><aside class="nb-note nb-note--warning"><p>ここに注意 &lt;b&gt;</p></aside></div>` が出れば完成です（`text` の `<b>` はエスケープされています）。

> [!NOTE]
> 包みのクラスは、公開ページでは `cms-plugin-block` です。`aegis-` で始まる識別子は、公開ページでは `[server].namespace`（既定 `cms`）に置き換わります（[設定](configuration.md)）。コンテンツ API の `rendered_html` では `aegis-plugin-block` のままです。プラグイン自身が出す `nb-note` のようなクラスは、そのまま出ます。

Admin Studio（`http://localhost:8080/admin/`）では、ブロックの追加ボタンに「ノート」が並び、`label` と `fields` で宣言した入力欄のフォームで編集できます。Markdown では 1 行で書けます。

```markdown
::: my-note/note {"text": "ここに注意", "tone": "warning"}
```

プラグインのコードを直したら、`plugin.wasm` を作り直して置き換えるだけです。**記事を保存し直さなくても**、次の表示から変わります（Admin Studio のプラグイン画面で、いったん無効 → 有効にすると、すぐに読み直します）。

---

## つまずいたとき

| 症状 | 原因と直し方 |
| --- | --- |
| `current package believes it's in a workspace when it's not` | `plugins/<name>/` がルートのワークスペースのメンバーではない。`Cargo.toml` の末尾に空の `[workspace]` を足す（手順 1） |
| `failed to load manifest` / パスが見つからない | コピーしたあと、`aegis-plugin-sdk` の `path` が 1 段深いまま。`../../crates/aegis-plugin-sdk` にする |
| `mismatched types: expected HashMap<String, Value>, found Value`（テスト） | SDK の `BlockNode.attributes` は `HashMap`。`BlockNode::new` と `set_attribute` を使う |
| 再ビルドしても表示が変わらない（開発サーバー） | 自動ビルドは `Cargo.toml` のあるフォルダーで `plugin.wasm` へ自動コピーする。`--build-command` や `dev.build_command` を使っているなら、そのコマンド自身が `plugin.wasm` を更新すること |
| サーバーが起動しない | 有効にしたプラグインが読み込めない（`name` とフォルダー名の違い、`blocks` の不備、`entrypoint` が無い）。起動ログと `plugin list` を見る |
| 公開ページに何も出ない | `render_blocks` が `None` を返している（入力不足・名前の不一致）。または、プラグインが無効。`GET /api/v1/blocks` に出ているか確かめる |
| 表示が崩れる | 利用者の入力を `escape` していない。サニタイズは `<script>` などを取り除くだけ |
| `all fuel consumed` | 1 回の描画の命令数が `resource_limits.max_fuel_units` を超えた。ブロックの数が多いときは上限を上げるか、処理を軽くする |

## 次に読むもの

- [プラグインフック `render-blocks`（1.10）](plugin_hooks.md#110-render-blocks)・[`blocks` の宣言（3.1）](plugin_hooks.md#31-カスタムブロック-blocks)
- お手本: [`plugins/official-blocks`](../plugins/official-blocks)（ボタン・リンクカード・動画。外部サイトの埋め込みと、入力の検証が入った実例）
- [プラグイン開発サーバー](plugin_dev_server.md)・[Wasm プラグイン開発ガイド](plugin_sdk_guide.md)
