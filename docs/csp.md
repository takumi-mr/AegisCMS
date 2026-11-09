[← ドキュメント一覧に戻る](README.md)

# Content-Security-Policy（CSP）の方針

公開サイトと Admin Studio の CSP の、現状と、公開ページを Strict CSP（インラインのスクリプトを許さない）にする設計です。

> [!NOTE]
> すべて実装済みです（実装の分け方は §7。フォントは §8）。**既定は `strict_csp = "on"`（破壊的変更）**。前の動きに戻すには `"off"`。

---

## 1. 現状（実装済み）

| 対象 | `script-src` | 備考 |
|---|---|---|
| 公開ページ・API | `'self' 'unsafe-inline'` | `'unsafe-eval'` は入れていない（`eval` を使う機能が無いため）。プラグインが宣言したホスト（`capabilities.csp.script_domains` など）と、CAPTCHA のホストが足される |
| Admin Studio（`/admin/`） | `'self'` | `unsafe-inline` なし。`style-src` も `'self'` だけ（§1.1） |
| テーマ適用ページのプレビュー | 公開ページと同じ | `GET /api/v1/preview/frame/{key}` が、`sandbox allow-scripts` を足して返す。Strict のときは、ページを置いたときに集めたハッシュも CSP に足す |
| 公開ページ（`[security].strict_csp = "on"`。**既定**） | `'self'` + ページのインラインのスクリプトのハッシュ + 宣言したホスト | `'unsafe-inline'`・`'unsafe-eval'` なし（§6）。`style-src` は `'self'` + ページの `<style>` のハッシュ（`unsafe-inline` なし。`style=""` 属性だけ `style-src-attr 'unsafe-inline'` で許す。§6.1） |

- 既定の値は `headers::DEFAULT_CONTENT_SECURITY_POLICY`・`ADMIN_CONTENT_SECURITY_POLICY`。管理者が「設定 › ヘッダー」で CSP を上書きしているときは、その値をそのまま使う。
- `admin/e2e/csp.spec.ts` が、Admin Studio と公開ページの主な画面を既定の CSP で動かし、止められたスクリプトとスクリプトのエラーを検出する。
- 詳しい経緯は [architecture.md の「デフォルト防御ヘッダー値」](architecture.md)。

---

### 1.1 Admin Studio の `style-src`

Admin Studio は、スタイルのインライン（`<style>` 要素・`style` 属性）を使いません。React の `style` は CSSOM（`element.style`）で設定するので、`style-src` に当たりません。そこで、`style-src` も `'self'` だけにしました。

- **HTML を表示する 3 か所は、画面の中に直接入れず、サーバーが置いた別の文書（`iframe`）で見せる**: 本文の HTML（「HTML 出力」のタブ。プラグインのブロックは自分の `<style>` を持つ）、テーマ適用ページ、通知のメールのプレビュー（メールはインラインのスタイルが前提）。`srcdoc` や直接の挿入は、Admin Studio の CSP を引き継いで、これらのスタイルを止めてしまうため。
- サーバーが `GET /api/v1/preview/frame/{key}` で返す（`POST /api/v1/preview` の `frame_key`・`body_frame_key`、`POST /api/v1/notifications/preview` の `frame_key`）。公開ページと同じ CSP に `sandbox allow-scripts` を足すので、中身のスクリプトも管理画面のセッションに届かない。本文の文書には、読みやすい最小限のスタイルを付ける。
- 副産物: 本文の HTML を、管理画面のオリジンの DOM に入れなくなった（`dangerouslySetInnerHTML` をやめた）。
- Admin Studio が読み込んでいた Google Fonts（`fonts.googleapis.com`）は、もともと CSP に止められて読めていなかったので、`index.html` から外した（フォントは、システムのフォントに倒れる）。

## 2. 公開ページに `unsafe-inline` が残っていた理由（§6 で外した）

インラインの `<script>` や `onclick` を、HTML に直接出すものがあるため。

- 公式プラグイン（`page-toc`・`back-to-top`・`share-buttons`・`lightbox`・`external-links`・`code-lines`・`code-enhancer`・`mermaid`・`ads`・`clarity`）の `extra_head`
- 組み込みのテンプレート（コメント欄のフォームのスクリプト、`onclick`）とセットアップ画面
- 第三者のテーマ

`'unsafe-inline'` があると、サニタイズをすり抜けたスクリプトも動く。サニタイズが唯一の防御になっている。

---

## 3. 目標（設計）

公開ページの `script-src` を、**信頼できる出どころのスクリプトだけ**を許す形にする。

```
script-src 'self' 'sha256-<ページごとのインラインのスクリプトのハッシュ>' https://<マニフェストで宣言したホスト>
```

### 3.1 nonce ではなくハッシュにする理由

- nonce は、リクエストごとに違う値が要る。公開ページは CDN・リバースプロキシが HTML を応答ごと共有キャッシュに置く（`[cdn]` が `Cache-Control: public` と `Cache-Tag` を付ける）。キャッシュされた応答のヘッダーと本文に同じ nonce が入り、多くの訪問者に使い回されるので、nonce が予測不能でなくなり、守りにならない。
- ハッシュは**ページの内容で決まる**ので、キャッシュされても変わらない。ヘッダーと本文が同じ応答に入っていれば、常に一致する。
- `'strict-dynamic'` は使わない。使うと、ブラウザーは `'self'` とホストの指定を無視し、nonce 付きのスクリプトが読み込む子スクリプトを、ホストの確認なしに信頼してしまう。マニフェストで宣言した `script_domains` を、ブラウザーが強制し続けるようにする（Clarity の起動コードが `www.clarity.ms` を読む、のような動的な読み込みも、宣言したホストなら通る）。

### 3.2 「信頼できる出どころ」でだけハッシュを計算する

**最終の HTML にある `<script>` を全部ハッシュする方式は、守りにならない。** 攻撃者が差し込んだスクリプトも、一緒にハッシュされて許可されるため。ハッシュは、次の出どころで、**出すときに**計算する。

1. **プラグイン**: 構造化した注入（§4）で受け取ったインラインのスクリプト
2. **テンプレート**: 組み込みのテンプレートは、インラインのスクリプトを外部ファイル（`'self'`）にして、ハッシュを要らなくする。テーマが出すインラインのスクリプトは、テンプレート関数（§5）で出し、そのときにハッシュを登録する
3. **本文（`content_html`）**: ハッシュしない。スクリプトはサニタイズで取り除かれる。万一残っても、CSP で止まる

---

## 4. プラグインの ABI（破壊的変更）

プラグインは、生の HTML 文字列に `<script>` を書いて返すのをやめ、**構造化したデータ**で返す。ホストが検査して、タグを作り、ハッシュを計算する。プラグインはハッシュも nonce も知らない。

```rust
// aegis-core
pub struct InjectedScript {
    pub source: ScriptSource,
    /// 同意の区分（`analytics` など）。あれば、ホストが `type="text/plain" data-aegis-consent="..."` の形で出す
    pub consent: Option<String>,
}

pub enum ScriptSource {
    /// `<script src="https://..." async defer>`
    External { url: String, async_load: bool, defer: bool, crossorigin: bool },
    /// `<script>コード</script>`
    Inline { code: String },
}
```

- `PageRenderContext` に `scripts: Vec<InjectedScript>` を足す（`on-render-page` が返す。位置は `<head>` だけ）。ABI のバージョンを上げる（`abi_version` 2）。外部のスクリプトの URL は、このサイトのパスも宣言なしで通す（`/assets/x.js`）。
- **`extra_head` / `append_content` / `prepend_content` の中の `<script>` は、ホストが取り除く**（lol-html で解析して落とす。`on*` 属性も落とす）。`<link>`・`<meta>`・`<style>` などは今までどおり。
- **ホストの検査**（`on-render-page` の結果に対して、タグを作る前）:
  - `External`: `https://` の URL で、ホストがマニフェストの `capabilities.csp.script_domains` にある。属性は、ホストが決めた `src`・`async`・`defer` だけ（`onload` などを書けない。URL は属性として正しくエスケープする）
  - `Inline`: マニフェストに **`capabilities.inline_scripts: true`** の宣言がある（最小権限。宣言がなければ捨てて、警告を残す）。大きさの上限（既定 64 KB）
  - 区分の文字列は、`[a-z0-9_-]{1,32}` だけ
  - 違反したスクリプトは捨てる。ページは壊さず、`render` の警告と監査ログに残す
- **ハッシュ**: ホストが、出したインラインのスクリプトの本文（`type="text/plain"` で出すものも、同意のあとに `consent.js` が同じ本文で実行するので同じ）の SHA-256 を `sha256-<base64>` にして、応答の CSP に足す。
- **`on-render-page` の結果のキャッシュ**（`[runtime].render_cache_mb`）は、これまでどおり。ハッシュは結果から決まるので、キャッシュしてよい。

---

## 5. テーマとテンプレート

- **組み込みのテンプレート**: インラインのスクリプトと `onclick` をやめる。コメント欄のフォームのスクリプトは外部ファイル（`/_aegis/comments.js` のように、`consent.js` と同じ配信）にし、セットアップ画面も外部ファイルにする。`onclick="window.scrollTo..."` は、外部のスクリプトでイベントを結ぶ。
- **第三者のテーマ**: `<script>` のインラインと `on*` 属性は、Strict のときに動かなくなる（破壊的変更）。直し方は 2 つ（2 つ目は 4 で足す）。
  - スクリプトを `themes/{名前}/assets/*.js` にして、`<script src>` で読む（`'self'`）。
  - どうしてもインラインにするものは、テンプレート関数 `{{ inline_script("コード") }}` で出す。ホストがタグを作り、ハッシュを登録する。

---

## 6. 切り替え（`[security].strict_csp`）

**既定は `"on"`**（破壊的変更）。

| 値 | 動き |
|---|---|
| `"on"`（既定） | Strict の CSP を強制する（`'unsafe-inline'` を外し、信頼できる出どころのインラインのスクリプトはハッシュで許す） |
| `"report-only"` | 前の CSP（`script-src 'self' 'unsafe-inline'`）のまま、Strict の CSP を `Content-Security-Policy-Report-Only` でも出す。ブラウザーのコンソールで、止まるものを確かめられる。移行の確認用 |
| `"off"` | 前の CSP に戻す。ただし §4 の検査と、`<script>` の取り除きは常に効く（ABI v2 の約束） |

**`on` で動かなくなるもの**: テーマ自身の、直接書いたインラインの `<script>`・`on*` 属性（外部ファイルか `{{ inline_script() }}` にする。[テーマの書き方 6a](theme_authoring_guide.md#6a-スクリプトの書き方インラインを書かない)）、HTML の断片に直接書いた `<script>`（`core/html` の通常のモードなどはもともとサニタイズで除かれる）、`'unsafe-inline'` が要る外部サービスの貼り付けコード（AdSense・解析・CAPTCHA のうち、インラインの起動コードを持つもの。公式プラグイン `official-ads`・`official-clarity` は、構造化した注入で対応済み）。

- 管理者が CSP を上書きしているときは、その値が優先で、`strict_csp` は効かない。
- 公開ページの `style-src` も同じ仕組みで Strict になる（§6.1）。Admin Studio は §1.1。

### 6.1 スタイル（`<style>` はハッシュ、`style=""` 属性は当面許す）

`strict_csp = "on"` では、`style-src` からも `'unsafe-inline'` を外し、**信頼できる出どころが出した `<style>` 要素の本文のハッシュ**を足します（スクリプトと同じ仕組み。`aegis_core::csp::record_inline_style` / `record_styles_in`。名前空間の置き換えのあとの本文で取る）。ハッシュを登録するのは次の出どころだけです。

- 組み込みのテンプレート: `{% filter inline_style %}…{% endfilter %}`（または `{{ inline_style("CSS") }}`）で出したもの。**テーマも同じ書き方で `<style>` を書ける**（直接書いた `<style>` は止まる）。`extra_css`（プラグインの CSS・サイトのカスタム CSS）もこの中に入る。
- 組み込みのブロックのスタイル（`BLOCK_STYLES`）、管理者が書いたヘッダーの `<style>`（`custom_head_code`）、組み込みのページ（初期セットアップ・メンテナンス・メルマガの申し込み）。
- プラグインが `on-render-page`（`extra_head`・`prepend_content`・`append_content`）と `render-blocks` で返した `<style>`（ホストが信頼してハッシュを登録する。プラグインのコードはインストールした管理者が信頼したもの）。**ユーザーが書いた本文の `<style>`（`core/html` の通常のモード）は、ハッシュを登録しないので止まる**。
- `core/html` の**サンドボックスのモード**（`<iframe srcdoc>`）は、親の CSP を引き継ぐので、中のインラインの `<style>` と `<script>`（管理者が書いたものと、高さを同期する組み込みのもの）のハッシュも登録する。外部のホストのスクリプトは、親の CSP が許すものだけ。

**`style=""` 属性は、当面 `style-src-attr 'unsafe-inline'` で許します**（組み込みのテンプレートに約 160 か所あり、テーマ・プラグインも使うため）。`<style>` 要素の差し込みは止まり、属性は止まらない、という段階です。属性もなくす（クラスに置き換える）のは次の段階（TODO）。`style-src-attr` を CSP に書いてあれば（上書き）、その値のままにします。

**動かなくなるもの**: テーマが直接書いた `<style>`（`inline_style` に包む。または `assets/*.css` を `<link>` で読む）、ユーザーが `core/html` の通常のモードで書いた `<style>`（サンドボックスのモードか、テーマの CSS に）。前の動きは `strict_csp = "off"`、確かめるだけなら `"report-only"`。

---

## 7. 実装の分け方

1. **設計書**（このページ）。
2. **ABI v2**（**済**）: `InjectedScript`・`capabilities.inline_scripts`、ホストの検査（`aegis_runtime::scripts`。lol-html で `<script>` と `on*` 属性を取り除く・タグを作る）、SDK（Rust・AssemblyScript・TinyGo）、公式プラグイン（`page-toc`・`back-to-top`・`share-buttons`・`lightbox`・`external-links`・`code-lines`・`code-enhancer`・`mermaid`・`ads`・`clarity`）の移行、プラグイン開発サーバーのプレビュー、[plugin_abi_specification.md](plugin_abi_specification.md)・[plugin_hooks.md](plugin_hooks.md)・[versioning.md](versioning.md) の更新。CSP はまだ変えない。
   - 移行で変わったこと: `official-mermaid` の `script_url` は、このサイトのパスか `cdn.jsdelivr.net` の `https://` だけ（ほかのホストはホストが捨てる）。`official-ads` の `custom_head_code` の `<script>` は使えなくなった（`<meta>`・`<link>` などは使える。スクリプトの安全な足し方は未決）。`official-code-enhancer` の Prism は、`<script data-manual>` の代わりに `window.Prism.manual = true` を先に実行する。
3. **組み込みテンプレートの外部化**（**済**）: コメント欄のスクリプト（`/_aegis/comments.js`。URL は `CommentSection.script_url`）、初期セットアップ画面（`/_aegis/setup.js`）、アクセス解析のビーコン（`/_aegis/analytics.js`。受け口は `data-endpoint`）を外部ファイルにし、記事の「トップへ戻る」の `onclick` はページ内リンク（`<a href='#'>`。滑らかなスクロールは CSS の `scroll-behavior`）にした。組み込みのテンプレートと同梱のテーマ 5 つが、インラインのコードを出さないことを `aegis-theme` のテスト（全テンプレートを描画して `<script>`（`src` なし）と `on*` を探す）と、E2E（`script-src 'self'` だけの CSP でコメントを投稿する）で確かめる。テーマの作り方（[theme_authoring_guide.md の 6a](theme_authoring_guide.md#6a-スクリプトの書き方インラインを書かない)）に、インラインを書かない方針を追記した。`inline_script()`（§5）は、ハッシュの登録の仕組みと一緒に 4 で足す。
4. **ハッシュの収集と CSP**（**済**）: 信頼できる出どころ（プラグインの `scripts` のインライン、テンプレートの `inline_script()`）が出すときに、本文の SHA-256 を `aegis_core::csp::record_inline_script` で記録する。記録の受け口は、`security_headers_middleware` がリクエストごとに作る（`tokio::task_local`。受け口の外では何もしない）。応答の本文は名前空間（`aegis-` → `cms-`）の置き換えを受けるので、**ハッシュも置き換えたあとの本文で取る**（受け口に、同じ置き換えを渡す）。`headers::strict_script_csp` が、`script-src` から `'unsafe-inline'`・`'unsafe-eval'` を外してハッシュを足す（宣言したホストは残し、`strict-dynamic` は使わない）。`[security].strict_csp` は `off`（既定）/ `report-only`（今の CSP に加えて `Content-Security-Policy-Report-Only`）/ `on`。管理者が CSP を上書きしているときは、その値が優先。Admin Studio（`/admin/`）は、もともと `unsafe-inline` が無い。`admin/e2e/aegis.e2e.toml` は `on` で、全 E2E が Strict の下で動く。
6. **公開ページの `style-src` から `'unsafe-inline'` を外す**（**済**。破壊的変更。§6.1）。`<style>` はハッシュ、`style=""` 属性は `style-src-attr 'unsafe-inline'` で許す。`core/html` のサンドボックス（`srcdoc`）は、これまでの Strict でもスクリプトが止まっていた（親の CSP を引き継ぐため）ので、中身のハッシュを登録して直した。
5. **既定を `on` に**（**済**。破壊的変更）。`[security].strict_csp` の既定を `"on"` にした。前の動きは `"off"`、確かめるだけなら `"report-only"`。あわせて Admin Studio の `style-src` を `'self'` だけにした（§1.1）。

### 7.1 決めたこと・決めていないこと

- 決めた: ハッシュ方式（共有キャッシュと両立するため）、`strict-dynamic` を使わない、プラグインに nonce・ハッシュを見せない、ABI の破壊的変更を許す。
- 決めた（4 で）: `inline_script()` は、テンプレート関数が `aegis_core::csp::inline_script_tag` を呼んでタグを作り、本文のハッシュをリクエストの受け口に記録する。`report-only` の報告の受け口は持たない（ブラウザーのコンソールで確かめる）。
- 決めた（5 で）: 既定を `on` にする（移行の期間は置かず、破壊的変更として出す）。`aegis-server theme dev`（テーマの開発サーバー）も、プレビューのページに公開サイトの既定の CSP（Strict）を付ける（**済**）。止められたもの（インラインの `<script>`・`<style>`・`on*` 属性、宣言していないホストのフォント・画像）は、ページの右下に理由と直し方つきで出る。`--csp report-only`（強制しない）・`--csp off` もある。テーマが `inline_script` / `inline_style` で出したものと、開発サーバー自身のスクリプトだけがハッシュで許される。

---

## 8. テーマのフォント・外部のスタイル（`theme.json` の `csp`）

公開ページの CSP の `style-src` は `'self'` と `<style>` のハッシュ（§6.1）、`font-src` は `'self' data: https:` です。そのため、**外部のスタイルシート（Google Fonts の `fonts.googleapis.com` など）は、CSP に止められます**（以前は、組み込みの default テーマも同梱の 4 つのテーマも、Google Fonts を読んでいて、止められて代わりの書体になっていました）。直し方は 2 つです。

- **フォントを自分で配信する**（組み込みの default テーマの方式）: woff2 を自分のサイトに置き、`@font-face` で読む。外部に何も頼らないので、CSP に足すものがなく、訪問者の IP アドレスも外に出ない。組み込みの `base.html` は、Plus Jakarta Sans と JetBrains Mono の **Latin のサブセット**を、`/_aegis/fonts/<ファイル>`（バイナリに埋め込み。ライセンスは SIL OFL 1.1。[`crates/aegis-server/assets/fonts/README.md`](../crates/aegis-server/assets/fonts/README.md)）から配信する。日本語など、ほかの文字は、システムのフォントに倒れる。
- **外部のホストを `theme.json` で宣言する**（日本語のウェブフォントのように、自分で配信できないとき）: テーマの `theme.json` に `csp` を書くと、そのテーマが有効な間、サーバーが公開ページの CSP に足す。

```json
{
  "id": "roastery",
  "csp": {
    "style_domains": ["fonts.googleapis.com"],
    "font_domains": ["fonts.gstatic.com"]
  }
}
```

  - 書けるのは `style_domains`（`style-src`）・`font_domains`（`font-src`）・`img_domains`（`img-src`）。**スクリプトのホストは宣言できない**（テーマのスクリプトは、自分のファイルか `inline_script()`）。
  - ホスト名だけ（ワイルドカード・スキーム・ポート・パスは不可）、一覧ごとに 10 個まで。誤りのあるテーマは導入できず、動いているテーマの誤りは無視する（警告を出す）。
  - 管理画面の「テーマ」のカードに、「ほかのサイトから読み込む: …（訪問者の IP アドレスが送られます）」と出る。外部のフォントの CDN には、訪問者の IP アドレスが届くので、プライバシーポリシーに書く。
  - 同梱のテーマのうち、Google Fonts を読む `emerald-luxury`・`minimal-editorial`・`roastery`・`trailhead` が宣言している（`cyber-neon` は読まない）。

管理者が管理画面で CSP を上書きしているときも、宣言したホストは足される（プラグインの `capabilities.csp` と同じ）。
