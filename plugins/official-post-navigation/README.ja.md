# official-post-navigation

[English](README.md) | **日本語**

記事詳細ページの末尾（または指定位置）に、「前の記事」「次の記事」へのナビゲーションリンクを表示する公式 Wasm プラグインです（`on-render-page` フック）。

## 特徴

- **前後の記事を選ぶのは AegisCMS 本体**です（このプラグインではありません）。同じカテゴリを優先するか全体の日付順か、公開日と作成日のどちらで並べるかは、Admin Studio の Site か `[adjacent_articles]` で設定します（[設定リファレンス](https://github.com/takumi-mr/AegisCMS/blob/main/docs/configuration.ja.md)）。テーマにも同じ記事が `prev_article` / `next_article` として渡ります。
- **美しいレスポンシブ UI**:
    - 左右2カラム分割レイアウト（既定）、上下スタック、カード型スタイルから選択可能。
    - サムネイル画像（`featured_image` / `image`）の表示に対応。
    - 先頭・末尾の記事で片方しか存在しない場合でも、左右バランスが崩れないプレースホルダー制御。
- **個別制御**:
    - 記事のカスタムフィールドで `hide_post_nav = true` を設定すると、その記事のみナビゲーションを非表示にできます。

## 設定 (`[plugins.settings.official-post-navigation]`)

```toml
[plugins.settings.official-post-navigation]
# 対象コンテンツタイプ (カンマ区切り、既定: "article")
content_types = "article"

# 表示位置: "bottom" (末尾) / "top" (先頭) / "both" (両方)
position = "bottom"

# レイアウトスタイル: "split" (左右分割) / "stacked" (上下) / "cards" (カード風)
layout = "split"

# ラベルの表示 (既定: true)
show_labels = true

# カスタムラベル (空欄時は言語に応じた既定文言)
prev_label = ""
next_label = ""

# サムネイル画像の表示 (既定: true)
show_thumbnail = true

# アクセントカラー (CSS カラー指定。省略時はテーマの --accent)
accent_color = ""
```
