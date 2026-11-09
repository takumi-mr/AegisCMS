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
| `where` | 列名 → `{ 演算子: 値 }`。演算子は `eq` `ne` `gt` `gte` `lt` `lte` `in`。複数の列は AND |
| `group_by` | 列のリスト。日付の列は `bucket: day \| month \| year` を指定できる |
| `select` | 集計のリスト。`fn` は `count` `sum` `avg` `min` `max`。`as` は結果のキー名。**省略するとレコードの一覧を返す**（2.2） |
| `order_by` | `field` は列名か `select` の `as`。`dir` は `asc` / `desc` |
| `limit` | 件数の上限。サーバーの最大値（既定 100）を超えられない。省略時は最大値 |

### 2.1 リレーション列での `group_by`

`type: relation` の列（`category_id` など）で `group_by` したとき、結果の行にはキーの値に加えて**参照先の `title` と `slug`** を入れる。

```json
{ "category_id": { "id": "…", "title": "Shoes", "slug": "shoes" }, "n": 12 }
```

解決は `resolve_relations_for_records` と同じく、行ごとではなく `IN (...)` で 1 回にまとめる（[architecture.md 2.3](architecture.md#23-スキーマからのコード生成とシステム標準カラム)）。

### 2.2 `select` を省略したとき（一覧）

集計せず、条件に合うレコードを `order_by` / `limit` で絞って返す。各要素の形は、テーマの一覧（`items`）と同じ（`title` `slug` `excerpt` `published_at` `fields` `relations`）。「セール中の商品の上位 4 件」「最新のお知らせ 3 件」のような使い方を想定する。`group_by` と同時には使えない。

## 3. 検証

起動時（と `task migrate` 時）に、すべての定義をスキーマと突き合わせる。**1 つでも誤りがあれば起動を止める**（プラグインの読み込みと同じ fail-closed）。

- 存在しない `from` / 列 / `as`
- 列の型と比較する値の型が合わない（数値の列に文字列など）
- `sum` / `avg` を数値でない列に使う
- `group_by` と `select` の組み合わせが成り立たない（集計を使うのに、`group_by` にない列を出力するなど）
- `name` の重複、ファイル名との不一致
- `limit` が最大値を超える

## 4. 実行

- ホストが定義から SQL を組み立て、値は `sqlx` のバインド変数で渡す。列名は検証済みの宣言から取り、入力から取らない。
- ホストが**必ず**足す条件（宣言では外せない）:
  - 公開中（公開版のスナップショットを読む。[architecture.md 2.2](architecture.md#22-databaseadapter)）
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

- 1 つのクエリの結果が大きいときの上限（行数ではなく、バイト数も見るか）。
- `where` に `null` の比較（`is_null`）を入れるか。
- 型をまたぐ結合（`from` が 1 つの型に限られる）は第 2 段階以降。要望を見て決める。
- Admin Studio にクエリの一覧と、結果のプレビューを出すか。
- `theme dev` / Theme Builder で、クエリの結果をどう用意するか（サンプルデータか、実サーバーへの接続か）。
