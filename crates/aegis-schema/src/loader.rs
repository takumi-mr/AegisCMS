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

        for entry in walkdir::WalkDir::new(path).min_depth(1).max_depth(2) {
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

        Ok(schemas)
    }
}
