# official-reading-time

[English](README.md) | **日本語**

記事や固定ページの本文を解析し、推定読了時間と文字数・単語数のインジケーター（バッジ）を表示する公式 Wasm プラグインです（`on-render-page` フック）。

## 特徴

- **日本語・英語のハイブリッド対応**:
    - 日本語・中国語・韓国語などの CJK 文字は文字数単位（既定: 500 文字/分）で計算。
    - 英語などの欧文はスペース区切りの単語数単位（既定: 200 単語/分）で計算。
    - 混在テキストでも正確な読了時間を算出します。
- **柔軟な見た目と配置**:
    - ピル型バッジ（既定）、枠線付きカード、シンプルなテキスト表示から選択可能。
    - 本文の先頭（既定）、末尾、または両方に配置可能。
    - テーマの `--accent` カラーと連動する時計アイコン付き。
- **記事ごとの個別非表示**:
    - 記事のカスタムフィールドで `hide_reading_time = true` を設定すると、その記事のみ非表示にできます。

## 設定 (`[plugins.settings.official-reading-time]`)

```toml
[plugins.settings.official-reading-time]
# 対象コンテンツタイプ (カンマ区切り、既定: "article")
content_types = "article"

# 表示モード: "both" (読了時間と文字数両方) / "time_only" (時間のみ) / "chars_only" (文字数のみ)
display_mode = "both"

# 表示位置: "top" (先頭) / "bottom" (末尾) / "both" (両方)
position = "top"

# バッジのスタイル: "badge" (角丸ピル) / "simple" (シンプルテキスト) / "card" (枠線付きカード)
badge_style = "badge"

# 時計アイコンの表示 (既定: true)
show_icon = true

# 読書速度 (単語/分、欧文向け、既定: 200)
words_per_minute = 200

# 読書速度 (文字/分、CJK向け、既定: 500)
chars_per_minute = 500

# アクセントカラー (CSS カラー指定。省略時はテーマの --accent)
accent_color = ""

# カスタム表示フォーマット ({time}, {chars}, {words} が使用可能。空欄時は自動)
format = ""
```
