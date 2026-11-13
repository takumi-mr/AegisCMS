# official-ads

[English](README.md) | **日本語**

AegisCMS 公式の広告配信プラグインです。
Google AdSense および独自バナー／アフィリエイト広告の配信に対応し、`<head>` へのスクリプトタグ注入、カスタムブロック（`official-ads/display`）による本文への広告配置、本文先頭・末尾への自動挿入、動的 CSP (Content Security Policy) 連携、そして Cookie 同意バナー（`[consent]`）との連携を提供します。

---

## 主な機能

1. **Google AdSense 連携**:
    - パブリッシャー ID（`client_id`）を設定するだけで、`on-render-page` フックにより `<head>` に広告スクリプトを安全に自動注入。
    - レスポンシブ、レクタングル、横長、縦長など各種フォーマットに対応。
2. **カスタムブロック `official-ads/display`**:
    - 記事や固定ページの本文中に、エディタから自由に広告ユニットを配置可能。
    - スロット ID ごとの個別設定や、任意のアフィリエイトタグ・カスタムバナー HTML の表示にも対応。
3. **本文への自動挿入**:
    - 手動でブロックを配置しなくても、設定（`auto_insert_position`）により記事本文の先頭、末尾、または両方に自動で広告ユニットを挿入可能。
    - 特定の記事で広告を非表示にしたい場合は、記事のカスタムフィールドに `hide_ads: true` を設定することで除外できます。
4. **動的 CSP 設定との連携**:
    - プラグインが有効化されると、Google AdSense や広告配信に必要な外部ドメイン（`pagead2.googlesyndication.com`, `googleads.g.doubleclick.net`, `tpc.googlesyndication.com`, `ep2.adtrafficquality.google`）がサーバーの CSP（`script-src`, `frame-src`, `connect-src`）に動的に自動マージされます。
5. **Cookie 同意バナー（`[consent]`）完全対応**:
    - `require_consent = true`（既定）の場合、訪問者が Cookie 同意バナーで「マーケティング」区分に同意するまで広告スクリプトのロード・実行を自動的に保留します（`data-aegis-consent="marketing"`）。
    - 同意ボタンが押されると即座に広告がロード・配信開始されます。

---

## 設定方法

Admin Studio（管理画面 > プラグイン > 広告配信プラグイン）または設定ファイル `aegis.toml` で設定します。

```toml
[plugins]
enabled = ["official-ads"]

[plugins.settings.official-ads]
client_id = "ca-pub-1234567890123456"       # Google AdSense のパブリッシャー ID
default_slot = "1234567890"                  # 既定の広告スロット ID
auto_insert_position = "bottom"              # 本文への自動挿入: none / top / bottom / both
require_consent = true                       # Cookie 同意バナーと連携（既定: true）
consent_category = "marketing"               # 同意区分（既定: marketing）
content_types = "article"                    # 広告を配信するコンテンツ種別（カンマ区切り）
```

---

## カスタムブロックの使用例

エディタのブロック追加から「広告 / バナー」（`official-ads/display`）を選択します。

- **AdSense ユニット**:
    - `slot`: 広告スロット ID（未指定時は設定の `default_slot` を使用）
    - `format`: `auto`（自動 / レスポンシブ）、`rectangle`（レクタングル）、`horizontal`（横長）、`vertical`（縦長）
    - `align`: `center`（中央揃え）、`left`（左寄せ）、`right`（右寄せ）
- **カスタムバナー / アフィリエイト**:
    - `custom_html`: `<a href="https://example.com"><img src="https://example.com/banner.png" alt="広告"></a>`
    - `custom_html` が指定された場合は、AdSense よりも優先して表示されます。Cookie 同意バナー連携が有効な場合は、iframe や script に自動で同意属性が付与されます。

---

## `custom_head_code` で使えるタグ

`custom_head_code`（管理画面の「カスタムヘッダータグ」）に書いた HTML は、`<head>` にそのまま入りますが、**`<script>` と `on*` 属性（`onload` など）はホストが取り除きます**。スクリプトは、プラグインが宣言したホストのものだけ、プラグイン自身が足せます（AdSense のスクリプトは `client_id` から足されます）。`<meta>`・`<link>`・`<style>` は使えます。経緯は [CSP の方針](../../docs/csp.ja.md)。
