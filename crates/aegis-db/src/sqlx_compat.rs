//! SQLx 0.9 `SqlSafeStr` 互換レイヤー。
//! 動的に構築された SQL クエリ文字列を `AssertSqlSafe` で透過的にラップし、SQLx 0.8 と同様に実行可能にする。

pub use ::sqlx::*;

#[inline]
pub fn query<'a, DB>(
    sql: &'a str,
) -> ::sqlx::query::Query<'a, DB, <DB as ::sqlx::Database>::Arguments>
where
    DB: ::sqlx::Database,
{
    ::sqlx::query(::sqlx::AssertSqlSafe(sql))
}

#[inline]
pub fn query_as<'a, DB, O>(
    sql: &'a str,
) -> ::sqlx::query::QueryAs<'a, DB, O, <DB as ::sqlx::Database>::Arguments>
where
    DB: ::sqlx::Database,
    O: for<'r> ::sqlx::FromRow<'r, <DB as ::sqlx::Database>::Row>,
{
    ::sqlx::query_as(::sqlx::AssertSqlSafe(sql))
}

#[inline]
pub fn query_scalar<'a, DB, O>(
    sql: &'a str,
) -> ::sqlx::query::QueryScalar<'a, DB, O, <DB as ::sqlx::Database>::Arguments>
where
    DB: ::sqlx::Database,
    (O,): for<'r> ::sqlx::FromRow<'r, <DB as ::sqlx::Database>::Row>,
{
    ::sqlx::query_scalar(::sqlx::AssertSqlSafe(sql))
}

#[inline]
pub fn raw_sql(sql: &str) -> ::sqlx::RawSql {
    ::sqlx::raw_sql(::sqlx::AssertSqlSafe(sql))
}
