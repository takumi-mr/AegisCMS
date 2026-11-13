# official-post-navigation

[English](README.md) | **日本語**

記事詳細ページの末尾（または指定位置）に、「前の記事」「次の記事」へのナビゲーションリンクを表示する公式 Wasm プラグインです（`on-render-page` フック）。

## 特徴

- **柔軟な前後記事の算出ルール**:
  - `mode = "category"`（既定）: 同じカテゴリ内の前後の記事を優先探索。同カテゴリ内に前後の記事がない場合は、日付順の前後に自動フォールバック可能（`fallback_to_all`）。
  - `mode = "date"`: カテゴリを問わず、全体の日付（公開日または作成日）順で前後の記事を探索。
- **美しいレスポンシブ UI**:
  - 左右2カラム分割レイアウト（既定）、上下スタック、カード型スタイルから選択可能。
  - サムネイル画像（`featured_image` / `image`）の表示に対応。
  - 先頭・末尾の記事で片方しか存在しない場合でも、左右バランスが崩れないプレースホルダー制御。
- **個別制御**:
  - 記事のカスタムフィールドで `hide_post_nav = true` を設定すると、その記事のみナビゲーションを非表示にできます。

## 設定 (`[plugins.settings.official-post-navigation]`)

```toml
[plugins.settings.official-post-navigation]
# 算出モード: "category" (同一カテゴリ優先) / "date" (全体の日付順)
mode = "category"

# 同一カテゴリに前後記事が無い場合の全体日付順フォールバック (既定: true)
fallback_to_all = true

# 並び順の基準: "published_at" (公開日順、既定) / "created_at" (作成日順)
order_by = "published_at"

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

