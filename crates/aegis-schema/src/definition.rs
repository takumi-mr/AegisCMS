use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    String,
    Text,
    Integer,
    Boolean,
    Timestamp,
    Relation,
    Jsonb,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldDefinition {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: FieldType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub nullable: bool,
    #[serde(default)]
    pub unique: bool,
    #[serde(default)]
    pub index: bool,
    #[serde(default)]
    pub max_length: Option<usize>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub indexed_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchemaDefinition {
    pub content_type: String,
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub fields: Vec<FieldDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationTarget {
    pub content_type: String,
    pub field: String,
}

impl RelationTarget {
    pub fn parse(target: &str) -> Option<Self> {
        let parts: Vec<&str> = target.split('.').collect();
        match parts.as_slice() {
            [ct, field] => Some(Self {
                content_type: ct.to_string(),
                field: field.to_string(),
            }),
            [ct] => Some(Self {
                content_type: ct.to_string(),
                field: "id".to_string(),
            }),
            _ => None,
        }
    }
}

impl SchemaDefinition {
    pub fn table_name(&self) -> String {
        format!("content_{}", self.content_type.replace('-', "_"))
    }

    /// Return list of fields that define relations to other content types along with parsed target
    pub fn relation_fields(&self) -> Vec<(&FieldDefinition, RelationTarget)> {
        self.fields
            .iter()
            .filter(|f| f.field_type == FieldType::Relation)
            .filter_map(|f| {
                f.target
                    .as_deref()
                    .and_then(RelationTarget::parse)
                    .map(|t| (f, t))
            })
            .collect()
    }
}
