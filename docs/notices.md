[← ドキュメント一覧に戻る](README.md)

# お知らせバナー

メンテナンスやセールの告知を、期間を決めて公開ページの上部に出します。Admin Studio の **設定 › お知らせ** で、メッセージ・リンク・種類・表示期間・閉じるボタンの有無を編集します(設定ファイルは要りません)。

```text
 公開ページに、サーバーが差し込む(テーマに関係なく、404 ページにも付く)
   <head> ─ <link>   /_aegis/notices.css    組み込みの notices.html 用のスタイル
          ─ <script src="/_aegis/notices.js" defer>
   <body> の直後 ─ お知らせの HTML   テーマの templates/notices.html(無ければ組み込み)を描いたもの
```

表示中のお知らせが無いページには、何も差し込みません(CSS・スクリプトも読み込みません)。

---

## 1. お知らせの項目

| 項目 | 説明 |
| --- | --- |
| メッセージ | 1 行、500 文字まで(HTML は使えません。出力時にエスケープします) |
| リンク / リンクの文字 | 任意。`/` で始まるパスか `http(s)://`。文字を省略すると「詳しく見る」(`[theme].locale` の言語) |
| 種類 | `info` / `success` / `warning` / `alert`。`warning`・`alert` は `role="alert"`、ほかは `role="status"` |
| 表示期間 | 開始(含む)と終了(含まない)。どちらも省略でき、省略した側は期限なし。画面の入力はブラウザーのローカル時間で、サーバーには UTC で渡る |
| 訪問者が閉じられる | `true` で閉じるボタンを出す |
| 有効 | `false` で期間内でも出さない(下書き) |

- 最大 20 件まで登録でき、**同時に出るのは作成順で先頭の 3 件**です。
- 状態は **表示中 / 予定 / 終了 / 無効** で、期間は表示のたびに判定します(期限が来ても再起動や操作は要りません。ただしキャッシュ済みのページは、キャッシュが切れるまで古いままです)。
- 訪問者が閉じたお知らせは `localStorage`(`aegis_notices`)に版つきで覚え、**お知らせを編集すると版が変わって、もう一度出ます**。ストレージが使えない環境では、そのページを見ている間だけ閉じます。JavaScript が無効なら閉じるボタンは出ず、お知らせは出たままです。

## 2. API

すべて `admin` ロールが必要です。エラーの `error` は英語、`code` は `invalid_notice`(`field` に不正な項目)/ `too_many_notices` / `notice_not_found` です。

| メソッド | パス | 説明 |
| --- | --- | --- |
| `GET` | `/api/v1/notices` | 一覧(`status` つき) |
| `POST` | `/api/v1/notices` | 追加(201) |
| `PUT` | `/api/v1/notices/{id}` | 変更 |
| `DELETE` | `/api/v1/notices/{id}` | 削除(204) |

保存はサイト設定(`_aegis_settings` の `notices.items`)に JSON で持つので、DB のテーブルは増えず、バックアップにも含まれます。変更は監査ログ(`notice.create` / `notice.update` / `notice.delete`)に残り、クラスターの他ノードと CDN のキャッシュに反映されます。

## 3. テーマでお知らせを作る(`templates/notices.html`)

テーマが `templates/notices.html` を持つと、組み込みの代わりに使います(`base.html` を継承しない断片です)。`fields.notices` の各要素が次の値を持ちます。

| 値 | 説明 |
| --- | --- |
| `id` / `rev` | お知らせの ID と版(閉じた記録に使う) |
| `message` / `link_url` / `link_label` | 本文とリンク(`link_label` は空のことがある) |
| `style` | `info` / `success` / `warning` / `alert` |
| `dismissible` | 閉じられるか |

スクリプトは次の属性で要素を見つけます。**これを残せば構造も見た目も自由**です。

| 属性 | 場所 |
| --- | --- |
| `data-aegis-notice="{{ notice.id }}"` と `data-aegis-notice-rev="{{ notice.rev }}"` | 1 件分の要素 |
| `data-aegis-notice-close` | 閉じるボタン(`hidden` を付けておく。スクリプトが表示する) |

閉じるボタンの無い要素は常に残ります。最後の 1 件が閉じられると、親の空の要素も取り除きます。`t("notice.label")` / `t("notice.more")` / `t("notice.close")` で文言を引けます。組み込みの CSS(`:where()` で詳細度ゼロ)は、テーマの CSS で簡単に上書きできます。

テーマ開発サーバー(`aegis-server theme dev`)は、サンプルのお知らせで `notices.html` をプレビューします。
