use crate::definition::{FieldType, SchemaDefinition};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SqlDialect {
    #[default]
    Sqlite,
    Postgres,
    MySql,
}

pub struct SchemaMigration;

impl SchemaMigration {
    pub fn generate_create_table(
        schema: &SchemaDefinition,
        dialect: SqlDialect,
    ) -> (String, Vec<String>) {
        let table = schema.table_name();

        let mut col_defs = match dialect {
            SqlDialect::Sqlite => vec![
                "id TEXT PRIMARY KEY".to_string(),
                "content_type TEXT NOT NULL".to_string(),
                "title TEXT NOT NULL".to_string(),
                "slug TEXT NOT NULL UNIQUE".to_string(),
                "status TEXT NOT NULL DEFAULT 'draft'".to_string(),
                "blocks_json TEXT NOT NULL DEFAULT '[]'".to_string(),
                "author_id TEXT".to_string(),
                "published_at TEXT".to_string(),
                "created_at TEXT NOT NULL".to_string(),
                "updated_at TEXT NOT NULL".to_string(),
            ],
            SqlDialect::Postgres => vec![
                "id VARCHAR(64) PRIMARY KEY".to_string(),
                "content_type VARCHAR(64) NOT NULL".to_string(),
                "title VARCHAR(255) NOT NULL".to_string(),
                "slug VARCHAR(255) NOT NULL UNIQUE".to_string(),
                "status VARCHAR(32) NOT NULL DEFAULT 'draft'".to_string(),
                "blocks_json TEXT NOT NULL DEFAULT '[]'".to_string(),
                "author_id VARCHAR(64)".to_string(),
                "published_at VARCHAR(64)".to_string(),
                "created_at VARCHAR(64) NOT NULL".to_string(),
                "updated_at VARCHAR(64) NOT NULL".to_string(),
            ],
            SqlDialect::MySql => vec![
                "id VARCHAR(64) PRIMARY KEY".to_string(),
                "content_type VARCHAR(64) NOT NULL".to_string(),
                "title VARCHAR(255) NOT NULL".to_string(),
                "slug VARCHAR(255) NOT NULL UNIQUE".to_string(),
                "status VARCHAR(32) NOT NULL DEFAULT 'draft'".to_string(),
                "blocks_json LONGTEXT NOT NULL".to_string(),
                "author_id VARCHAR(64)".to_string(),
                "published_at VARCHAR(64)".to_string(),
                "created_at VARCHAR(64) NOT NULL".to_string(),
                "updated_at VARCHAR(64) NOT NULL".to_string(),
            ],
        };

        // 一覧（作成日の新しい順、公開状態での絞り込み）のページングで並べ替えを避けるため、
        // `ORDER BY created_at DESC, id DESC` と同じ並びのインデックスを作る
        let standard_indices = [
            ("slug", "slug"),
            ("status", "status"),
            ("created", "created_at, id"),
            ("status_created", "status, created_at, id"),
        ];
        let mut indices: Vec<String> = standard_indices
            .iter()
            .map(|(name, columns)| match dialect {
                SqlDialect::Sqlite | SqlDialect::Postgres => format!(
                    "CREATE INDEX IF NOT EXISTS idx_{}_{} ON {} ({});",
                    table, name, table, columns
                ),
                SqlDialect::MySql => format!(
                    "CREATE INDEX idx_{}_{} ON {} ({});",
                    table, name, table, columns
                ),
            })
            .collect();

        for field in &schema.fields {
            // 予約語やデフォルトカラムと被らない場合のみ追加
            if [
                "id",
                "content_type",
                "title",
                "slug",
                "status",
                "blocks_json",
                "author_id",
                "published_at",
                "created_at",
                "updated_at",
            ]
            .contains(&field.name.as_str())
            {
                continue;
            }

            let sql_type = match (field.field_type, dialect) {
                (FieldType::String, SqlDialect::Sqlite) => "TEXT",
                (FieldType::String, SqlDialect::Postgres) => "VARCHAR(255)",
                (FieldType::String, SqlDialect::MySql) => "VARCHAR(255)",

                (FieldType::Text, SqlDialect::Sqlite) => "TEXT",
                (FieldType::Text, SqlDialect::Postgres) => "TEXT",
                (FieldType::Text, SqlDialect::MySql) => "LONGTEXT",

                (FieldType::Integer, SqlDialect::Sqlite) => "INTEGER",
                (FieldType::Integer, SqlDialect::Postgres) => "BIGINT",
                (FieldType::Integer, SqlDialect::MySql) => "BIGINT",

                (FieldType::Boolean, SqlDialect::Sqlite) => "INTEGER",
                (FieldType::Boolean, SqlDialect::Postgres) => "BOOLEAN",
                (FieldType::Boolean, SqlDialect::MySql) => "BOOLEAN",

                (FieldType::Timestamp, SqlDialect::Sqlite) => "TEXT",
                (FieldType::Timestamp, SqlDialect::Postgres) => "VARCHAR(64)",
                (FieldType::Timestamp, SqlDialect::MySql) => "VARCHAR(64)",

                (FieldType::Relation, SqlDialect::Sqlite) => "TEXT",
                (FieldType::Relation, SqlDialect::Postgres) => "VARCHAR(64)",
                (FieldType::Relation, SqlDialect::MySql) => "VARCHAR(64)",

                (FieldType::Jsonb, SqlDialect::Sqlite) => "TEXT",
                (FieldType::Jsonb, SqlDialect::Postgres) => "JSONB",
                (FieldType::Jsonb, SqlDialect::MySql) => "JSON",
            };

            let mut def = format!("{} {}", field.name, sql_type);
            if field.required && !field.nullable {
                def.push_str(" NOT NULL");
                if let Some(default) = Self::not_null_default(field.field_type, dialect, sql_type) {
                    def.push_str(" DEFAULT ");
                    def.push_str(default);
                }
            }
            if field.unique {
                def.push_str(" UNIQUE");
            }
            col_defs.push(def);

            if field.index {
                match dialect {
                    SqlDialect::Sqlite | SqlDialect::Postgres => {
                        indices.push(format!(
                            "CREATE INDEX IF NOT EXISTS idx_{}_{} ON {} ({});",
                            table, field.name, table, field.name
                        ));
                    }
                    SqlDialect::MySql => {
                        indices.push(format!(
                            "CREATE INDEX idx_{}_{} ON {} ({});",
                            table, field.name, table, field.name
                        ));
                    }
                }
            }
        }

        let create_table_sql = format!(
            "CREATE TABLE IF NOT EXISTS {} (\n  {}\n);",
            table,
            col_defs.join(",\n  ")
        );

        (create_table_sql, indices)
    }

    /// `NOT NULL` 列に付与する、列型に合った既定値リテラル（付与できない型は `None`）
    fn not_null_default(
        field_type: FieldType,
        dialect: SqlDialect,
        sql_type: &str,
    ) -> Option<&'static str> {
        match (field_type, dialect) {
            (FieldType::Integer, _) => Some("0"),
            (FieldType::Boolean, SqlDialect::Sqlite) => Some("0"),
            (FieldType::Boolean, _) => Some("FALSE"),
            // MySQL の JSON 型はリテラルの既定値を持てない
            (FieldType::Jsonb, SqlDialect::MySql) => None,
            (FieldType::Jsonb, _) => Some("'{}'"),
            // MySQL の TEXT 系はリテラルの既定値を持てない
            (_, SqlDialect::MySql) if sql_type.contains("TEXT") => None,
            _ => Some("''"),
        }
    }

    pub fn generate_schema_migration_sql(
        schema: &SchemaDefinition,
        dialect: SqlDialect,
    ) -> Vec<String> {
        let (create_table, indices) = Self::generate_create_table(schema, dialect);
        let mut stmts = vec![create_table];
        stmts.extend(indices);
        stmts
    }
}

pub struct SqliteMigration;

impl SqliteMigration {
    pub fn generate_create_table(schema: &SchemaDefinition) -> (String, Vec<String>) {
        SchemaMigration::generate_create_table(schema, SqlDialect::Sqlite)
    }

    pub fn generate_schema_migration_sql(schema: &SchemaDefinition) -> Vec<String> {
        SchemaMigration::generate_schema_migration_sql(schema, SqlDialect::Sqlite)
    }
}
