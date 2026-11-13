# official-link-embed

**English** | [日本語](README.ja.md)

An official plugin that, when a paragraph in the body is only a URL, turns it into a link card (an `official-link-embed/embed` block) built from that page's OGP / oEmbed. The data is fetched **inside the sandbox** at save time and stored in the block's attributes, so public pages make no network requests.

- For the specification, settings, and network restrictions, see [section 5 of docs/blocks.md](../../docs/blocks.md#5-ogp--oembed) (in Japanese).
- The fetch destinations are the manifest's defaults (`qiita.com`, `zenn.dev`, `github.com`, `note.com`, `x.com`, `twitter.com`, `publish.twitter.com`), plus anything added with `extra_allowed_domains` in `[plugins.settings.official-link-embed]`. It does not connect to internal IP addresses.
- Third-party HTML is not used (the oEmbed `html` is not read; only the page's body text is used as the description).

## Build

```bash
task plugin:build:link-embed   # rebuild plugin.wasm (requires rustup target add wasm32-unknown-unknown)
cargo test -p aegis-plugin-link-embed
cargo test -p aegis-runtime --test sdk_plugins link_embed   # run the built wasm on the host
```
