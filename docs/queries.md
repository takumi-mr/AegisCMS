# Declarative Queries (Aggregations and Conditional Lists)

**English** | [日本語](queries.ja.md)

> [!NOTE]
> Implemented (`query` in `aegis-schema`, `published_columns` in `aegis-db`, `queries` in `aegis-server`). For the remaining work, see [TODO.md](../TODO.md).

A mechanism for **aggregating and extracting, with conditions, the custom fields you added in a schema (`schemas/*.yaml`) and passing the results to themes and plugins**. Queries are declared in YAML, and only the server (host) runs them. No Rust source needs to change.

## 1. Purpose and What It Does Not Do

Purpose:

- Satisfy, by declaration alone, the wish "once I add a field, I want counts, sums, top N, and so on by that field".
- Do not let plugins touch the DB. What a plugin receives is only the result JSON.

What it does not do (decisions to avoid opening holes):

- **Conditions that depend on the viewer or the page** (`$page.*`, `$user.*`, etc.). Condition values are fixed values only. The result depends on neither the viewer nor the page, so it is enough for the server to hold one result per query name.
- **Raw SQL**. Columns and types are chosen from the schema declaration. We do not grow a custom DSL into a subset of SQL (`OR`, `HAVING`, subqueries, and **joins across types** are not supported. For alternatives, see 10).
- **A plugin-specific DB or cache.**
- **Aggregating content with `access` (membership level) set.** Only content with an empty `access` (public) is targeted.
- Per-viewer results such as "recently viewed pages" or "favorites". These are handled by other mechanisms (browser-side storage, member features).

## 2. How to Write a Definition

One query per file in `queries/<name>.yaml` (at the same level as `schemas/`; the location is `[paths].queries_dir`).

```yaml
name: monthly_sales          # match the file name. Lowercase letters, digits, and _ only
from: product                # a type in schemas/product.yaml
where:
  price: { gte: 1000 }       # fixed values only
group_by:
  - { field: published_at, bucket: month }
select:
  - { fn: count, as: n }
  - { fn: sum, field: price, as: total }
order_by:
  - { field: month, dir: desc }
limit: 12
```

| Key        | Description                                                                                                                                                                                                                                    |
| ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `from`     | The content type (required)                                                                                                                                                                                                                    |
| `where`    | Column name → `{ operator: value }`. Operators are `eq` `ne` `gt` `gte` `lt` `lte` `in` `is_null`. Multiple columns are ANDed. For how `null` is handled, see 2.3                                                                              |
| `group_by` | A list of columns. Date columns can specify `bucket: day \| month \| year`                                                                                                                                                                     |
| `select`   | A list of aggregations. `fn` is `count` `sum` `avg` `min` `max`. `as` is the key name in the result. **If omitted, a list of records is returned** (2.2)                                                                                       |
| `order_by` | `field` is a column name or a `select` `as`. `dir` is `asc` / `desc`                                                                                                                                                                           |
| `limit`    | The upper limit on the number of rows. It cannot exceed the server's maximum (default 100). If omitted, the maximum. **The limit looks at row count only** (not bytes; designs where one row gets large are the query author's responsibility) |

### 2.1 `group_by` on a Relation Column

When you `group_by` a `type: relation` column (such as `category_id`), the result rows include the **target's `title` and `slug`** in addition to the key value.

```json
{ "category_id": { "id": "…", "title": "Shoes", "slug": "shoes" }, "n": 12 }
```

Resolution is done in one go with `IN (...)` rather than per row, like `resolve_relations_for_records` ([architecture.md 2.3](architecture.md#23-code-generation-from-schemas-and-the-system-standard-columns)).

### 2.2 When `select` Is Omitted (a List)

It does not aggregate; it returns the records matching the conditions, narrowed by `order_by` / `limit`. The shape of each element is the same as the theme's list (`items`) (`title` `slug` `excerpt` `published_at` `fields` `relations`). **The body (`blocks`, `content_html`) is not included** (it would be heavy even with few rows, and a row-count-only limit cannot protect against that). Intended uses include "the top 4 products on sale" and "the latest 3 notices". It cannot be used together with `group_by`.

### 2.3 How `null` Is Handled

Having no value is written with the dedicated `is_null`, not with a comparison.

```yaml
where:
  discount_price: { is_null: false }   # has a value
  archived_at: { is_null: true }       # has no value
```

- The value of `is_null` is a boolean only.
- Passing `null` to `eq` `ne` `gt` `gte` `lt` `lte` is a **validation error**. So is a `null` mixed into an `in` list. In SQL, `col = NULL` never matches, and in YAML, forgetting to write a value (`{ gte: }`) becomes `null`, so it is not let through silently.
- `is_null` on a non-`nullable` column is a validation error (because the condition would always have the same result).
- Aggregation follows the SQL standard: `sum` `avg` `min` `max`, and `count` with `field` specified, exclude `null`. `count` without `field` is the row count.

## 3. Validation

At startup (and at `task migrate`), all definitions are checked against the schemas. **If even one has an error, startup stops** (fail-closed, the same as loading plugins).

- A nonexistent `from` / column / `as`
- The column's type does not match the compared value's type (a string for a numeric column, etc.)
- Using `sum` / `avg` on a non-numeric column
- An impossible combination of `group_by` and `select` (using aggregation but outputting a column not in `group_by`, etc.)
- A duplicate `name`, or a mismatch with the file name
- `limit` exceeds the maximum
- Passing `null` to a comparison operator or `in`, or using `is_null` on a non-`nullable` column (2.3)
- `from` is not a content type declared in a schema (built-in tables such as tags, comments, and orders are out of scope)

## 4. Execution

- The host builds SQL from the definition and passes values as `sqlx` bind variables. Column names come from the validated declaration and never from input.
- Conditions the host **always** adds (they cannot be removed by the declaration):
    - Published (reads the published version's values; 4.1)
    - `access` is empty
- The result must be the same on the SQLite / PostgreSQL / MySQL adapters.

### 4.1 The Version Read: Mirrored to Public-Side Columns

Public pages read the snapshot of the publication pointer, so if the working version's columns were read, values being edited after publication (such as an unpublished price change) would appear in aggregates. And expanding the snapshot's JSON in SQL differs in method and indexes per DB. So, **at a publish operation (first publication or "update the published version"), only the columns the queries use are copied to the public-side columns.**

- The columns copied are the union of the columns appearing in the queries' `where` / `group_by` / `select` / `order_by`. It is computed from all queries at startup.
- The destination is a side table per content kind, `_aegis_published_<type>` (named like `content_<type>`). The publication record `_aegis_content_publications` has fixed columns shared by all kinds, so it cannot hold per-kind columns. The column types are the same as `content_<type>`. It is written in **the same transaction** as updating the publication pointer (publish, update, and scheduled publish rewrite it; unpublish and delete remove it).
- Only content that is published and has an empty `access` goes into the side table.
- The side table keeps a note of the "definition of the columns in use" computed at startup (`signature`; `_aegis_published_meta`). When any of the columns, types, indexes, or time zone used changes, **the side table is recreated** and refilled from the published snapshots (no `ADD COLUMN`). Even if you delete a query, the side table remains.
- Datetimes are kept as local time in the configured time zone, `YYYY-MM-DDTHH:MM:SS`. `bucket` is a string `SUBSTR`, and datetime condition values are compared after the same conversion, so the SQL does not depend on the DB. `min` / `max` of datetimes are also returned in this form (with no offset).
- Edits to the working version do not appear in aggregates until the published version is updated.
- **`task migrate`**: after the schema migration, the side tables are prepared (nothing happens if the definition is the same). If a query definition has an error, it fails without changing anything.
- **`task restore`**: when restoring the DB (unless `--no-database`), the query definitions are validated **before** the restore (if there is an error, it stops without changing anything), and **after** the restore the side tables are **all recreated**. The side tables are derived data that can be derived from the publication records and are not included in backups. A restore only overwrites the same keys, so content that was originally in the restore destination remains, and the side tables match the publication records after the restore (including the ones that remained).

### 4.2 Settings

`[queries]` ([configuration.md 2.26](configuration.md#226-queries--declarative-queries)).

| Key               | Default | Description                                                                                                                                   |
| ----------------- | ------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| `max_limit`       | 100     | The maximum value of `limit` (rows)                                                                                                           |
| `timeout_ms`      | 2000    | Timeout for one execution                                                                                                                     |
| `max_concurrency` | 8       | Number that can run at the same time                                                                                                          |
| `max_in_items`    | 100     | Maximum length of an `in` list                                                                                                                |
| `timezone`        | `UTC`   | The time zone used for date `bucket`s and datetime conditions (IANA name; an error is a startup error; changing it recreates the side tables) |

### 4.3 Indexes

Indexes are **not created automatically** from query declarations. Write the indexes you need in the declaration in `schemas/*.yaml`. Slowness from forgetting to write them is stopped by the execution timeout.

## 5. How Results Are Passed

- **Themes**: read as `queries.<name>` from the template global variable `queries`. The result does not depend on the page, so it is not put in `PageRenderContext`; the latest set held by the server is read (no need to rebuild the environment). A query with no such name is undefined, and subscript access on undefined (`queries.x[0]`) is a rendering error.
- **Plugins**: it goes into `queries` in the input of `on_render_page`. Only the results of names written in the manifest's `capabilities.render_page.queries` are passed (empty if not written), and they are read-only (returned values are not reflected and are not visible to the next plugin). Plugins only read the result JSON and cannot touch the DB or query definitions. It is independent of `render_page.input`. `PageRenderContext` in the SDKs of all languages (Rust / AssemblyScript / TinyGo) has `queries`.
- **`theme dev` / Theme Builder**: queries are not run (because no DB is used); the theme's `samples/queries.json` (query name → array of results) is put into `queries`. `--init-queries` can make a scaffold from the site's definitions and schemas ([theme_authoring_guide.md](theme_authoring_guide.md#theme-dev-check-quickly-without-a-db)). We do not take the approach of issuing real queries (connecting to the site's DB or server): data close to production would appear on the local screen, settings would increase, and checks for empty, zero-result, or long-string cases could not be made.
- Result shape: aggregations are an array of rows (keys are the `group_by` output names and the `select` `as`). A list with `select` omitted has the same shape as the elements of the theme's list (`items`) (without the body).
- Strings in results are values that users entered. Do not add `safe` in templates ([theme_authoring_guide.md 3.3](theme_authoring_guide.md)).

## 6. Caching

- Results are held not in `[cache]` but as **a set of "query name → result" in the server's memory**. They depend on neither the viewer nor the page, so there is no DB access per page display.
- All queries are computed at startup before serving. On content creation, update, publish, unpublish, delete, restore, and scheduled publish (all changes that go through `cdn::purge_content`), only the queries whose `from` is that kind are recomputed and swapped in. The change notice reaches all servers over the event bus (the same backend as `[cache]`), so even with multiple servers each one updates its own set (with the in-memory backend, only one server).
- A query whose computation fails keeps its previous result and logs a warning. An old computation that finishes late does not overwrite a newer result. They are protected by the timeout (`timeout_ms`) and the concurrency limit (`max_concurrency`).
- After the side table definition changes (adding a query, changing a column, changing the time zone), it is recreated by a restart.

## 7. Examples

A product site:

```yaml
# Number of products per category (sidebar)
name: product_count_by_category
from: product
group_by: [{ field: category_id }]
select: [{ fn: count, as: n }]
order_by: [{ field: n, dir: desc }]
```

```yaml
# The top 4 products on sale (select omitted = a list)
name: sale_products
from: product
where: { on_sale: { eq: true } }
order_by: [{ field: published_at, dir: desc }]
limit: 4
```

```yaml
# Range for a price filter
name: price_range
from: product
select:
  - { fn: min, field: price, as: lowest }
  - { fn: max, field: price, as: highest }
```

A product landing page:

```yaml
# Number of case studies (the number in the hero)
name: case_count
from: case_study
select: [{ fn: count, as: n }]
```

```yaml
# Cheapest and most expensive pricing plans
name: plan_range
from: plan
select:
  - { fn: min, field: monthly_price, as: from_price }
  - { fn: max, field: monthly_price, as: to_price }
```

On the theme side:

```jinja
{{ queries.case_count[0].n }} case studies
{% for c in queries.product_count_by_category %}
  {{ c.category_id.title }} ({{ c.n }})
{% endfor %}
```

## 8. Open Design Decisions

- Whether to show the query list and a result preview in Admin Studio.

## 9. Notes on Use

- `ne` (`<>`) does not include rows whose value is NULL, as in SQL. When you want NULL included, use it together with `is_null` as appropriate (OR cannot be written).
- Edits after publication do not appear in results until the published version is updated. Members-only (non-empty `access`) content does not appear.
- As the number of items grows, add `index: true` in `schemas/*.yaml` to the columns used in `where` / `group_by` / `order_by` (the same index is created on the side table).

## 10. The Policy of Not Joining Across Types, and Alternatives

`from` is a single content type only, and joins are not supported. This is to avoid the DSL growing into a subset of SQL (once joins are in, requests for `OR`, `HAVING`, and subqueries follow, and validation, side tables, and the result shape all become complex).

**What you can do** (not joins, but re-fetching the result):

- A relation column you `group_by` is expanded into the target's `{id, title, slug}` (2.1). A list (with `select` omitted) gets `relations` (2.2).
- Aggregate on the "many" side type. For example, if a review type has `product_id`, `group_by: [{ field: product_id }]` gives counts and averages per product.

**What you cannot do**:

| Situation                            | Example                                                                                                                                                                    |
| ------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Filter by an attribute of the target | "Products whose category name is Shoes". You can only filter by a fixed id value such as `category_id: { eq: "<id>" }`                                                     |
| Order by another type's aggregate    | Show "products with a high average review" as a product list                                                                                                               |
| Read system tables                   | Products by order count, articles by comment count, counts per tag. `from` is only content types declared in schemas; tags, comments, orders, and reviews are out of scope |

**Alternatives**:

- **Combine multiple queries**: declare two aggregations and match them up with a template loop or a plugin. Plugins can receive the results of multiple queries read-only (5), so ratios and ranks can be computed too. The limit for one query (`max_limit`) is a row count, so matching up is realistic up to a few hundred rows. Since each query is computed at a different time, they may briefly disagree right after an update.
- **Denormalize**: keep a value such as "average rating" as a field on the type you want to sort (updated by a plugin or the like). Once it is a field, it can be used in `where` / `order_by` as is.
- **Expose system tables with dedicated features**: when you need things like counts per tag or comment counts, do not add them to the general query; design separately so that each feature (tags, comments) produces the result.

**What could be considered as an exception**: **filtering by the target's slug** (`category: { eq: shoes }`) could be added not as a join but as a transformation that replaces it with an id at load time. To be decided when requested.
