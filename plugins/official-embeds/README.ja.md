# official-embeds

[English](README.md) | **日本語**

外部のサービスのページを、本文に `<iframe>` で埋め込むブロックを足す公式プラグインです。動画（YouTube・Vimeo）・地図（OpenStreetMap・Google マップ）・CodePen・Google カレンダー・Google フォームに対応します（[`render-blocks` フック](../../docs/plugin_hooks.ja.md#110-render-blocks)と[外部のページの埋め込み](../../docs/plugin_hooks.ja.md#32-外部のページの埋め込み-capabilitiesembed)の実例）。

外部のサイトに依存するブロックは、このプラグインにまとめています。外部に依存しないブロック（ボタン・カード・ギャラリーなど）は [`official-blocks`](../official-blocks/) にあります。埋め込みを使わないサイトは、このプラグインを有効にしなければ、CSP にどのホストも足されません。

| ブロック                          | 入力欄                                                                                                | 表示                                |
| --------------------------------- | ----------------------------------------------------------------------------------------------------- | ----------------------------------- |
| `official-embeds/video`           | 動画の URL（必須。YouTube・Vimeo）・タイトル（読み上げ用）・形（16:9 / 4:3 / 9:16）                   | 動画のプレーヤー                    |
| `official-embeds/openstreetmap`   | 緯度（必須）・経度（必須）・拡大率（1〜19、既定 15）・場所の名前                                      | マーカーつきの OpenStreetMap の地図 |
| `official-embeds/codepen`         | Pen の URL（必須）・最初に開くタブ・高さ（200〜1200px、既定 400）・クリックで実行・タイトル           | CodePen の Pen                      |
| `official-embeds/google-map`      | 埋め込みのコード（必須）・形（16:10 / 4:3 / 1:1）・場所の名前                                         | Google マップの地図                 |
| `official-embeds/google-calendar` | カレンダー（必須。ID か埋め込みのコード）・表示（月 / 週 / 予定リスト）・タイムゾーン・高さ・タイトル | Google カレンダー                   |
| `official-embeds/google-form`     | フォームの URL（必須。URL か埋め込みのコード）・高さ（300〜3000px、既定 800）・タイトル               | Google フォーム                     |

## 共通の動き

- **入力された URL や HTML は、そのまま出しません。** 動画の ID・Pen の ID・カレンダー ID など、必要な値だけを取り出して検証し、`<iframe>` の `src` はこのプラグインが組み立てます。サービスが出す埋め込みのコード（`<iframe ...>`）を貼った場合も、使うのは `src` の中の値だけです（`style` などの属性は捨てます）。
- 読めない URL・形の違う値のブロックは表示しません。
- 埋め込めるのはマニフェストの `capabilities.embed.frame_domains` に書いたホストだけです。有効にしている間、このホストは CSP の `frame-src` に自動で足されます。

| サービス          | `frame-src` に足すホスト   |
| ----------------- | -------------------------- |
| YouTube           | `www.youtube-nocookie.com` |
| Vimeo             | `player.vimeo.com`         |
| OpenStreetMap     | `www.openstreetmap.org`    |
| CodePen           | `codepen.io`               |
| Google マップ     | `www.google.com`           |
| Google カレンダー | `calendar.google.com`      |
| Google フォーム   | `docs.google.com`          |

- CSP はホスト単位で効きます。たとえば `www.google.com` を足すと、google.com のほかのページもフレームに入れられるようになります（本文に入るのは、このプラグインが組み立てた `/maps/embed` だけです）。
- **プライバシー**: 埋め込みを表示すると、閲覧者の IP アドレスなどがそのサービスに送られ、サービスによっては Cookie が使われます。プライバシーポリシーに、使う外部のサービスを書いてください。Cookie の同意（`[consent]`）を待ってから読み込む仕組みには、まだ対応していません。
- サーバーから外部への通信や KVS は使いません（埋め込みは閲覧者のブラウザーが読み込みます）。

## 動画（YouTube・Vimeo）

- YouTube は `youtube.com/watch?v=`・`youtu.be/`・`youtube.com/shorts/`・`youtube.com/embed/`・`youtube.com/live/` の URL を読み、再生開始位置（`t=90`・`t=1m30s`・`start=90`）も使います。Vimeo は `vimeo.com/<ID>`（限定公開の `vimeo.com/<ID>/<ハッシュ>`）と `player.vimeo.com/video/<ID>`。
- YouTube はプライバシー強化モード（`www.youtube-nocookie.com`）、Vimeo は `dnt=1`（視聴の記録をしない）で埋め込みます。

## 地図（OpenStreetMap）

- API キーは要りません。住所からの検索はせず、緯度・経度で指定します（openstreetmap.org で場所を右クリックすると座標をコピーできます）。
- 緯度は -85〜85、経度は -180〜180 の範囲外だとブロックを表示しません。地図の下に「大きな地図で見る」リンクと著作権表示（© OpenStreetMap）を出します。

## CodePen

- `codepen.io/<ユーザー>/pen/<ID>` の URL（`/full/`・`/details/`・`/embed/` も）か、CodePen の「Embed」で出る埋め込みのコードを貼ります。
- 「最初に開くタブ」で、実行結果だけか、HTML・CSS・JS のどれかと実行結果を並べるかを選びます。
- 「クリックで実行」にすると、プレビューを出し、訪問者がクリックしたときだけ Pen を動かします（重い Pen やページに複数置くとき向け）。
- 非公開（Private）の Pen は、CodePen の有料プランでないと埋め込めません。

## Google マップ

- Google マップで場所を開き、「共有」›「地図を埋め込む」›「HTML をコピー」で出るコードを貼ります（`https://www.google.com/maps/embed?pb=...` の URL だけでも可）。API キーは要りません。
- 使うのは URL の `pb` の値だけです。`www.google.com/maps/embed` 以外の URL（地図のページの URL・短縮 URL など）は読みません。
- 場所の名前を入れると、地図の下に出し、フレームの読み上げにも使います。

## Google カレンダー

- カレンダーは**一般公開**にしておく必要があります（Google カレンダーの設定 ›「アクセス権限」›「一般公開して誰でも利用できるようにする」）。公開していないと、閲覧者には中身が見えません。
- 入力欄には、カレンダー ID（`xxxx@group.calendar.google.com`・`xxxx@gmail.com`）か、設定の「カレンダーの統合」にある埋め込みのコード・URL を貼ります。埋め込みのコードに複数のカレンダーがあれば、そのまま並べます（10 件まで）。
- タイムゾーン（`Asia/Tokyo` など）を空欄にすると、埋め込みのコードにあるもの、無ければカレンダーのものを使います。
- 埋め込みのコードの色・表示する項目などの細かい指定は引き継ぎません（カレンダー・タイムゾーン・表示だけ）。

## Google フォーム

- フォームの「送信」›「<>」（HTML を埋め込む）のコードか、フォームの URL（`docs.google.com/forms/d/e/<ID>/viewform`）を貼ります。
- 短縮 URL（`forms.gle/...`）は、開かないと ID が分からないため使えません。ブラウザーで開いて、長い URL をコピーしてください。
- フォームの高さは自動では変わりません。質問の数に合わせて「高さ」を調整してください（足りないとフォームの中にスクロールが出ます）。

## 使い方

設定ファイルの `[plugins].enabled` に `"official-embeds"` を足すか、Admin Studio の **Settings › Plugins** で有効にします。エディタのブロックの追加ボタンに「動画」「Google マップ」などが出ます。

```toml
[plugins]
enabled = ["official-embeds"]
```

- 配置・余白は詳細度 0 の CSS（`:where()`）です。テーマで変えるときは `.oe-frame`・`.oe-video`・`.oe-map` などのクラスを指定します。

## ビルド

```bash
task plugin:build:embeds   # plugin.wasm を作り直す（要 rustup target add wasm32-unknown-unknown）
cargo test -p aegis-plugin-embeds
```
