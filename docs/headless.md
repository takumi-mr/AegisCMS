[← ドキュメント一覧に戻る](README.md)

# ヘッドレス配信 API

認証なしで、**公開済みのコンテンツだけ**を JSON で返す読み取り専用の API です。AegisCMS のテーマで描画する代わりに、別のサイト（Next.js・Astro など）やアプリがコンテンツを取りに来られるようにします。

返すデータの形は**スキーマ（`schemas/*.yaml`）で決まります**。スキーマを書くと、編集画面・編集用 API・型・OpenAPI が揃うのと同じ流れで、配信用 API も揃います。

## 使い方

既定は無効です。設定ファイルの `[headless]` で有効にします（設定の一覧は [設定リファレンス 2.25](configuration.md#225-headless--ヘッドレス配信-api)）。

```toml
[headless]
enabled = true
content_types = ["article", "page"]   # 配信する型（既定）
```

```bash
# 公開済みの一覧（新しい順。続きは next_cursor を cursor に渡す）
curl "http://localhost:8080/api/v1/public/contents/article?limit=10"
# 1 件（本文のブロックつき。?include=html で描画済みの HTML も）
curl "http://localhost:8080/api/v1/public/contents/article/hello-world?include=html"
# 配信する型・フィールドだけの OpenAPI
curl "http://localhost:8080/api/v1/public/openapi.json"
```

内部向けのフィールドは、スキーマで `public: false` と書くと出なくなります。

```yaml
fields:
  - name: internal_note
    type: string
    public: false
```

---

## 1. 位置づけと範囲

- AegisCMS の主軸は、テーマで描画する従来型のまま変えません。ヘッドレス配信は、使う人だけが有効にする追加の入口です（既定は無効）。
- 汎用のバックエンドではありません。**読み取りだけ**で、エンドユーザーが任意のデータを書き込む API・行ごとの権限・リアルタイムは作りません。コメント・問い合わせ・注文など、書き込みを伴う機能は今までどおり各機能の入口を使います。
- 編集用の `/api/v1/contents/...`（Bearer トークンが必要。下書きも返す。[記事・コンテンツ投稿 API](content-api.md)）とは別の入口にします。理由は、公開用に流用すると、下書きを返さないことが実装の書き方に依存してしまうためです。

### やらないこと（この設計の範囲外）

- 下書き・プレビューの取得（API キーが要るので、別の設計にする）
- 検索 API（サイト内検索の `GET /api/v1/search` が既にある）
- GraphQL
- 書き込み系の API

---

## 2. 構成

```text
 クライアント ──▶ サーバー（認証なしのルート・レート制限・CORS・キャッシュのヘッダー）
                    │
                    ▼
              aegis-headless（新しいクレート）
              絞り込みの引数の検証 / スキーマに従う整形 / カーソル / ETag
                    │  使えるのは「公開版の読み取り」だけ
                    ▼
              aegis-db（get_published_content / list_published_contents_page）
```

### 2.1 クレート `aegis-headless`

検索（`aegis-search`）と同じ作り方にします。サーバーの Cargo の feature `headless` で組み込みます。

- **依存**: `aegis-core`・`aegis-schema` と、DB の読み取りの入口だけです。サーバーの内部状態（`AppState`）には依存しません。
- **DB の入口を絞る**: クレートは `dyn DatabaseAdapter` を受け取らず、公開版の取得だけを持つ小さなトレイト（仮に `PublishedSource`）を受け取ります。サーバーがアダプターに対して実装します。編集用のメソッドが型として見えないので、下書きを返す処理を書けません。
- **持つもの（純粋なロジック）**:
  - 一覧の引数（`limit`・`cursor`・`category`・`tag`）の検証と、上限の丸め
  - スキーマに従ったレコードの整形（公開するフィールドだけを選ぶ）
  - カーソルの符号化・復号（改ざんされたら 400）
  - `ETag` の計算
- **サーバー側の担当**: ルートの登録、レート制限、CORS、`Cache-Control`、CDN のキャッシュタグ、設定の読み込み。他のクレート（commerce・comments）と同じ分担です。

### 2.2 既存の仕組みの再利用（新しい問い合わせは足さない）

| 用途 | 使うもの | 補足 |
|---|---|---|
| 1 件 | `get_published_content(schema, slug_or_id)` | |
| 一覧 | `list_published_contents_page(schema, &ContentListQuery)` | `status`・`limit`/`offset`・キーセット（`cursor_*`）・`tag_id`・`category_id` の絞り込みが対応。公開の記録の列と索引で SQL に任せているので、記事数に依らない |
| リレーションの解決 | `resolve_published_relations` | 公開版の記録（`_aegis_content_publications`）から**公開中のものだけ**を `id` / `slug` で引く。編集用の `resolve_relations` はコンテンツ本体のテーブルを引き、未公開のタイトルや下書きも返すので使わない。値が空のフィールドは問い合わせない |
| キャッシュの消去 | `cdn::content_invalidation_tags`・`purge_content` | 公開ページと同じタグを付ければ、更新時に同じ仕組みで消える |

公開版の一覧は、追加の問い合わせを増やさない範囲だけを使います。**追加フィールドでの絞り込みや部分一致は対応しません**（先に検討して見送った項目と同じ理由です）。

---

## 3. エンドポイント

すべて `GET`、認証なしです。

| パス | 内容 |
|---|---|
| `/api/v1/public/contents/{type}` | 公開済みの一覧 |
| `/api/v1/public/contents/{type}/{slug}` | 公開済みの 1 件 |
| `/api/v1/public/openapi.json` | 公開する型だけの OpenAPI |

### 3.1 一覧のクエリ

| 引数 | 内容 |
|---|---|
| `limit` | 1 ページの件数。既定 20、上限 `max_page_size`（既定 100） |
| `cursor` | 前のページの `next_cursor`。中身は不透明（クライアントは解釈しない） |
| `category` | カテゴリの ID。未分類は `none` |
| `tag` | タグの slug |

### 3.2 応答の形

一覧:

```json
{
  "items": [ { "...": "1 件の形" } ],
  "next_cursor": "opaque-string-or-null",
  "total": 123
}
```

1 件（`items` の要素、および単体の応答）:

```json
{
  "id": "…",
  "content_type": "article",
  "title": "…",
  "slug": "…",
  "excerpt": "…",
  "published_at": "2026-11-05T00:00:00Z",
  "updated_at": "2026-11-05T00:00:00Z",
  "fields": { "excerpt": "…", "custom_meta": { } },
  "relations": { "category_id": { "id": "…", "title": "…", "slug": "…", "content_type": "category" } },
  "blocks": [ ]
}
```

- 一覧の要素は要約（`blocks` なし）、単体は `blocks` を含めます（編集用 API の `view=summary` と同じ分け方）。
- **返さない標準の項目**: `author_id`（内部のユーザー ID）、`status`、`unpublish_at`、`comments_open`、`rendered_html`（§6 の 2）。
- `fields` の値は、編集用 API と同じ形です（jsonb はオブジェクト、リレーションは `relations` に解決済み）。

---

## 4. スキーマで決まること

### 4.1 どの型を公開するか

設定の許可リストで決めます。**既定は記事と固定ページだけ**です。商品の在庫など、内部向けの値を持つ型は、明示しない限り出ません。許可リストにない型は `404` です（存在しない型と区別しない）。

### 4.2 どのフィールドを返すか

フィールドに公開の可否を持たせます。

```yaml
fields:
  - name: internal_note
    type: string
    public: false      # 配信 API に出さない
```

- `public` の既定は「公開」です（§6 の 1）。非公開にしたいフィールドにだけ `public: false` を書きます。
- 標準の項目（タイトル・スラッグ・日時・本文）は、上の §3.2 の決まりどおりで、スキーマで変えられません。
- `category_id`（組み込みのフィールド）は常に公開します。

### 4.3 OpenAPI と型

既存の `OpenApiGenerator`・`TypeScriptGenerator` を使い、公開する型・フィールドだけの定義を `GET /api/v1/public/openapi.json` で配信します（稼働中の設定が反映される）。

---

## 5. 安全性

- **公開済みだけを返す**: DB の入口が公開版（`published` のスナップショット）に限られます。下書き・レビュー待ち・予約中・アーカイブは取れません。
- **期限切れを返さない**: 公開ページと同じく、リクエスト時に `unpublish_at` を確かめます（スケジューラのアーカイブ化を待たない）。1 件は `404`、一覧は結果から除きます（除いてもカーソルは途切れない）。読めない日時は公開していないものとして扱います。一覧の `total` は DB の件数なので、アーカイブ化まで（最大 30 秒）は期限切れの分を含みます。
- **関連コンテンツは公開版だけ**: `relations` の参照先は公開版の記録から引き、下書き・編集中のタイトル・予約中は出ません。参照先の型が公開する型に入っているかは問いません（返すのは `id`・`content_type`・`title`・`slug` だけで、公開済みならサイトで見える情報です）。
- **本文のブロックは保存されたまま返す**: `core/html` などのブロックは、保存された HTML をそのまま含みます（サニタイズは描画時）。クライアントが DOM に差し込むときは自分でサニタイズするか、サニタイズ済みの `?include=html` を使ってください。
- **レート制限**: 認証なしなので、IP ごとの制限をかけます（既存のキャッシュバックエンドの方式。実装時に問い合わせ受付などに合わせる）。
- **CORS**: `/api/v1/public/` と `/api/v1/search` の `GET` は、全オリジンに `Access-Control-Allow-Origin: *` を返します（読み取り専用の公開データ）。認証が要る API は、`[server].cors_origins` に書いたオリジンにだけ許します（[configuration.md の 4.3.1](configuration.md)）。
- **入力**: `limit` は上限で丸め、`cursor` は署名か検証つきの符号化で、改ざんされたら `400`。型名・slug は既存の識別子の検証を通します。
- **エラーメッセージは英語のみ**（規約）。

### 5.1 キャッシュ

- `Cache-Control: public, max-age=<秒>`（設定。既定 60）と、`ETag`（`updated_at` と id から計算）。`If-None-Match` で `304`。
- 公開ページと同じキャッシュタグ（`[cdn]`）を付け、コンテンツの更新・公開・非公開で既存の仕組みが消去する。

---

## 6. 決めたこと

1. **`public` の既定は公開**。隠すものだけ `public: false` と書く。型ごとの許可リスト（§4.1）が一段目の守りになっており、同梱のスキーマは全フィールドを公開しても問題ないため。商品を公開する場合は、内部向けのフィールドに `public: false` を書く。
2. **本文は `blocks`（AST）を返す**。描画済みの HTML は `?include=html` のときだけ。描画はプラグイン（Wasm）を呼ぶので、既定のコストに入れない。
3. **一覧の並びは既定のみ**（作成日時の新しい順）。公開日時順（`PublishedOrder`）は要望が出てから。
4. **商品は対象外**。価格・在庫の見せ方（在庫数を出さず有無だけにする、など）は、需要が出てから別に設計する。
5. **設定は `[headless]`**。

```toml
[headless]
enabled = false                         # 既定は無効
content_types = ["article", "page"]     # 公開する型
max_page_size = 100
cache_max_age = 60                      # Cache-Control の max-age（秒）
```

---

## 7. テスト方針

- **漏れないこと**（最優先）: 下書き・レビュー待ち・予約中・アーカイブ・`unpublish_at` を過ぎた公開、がどれも一覧と単体で返らない。許可リストにない型が `404`。
- **返さない項目**: `author_id`・`status`・`public: false` のフィールドが応答にない。
- **入力**: `limit` の上限、改ざんしたカーソルが `400`。
- **キャッシュ**: `ETag` と `If-None-Match` の `304`、更新後に `ETag` が変わる。
- **性能**: 配信 API は新しい問い合わせを足さず、公開版の一覧（公開の記録の列と索引）をそのまま使うので、記事数に依らない作りです。配信 API としての計測はしていません。
- 実機の PostgreSQL / MySQL を使う確認は、ローカルの品質チェックで行う（[開発ガイド 7](development.md#7-データベース実機検証-postgresql--mysql)）。
