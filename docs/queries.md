# 宣言的クエリ（集計・条件つき一覧）

> [!NOTE]
> **状態: 設計のみ（未実装）**。実装の進み具合は [TODO.md](../TODO.md) を参照。

スキーマ（`schemas/*.yaml`）で増やしたカスタム項目を、**条件つきで集計・抽出して、テーマやプラグインに渡す**ための仕組みです。クエリは YAML で宣言し、実行はサーバー（ホスト）だけが行います。Rust のソースを変える必要はありません。

## 1. 目的と、しないこと

目的:

- 項目を足したら、その項目で「件数」「合計」「上位 N 件」などを出したい、を宣言だけで満たす。
- プラグインに DB を触らせない。プラグインが受け取るのは結果の JSON だけ。

しないこと（穴を空けないための決定）:

- **閲覧者・ページに依存する条件**（`$page.*`、`$user.*` など）。条件の値は固定値だけ。結果は閲覧者にもページにも依存しないので、キャッシュはクエリ名だけをキーにできる。
- **生の SQL**。列や型はスキーマの宣言から選ぶ。
- **プラグイン専用の DB やキャッシュ**。
- **`access`（会員レベル）が付いたコンテンツの集計**。`access` が空（公開）のものだけを対象にする。
- 「最近見たページ」「お気に入り」のような閲覧者ごとの結果。別の仕組み（ブラウザ側の保存、会員機能）で扱う。

## 2. 定義の書き方

`queries/<name>.yaml`（`schemas/` と同じ階層）に 1 ファイル 1 クエリ。

```yaml
name: monthly_sales          # ファイル名と一致させる。英小文字・数字・_ のみ
from: product                # schemas/product.yaml の型
where:
  price: { gte: 1000 }       # 固定値のみ
group_by:
  - { field: published_at, bucket: month }
select:
  - { fn: count, as: n }
  - { fn: sum, field: price, as: total }
order_by:
  - { field: month, dir: desc }
limit: 12
```

| キー | 内容 |
| --- | --- |
| `from` | コンテンツ型（必須） |
| `where` | 列名 → `{ 演算子: 値 }`。演算子は `eq` `ne` `gt` `gte` `lt` `lte` `in` `is_null`。複数の列は AND。`null` の扱いは 2.3 |
| `group_by` | 列のリスト。日付の列は `bucket: day \| month \| year` を指定できる |
| `select` | 集計のリスト。`fn` は `count` `sum` `avg` `min` `max`。`as` は結果のキー名。**省略するとレコードの一覧を返す**（2.2） |
| `order_by` | `field` は列名か `select` の `as`。`dir` は `asc` / `desc` |
| `limit` | 行数の上限。サーバーの最大値（既定 100）を超えられない。省略時は最大値。**上限は行数だけで見る**（バイト数は見ない。1 行が大きくなる設計は、クエリを書く側の責務） |

### 2.1 リレーション列での `group_by`

`type: relation` の列（`category_id` など）で `group_by` したとき、結果の行にはキーの値に加えて**参照先の `title` と `slug`** を入れる。

```json
{ "category_id": { "id": "…", "title": "Shoes", "slug": "shoes" }, "n": 12 }
```

解決は `resolve_relations_for_records` と同じく、行ごとではなく `IN (...)` で 1 回にまとめる（[architecture.md 2.3](architecture.md#23-スキーマからのコード生成とシステム標準カラム)）。

### 2.2 `select` を省略したとき（一覧）

集計せず、条件に合うレコードを `order_by` / `limit` で絞って返す。各要素の形は、テーマの一覧（`items`）と同じ（`title` `slug` `excerpt` `published_at` `fields` `relations`）。**本文（`blocks`・`content_html`）は含めない**（行数が少なくても重くなり、行数だけの上限では守れないため）。「セール中の商品の上位 4 件」「最新のお知らせ 3 件」のような使い方を想定する。`group_by` と同時には使えない。

### 2.3 `null` の扱い

値がないことは、比較ではなく専用の `is_null` で書く。

```yaml
where:
  discount_price: { is_null: false }   # 値がある
  archived_at: { is_null: true }       # 値がない
```

- `is_null` の値は真偽値だけ。
- `eq` `ne` `gt` `gte` `lt` `lte` に `null` を渡すのは**検証エラー**。`in` のリストに `null` が混ざるのも同じ。SQL では `col = NULL` が常に一致せず、YAML では値の書き忘れ（`{ gte: }`）が `null` になるため、黙って通さない。
- `nullable` でない列への `is_null` は検証エラー（条件が常に同じ結果になるため）。
- 集計は SQL の標準に従い、`sum` `avg` `min` `max` と、`field` を指定した `count` は `null` を除く。`field` を省略した `count` は行数。

## 3. 検証

起動時（と `task migrate` 時）に、すべての定義をスキーマと突き合わせる。**1 つでも誤りがあれば起動を止める**（プラグインの読み込みと同じ fail-closed）。

- 存在しない `from` / 列 / `as`
- 列の型と比較する値の型が合わない（数値の列に文字列など）
- `sum` / `avg` を数値でない列に使う
- `group_by` と `select` の組み合わせが成り立たない（集計を使うのに、`group_by` にない列を出力するなど）
- `name` の重複、ファイル名との不一致
- `limit` が最大値を超える
- 比較演算子や `in` に `null` を渡す、`nullable` でない列に `is_null` を使う（2.3）

## 4. 実行

- ホストが定義から SQL を組み立て、値は `sqlx` のバインド変数で渡す。列名は検証済みの宣言から取り、入力から取らない。
- ホストが**必ず**足す条件（宣言では外せない）:
  - 公開中（**どの版を読むかは未決**。8 の先頭を参照）
  - `access` が空
- SQLite / PostgreSQL / MySQL の各アダプタで同じ結果になること。
- 実行時間にタイムアウトを設ける。同時に実行できる数も制限する。

## 5. 結果の渡し方

- **テーマ**: テンプレートの文脈に `queries.<name>` として入る。そのテーマ（またはページ）で使うものだけが対象。
- **プラグイン**: `on_render_page` の入力に `queries.<name>` として入る。マニフェストの `capabilities.render_page.input` と同じ考え方で、使うクエリ名を宣言した分だけ渡す（宣言のないものは空）。プラグインは結果の JSON を読むだけで、DB にもクエリ定義にも触れない。
- 結果の中の文字列は、利用者が入力した値である。テンプレートでは `safe` を付けない（[theme_authoring_guide.md 3.3](theme_authoring_guide.md)）。

## 6. キャッシュ

- キーは**クエリ名と定義のハッシュ**だけ（閲覧者・ページに依存しないため）。保存先は `[cache]`。
- 無効化: `from` の型のコンテンツの作成・更新・公開・削除・復元で、その型を使うクエリを消す。タグは型名にする（[architecture.md 3.7](architecture.md) の Cache-Tag と同じ考え方）。
- 定義のハッシュをキーに含めるので、定義を変えて再起動すれば古い結果は使われない。
- 日付の `bucket` は、サーバーのタイムゾーン設定に従う。

## 7. 使用例

商品サイト:

```yaml
# カテゴリ別の商品数（サイドバー）
name: product_count_by_category
from: product
group_by: [{ field: category_id }]
select: [{ fn: count, as: n }]
order_by: [{ field: n, dir: desc }]
```

```yaml
# セール中の商品の上位 4 件（select を省略 = 一覧）
name: sale_products
from: product
where: { on_sale: { eq: true } }
order_by: [{ field: published_at, dir: desc }]
limit: 4
```

```yaml
# 価格帯フィルタの範囲
name: price_range
from: product
select:
  - { fn: min, field: price, as: lowest }
  - { fn: max, field: price, as: highest }
```

プロダクトの LP:

```yaml
# 導入事例の数（ヒーローの数字）
name: case_count
from: case_study
select: [{ fn: count, as: n }]
```

```yaml
# 料金プランの最安・最高
name: plan_range
from: plan
select:
  - { fn: min, field: monthly_price, as: from_price }
  - { fn: max, field: monthly_price, as: to_price }
```

テーマ側:

```jinja
導入事例 {{ queries.case_count[0].n }} 社
{% for c in queries.product_count_by_category %}
  {{ c.category_id.title }} ({{ c.n }})
{% endfor %}
```

## 8. 未解決の設計判断

- **集計が読む版（実装前に決める）**: 公開ページは公開ポインターのスナップショットを読み、`category_id` 以外のフィールドでの絞り込みは全件を読んでから Rust 側で行っている（[architecture.md の 2.1](architecture.md#2-コンテンツと-db) の公開ポインターと、3 章の `list_published_articles` の説明）。SQL で集計するなら、(a) 作業版の列を `status = published` で読む（速い。ただし公開後に編集中の値が集計に出る）、(b) スナップショットの JSON を読む（正確。ただし SQL で JSON を展開する手段と索引が DB ごとに違う）、(c) 公開操作のたびに集計に使う列だけを公開側の列へ写す（正確で速い。列の追加と同期が増える）、のどれかを選ぶ。
- 実行時間のタイムアウトと同時実行数の具体的な値、`in` のリストの長さの上限。
- `where` / `group_by` / `order_by` に使う列への索引を、宣言から自動で作るか（`schemas/*.yaml` の索引の宣言と、どう揃えるか）。
- 日付の `bucket` で使うタイムゾーン（サイトの設定か、UTC か）。
- `from` に指定できる型を、スキーマで宣言したコンテンツ型だけにするか（タグ・コメント・注文などの組み込みの表を含めない前提。今はこの前提）。
- 型をまたぐ結合（`from` が 1 つの型に限られる）は第 2 段階以降。要望を見て決める。
- Admin Studio にクエリの一覧と、結果のプレビューを出すか。
- `theme dev` / Theme Builder で、クエリの結果をどう用意するか（サンプルデータか、実サーバーへの接続か）。
