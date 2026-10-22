# AegisCMS コンテンツ公開 & テーマエンジン開発ガイド

AegisCMS のコンテンツ公開エンジン（Public Publishing Engine）とテーマエンジン（MiniJinja）、およびプラグインによる動的テーマ書き換え仕様について解説します。

---

## 1. 公開ルーティングアーキテクチャ

AegisCMS は Headless API Gateway、管理画面 Admin Studio、および高速な公開フロントエンド Web サイトを 1 つのバイナリで安全にホストします。

| パス / ルート | 配信内容 | 備考 |
|---|---|---|
| `GET /` | 公開トップページ（最新記事一覧） | テンプレート `index.html` 描画 |
| `GET /articles` | 記事一覧 | 同上 |
| `GET /articles/{slug}` | 記事詳細ページ | テンプレート `article.html` 描画 (`status == "published"` のみ公開) |
| `GET /{slug}` | 固定ページまたは記事詳細 | テンプレート `page.html` または `article.html` 描画 |
| `GET /theme/assets/*` | テーマ固有の静的アセット (CSS/JS/画像) | `themes/{active}/assets/` より高速配信 |
| `GET /admin`, `/admin/*` | Admin Studio 管理画面 SPA | Vite + React 管理クライアント |
| `GET /api/v1/*` | REST API Gateway | スキーマ・コンテンツ CRUD・ストレージ・認証 |
| `GET /uploads/*` | ローカルストレージアセット | アップロード画像・メディア配信 |

### ゼロ・トラストな公開制御
- 下書き状態（`status: "draft"`）やアーカイブ状態（`status: "archived"`）のコンテンツは、一般公開エンドポイント（`/`, `/articles/{slug}`, `/{slug}`）では一切表示されず、安全に `404 Not Found` テンプレートが描画されます。
- 記事本文は `HtmlRenderer` による決定論的 HTML 生成の後、Cloudflare 製 `lol-html` ストリーミングリライター（`StreamingHtmlRewriter`）を透過的に通過し、外部リンクの `rel="noopener noreferrer"` 自動付与や画像の `loading="lazy"` / `decoding="async"` 強制適用が担保されます。

---

## 2. テンプレートエンジン (MiniJinja)

AegisCMS では Jinja2 / Django 互換で高速・軽量・堅牢なテンプレートエンジン `MiniJinja` を採用しています。

### テンプレート読み込みの 3 層フォールバック
テンプレート解決時、以下の優先順位で探索されます：

1. **プラグイン / 動的登録テンプレート (最優先)**:
   - Wasm プラグインや実行時コードが `ThemeEngine::register_template(name, content)` で登録したインメモリテンプレート。
2. **ファイルシステムテーマ (`themes/{active}/templates/`)**:
   - ディスク上のテーマディレクトリに配置された HTML テンプレート。
3. **ビルトイン・デフォルトテンプレート (フォールバック)**:
   - コアバイナリ内にコンパイル時に組み込まれた洗練されたモダンデザインテンプレート（`base.html`, `index.html`, `article.html`, `page.html`, `404.html`）。追加のファイルが一切存在しなくても単体で美しく動作します。

### 標準テンプレート構成
- `base.html`: HTML5 スケルトン、レスポンシブ、モダンタイポグラフィ、ナビゲーションバー、フッター、アンビエント背景グロー、プラグイン用 CSS/Head 注入スロット。
- `index.html`: `base.html` を継承。ヒーローセクション、カードグリッド、カテゴリバッジ、公開日、抜粋。
- `article.html`: `base.html` を継承。パンくずリスト、タイトル、メタ情報、Prose 本文スタイル、前後のナビゲーション。
- `page.html`: 固定ページ用レイアウト。
- `404.html`: 404 エラーページ。

---

## 3. Plugin によるテーマ書き換え機構

AegisCMS では、プラグインが Web ページのテーマやコンテキストを動的・プログラマブルに書き換えることができます。

### 3.1 `on-render-page` フックによるコンテキスト・スタイルの書き換え

ページ描画直前にプラグインの `on_render_page` フックが呼び出され、`PageRenderContext` を受け取って変更できます。

- 公開ページの表示に加え、Admin Studio のプレビュー（`POST /api/v1/preview` の `page_html`、「Themed Page」タブ）でも、編集のたび（入力が 300ms 止まるごと）に呼ばれます。プレビューでは `published_at` / `updated_at` が `null`、`author` がログインユーザーになります。**副作用（KVS への書き込み、外部通知など）を持たせないでください。**
- プレビューのページは Admin Studio 内の `sandbox` 付き `iframe`（スクリプト無効）に表示します。`extra_head` などで注入したスクリプトはプレビューでは動きません（公開ページでは動きます）。

#### `PageRenderContext` の主要プロパティ
```rust
pub struct PageRenderContext {
    pub site_title: String,                  // サイトタイトル
    pub site_description: String,            // サイト説明
    pub template: String,                    // 使用するテンプレート名 (例: "article.html")
    pub content_type: String,                // "article", "page", "index" 等
    pub title: String,                       // 記事/ページタイトル
    pub slug: String,                        // スラッグ
    pub content_html: String,                // 本文 HTML
    pub author: Option<String>,              // 著者名
    pub published_at: Option<String>,        // 公開日時
    pub fields: HashMap<String, Value>,      // カスタムフィールド
    pub relations: HashMap<String, Value>,   // 解決されたリレーション
    pub items: Vec<ContentSummary>,          // 一覧用記事リスト
    pub extra_css: String,                   // 注入するカスタム CSS
    pub extra_head: String,                  // <head> に注入する HTML タグ
    pub theme_vars: HashMap<String, String>, // テーマカラーなどの動的変数
}
```

#### Rust プラグイン SDK での実装例
```rust
use aegis_plugin_sdk::prelude::*;

#[derive(Default)]
pub struct CyberpunkThemePlugin;

#[aegis_plugin]
impl Plugin for CyberpunkThemePlugin {
    fn on_render_page(&mut self, mut ctx: PageRenderContext) -> Result<PageRenderContext> {
        // 1. サイトタイトルの動的修飾
        ctx.site_title = format!("{} [Cyberpunk Neon]", ctx.site_title);

        // 2. テーマ変数の注入 (CSS 変数や特定クラスの上書き)
        ctx.extra_css.push_str(r#"
            :root {
                --accent: #00ffcc;
                --bg-primary: #050508;
                --bg-card: rgba(10, 15, 30, 0.85);
            }
            .brand-logo { text-shadow: 0 0 10px #00ffcc; }
        "#);

        // 3. メタタグやアナリティクススクリプトの注入
        ctx.extra_head.push_str(r#"<meta name="theme-color" content="#00ffcc">"#);

        // 4. 特定の記事タイプやスラッグの場合に別テンプレートへ切り替え
        if ctx.slug.starts_with("special-") {
            ctx.template = "special_layout.html".to_string();
        }

        Ok(ctx)
    }
}
```

### 3.2 テンプレートの動的オーバーライド

プラグインは `ThemeEngine::register_template(name, content)` を呼び出すことで、特定のテンプレート（例えば `article.html` や `custom_header.html`）を文字列として動的登録できます。
MiniJinja の動的ローダーがこれを最優先で認識するため、サーバーの再起動やファイル配置なしにテーマの見た目を完全に差し替えることが可能です。

---

## 4. 設定ファイル (`config/default.toml`)

```toml
[theme]
active = "default"
site_title = "AegisCMS Official Site"
site_description = "Next-Generation High-Performance Secure CMS with Sandboxed WebAssembly Extensibility"
# custom_css = ":root { --accent: #10b981; }" # オプション: グローバルカスタム CSS

[paths]
themes_dir = "themes"
```
