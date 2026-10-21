use crate::definition::{FieldType, SchemaDefinition};

pub struct SqliteMigration;

impl SqliteMigration {
    pub fn generate_create_table(schema: &SchemaDefinition) -> (String, Vec<String>) {
        let table = schema.table_name();
        let mut col_defs = vec![
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
        ];

        let mut indices = vec![
            format!(
                "CREATE INDEX IF NOT EXISTS idx_{}_slug ON {} (slug);",
                table, table
            ),
            format!(
                "CREATE INDEX IF NOT EXISTS idx_{}_status ON {} (status);",
                table, table
            ),
        ];

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

            let sql_type = match field.field_type {
                FieldType::String => "TEXT",
                FieldType::Text => "TEXT",
                FieldType::Integer => "INTEGER",
                FieldType::Boolean => "INTEGER",
                FieldType::Timestamp => "TEXT",
                FieldType::Relation => "TEXT",
                FieldType::Jsonb => "TEXT",
            };

            let mut def = format!("{} {}", field.name, sql_type);
            if field.required && !field.nullable {
                def.push_str(" NOT NULL DEFAULT ''");
            }
            if field.unique {
                def.push_str(" UNIQUE");
            }
            col_defs.push(def);

            if field.index {
                indices.push(format!(
                    "CREATE INDEX IF NOT EXISTS idx_{}_{} ON {} ({});",
                    table, field.name, table, field.name
                ));
            }
        }

        let create_table_sql = format!(
            "CREATE TABLE IF NOT EXISTS {} (\n  {}\n);",
            table,
            col_defs.join(",\n  ")
        );

        (create_table_sql, indices)
    }

    pub fn generate_schema_migration_sql(schema: &SchemaDefinition) -> Vec<String> {
        let (create_table, indices) = Self::generate_create_table(schema);
        let mut stmts = vec![create_table];
        stmts.extend(indices);
        stmts
    }
}
