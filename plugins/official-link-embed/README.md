# official-link-embed

本文の段落が URL だけのとき、そのページの OGP / oEmbed から作ったリンクカード（`official-link-embed/embed` ブロック）にする公式プラグインです。保存のときに**サンドボックスの中**で取得してブロックの属性に保存するので、公開ページは通信しません。

- 仕様・設定・通信の制限は [docs/blocks.md の 5](../../docs/blocks.md#5-ogp--oembed)。
- 取得先は、マニフェストの既定（`qiita.com`・`zenn.dev`・`github.com`・`note.com`・`x.com`・`twitter.com`・`publish.twitter.com`）に、`[plugins.settings.official-link-embed]` の `extra_allowed_domains` で足します。内部の IP アドレスへは通信しません。
- 第三者の HTML は使いません（oEmbed の `html` は読まず、本文のテキストだけを説明にします）。

## ビルド

```bash
task plugin:build:link-embed   # plugin.wasm を作り直す（要 rustup target add wasm32-unknown-unknown）
cargo test -p aegis-plugin-link-embed
cargo test -p aegis-runtime --test sdk_plugins link_embed   # ビルド済みの wasm をホストの上で動かす
```
