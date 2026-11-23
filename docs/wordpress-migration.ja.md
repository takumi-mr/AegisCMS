[← ドキュメント一覧へ戻る](README.ja.md)

[English](wordpress-migration.md) | **日本語**

# WordPress からの移行

`aegis-server import wordpress` は、WordPress の書き出しファイルを読んで、稼働中の AegisCMS にコンテンツを作ります。ワンボタンでの移行は目指していません。移せるものを移し、手で直す必要があるものは報告に書き出します。

## 用意するもの

1. WordPress の **ツール → エクスポート** で「すべてのコンテンツ」を選び、XML ファイル（WXR）をダウンロードします。
2. 稼働中の AegisCMS と、**admin** ユーザーのログイン。API トークンでは足りません（トークンは `editor` までで、取り込みはユーザーの作成と元の公開日の保持をするため、admin だけができます）。2 段階認証のユーザーは使えないので、取り込み用に別の admin を使ってください。
3. 取り込み中は、元のサイトが見られる状態にしておきます。画像を元のサイトから取ってくるためです。

## まず試す

```bash
aegis-server import wordpress export.xml --dry-run
```

`--dry-run` はサーバーに触れません。作られる件数、変換できないもの（ショートコードや、そのまま残した HTML）を含む投稿、取り込まないものを、`wordpress-import-report.md` に書きます。

## 取り込む

```bash
export AEGIS_PASSWORD='...'
aegis-server import wordpress export.xml --url http://127.0.0.1:8080 --username admin
```

同じファイルを何度でも取り込めます。すでにある項目（slug が同じもの）は飛ばすので、問題を直してもう一度流せます。失敗した項目は報告に出て、コマンドはエラーで終わります。

| オプション        | 内容                                                                                  |
| ----------------- | ------------------------------------------------------------------------------------- |
| `--url`           | 取り込み先のサーバー（`AEGIS_URL`。既定は `http://127.0.0.1:8080`）                   |
| `--username`      | ログインする admin（`AEGIS_USERNAME`）。パスワードは `--password` か `AEGIS_PASSWORD` |
| `--dry-run`       | 報告だけ出す                                                                          |
| `--report <file>` | 報告の書き出し先（既定は `wordpress-import-report.md`）                               |
| `--skip-media`    | 画像を取り込まない（本文は元のサイトの画像を指したまま）                              |
| `--skip-authors`  | ユーザーを作らない（すべて、取り込みをした admin の投稿になる）                       |

## 移るもの

| WordPress                  | AegisCMS                                                                                                                                                         |
| -------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 投稿                       | `article`。元の公開日・抜粋・最初のカテゴリ・タグを引き継ぐ                                                                                                      |
| 固定ページ                 | `page`                                                                                                                                                           |
| カテゴリ                   | `category` のコンテンツ                                                                                                                                          |
| タグ                       | タグ。slug が半角英数字でないタグ（日本語など）は `wp-tag-<ID>` になる。名前はそのまま                                                                           |
| メディア                   | `wordpress/` の下に、年・月のフォルダーを保って置く。本文のリンク（`-300x200` のような縮小版も）は書き換える                                                     |
| 著者                       | ロールが `contributor` のユーザー。**無効**で、パスワードはランダム。有効にしてパスワードを設定すると、ログインできるようになる                                  |
| 下書き・レビュー待ち・予約 | 同じ状態                                                                                                                                                         |
| パスワード付きの投稿       | 元の状態のまま、`access = "password"`（パスワード保護）として取り込む                                                                                            |
| 非公開の投稿               | 公開されないよう**下書き**として取り込む。報告に出る                                                                                                             |
| ゴミ箱                     | 取り込まない                                                                                                                                                     |
| コメント                   | 状態（承認・承認待ち・スパム・ゴミ箱）・日時・返信の関係ごと取り込む。公開済みの投稿だけ。メールは送らない。その型で `[comments]` を有効にしておく               |
| 旧 URL                     | 旧パーマリンクから新しい URL への 301 のリダイレクト。投稿が多いサイトは、規則を何千も作らず、パーマリンクの形（例: `/{p1}/{p2}/{slug}`）を 1 つのパターンにする |

投稿はブロックに変換します（段落・見出し・リスト・引用・コード・表・画像・区切り線・折りたたみ）。正確に直せないものは HTML ブロックのまま残し、報告に出します。画像の説明（キャプション）は、画像ブロックが持てないため、画像の下の斜体の段落にします。段落内の改行・`<sup>`・入れ子のリストは、`core/html` ブロックに残します。

## 移らないもの

- PHP のテーマ・プラグイン。テーマの見た目は移せます（[テーマを移す](#テーマを移す)）。コンテンツが何を使っていたかは報告で分かります。
- `[caption]`・`[embed]`・`[gallery]` 以外のショートコード、ページビルダーの内容、WooCommerce の商品、フォームのプラグイン、独自の投稿タイプ。
- アイキャッチ画像、親カテゴリ・親ページ（フラットになります）、SEO プラグインの項目。
- ピンバック・トラックバック、ログインユーザーとしてのコメントの投稿者（名前だけになります）、`/?p=123` の形の旧 URL（トップページのクエリは転送できません）。

## テーマを移す

WordPress のテーマは PHP で、AegisCMS のテーマでは PHP は動きません。持ってこられるのは**見た目**（CSS・画像・フォント）です。スターターテーマ `wordpress-classic` は、昔ながらの WordPress テーマと同じクラス名（`site-header`・`entry-title`・`entry-content`・`hentry`・`widget`・`nav-links` など）で出力するので、元の `style.css` が、ほとんど（または全く）直さずに効きます。

```bash
aegis-server theme import-wordpress path/to/wp-content/themes/my-old-theme
aegis-server theme dev themes/my-old-theme
```

`theme import-wordpress` は `themes/<id>/` を作ります（ID は元のテーマの Text Domain から付けます。`--id` でも指定できます）。

- スターターのテンプレートと `style.css`
- 元のテーマのファイル（`style.css`・画像・フォント・ほかの CSS と JavaScript）を、フォルダーの構造のまま `assets/wp/` に写します。`url(images/bg.png)` がそのまま効きます。PHP・翻訳・`node_modules`・隠しファイルは写しません
- 元のテーマのヘッダー（名前・作者・バージョン・ライセンス）と `screenshot.png`（プレビュー）から `theme.json` を作ります。CSS が使う Google Fonts は `csp` に宣言します
- 元の `style.css` は、スターターの CSS の**あとに**読み込むので、元の指定が勝ちます

最後に、確かめる箇所を一覧で出します。`theme dev`（または Theme Builder）で見た目を確かめ、違うところを直して、`[theme].active` を新しい ID にします。

### テンプレート階層の対応

| WordPress                               | AegisCMS                    | 備考                                                                                                                 |
| --------------------------------------- | --------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `header.php` + `footer.php`             | `templates/base.html`       | 全ページ共通の骨組み。元のヘッダー・フッターのマークアップはここに入れます                                           |
| `index.php`・`home.php`                 | `index.html`                | 投稿の一覧                                                                                                           |
| `front-page.php`                        | `front.html`                | `[theme].front_page = "template"` にします                                                                           |
| `single.php`・`single-post.php`         | `article.html`              | 投稿                                                                                                                 |
| `page.php`                              | `page.html`                 | 固定ページ                                                                                                           |
| `category.php`・`archive.php`           | `category.html`             | カテゴリの一覧                                                                                                       |
| `tag.php`                               | `tag.html`                  | タグの一覧                                                                                                           |
| `search.php`                            | `search.html`               | サイト内検索（[search.ja.md](search.ja.md)）                                                                         |
| `404.php`                               | `404.html`                  |                                                                                                                      |
| `comments.php`                          | `comments.html`             | テーマに無ければ、組み込みのコメント欄を使います                                                                     |
| `sidebar.php`・ウィジェット             | —                           | 対応していません。固定の内容は `base.html` に書きます                                                                |
| `functions.php`                         | —                           | メニュー・ウィジェット・画像サイズ・カスタマイザーの設定は取り込みません                                             |
| `template-parts/*.php`                  | `{% include "name.html" %}` | マークアップをテンプレートに写します（スターターの `loop.html` が例です）                                            |
| `page-{slug}.php`・カスタムテンプレート | 固定ページの `layout` 欄    | `layout` を `blank` か、テーマのテンプレート名にします（[theme_authoring_guide.ja.md](theme_authoring_guide.ja.md)） |

### テンプレートタグ・変数の対応

| WordPress                                              | AegisCMS                                                                                                  |
| ------------------------------------------------------ | --------------------------------------------------------------------------------------------------------- |
| `bloginfo('name')`                                     | `{{ site_title }}`                                                                                        |
| `bloginfo('description')`                              | `{{ site_description }}`                                                                                  |
| `home_url('/')`                                        | `{{ base_path }}/`                                                                                        |
| `get_stylesheet_uri()`・`get_template_directory_uri()` | `{{ base_path }}/theme/assets/…`                                                                          |
| `language_attributes()`                                | `lang="{{ locale }}"`                                                                                     |
| `wp_head()`                                            | `{{ extra_head \| safe }}`（と `{{ extra_css \| inline_style }}`）                                        |
| `wp_footer()`                                          | 足すものはありません                                                                                      |
| `body_class()`                                         | `base.html` に書く（スターターは `single`・`page`・`home blog`・`archive`・`error404` を出します）        |
| `have_posts()` / `the_post()` のループ                 | `{% for item in items %}…{% endfor %}`                                                                    |
| `the_title()`                                          | `{{ title }}`（1 件）/ `{{ item.title }}`（一覧の中）                                                     |
| `the_content()`                                        | `{{ content_html \| safe }}`                                                                              |
| `the_excerpt()`                                        | `{{ item.excerpt }}`                                                                                      |
| `the_permalink()`                                      | `{{ item.fields.url }}`                                                                                   |
| `get_the_date()`                                       | `{{ published_at \| date_short }}`                                                                        |
| `the_author()`                                         | `{{ author }}`                                                                                            |
| `the_category()`                                       | `{{ relations.category_id.title }}`。リンクは `{{ base_path }}/category/{{ relations.category_id.slug }}` |
| `the_tags()`                                           | `{% for tag in fields.tags %}{{ tag.name }}{{ tag.url }}{% endfor %}`                                     |
| `comments_template()`                                  | `{% if comments %}{% include "comments.html" %}{% endif %}`                                               |
| `the_posts_navigation()`・`paginate_links()`           | `pagination`（`prev_url`・`next_url`・`page`・`total_pages`）                                             |
| `wp_nav_menu()`                                        | `base.html` にリンクを書く                                                                                |
| `__()`・`_e()`・`esc_html__()`                         | 文言を直接書くか、`{{ t("key") }}`                                                                        |
| `is_front_page()`・`is_single()`・`is_page()`          | `content_type`（`index`・`article`・`page`・`error` など）                                                |

PHP のテーマに無かった決まりごと: リンクには `base_path` を付ける、テンプレートにインラインのスクリプトや `style=` 属性を書かない（CSP が止めます）、`| safe` を付けてよいのは `content_html` だけ。詳しくは[テーマ作成ガイド](theme_authoring_guide.ja.md)。
