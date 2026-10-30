[← ドキュメント一覧に戻る](README.md)

# AegisCMS コンテンツ公開 & テーマエンジン開発ガイド

AegisCMS のコンテンツ公開エンジン（Public Publishing Engine）とテーマエンジン（MiniJinja）、およびプラグインによる動的テーマ書き換え仕様について解説します。

> テーマを作る手順（ディレクトリ構成・`theme.json`・テンプレートごとの変数・CSS とスマートフォン対応・確認方法）は [テーマ作成ガイド](./theme_authoring_guide.md) にまとめています。

---

## 1. 公開ルーティングアーキテクチャ

AegisCMS は Headless API Gateway、管理画面 Admin Studio、および高速な公開フロントエンド Web サイトを 1 つのバイナリで安全にホストします。

| パス / ルート | 配信内容 | 備考 |
|---|---|---|
| `GET /` | 公開トップページ | `[theme].front_page` で切り替え: `posts`（既定。記事一覧 `index.html`、`?page=N` でページ送り）、`template`（テーマの `front.html`）、`page:<slug>`（固定ページ） |
| `GET /articles` | 記事一覧 | テンプレート `index.html` 描画。`[theme].posts_per_page` 件ずつ、`?page=N` でページ送り（範囲外は 404） |
| `GET /category/{slug}` | カテゴリの記事一覧 | テンプレート `category.html` 描画。記事の `category` への関係フィールド（`category_id`）で絞り込む。ページ送りは同上 |
| `GET /feed.xml` | 新しい記事の Atom フィード | 最新 20 件。各ページの `<head>` に自動検出用の `<link>` が入る |
| `GET /category/{slug}/feed.xml` | カテゴリの記事の Atom フィード | 同上 |
| `GET /articles/{slug}` | 記事詳細ページ | テンプレート `article.html` 描画 (`status == "published"` のみ公開) |
| `GET /{slug}` | 固定ページまたは記事詳細 | テンプレート `page.html` または `article.html` 描画 |
| `GET /theme/assets/*` | テーマ固有の静的アセット (CSS/JS/画像) | `themes/{active}/assets/` より高速配信 |
| `GET /admin`, `/admin/*` | Admin Studio 管理画面 SPA | Vite + React 管理クライアント |
| `GET /api/v1/*` | REST API Gateway | スキーマ・コンテンツ CRUD・ストレージ・認証 |
| `GET /uploads/*` | ローカルストレージアセット | アップロード画像・メディア配信 |

### ゼロ・トラストな公開制御
- 下書き状態（`status: "draft"`）・予約公開の待ち（`status: "scheduled"`）・アーカイブ状態（`status: "archived"`）のコンテンツは、一般公開エンドポイント（`/`, `/articles/{slug}`, `/{slug}`, `/category/{slug}`, `/feed.xml`, `/products`）では一切表示されず、安全に `404 Not Found` テンプレートが描画されます。
- 記事本文は `HtmlRenderer` による決定論的 HTML 生成の後、Cloudflare 製 `lol-html` ストリーミングリライター（`StreamingHtmlRewriter`）を透過的に通過し、外部リンクの `rel="noopener noreferrer"` 自動付与や画像の `loading="lazy"` / `decoding="async"` 強制適用が担保されます。

### 予約公開（スケジュール公開）
- `status: "scheduled"` と公開日時 `published_at`（RFC 3339、タイムゾーン付き）を指定して保存すると、その日時にサーバーが自動で `published` にします。Admin Studio ではエディタ・商品フォームの状態で「予約公開」を選び、公開日時を入力します。

  ```bash
  curl -X POST http://localhost:8080/api/v1/contents/article \
    -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
    -d '{"title": "新製品のお知らせ", "slug": "new-product", "status": "scheduled",
         "published_at": "2026-11-01T09:00:00+09:00", "blocks": []}'
  ```
- 公開日時は UTC に揃えて保存され（例: `2026-11-01T00:00:00Z`）、公開後もそのまま記事の `published_at`（公開日）になります。
- サーバーは 30 秒ごとに予約日時を確認するため、公開は指定した日時から最大 30 秒ほど遅れます。サーバーが止まっていた間に日時を迎えたコンテンツは、起動後の最初の確認で公開されます。
- `published_at` が無い・読めない場合は `400 Bad Request`。過ぎた日時を指定するとその場で公開（`published`、公開日時は保存した時刻）します。`scheduled` 以外の状態では `published_at` は無視されます。
- 公開前に日時を変えたり下書きに戻したりすると、その内容が優先されます（公開処理は「まだ同じ日時で予約されている」ときだけ行われるため、複数台のサーバーで動かしても二重には公開されません）。
- 予約による公開は監査ログ `content.publish`（操作者なし）に記録されます（[監査ログ](audit.md)）。
- 通知（`[notifications]`）が有効なら、公開したことを著者にメールで知らせます（`content.published`。[通知 2](notifications.md#2-送る通知)）。

### 予約での非公開化（期間限定公開）
- 公開中（`status: "published"`）または予約公開（`status: "scheduled"`）のコンテンツに、非公開予定日時 `unpublish_at`（RFC 3339、タイムゾーン付き）を設定できます。キャンペーン情報や期間限定セール商品などに利用します。
- `unpublish_at` は必ず公開日時（予約公開の `published_at` または現在時刻）より未来の日時を指定する必要があります（過去の日時や公開日時以前を指定した場合は `400 Bad Request`）。下書き（`draft`）に戻した場合は `unpublish_at` は自動クリアされます。
- **リクエスト時の防御判定**: 公開エンドポイント（`/`, `/articles/{slug}`, `/products` 等）は、スケジューラの定期実行（30 秒間隔）を待たずに、リクエスト時点で `unpublish_at <= now` となったコンテンツを即座に 404 として非公開化します。
- **スケジューラによるアーカイブ化**: サーバーは 30 秒ごとに期限を迎えた公開コンテンツを検出し、自動で `status = "archived"` に更新します。この非公開化は監査ログ `content.unpublish`（操作者なし）に記録され、通知が有効なら著者に知らせます（`content.unpublished`）。

### 公開スケジュール・カレンダー
- Admin Studio 上部の「カレンダー」タブ（`GET /api/v1/schedule`）から、月間カレンダーグリッドおよびタイムラインアジェンダ形式で、予約公開予定・非公開予定・公開中コンテンツを確認できます。
- 日付セルまたはアジェンダカードから詳細モーダルを開き、非公開日時を直接変更・解除（クイック編集）したり、エディタに直接遷移して本文を修正したりできます。
- コンテンツタイプ（記事・商品など）や予定種別（予約公開・非公開予定あり・公開中）によるフィルタリングにも対応しています。

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

### テンプレートで使えるグローバル変数
- `base_path`: サイトを公開しているパス（`[server].base_path`、例: `/cms`。ルートで公開している場合は空文字）。サイト内へのリンクやテーマアセットは `{{ base_path }}/articles/{{ item.slug }}`、`{{ base_path }}/theme/assets/style.css` のように先頭に付けてください。付けなくてもルート公開では動きますが、サブパス配下（[設定リファレンス](./configuration.md#42-サブパスで公開する)）ではリンク切れになります。
- `admin_url`: Admin Studio へのリンクの URL（`base_path` と `[server].admin_path` 込み、例: `/cms/admin/`）。**出さない設定のときは空文字**なので、`{% if admin_url %}<a href="{{ admin_url }}">...</a>{% endif %}` のように囲んでください（`/admin/` を直接書くと、`admin_path` を変えたサイトでリンク切れになるうえ、隠したいパスを公開してしまいます）。出す条件は `[theme].show_admin_link`。
- `locale`: 表示言語（`[theme].locale`、`en` / `ja`。`<html lang="{{ locale }}">` に使う）。
- `t("キー", n=件数, 名前=値)`: 組み込みの文言を表示言語で返す関数（例: `{{ t("nav.home") }}` → `Home` / `ホーム`）。キーは `crates/aegis-theme/src/messages.rs` の表にある。`n` を渡すと単数・複数（`キー.one` / `キー.other`）を選び、`{名前}` の位置に値を（HTML エスケープして）差し込む。結果はエスケープ済みの文字列で、`<strong>` などを含むものがある。独自の文言はテーマの中に直接書く。
- `show_powered_by`: フッターに「Powered by AegisCMS」などの表記を出すか（`[theme].show_powered_by`、既定は `true`）。独自テーマでも表記は `{% if show_powered_by %}...{% endif %}` で囲んでください。
- `pagination`（`index.html` のみ）: `page`（1 始まり）、`per_page`、`total_items`、`total_pages`、`prev_url` / `next_url`（`base_path` 込み。無ければ空）。例:

```jinja
{% if pagination and pagination.total_pages > 1 %}
<nav>
  {% if pagination.prev_url %}<a href="{{ pagination.prev_url }}" rel="prev">Newer</a>{% endif %}
  Page {{ pagination.page }} / {{ pagination.total_pages }}
  {% if pagination.next_url %}<a href="{{ pagination.next_url }}" rel="next">Older</a>{% endif %}
</nav>
{% endif %}
```

### テンプレートのキャッシュ
コンパイルしたテンプレートは再利用し、テーマの切り替え（`PUT /api/v1/themes/active`）とプラグインによるテンプレートの登録時に読み直します。テーマのファイルを編集しながら確認する場合は `[theme].reload_templates = true` にすると、描画のたびに読み直します（本番では `false` のままにしてください）。

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
    pub pagination: Option<Pagination>,      // トップページのページ送り（index.html のみ）
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
# favicon = "/uploads/favicon.png"  # オプション: favicon（/ から始まるパスか http(s) の URL）
# custom_css = ":root { --accent: #10b981; }" # オプション: グローバルカスタム CSS
posts_per_page = 10        # 記事一覧の 1 ページあたりの記事数（?page=N でページ送り）
front_page = "posts"       # トップページ: "posts"（記事一覧）/ "template"（テーマの front.html）/ "page:<slug>"（固定ページ）
reload_templates = false   # true でテンプレートを描画のたびに読み直す（テーマ開発用）
locale = "en"              # 組み込みテンプレート・コメント欄・カートの言語（en / ja）
show_powered_by = true     # false でフッターの「Powered by AegisCMS」を出さない
# show_admin_link = false  # 公開ページに Admin Studio へのリンクを出すか（未指定: [server].admin_path が既定の /admin のときだけ出す）

[paths]
themes_dir = "themes"
```

### ZIP からのインストール

Admin Studio のテーマ画面の「ZIP をアップロード」または `POST /api/v1/themes/install`（`admin`、multipart の `file` フィールド）で、テーマのパッケージをインストールできます。`theme.json` を ZIP の直下か 1 段下から探し、`ThemeEngine::validate_theme_package` で検証してから `themes/<id>/` に置きます（同じ ID のテーマは置き換え。有効化はしない。監査ログ `theme.install`）。展開の制限と署名（`signature.json`）はプラグインと同じで、`[theme].require_signatures` / `[theme].trusted_public_keys` で設定します（[プラグインのインストールと署名](./plugin_hooks.md#54-zip-パッケージからのインストールと署名)）。

### サイト名と favicon（Admin Studio）

`site_title` と `favicon` は Admin Studio の **Settings › Site**（`admin` のみ）からも変更できます。保存した値は DB（`_aegis_settings`）に入り、設定ファイルの値より優先されます。サーバーの再起動なしで公開サイトに反映され、欄を空にして保存すると設定ファイルの値に戻ります。

- favicon は `<link rel="icon" href="...">` としてテンプレートの `extra_head` に入ります（すべてのテーマが出力する変数のため、テーマ側の対応は不要です）。`on-render-page` プラグインからも見え、書き換えられます。
- favicon に使えるのは `/` から始まるパス（アップロードした画像の URL など）と http(s) の URL です。`javascript:` / `data:` / `//host` は受け付けません。
- API: `GET /api/v1/site` は `{"title", "favicon", "defaults": {"title", "favicon"}}`（`title` / `favicon` は Admin Studio で設定した値、未設定なら `null`）を返します。`PUT /api/v1/site` に `{"title": "...", "favicon": "..."}` を送ると変更し、`null` か空文字の項目は設定ファイルの値に戻します（どちらも `admin` のみ。監査ログ `site.update`）。
- 通知のテンプレートの `site_name` には起動時のサイト名を使います。Admin Studio で変えた名前は、サーバーの再起動後に通知へ反映されます。
