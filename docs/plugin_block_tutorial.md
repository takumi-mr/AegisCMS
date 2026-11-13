[← Back to the documentation index](README.md)

**English** | [日本語](plugin_block_tutorial.ja.md)

# Creating Your First Block Plugin (15 Minutes)

You will create a Rust plugin that adds a **custom block** that can be placed in the body, from the template, and take it as far as showing it in an article on a real server. Here we build a "note" (a callout that makes supplementary information or warnings stand out) as a plugin called `my-note`.

The mechanism first: the block's **definition** (name and input fields) is in `plugin.manifest.json`, and it is Wasm's `render_blocks` that **builds the HTML**. What is saved in the article is only the block name and attributes (`attributes`), and the plugin builds the HTML on each display ([Body blocks 4](blocks.md#4-plugin-blocks), [Hooks 1.10](plugin_hooks.md#110-render-blocks)).

| Step                                       | Time                   | What you use                   |
| ------------------------------------------ | ---------------------- | ------------------------------ |
| 1. Preparation and generating the scaffold | 1 min                  | Rust, `wasm32-unknown-unknown` |
| 2. Write the block                         | 5 min                  | An editor                      |
| 3. Test and build                          | 1 min the first time   | `cargo`                        |
| 4. Check with the dev server               | A few seconds to 1 min | `aegis-server plugin dev`      |
| 5. Place it in an article on a real server | 3 min                  | `aegis-server`, `curl`         |

> [!NOTE]
> The dev server is split into the standalone crate `aegis-plugin-dev`, so it starts quickly, in seconds to tens of seconds, even the first time.

---

## 1. Preparation and Copying the Template

```bash
rustup target add wasm32-unknown-unknown
```

At the repository root, run the `task plugin:new` command to generate a scaffold for a block. Renaming, relative path adjustment, and making it a standalone crate (`[workspace]`) are done automatically.

```bash
task plugin:new -- my-note --type block
```

(If you copy by hand, after `cp -r sdk/templates/rust-block plugins/my-note`, fix the name and paths in `Cargo.toml`, the `[workspace]` at the end, and the `name` in `plugin.manifest.json`.)

Touch up 3 places in the copy.

**`plugins/my-note/Cargo.toml`**

```toml
[package]
name = "my-note"                 # ← change the name (the artifact becomes my_note.wasm)
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
# As in the template it points one level too deep, so make it one level shallower
aegis-plugin-sdk = { path = "../../crates/aegis-plugin-sdk", version = "0.2.0" }

[workspace]                      # ← add an empty [workspace] at the end (reason below)
```

- **Why the empty `[workspace]` is needed**: the root `Cargo.toml` makes only the official plugins members of the workspace. `plugins/my-note/` is not a member, so without it `cargo` stops with `error: current package believes it's in a workspace when it's not`. When bringing it in as an official plugin, do not write this; add `"plugins/my-note"` to the root `[workspace] members` instead.
- If you place it outside the repository, make `aegis-plugin-sdk` `{ git = "https://github.com/takumi-mr/AegisCMS", tag = "v0.2.0" }` (`tag` is a version from [Releases](https://github.com/takumi-mr/AegisCMS/releases)).
- Do not write `serde_json` (the SDK re-exports it. In code, `aegis_plugin_sdk::serde_json`).

**`plugins/my-note/plugin.manifest.json`**: make `"name"` `"my-note"`. The other items work as they are.

---

## 2. Writing the Block

### 2.1 The Definition: `blocks` in `plugin.manifest.json`

The template has a block called `note`, whose input fields are the text (`text`) and the tone (`tone`).

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

- The name saved in the body is `<plugin name>/<name>`, that is, `my-note/note`.
- `capabilities.hooks` needs `"render-blocks"` (it is in the template).
- The input field `type` and the types of values that go in `attributes` are as in the table of [Plugin hooks 3.1](plugin_hooks.md#31-custom-blocks-blocks). **`select` arrives as a string even for numeric choices** (`"3"`).

### 2.2 Rendering: `src/lib.rs`

`render_blocks` is given this plugin's blocks **all together**. Return HTML in the same number and order as the input, and make blocks you do not render `None`.

```rust
fn render_note(block: &BlockNode) -> Option<String> {
    let text = attr(block, "text");          // a string with surrounding whitespace removed ("" if absent)
    if text.is_empty() {
        return None;                         // show nothing for a block with insufficient input
    }
    let tone = match attr(block, "tone") {
        "warning" => "warning",
        _ => "info",                         // unknown values fall back to the default
    };
    Some(format!(
        "<aside class=\"nb-note nb-note--{}\"><p>{}</p></aside>",
        tone,
        escape(text)                         // always escape user input
    ))
}
```

Rules to follow:

- **Always `escape` user input.** The HTML you return is sanitized before display, but that only removes things like `<script>` and `on*` attributes. Tags that break the layout remain.
- **Do not assume attribute types.** Blocks saved from the API or Markdown can hold arbitrary JSON. The template's `attr` treats anything that is not a string as empty.
- **Attach CSS only to the first block.** They are passed together, so you can output `<style>` just once. Making the specificity 0 with `:where()` lets the theme's CSS override it.
- To output an external `<iframe>`, declare it in the manifest's `capabilities.embed.frame_domains` ([Plugin hooks 3.2](plugin_hooks.md#32-embedding-external-pages-capabilitiesembed)).

---

## 3. Test and Build

### 3.1 Testing (Native Execution Without Wasm)

The SDK includes mocks, so `cargo test` alone lets you verify the logic. Build a block with `BlockNode::new(name)` and add attributes with `set_attribute`.

```bash
cd plugins/my-note
cargo test
```

Look at the template's tests and add yours in the same shape (putting `json!({...})` straight into `attributes` is a compile error. The SDK's `attributes` is a `HashMap`).

### 3.2 Build to Wasm and Place It as `plugin.wasm`

```bash
cargo build --target wasm32-unknown-unknown --release
cp target/wasm32-unknown-unknown/release/my_note.wasm plugin.wasm
```

`target/` is inside that folder for a standalone crate (one with an empty `[workspace]`), and at the repository root for a workspace member. `cargo metadata --format-version 1 --no-deps | grep -o '"target_directory":"[^"]*"'` shows the location.

---

## 4. Check with the Dev Server

Without a DB or login, you can check the hook's input and output, fuel (instruction count), and execution time ([Dev server](plugin_dev_server.md)).

```bash
# At the repository root (with task: task dev:plugin -- plugins/my-note)
cargo run -p aegis-server -- plugin dev plugins/my-note
```

In another terminal, call `render-blocks`. The input is `{"blocks": [...]}`.

```bash
curl -s -X POST http://127.0.0.1:8091/__dev/api/invoke \
  -H "x-aegis-dev: 1" -H "Content-Type: application/json" \
  -d '{"hook": "render-blocks",
       "input": {"blocks": [
         {"name": "my-note/note", "attributes": {"text": "5 < 6 & more", "tone": "warning"}},
         {"name": "my-note/note", "attributes": {"text": ""}}
       ]}}'
```

If `output.html` returns the first HTML (with the `<style>` attached) and `null` for the second, it succeeded. Also check that the `<` and `&` you put in `text` are escaped.

- **Saving the source rebuilds automatically.** In a folder with a Rust `Cargo.toml`, it runs `cargo build --target wasm32-unknown-unknown --release` and copies the result to `plugin.wasm` automatically (the copy in step 3.2 is needed only when not using the dev server). To change the command, use the manifest's `"dev": { "build_command": "..." }` or `--build-command` ([2.7](plugin_dev_server.md#27-build-trigger-api-post-__devapibuild)).
- The dev server's preview (`/__dev/api/preview`) is a **simple display that goes through neither sanitization nor the `aegis-plugin-block` wrapper**. Check the real output on a real server in the next step.

---

## 5. Place It in an Article on a Real Server

### 5.1 Prepare the Server

Prepare a working config file (the database is also created inside it) and enable the plugin.

```bash
mkdir -p /tmp/aegis-try && cp config/default.toml /tmp/aegis-try/config.toml
# Edit /tmp/aegis-try/config.toml:
#   [database] url = "sqlite:///tmp/aegis-try/aegis.db?mode=rwc"
#   [plugins]  enabled = ["my-note"]
export AEGIS_CONFIG=/tmp/aegis-try/config.toml
```

```bash
cargo run -p aegis-server -- plugin list     # shows my-note 0.1.0 enabled render-blocks
cargo run -p aegis-server -- init --non-interactive --admin-user admin \
  --admin-password 'change-me-please' --sample-data false
cargo run -p aegis-server -- run             # http://localhost:8080
```

> [!NOTE]
> For the `aegis-server` command, if already built, calling `target/debug/aegis-server` directly is the same (`cargo run -p aegis-server --` waits a bit for its checks). If an enabled plugin cannot be loaded, the server does not start (the cause appears in the startup log and `plugin list`).

### 5.2 Place the Block in an Article

Log in and create an article with `blocks` that have `name` and `attributes` (you do not write HTML).

```bash
TOKEN=$(curl -s -X POST http://localhost:8080/api/v1/auth/session \
  -H "Content-Type: application/json" \
  -d '{"username": "admin", "password": "change-me-please"}' | python3 -c 'import sys,json; print(json.load(sys.stdin)["id"])')

curl -s -X POST http://localhost:8080/api/v1/contents/article \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"title": "Note test", "slug": "note-test", "status": "published",
       "blocks": [
         {"name": "core/paragraph", "text": "This is the body."},
         {"name": "my-note/note", "attributes": {"text": "Careful here <b>", "tone": "warning"}}
       ]}' > /dev/null

curl -s http://localhost:8080/note-test | grep -o '<div class="cms-plugin-block".*</div>'
```

If the public page shows `<div class="cms-plugin-block" data-block="my-note/note"><style>…</style><aside class="nb-note nb-note--warning"><p>Careful here &lt;b&gt;</p></aside></div>`, you are done (the `<b>` in `text` is escaped).

> [!NOTE]
> The wrapper's class on public pages is `cms-plugin-block`. Identifiers starting with `aegis-` are replaced on public pages with `[server].namespace` (default `cms`) ([Configuration](configuration.md)). In the content API's `rendered_html` it remains `aegis-plugin-block`. Classes the plugin itself outputs, such as `nb-note`, appear as they are.

In Admin Studio (`http://localhost:8080/admin/`), "Note" lines up in the add-block button and can be edited in a form with the input fields you declared with `label` and `fields`. In Markdown it can be written on one line.

```markdown
::: my-note/note {"text": "Careful here", "tone": "warning"}
```

When you fix the plugin's code, just rebuild and replace `plugin.wasm`. **Without re-saving the article**, it changes from the next display (in Admin Studio's plugins screen, disabling and then enabling it makes it reload immediately).

---

## When You Get Stuck

| Symptom                                                                  | Cause and fix                                                                                                                                                                 |
| ------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `current package believes it's in a workspace when it's not`             | `plugins/<name>/` is not a member of the root workspace. Add an empty `[workspace]` at the end of `Cargo.toml` (step 1)                                                       |
| `failed to load manifest` / path not found                               | After copying, the `path` of `aegis-plugin-sdk` is still one level too deep. Make it `../../crates/aegis-plugin-sdk`                                                          |
| `mismatched types: expected HashMap<String, Value>, found Value` (tests) | The SDK's `BlockNode.attributes` is a `HashMap`. Use `BlockNode::new` and `set_attribute`                                                                                     |
| The display does not change after a rebuild (dev server)                 | The automatic build copies to `plugin.wasm` in a folder with `Cargo.toml`. If you use `--build-command` or `dev.build_command`, that command itself must update `plugin.wasm` |
| The server does not start                                                | An enabled plugin cannot be loaded (a `name` vs folder name mismatch, a faulty `blocks`, or a missing `entrypoint`). Look at the startup log and `plugin list`                |
| Nothing appears on the public page                                       | `render_blocks` returns `None` (insufficient input or a name mismatch), or the plugin is disabled. Check that it appears in `GET /api/v1/blocks`                              |
| The display is broken                                                    | User input is not `escape`d. Sanitization only removes things like `<script>`                                                                                                 |
| `all fuel consumed`                                                      | One rendering exceeded `resource_limits.max_fuel_units`. When there are many blocks, raise the limit or make the processing lighter                                           |

## What to Read Next

- [Plugin hooks `render-blocks` (1.10)](plugin_hooks.md#110-render-blocks), [the `blocks` declaration (3.1)](plugin_hooks.md#31-custom-blocks-blocks)
- A model: [`plugins/official-blocks`](../plugins/official-blocks) (buttons, link cards, video. A real example with external site embeds and input validation)
- [Plugin dev server](plugin_dev_server.md), [Wasm plugin development guide](plugin_sdk_guide.md)
