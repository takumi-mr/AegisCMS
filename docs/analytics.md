# アクセス解析

公開サイトのページビューと訪問者数を、Cookie を使わずに数えます（`[analytics]`）。Admin Studio の **Analytics** タブで、日ごとの推移・よく見られたページ・参照元・端末の種類を見られます。Google Analytics のような外部サービスに閲覧者のデータを渡さず、Cookie の同意バナーも要りません。

機能はクレート `aegis-analytics` にあり、サーバーの Cargo の feature `analytics`（既定で組み込み。動かすのは `enabled = true` のときだけ）で組み込みます。

```text
 公開ページ（<head> の小さなスクリプト）
        │  POST /api/v1/analytics/event  {"p": "/articles/hello", "r": "https://www.google.com/"}
        │  （Do Not Track・Global Privacy Control が有効なブラウザは送らない）
        ▼
 クローラーを除く → その日の訪問者を見分ける（ハッシュ。キャッシュにだけ置き、2 日で消える）
        │
        ▼
 メモリで日ごとに集計 ──(30 秒ごと・終了時)──▶ _aegis_analytics（日 × ページ・参照元・端末ごとの数だけ）
```

---

## 1. 有効にする

```toml
[analytics]
enabled = true
retention_days = 400        # 集計を残す日数（0 で消さない）
flush_interval_secs = 30    # 集計を DB に書く間隔（秒）
exclude_paths = []          # 数えないパス（前方一致。例: ["/preview"]）
```

有効にすると、テーマで描くすべての公開ページの `<head>`（`extra_head`）にビーコンが入ります。テーマ側の作業はありません。

## 2. 何を数えるか

| 項目 | 内容 |
| --- | --- |
| ページビュー | ページが表示された回数 |
| 訪問者 | その日のユニークな訪問者。日をまたいだ同じ人は別に数えます（期間の訪問者数は日ごとの合計） |
| よく見られたページ | パスごとのページビューと訪問者（クエリ文字列・`#` 以降は除く） |
| 参照元 | ほかのサイトから来たページ表示の、参照元のホスト名（`www.` は除く）。「直接・参照元なし」はブックマーク・アプリ・参照元を送らないサイトから。サイトの中での移動は数えません |
| 端末 | User-Agent から判定した `desktop` / `mobile` / `tablet` |

数えないもの:

- クローラー・監視サービス・コマンドラインのツール（User-Agent で判定）と、自動操作のブラウザ（`navigator.webdriver`）
- Do Not Track（`DNT: 1`）・Global Privacy Control（`Sec-GPC: 1`）を有効にしたブラウザ
- JavaScript を動かさないアクセス
- `exclude_paths` のパス

## 3. プライバシー

- Cookie・localStorage は使いません。閲覧者のブラウザに何も残しません。
- 訪問者は「その日だけの秘密の値 ＋ IP アドレス ＋ User-Agent ＋ ホスト名」の SHA-256 で見分けます。秘密の値は毎日作り直してキャッシュ（`[cache]`。Redis なら全サーバーで共有）にだけ置くので、日をまたぐと同じ人とは分かりません。
- IP アドレス・User-Agent・ハッシュは DB に保存しません。DB にあるのは日ごとの集計の数だけです。
- 閲覧者の情報を外部のサービスに送りません。

プライバシーポリシーには、たとえば「Cookie を使わないアクセス解析で、ページの表示回数を集計しています。個人を特定する情報は保存していません」と書けます。

## 4. Admin Studio

**Analytics** タブ（`[analytics].enabled = true` のときだけ表示。viewer 以上）で、過去 7 / 30 / 90 日の集計を見られます。

- ページビューと訪問者の合計
- 日ごとの推移のグラフ（矢印キーで日ごとの値を読めます。表でも見られます）
- よく見られたページ・参照元・端末（上位 10 件）

日付は UTC で区切ります。

## 5. API

`GET /api/v1/analytics/summary?from=2026-10-01&to=2026-10-31&limit=10`（viewer 以上）

| パラメータ | 内容 |
| --- | --- |
| `from` / `to` | 期間（`YYYY-MM-DD`、UTC、両端を含む。最大 366 日）。省略時は今日までの 30 日 |
| `limit` | ページ・参照元・端末の上位の件数（既定 10、最大 100） |

```json
{
  "from": "2026-10-01",
  "to": "2026-10-31",
  "pageviews": 1234,
  "visitors": 456,
  "daily": [{ "day": "2026-10-01", "pageviews": 40, "visitors": 15 }],
  "top_pages": [{ "value": "/", "pageviews": 300, "visitors": 120 }],
  "top_referrers": [{ "value": "google.com", "pageviews": 80, "visitors": 70 }],
  "devices": [{ "value": "mobile", "pageviews": 600, "visitors": 250 }]
}
```

ビーコンの受け口 `POST /api/v1/analytics/event` は、本文 `{"p": パス, "r": 参照元}`（4KB まで）を受け取り、数えたかどうかにかかわらず `204` を返します。
