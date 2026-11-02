use crate::definition::SchemaDefinition;
use aegis_core::{AegisError, Result};
use std::path::Path;

pub struct SchemaLoader;

impl SchemaLoader {
    pub fn load_from_str(yaml_content: &str) -> Result<SchemaDefinition> {
        serde_yaml::from_str(yaml_content)
            .map_err(|e| AegisError::Schema(format!("Failed to parse schema YAML: {}", e)))
    }

    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<SchemaDefinition> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| AegisError::Schema(format!("Failed to read schema file: {}", e)))?;
        Self::load_from_str(&content)
    }

    pub fn load_dir<P: AsRef<Path>>(dir: P) -> Result<Vec<SchemaDefinition>> {
        let mut schemas = Vec::new();
        let path = dir.as_ref();
        if !path.exists() {
            return Ok(schemas);
        }

        for entry in walkdir::WalkDir::new(path)
            .sort_by_file_name()
            .min_depth(1)
            .max_depth(2)
        {
            let entry = entry.map_err(|e| AegisError::Schema(e.to_string()))?;
            let file_path = entry.path();
            if file_path.is_file() {
                if let Some(ext) = file_path.extension() {
                    if ext == "yaml" || ext == "yml" {
                        tracing::info!("Loading schema from {:?}", file_path);
                        let schema = Self::load_from_file(file_path)?;
                        schemas.push(schema);
                    }
                }
            }
        }

        schemas.sort_by(|a, b| a.content_type.cmp(&b.content_type));
        Ok(schemas)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_dir_deterministic_sorting() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let dir_path = temp_dir.path();

        // Create schema files in an order opposite to content_type sorting
        std::fs::write(
            dir_path.join("01_product.yaml"),
            "content_type: product\nlabel: Product\n",
        )
        .unwrap();
        std::fs::write(
            dir_path.join("02_page.yaml"),
            "content_type: page\nlabel: Page\n",
        )
        .unwrap();
        std::fs::write(
            dir_path.join("03_category.yaml"),
            "content_type: category\nlabel: Category\n",
        )
        .unwrap();
        std::fs::write(
            dir_path.join("04_article.yaml"),
            "content_type: article\nlabel: Article\n",
        )
        .unwrap();

        let schemas = SchemaLoader::load_dir(dir_path).expect("load_dir should succeed");
        let content_types: Vec<&str> = schemas.iter().map(|s| s.content_type.as_str()).collect();
        assert_eq!(
            content_types,
            vec!["article", "category", "page", "product"]
        );
    }
}
