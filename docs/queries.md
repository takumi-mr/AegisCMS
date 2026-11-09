# 宣言的クエリ（集計・条件つき一覧）

> [!NOTE]
> 実装済み（`aegis-schema` の `query`、`aegis-db` の `published_columns`、`aegis-server` の `queries`）。残りの作業は [TODO.md](../TODO.md) を参照。

スキーマ（`schemas/*.yaml`）で増やしたカスタム項目を、**条件つきで集計・抽出して、テーマやプラグインに渡す**ための仕組みです。クエリは YAML で宣言し、実行はサーバー（ホスト）だけが行います。Rust のソースを変える必要はありません。

## 1. 目的と、しないこと

目的:

- 項目を足したら、その項目で「件数」「合計」「上位 N 件」などを出したい、を宣言だけで満たす。
- プラグインに DB を触らせない。プラグインが受け取るのは結果の JSON だけ。

しないこと（穴を空けないための決定）:

- **閲覧者・ページに依存する条件**（`$page.*`、`$user.*` など）。条件の値は固定値だけ。結果は閲覧者にもページにも依存しないので、サーバーがクエリ名ごとに 1 つの結果を持てば足りる。
- **生の SQL**。列や型はスキーマの宣言から選ぶ。独自の DSL を SQL のサブセットに育てない（`OR`・`HAVING`・サブクエリ・**型をまたぐ結合**は対応しない。代わりの方法は 10）。
- **プラグイン専用の DB やキャッシュ**。
- **`access`（会員レベル）が付いたコンテンツの集計**。`access` が空（公開）のものだけを対象にする。
- 「最近見たページ」「お気に入り」のような閲覧者ごとの結果。別の仕組み（ブラウザ側の保存、会員機能）で扱う。

## 2. 定義の書き方

`queries/<name>.yaml`（`schemas/` と同じ階層。場所は `[paths].queries_dir`）に 1 ファイル 1 クエリ。

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
- `from` がスキーマで宣言したコンテンツ型でない（タグ・コメント・注文などの組み込みの表は対象外）

## 4. 実行

- ホストが定義から SQL を組み立て、値は `sqlx` のバインド変数で渡す。列名は検証済みの宣言から取り、入力から取らない。
- ホストが**必ず**足す条件（宣言では外せない）:
  - 公開中（公開版の値を読む。4.1）
  - `access` が空
- SQLite / PostgreSQL / MySQL の各アダプタで同じ結果になること。

### 4.1 読む版: 公開側の列に写す

公開ページは公開ポインターのスナップショットを読むため、作業版の列を読むと、公開後に編集中の値（未公開の価格変更など）が集計に出てしまう。かといってスナップショットの JSON を SQL で展開すると、DB ごとに手段と索引が違う。そこで、**公開操作（初回公開・「公開版を更新」）のときに、クエリが使う列だけを公開側の列へ写す**。

- 写す対象は、クエリの `where` / `group_by` / `select` / `order_by` に現れる列の和集合。起動時に全クエリから求める。
- 写す先は、コンテンツの種類ごとの副表 `_aegis_published_<type>`（`content_<type>` と同じ命名）。公開の記録 `_aegis_content_publications` は全種類で共有する固定の列なので、種類ごとの列は持てない。列の型は `content_<type>` と同じ。公開ポインターの更新と**同じトランザクション**で書く（公開・更新・予約公開は書き換え、取り下げ・削除は消す）。
- 副表に入るのは公開中で `access` が空のコンテンツだけ。
- 副表には、起動時に求めた「使う列の定義」の控え（`signature`。`_aegis_published_meta`）を持つ。使う列・型・索引・タイムゾーンのどれかが変わると、**副表を作り直して**公開中のスナップショットから埋め直す（`ADD COLUMN` はしない）。クエリを消しても副表は残す。
- 日時は設定のタイムゾーンの現地時刻 `YYYY-MM-DDTHH:MM:SS` で持つ。`bucket` は文字列の `SUBSTR`、日時の条件の値も同じ変換をして比べるので、SQL は DB に依らない。`min` / `max` の日時もこの形（オフセットなし）で返る。
- 作業版の編集は、公開版を更新するまで集計に出ない。
- **`task migrate`**: スキーマのマイグレーションのあと、副表を用意する（定義が同じなら何もしない）。クエリの定義に誤りがあれば、何も変えずにエラーにする。
- **`task restore`**: DB を復元するとき（`--no-database` でないとき）は、復元の**前**にクエリの定義を検証し（誤りがあれば何も変えずに止める）、復元の**後**に副表を**すべて作り直す**。副表は公開の記録から導ける派生データで、バックアップには含めない。復元は同じキーを上書きするだけなので、復元先にもとからあったコンテンツは残り、副表は復元後の公開の記録（残ったものも含む）と一致する。

### 4.2 設定

`[queries]`（[configuration.md の 2.26](configuration.md#226-queries--宣言的クエリ)）。

| キー | 既定 | 内容 |
| --- | --- | --- |
| `max_limit` | 100 | `limit` の最大値（行数） |
| `timeout_ms` | 2000 | 1 回の実行のタイムアウト |
| `max_concurrency` | 8 | 同時に実行できる数 |
| `max_in_items` | 100 | `in` のリストの最大の長さ |
| `timezone` | `UTC` | 日付の `bucket` と日時の条件で使うタイムゾーン（IANA 名。誤りは起動時エラー。変えると副表を作り直す） |

### 4.3 索引

クエリの宣言から索引は**自動では作らない**。必要な索引は `schemas/*.yaml` の宣言に書く。書き忘れによる遅さは、実行時間のタイムアウトで止まる。

## 5. 結果の渡し方

- **テーマ**: テンプレートのグローバル変数 `queries` から `queries.<name>` で読む。結果はページに依存しないので `PageRenderContext` には載せず、サーバーが持つ最新の一式を読む（環境の作り直しは要らない）。名前の無いクエリは未定義になり、未定義への添字アクセス（`queries.x[0]`）は描画のエラーになる。
- **プラグイン**: `on_render_page` の入力の `queries` に入る。マニフェストの `capabilities.render_page.queries` に書いた名前の結果だけが渡り（書かなければ空）、読み取り専用（返した値は反映されず、次のプラグインにも見えない）。プラグインは結果の JSON を読むだけで、DB にもクエリ定義にも触れない。`render_page.input` とは独立。Rust SDK の `PageRenderContext` には `queries` がある。
- **`theme dev` / Theme Builder**: クエリは実行せず（DB を使わないため）、テーマの `samples/queries.json`（クエリ名 → 結果の配列）を `queries` に入れる。`--init-queries` で、サイトの定義とスキーマから雛形を作れる（[theme_authoring_guide.md](theme_authoring_guide.md#theme-devdb-なしで素早く確かめる)）。実際のクエリを発行する方式（サイトの DB やサーバーへの接続）は採らない: 本番に近いデータが手元の画面に出ること、設定が増えること、空・0 件・長い文字列などの確認を作れないことのため。
- 結果の形: 集計は行の配列（キーは `group_by` の出力名と `select` の `as`）。`select` を省略した一覧は、テーマの一覧（`items`）の要素と同じ形（本文は含めない）。
- 結果の中の文字列は、利用者が入力した値である。テンプレートでは `safe` を付けない（[theme_authoring_guide.md 3.3](theme_authoring_guide.md)）。

## 6. キャッシュ

- 結果は `[cache]` ではなく、**サーバーのメモリ上の「クエリ名 → 結果」の一式**として持つ。閲覧者にもページにも依存しないので、ページの表示ごとの DB アクセスは無い。
- 起動時に全クエリを計算してから配信する。コンテンツの作成・更新・公開・取り下げ・削除・復元・予約公開（`cdn::purge_content` を通る変更すべて）で、`from` がその種類のクエリだけを計算し直して差し替える。変更の知らせはイベントバス（`[cache]` と同じバックエンド）で全サーバーに届くので、複数台でも各サーバーが自分の一式を更新する（メモリのバックエンドは 1 台だけ）。
- 計算に失敗したクエリは前の結果を残して警告を出す。遅れて終わった古い計算は、新しい結果を上書きしない。タイムアウト（`timeout_ms`）と同時実行数（`max_concurrency`）で守る。
- 副表の定義が変わった（クエリの追加・列の変更・タイムゾーンの変更）あとは、再起動で作り直される。

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

- Admin Studio にクエリの一覧と、結果のプレビューを出すか。
- AssemblyScript / TinyGo のプラグインテンプレートに `queries` を足すか。

## 9. 使うときの注意

- `ne`（`<>`）は SQL どおり、値が NULL の行を含まない。NULL も含めたいときは `is_null` と使い分ける（OR は書けない）。
- 公開後の編集は、公開版を更新するまで結果に出ない。会員限定（`access` が空でない）のコンテンツは出ない。
- 件数が増えたら、`where` / `group_by` / `order_by` に使う列に `schemas/*.yaml` の `index: true` を付ける（副表にも同じ索引ができる）。

## 10. 型をまたぐ結合をしない方針と、代わりの方法

`from` は 1 つのコンテンツ型だけで、結合は対応しない。DSL が SQL のサブセットに育っていくのを避けるため（結合を入れると、`OR`・`HAVING`・サブクエリの要望が続き、検証・副表・結果の形のすべてが複雑になる）。

**できること**（結合ではなく、結果の引き直し）:

- `group_by` したリレーション列は、参照先の `{id, title, slug}` に展開される（2.1）。一覧（`select` 省略）は `relations` が付く（2.2）。
- 多側の型で集計する。たとえばレビューの型が `product_id` を持てば、`group_by: [{ field: product_id }]` で商品ごとの件数・平均が出る。

**できないこと**:

| 場面 | 例 |
| --- | --- |
| 参照先の属性で絞る | 「カテゴリ名が Shoes の商品」。`category_id: { eq: "<id>" }` のように id の固定値で絞るしかない |
| 別の型の集計で並べる | 「レビューの平均が高い商品」を商品の一覧として出す |
| システムの表を読む | 注文数順の商品、コメント数順の記事、タグ別の件数。`from` はスキーマで宣言したコンテンツ型だけで、タグ・コメント・注文・レビューは対象外 |

**代わりの方法**:

- **複数のクエリを組み合わせる**: 集計を 2 つ宣言して、テンプレートのループか、プラグインで突き合わせる。プラグインは複数のクエリの結果を読み取り専用で受け取れる（5）ので、比率や順位も計算できる。1 クエリの上限（`max_limit`）が行数なので、突き合わせは数百行までが現実的。クエリごとに計算のタイミングが違うため、更新の直後にごく短い間だけ食い違うことがある。
- **非正規化する**: 「平均評価」のような値を、並べ替えたい側の型の項目として持つ（プラグインなどで更新する）。項目にすれば、`where` / `order_by` にそのまま使える。
- **システムの表は専用の機能で出す**: タグ別の件数やコメント数などが要るときは、汎用のクエリに足さず、それぞれの機能（タグ、コメント）が結果を出す形で別に設計する。

**例外として検討できるもの**: 参照先の **slug で絞る**（`category: { eq: shoes }`）は、結合ではなく、読み込み時に id へ置き換える変換として足せる。要望が出てから決める。
