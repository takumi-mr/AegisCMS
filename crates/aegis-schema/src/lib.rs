pub mod codegen;
pub mod definition;
pub mod loader;
pub mod migration;

pub use codegen::{OpenApiGenerator, TypeScriptGenerator};
pub use definition::{FieldDefinition, FieldType, SchemaDefinition};
pub use loader::SchemaLoader;
pub use migration::SqliteMigration;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_schema_and_generate_sql() {
        let yaml = r#"
content_type: article
label: "技術記事"
fields:
  - name: title
    type: string
    required: true
    max_length: 255
    index: true
  - name: slug
    type: string
    required: true
    unique: true
  - name: published_at
    type: timestamp
    nullable: true
    index: true
  - name: tech_category
    type: relation
    target: category.id
  - name: custom_meta
    type: jsonb
"#;

        let schema = SchemaLoader::load_from_str(yaml).expect("YAML parsing should succeed");
        assert_eq!(schema.content_type, "article");
        assert_eq!(schema.fields.len(), 5);

        let sqls = SqliteMigration::generate_schema_migration_sql(&schema);
        assert!(!sqls.is_empty());
        let create_table = &sqls[0];
        assert!(create_table.contains("CREATE TABLE IF NOT EXISTS content_article"));
        assert!(create_table.contains("tech_category TEXT"));
        assert!(create_table.contains("custom_meta TEXT"));

        // インデックス確認
        let indices_joined = sqls[1..].join("\n");
        assert!(indices_joined.contains("idx_content_article_slug"));
    }
}
