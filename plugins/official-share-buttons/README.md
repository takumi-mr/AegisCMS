# official-share-buttons

記事・固定ページに **SNS 共有ボタン**（X、LINE、URL コピー、Facebook、はてなブックマーク、Bluesky、Threads、Pocket、LinkedIn、およびカスタム共有先）を足す公式プラグインです（[`on-render-page` フック](../../docs/plugin_hooks.md)）。

- **標準対応プラットフォーム**: X (Twitter)、LINE、URL コピー、Facebook、はてなブックマーク、Bluesky、Threads、Pocket、LinkedIn
- **カスタム共有先**: `custom_services` 設定に URL テンプレートを指定することで、任意の Web サービス・SNS をボタンとして追加可能
- **表示スタイル**: `buttons`（アイコン＋文字）または `icons`（アイコンのみ）、形状（角丸 / カプセル / 四角）
- **配置**: 本文末尾（`bottom`、既定）、本文先頭（`top`）、両方（`both`）
- **プライバシー配慮**: 外部トラッキングスクリプトや iframe は一切読み込まず、クリーンなリンクとインライン SVG・クリップボード API のみで動作

## 使い方

設定ファイルの `[plugins].enabled` に `"official-share-buttons"` を足すか、Admin Studio の **Settings › Plugins** で有効にします。

```toml
[plugins]
enabled = ["official-share-buttons"]
```

## 設定

Admin Studio の **Settings › Plugins** の「設定」か、設定ファイルの `[plugins.settings.official-share-buttons]` でカスタマイズできます。

| キー | 既定値 | 説明 |
| --- | --- | --- |
| `content_types` | `article` | 共有ボタンを表示するコンテンツの種類（カンマ区切り） |
| `platforms` | `x,line,copy` | 表示する共有先（カンマ区切り、表示順）。`x`, `line`, `copy`, `facebook`, `hatena`, `bluesky`, `threads`, `pocket`, `linkedin` が利用可能 |
| `custom_services` | `""` | 追加のカスタム共有先（1行に1件、形式: `サービス名\|共有URLテンプレート\|背景色`）。使えるプレースホルダー: `{url}`, `{title}` |
| `position` | `bottom` | 配置位置: `bottom`（本文末尾）/ `top`（本文先頭）/ `both`（先頭と末尾の両方） |
| `style` | `buttons` | `buttons`（アイコン＋ラベル文字）/ `icons`（アイコンのみ） |
| `shape` | `rounded` | ボタンの角の丸み: `rounded`（角丸）/ `pill`（カプセル型）/ `square`（四角） |
| `heading` | `""` | ボタン上部の見出し文（空欄の場合は言語に応じた「この記事をシェアする」/「Share this article」） |
| `x_via` | `""` | X シェア時の `@ユーザー名`（via パラメータに付与） |
| `x_hashtags` | `""` | X シェア時のハッシュタグ（#なし、カンマ区切り） |

### 設定例（config.toml）

```toml
[plugins.settings.official-share-buttons]
platforms = "x,line,copy,hatena,bluesky"
position = "bottom"
style = "buttons"
shape = "pill"
custom_services = """
Mastodon|https://mastodon.social/share?text={title}&url={url}|#6364ff
Misskey|https://misskey-hub.net/share?text={title}&url={url}|#86b300
"""
```

### 記事ごとの非表示設定

特定の記事で共有ボタンを隠したい場合は、記事のカスタムフィールドに `hide_share_buttons: true` を設定します。

