[← ドキュメント一覧に戻る](README.md)

# Cookie の同意バナー

広告・アクセス解析・外部の動画や地図など、訪問者の同意が要る Cookie やスクリプトを使うサイトのために、同意を聞くバナーを出します(`[consent]`)。訪問者の選択を first-party の Cookie に残し、**同意が要るスクリプトや iframe を、同意があるまで実行しません**。

AegisCMS 自体は、サイトを動かすための Cookie(管理画面のセッションなど)しか使いません。アクセス解析(`[analytics]`)も Cookie を使いません。このバナーは、**あなたが足す**広告・解析・埋め込みの同意を管理するためのものです。

バナーの **HTML と文言はテーマのテンプレート `consent.html`** で決まります(テーマが持たなければ組み込みのもの)。**表示のしかた**(形・位置・背景・遅れ)は設定で変えられます。

```text
 公開ページに、サーバーが差し込む(テーマに関係なく、404 ページにも付く)
   <head> ─ <link>   /_aegis/consent.css       組み込みの consent.html 用のスタイル
          ─ <script type="application/json" id="aegis-consent-config">  動きの設定(実行されないデータ)
          ─ <script src="/_aegis/consent.js" defer>
   </body> の前 ─ バナーの HTML      テーマの templates/consent.html(無ければ組み込み)を描いたもの
        │
        ├─ 選択が無い／区分が変わった ──▶ バナーを表示（すべて拒否 / 選択する / すべて許可）
        └─ 選択がある ──▶ 許可した区分のスクリプト・iframe を実行（data-aegis-consent）
```

---

## 1. 有効にする

```toml
[consent]
enabled = true
policy_url = "/privacy"        # プライバシーポリシーへのリンク（任意）
```

サーバーを再起動すると、すべての公開ページにバナーが出ます。文言は `[theme].locale`(`en` / `ja`)の言語で、区分を決めないときは `analytics`(アクセス解析)と `marketing`(マーケティング)の 2 つです。

```toml
[consent]
enabled = true
layout = "box"                 # banner（画面の端の帯）/ box（隅のカード）/ modal（中央。背景を暗くする）
position = "bottom-left"       # 形ごとの位置。省略すると形の既定
backdrop = false               # 背景を暗くしてページを操作できなくする。省略時は modal だけ true
delay_ms = 1500                # 表示を遅らせる（ミリ秒）
cookie_days = 180              # 選択を残す日数（1〜395）
policy_url = "/privacy"
message = ""                   # 説明文を変えるとき（空ならテーマの既定の文）
reopen_button = true           # 選択したあとの「Cookie の設定」ボタン
revision = 1                   # 全員にもう一度聞きたいときに増やす

[[consent.categories]]
id = "analytics"               # 名前と説明は組み込みのものを使う
[[consent.categories]]
id = "marketing"
[[consent.categories]]
id = "video"                   # 組み込みの名前が無い区分は label が要る
label = "埋め込みの動画"
description = "YouTube などの動画を表示します。"
```

| キー | 型 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `enabled` | bool | `false` | `true` でバナーを出す |
| `layout` | string | `"banner"` | バナーの形。`banner` / `box` / `modal` |
| `position` | string | 形の既定 | 位置。`banner`: `bottom`(既定)/ `top`、`box`: `bottom-right`(既定)/ `bottom-left` / `top-right` / `top-left`、`modal`: `center` |
| `backdrop` | bool | `modal` だけ `true` | 背景を暗くして、ページを操作できなくする |
| `delay_ms` | integer | `0` | 表示を遅らせる時間(0〜10000) |
| `cookie_days` | integer | `180` | 選択を残す日数(1〜395) |
| `policy_url` | string | `""` | プライバシーポリシーの URL(`/` で始まるパスか `http(s)://`)。空ならリンクを出さない |
| `message` | string | `""` | 説明文(600 文字まで)。空ならテーマの既定の文 |
| `reopen_button` | bool | `true` | 選択したあとに出す、設定を開き直す小さなボタン |
| `revision` | integer | `1` | 増やすと、保存済みの選択が無効になって、もう一度聞く |
| `categories` | table[] | `analytics` と `marketing` | 同意の対象にする区分(10 個まで) |

- 区分の `id` は英小文字で始まり、英小文字・数字・`-`・`_`(32 文字以内)。`necessary`(必須)は常にオンで、定義できません。
- `analytics`・`marketing`・`preferences`・`functional` には、テーマの言語の名前と説明が付きます。ほかの `id` は `label`(60 文字まで)が必要です。`description` は 300 文字までです。
- **区分の `id` の並びか `revision` が変わると、全員にもう一度聞きます**(名前・説明・形を変えただけでは聞き直しません)。
- 値が不正なときはサーバーが起動しません(位置が形に合わない、など)。

## 2. 同意が要るスクリプトを書く

テーマのテンプレートや、プラグインが `extra_head` / 本文に出す HTML で、同意が要るものを**実行しない形**で書きます。同意があると、スクリプトが実行され、iframe に `src` が入ります。

```html
<!-- type を text/plain にして、data-aegis-consent に区分を書く -->
<script type="text/plain" data-aegis-consent="analytics"
        src="https://analytics.example.com/tag.js" async></script>

<script type="text/plain" data-aegis-consent="marketing">
  window.adQueue = window.adQueue || [];
</script>

<!-- iframe は src ではなく data-src に書く -->
<iframe data-aegis-consent="video" data-src="https://www.youtube-nocookie.com/embed/ID"
        width="560" height="315" title="動画"></iframe>
```

- 同意が**ない間は何も実行・読み込みされません**。バナーのスクリプトが読み込めなくても(ブロックされても)、テーマに `consent.html` が壊れていても、同意が要るものは動きません。
- 属性(`src`・`async`・`data-*` など)は、実行するときに引き継ぎます。インラインのスクリプトはそのまま実行します。
- **許可を取り消したとき**(設定を開き直して、許可していた区分をオフにしたとき)は、すでに動いたスクリプトを止められないので、ページを読み込み直します。
- プラグインから出すときも同じです。`on-render-page` で `extra_head` や `content_html` に、上の形の HTML を足します。同意の仕組みをプラグインが知る必要はありません。

### 2.1 JavaScript から

```js
window.AegisConsent.has('marketing');        // 許可されているか
window.AegisConsent.get();                   // { analytics: true, marketing: false }。まだ選んでいなければ null
window.AegisConsent.onChange((choices) => {  // 選択が分かったとき・変わったときに呼ぶ（分かっていれば今すぐ 1 回）
  if (choices.analytics) startTracking();
});
document.addEventListener('aegis:consent', (e) => console.log(e.detail.categories));
window.AegisConsent.open();                  // 設定を開く
```

### 2.2 設定を開き直すリンク

フッターなどに、`data-aegis-consent-open` を付けたリンクかボタンを置くと、クリックで設定が開きます。

```html
<a href="#" data-aegis-consent-open>Cookie の設定</a>
```

## 3. テーマでバナーを作る(`templates/consent.html`)

テーマに `templates/consent.html` を置くと、バナーの HTML・文言・見た目を丸ごと決められます。組み込みのテンプレートは `base.html` を継承しない**断片**で、サーバーが各ページの `</body>` の前に差し込みます(`base.html` を `extends` しないでください)。

### 3.1 テンプレートに渡る値

| 値 | 内容 |
| --- | --- |
| `fields.consent.layout` | `banner` / `box` / `modal` |
| `fields.consent.position` | 位置(`bottom`・`top`・`bottom-right`・`bottom-left`・`top-right`・`top-left`・`center`) |
| `fields.consent.backdrop` | 背景を暗くするか(`true` / `false`) |
| `fields.consent.policy_url` | プライバシーポリシーの URL(無ければ空) |
| `fields.consent.reopen_button` | 設定を開き直すボタンを出すか |
| `fields.consent.delay_ms` / `cookie_days` | 表示の遅れ・選択を残す日数 |
| `fields.message` | `[consent].message`(空なら、テーマの `t("consent.message")` など自分の文を使う) |
| `fields.categories` | 区分の一覧。`id`・`label`・`description`(組み込みの区分は、名前を決めていなければ空) |
| `fields.preview` | テーマ開発サーバーのプレビューのとき `true`(最初から表示する) |
| `site_title`, `locale`, `base_path`, `t("...")` | ほかのテンプレートと同じ(`t("consent.title")` などの組み込みの文言も使える) |

### 3.2 スクリプトと結びつける属性

スクリプトは、次の属性で要素を見つけて動かします。**見た目や構造は自由**ですが、これらの属性は残してください。

| 属性 | 役目 |
| --- | --- |
| `data-aegis-consent-root` | ダイアログ全体。**最初は `hidden`**(スクリプトが表示する)。開いている間 `data-aegis-consent-state="ask"`(最初の質問)または `"details"`(区分の選択)になる |
| `data-aegis-consent-action="reject-all"` / `"accept-all"` | すべて拒否 / すべて許可して閉じる。**最初のフォーカスは `reject-all`** に入る |
| `data-aegis-consent-action="customize"` | 区分の選択を開く(開くと自分は隠れる) |
| `data-aegis-consent-details` | 区分のチェックボックスを入れる部分。**最初は `hidden`** |
| `input[data-aegis-consent-category="<区分の ID>"]` | 区分ごとのチェックボックス(スクリプトが今の選択で埋める) |
| `data-aegis-consent-action="save"` | 選択した区分で保存して閉じる。**最初は `hidden`** |
| `data-aegis-consent-reopen` | 選択したあとに出る、設定を開き直すボタン。**最初は `hidden`** |

- 「選択する」(`customize` / `details` / `save`)を持たない、2 つのボタンだけのバナーも作れます。その場合、`accept-all` は全区分の許可、`reject-all` は全区分の拒否になります。
- `layout` が `modal` のときは、スクリプトが Tab キーのフォーカスをダイアログの中に閉じ込めます(`role="dialog"`、`aria-modal="true"` と、見出し・説明への `aria-labelledby` / `aria-describedby` を付けてください)。
- Escape で閉じられるのは、選択が済んでいるときだけです(最初の質問は、答えるまで閉じません)。
- `data-aegis-consent-root` が無いテンプレートでは、バナーは出ません(同意が要るものは動かないままです。ブラウザーのコンソールに警告が出ます)。

### 3.3 例: 2 つのボタンだけのバナー

```html
<aside data-aegis-consent-root hidden class="cookie-notice cookie-notice--{{ fields.consent.layout }}"
       role="dialog" aria-labelledby="cn-title">
  <p id="cn-title">{{ fields.message or "このサイトでは Cookie を使います。" }}</p>
  <button type="button" data-aegis-consent-action="reject-all">使わない</button>
  <button type="button" data-aegis-consent-action="accept-all">許可する</button>
</aside>
```

### 3.4 見た目

組み込みのテンプレートのスタイルは `/_aegis/consent.css` で、すべて詳細度 0 の `:where()` です。テーマの CSS で、`.aegis-consent-*` を指定すれば上書きできます。自分のテンプレートを作るときは、このファイルのクラスを使わず、テーマの CSS に書いてかまいません。

| クラス | 部分 |
| --- | --- |
| `.aegis-consent` / `.aegis-consent-layout-<形>` / `.aegis-consent-pos-<位置>` / `.aegis-consent-backdrop` | 外枠(形・位置・背景) |
| `.aegis-consent-body` | 本体のカード |
| `.aegis-consent-title` / `.aegis-consent-message` / `.aegis-consent-policy` | 見出し・説明・ポリシーへのリンク |
| `.aegis-consent-details` / `.aegis-consent-category` | 区分の一覧・1 つの区分 |
| `.aegis-consent-actions` / `.aegis-consent-button` | ボタンの並び・ボタン(拒否と許可は同じ見た目) |
| `.aegis-consent-reopen` | 選択したあとの「Cookie の設定」ボタン |

### 3.5 作りながら確かめる

`aegis-server theme dev`(と Theme Builder)では、`templates/consent.html` を `consent` のサンプルとして表示します。バナーは最初から表示され(`fields.preview`)、組み込みのスタイルが付きます。サンプルは `samples/consent.json` で変えられます(`{"fields": {...}}` と書きます。`fields` ごと置き換わるので、使う値はすべて書いてください)。実際の動き(ボタンの働き)は、サイトに差し込まれたときにスクリプトが付けるので、プレビューでは見た目の確認までです。

## 4. 仕組みと注意

- **CSP に強い**: スクリプトと CSS は外部ファイル(`/_aegis/consent.js` / `consent.css`)、設定は実行しない JSON(`type="application/json"`)なので、`script-src 'self'` だけの CSP でも動きます。`[server].base_path` がある場合は URL に付きます。
- **差し込み**: `text/html` の `GET` 応答の `</head>` の前と、最後の `</body>` の前に足します。API(`/api/`)、Admin Studio、アップロード、`/_aegis/` は触りません。`[consent].enabled = false` なら何もしません(本文も読みません)。`consent.html` を描けないとき(テンプレートの誤り)は、バナーだけを出さず、ページはそのまま返します(ログにエラーを出します)。
- **選択の保存**: Cookie `aegis_consent`(`Path=/`、`SameSite=Lax`、HTTPS では `Secure`、`cookie_days` 日)。値は `{"r": 版, "c": {区分: true/false}}` です。サーバーには送るだけで、保存や記録はしません(同意の記録が法的に必要な場合は、別途用意してください)。
- **Global Privacy Control**: 初期状態は常に「すべてオフ」で、勝手に許可することはありません。
- **設定を変えたとき**: サーバーを再起動してください。`[cdn]` でページをキャッシュしているときは、差し込んだ設定(JSON)とバナーの HTML もキャッシュされているので、再起動のあとに CDN のキャッシュをパージしてください。
- 設定は設定ファイルだけです(Admin Studio には画面がありません)。
- 法的な要件(対象の国、記録、Cookie の一覧の開示など)は、サイトごとに確かめてください。このバナーは、「同意の前に実行しない」「拒否が許可と同じくらい簡単」「いつでも変えられる」を満たすための部品です。
