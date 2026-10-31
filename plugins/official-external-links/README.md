# official-external-links

本文の**外部リンク**の扱いをそろえる公式プラグインです（[`on-render-page` フック](../../docs/plugin_hooks.md)）。

| 付けるもの | 内容 |
| --- | --- |
| 新しいタブで開く | `target="_blank"`（すでに `target` があるリンクは変えない） |
| `rel` | 新しいタブのとき `noopener`、設定で `nofollow`。既存の `rel` に足す |
| アイコン | 文字のあるリンクの後ろに ↗（飾りなので読み上げない）。新しいタブのリンクには、読み上げ用の「新しいタブで開きます」を足す |

- 「外部」は、`http(s)` のリンクのうち、**このサイトのドメイン**と `internal_domains`（サブドメインを含む）以外です。相対パス・`#`・`mailto:`・`tel:` は対象外です。
- 本体の `[security]` の `enforce_external_link_rel`（`rel="noopener noreferrer"`）・`open_external_links_in_new_tab` は、サーバーが HTML に直接付けます。このプラグインはそれに足す形で動き、併用できます。
- **ブラウザーのスクリプトが付ける**ので、スクリプトを実行しない検索エンジンには `nofollow` は見えません。検索エンジンへの指定が必須のときは、本文のリンクに `rel` を直接書いてください。
- 本文に `<a ` が無いページには何も足しません。スクリプトが動かなくても、リンクは普通に開きます。

## 使い方

設定ファイルの `[plugins].enabled` に `"official-external-links"` を足すか、Admin Studio の **Settings › Plugins** で有効にします。

```toml
[plugins]
enabled = ["official-external-links"]
```

テーマが `extra_head` と `extra_css` を出力している必要があります（組み込みのテンプレートは出力します）。

## 設定

Admin Studio の **Settings › Plugins** の「設定」か、設定ファイルの `[plugins.settings.official-external-links]` で変えます。省略した項目・不正な値は既定値になります。

| キー | 既定値 | 説明 |
| --- | --- | --- |
| `content_types` | `article,page` | 動かすコンテンツの種類（カンマ区切り） |
| `selector` | `.prose` | 本文の範囲（CSS セレクター。カンマ区切りで複数）。この中のリンクだけを対象にする |
| `new_tab` | `true` | 新しいタブで開く |
| `nofollow` | `true` | `rel="nofollow"` を付ける |
| `icon` | `true` | 外部リンクのアイコンを出す |
| `internal_domains` | （なし） | 内部として扱うドメイン（カンマ区切り。サブドメインを含む。例: `example.com`） |
| `locale` | `en` | 読み上げ用の文の言語（`en` / `ja`） |

```toml
[plugins.settings.official-external-links]
internal_domains = "example.com, example.net"
nofollow = false
locale = "ja"
```

## 見た目を変える

アイコンは `a.oel-external::after`、読み上げ用の文は `.oel-sr` です。設定で足りないところは、テーマの CSS で上書きしてください。

## ビルド

```bash
task plugin:build:content   # plugin.wasm を作り直す（要 rustup target add wasm32-unknown-unknown）
cargo test -p aegis-plugin-external-links
```
