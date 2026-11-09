[← ドキュメント一覧に戻る](README.md)

# Content-Security-Policy（CSP）の方針

公開サイトと Admin Studio の CSP の、現状と、公開ページを Strict CSP（インラインのスクリプトを許さない）にする設計です。

> [!NOTE]
> §1〜§6 は実装済みです（実装の分け方は §7）。既定は `strict_csp = "off"` で、今までどおりです。

---

## 1. 現状（実装済み）

| 対象 | `script-src` | 備考 |
|---|---|---|
| 公開ページ・API | `'self' 'unsafe-inline'` | `'unsafe-eval'` は入れていない（`eval` を使う機能が無いため）。プラグインが宣言したホスト（`capabilities.csp.script_domains` など）と、CAPTCHA のホストが足される |
| Admin Studio（`/admin/`） | `'self'` | `unsafe-inline` なし。管理画面のビルドはインラインのスクリプトを使わない |
| テーマ適用ページのプレビュー | 公開ページと同じ | `GET /api/v1/preview/frame/{key}` が、`sandbox allow-scripts` を足して返す。Strict のときは、ページを置いたときに集めたハッシュも CSP に足す |
| 公開ページ（`[security].strict_csp = "on"`） | `'self'` + ページのインラインのスクリプトのハッシュ + 宣言したホスト | `'unsafe-inline'`・`'unsafe-eval'` なし（§6） |

- 既定の値は `headers::DEFAULT_CONTENT_SECURITY_POLICY`・`ADMIN_CONTENT_SECURITY_POLICY`。管理者が「設定 › ヘッダー」で CSP を上書きしているときは、その値をそのまま使う。
- `admin/e2e/csp.spec.ts` が、Admin Studio と公開ページの主な画面を既定の CSP で動かし、止められたスクリプトとスクリプトのエラーを検出する。
- 詳しい経緯は [architecture.md の「デフォルト防御ヘッダー値」](architecture.md)。

---

## 2. 公開ページに `unsafe-inline` が残っている理由

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

テーマ・プラグインの移行が済むまでは、既定では切り替えない。

| 値 | 動き |
|---|---|
| `"off"`（既定） | 今のまま（`script-src 'self' 'unsafe-inline'`）。ただし §4 の検査と、`<script>` の取り除きは常に効く（ABI v2 の約束） |
| `"report-only"` | 今の CSP に加えて、Strict の CSP を `Content-Security-Policy-Report-Only` で出す。ブラウザーのコンソールで、止まるものを確かめられる |
| `"on"` | Strict の CSP を強制する（`'unsafe-inline'` を外す） |

- 管理者が CSP を上書きしているときは、その値が優先で、`strict_csp` は効かない。
- `style-src 'unsafe-inline'` は、この設計の対象外（`style` 属性が組み込みテンプレートに多い。別の見直し）。

---

## 7. 実装の分け方

1. **設計書**（このページ）。
2. **ABI v2**（**済**）: `InjectedScript`・`capabilities.inline_scripts`、ホストの検査（`aegis_runtime::scripts`。lol-html で `<script>` と `on*` 属性を取り除く・タグを作る）、SDK（Rust・AssemblyScript・TinyGo）、公式プラグイン（`page-toc`・`back-to-top`・`share-buttons`・`lightbox`・`external-links`・`code-lines`・`code-enhancer`・`mermaid`・`ads`・`clarity`）の移行、プラグイン開発サーバーのプレビュー、[plugin_abi_specification.md](plugin_abi_specification.md)・[plugin_hooks.md](plugin_hooks.md)・[versioning.md](versioning.md) の更新。CSP はまだ変えない。
   - 移行で変わったこと: `official-mermaid` の `script_url` は、このサイトのパスか `cdn.jsdelivr.net` の `https://` だけ（ほかのホストはホストが捨てる）。`official-ads` の `custom_head_code` の `<script>` は使えなくなった（`<meta>`・`<link>` などは使える。スクリプトの安全な足し方は未決）。`official-code-enhancer` の Prism は、`<script data-manual>` の代わりに `window.Prism.manual = true` を先に実行する。
3. **組み込みテンプレートの外部化**（**済**）: コメント欄のスクリプト（`/_aegis/comments.js`。URL は `CommentSection.script_url`）、初期セットアップ画面（`/_aegis/setup.js`）、アクセス解析のビーコン（`/_aegis/analytics.js`。受け口は `data-endpoint`）を外部ファイルにし、記事の「トップへ戻る」の `onclick` はページ内リンク（`<a href='#'>`。滑らかなスクロールは CSS の `scroll-behavior`）にした。組み込みのテンプレートと同梱のテーマ 5 つが、インラインのコードを出さないことを `aegis-theme` のテスト（全テンプレートを描画して `<script>`（`src` なし）と `on*` を探す）と、E2E（`script-src 'self'` だけの CSP でコメントを投稿する）で確かめる。テーマの作り方（[theme_authoring_guide.md の 6a](theme_authoring_guide.md#6a-スクリプトの書き方インラインを書かない)）に、インラインを書かない方針を追記した。`inline_script()`（§5）は、ハッシュの登録の仕組みと一緒に 4 で足す。
4. **ハッシュの収集と CSP**（**済**）: 信頼できる出どころ（プラグインの `scripts` のインライン、テンプレートの `inline_script()`）が出すときに、本文の SHA-256 を `aegis_core::csp::record_inline_script` で記録する。記録の受け口は、`security_headers_middleware` がリクエストごとに作る（`tokio::task_local`。受け口の外では何もしない）。応答の本文は名前空間（`aegis-` → `cms-`）の置き換えを受けるので、**ハッシュも置き換えたあとの本文で取る**（受け口に、同じ置き換えを渡す）。`headers::strict_script_csp` が、`script-src` から `'unsafe-inline'`・`'unsafe-eval'` を外してハッシュを足す（宣言したホストは残し、`strict-dynamic` は使わない）。`[security].strict_csp` は `off`（既定）/ `report-only`（今の CSP に加えて `Content-Security-Policy-Report-Only`）/ `on`。管理者が CSP を上書きしているときは、その値が優先。Admin Studio（`/admin/`）は、もともと `unsafe-inline` が無い。`admin/e2e/aegis.e2e.toml` は `on` で、全 E2E が Strict の下で動く。
5. **既定を `on` に**（移行の期間のあと）。テーマ・プラグインの移行ガイド。現状の既定は `off`。

### 7.1 決めたこと・決めていないこと

- 決めた: ハッシュ方式（共有キャッシュと両立するため）、`strict-dynamic` を使わない、プラグインに nonce・ハッシュを見せない、ABI の破壊的変更を許す。
- 決めた（4 で）: `inline_script()` は、テンプレート関数が `aegis_core::csp::inline_script_tag` を呼んでタグを作り、本文のハッシュをリクエストの受け口に記録する。`report-only` の報告の受け口は持たない（ブラウザーのコンソールで確かめる）。
- 未決: `strict_csp` の既定を `on` にする時期。
