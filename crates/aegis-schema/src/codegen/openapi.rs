use serde_json::{json, Map, Value};

use super::to_pascal_case;
use crate::definition::{FieldDefinition, FieldType, SchemaDefinition};

pub struct OpenApiGenerator;

impl OpenApiGenerator {
    /// Generate complete OpenAPI 3.1.0 specification as a serde_json::Value.
    pub fn generate_value(schemas: &[SchemaDefinition]) -> Value {
        let mut paths = Map::new();

        // 1. Core Endpoints
        paths.insert(
            "/api/v1/health".to_string(),
            json!({
                "get": {
                    "tags": ["System"],
                    "summary": "Health check",
                    "description": "Check if AegisCMS core service and database connection are healthy.",
                    "responses": {
                        "200": {
                            "description": "Service is healthy",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "status": { "type": "string", "example": "ok" },
                                            "version": { "type": "string", "example": "0.1.0" },
                                            "database": { "type": "string", "example": "connected" }
                                        },
                                        "required": ["status", "version", "database"]
                                    }
                                }
                            }
                        }
                    }
                }
            }),
        );

        paths.insert(
            "/api/v1/schemas".to_string(),
            json!({
                "get": {
                    "tags": ["Schemas"],
                    "summary": "List all content schemas",
                    "description": "Retrieve list of all active declarative schemas loaded by AegisCMS.",
                    "responses": {
                        "200": {
                            "description": "List of schemas",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": { "$ref": "#/components/schemas/SchemaDefinition" }
                                    }
                                }
                            }
                        }
                    }
                }
            }),
        );

        paths.insert(
            "/api/v1/schemas/{content_type}".to_string(),
            json!({
                "get": {
                    "tags": ["Schemas"],
                    "summary": "Get schema definition",
                    "parameters": [
                        {
                            "name": "content_type",
                            "in": "path",
                            "required": true,
                            "schema": { "type": "string" },
                            "description": "Content type identifier (e.g. article)"
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Schema definition details",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/SchemaDefinition" }
                                }
                            }
                        },
                        "404": {
                            "description": "Schema not found"
                        }
                    }
                }
            }),
        );

        paths.insert(
            "/api/v1/contents/{content_type}".to_string(),
            json!({
                "get": {
                    "tags": ["Contents"],
                    "summary": "List contents by content type",
                    "parameters": [
                        {
                            "name": "content_type",
                            "in": "path",
                            "required": true,
                            "schema": { "type": "string" }
                        },
                        {
                            "name": "limit",
                            "in": "query",
                            "required": false,
                            "description": "Page size (at most 500)",
                            "schema": { "type": "integer", "default": 50, "minimum": 1, "maximum": 500 }
                        },
                        {
                            "name": "offset",
                            "in": "query",
                            "required": false,
                            "schema": { "type": "integer", "default": 0 }
                        },
                        {
                            "name": "status",
                            "in": "query",
                            "required": false,
                            "schema": { "$ref": "#/components/schemas/ContentStatus" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "One page of content records (newest first) and the number of matching records",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "required": ["contents", "total", "limit", "offset"],
                                        "properties": {
                                            "contents": {
                                                "type": "array",
                                                "items": { "$ref": "#/components/schemas/BaseContentItem" }
                                            },
                                            "total": { "type": "integer" },
                                            "limit": { "type": "integer" },
                                            "offset": { "type": "integer" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                "post": {
                    "tags": ["Contents"],
                    "summary": "Create new content",
                    "parameters": [
                        {
                            "name": "content_type",
                            "in": "path",
                            "required": true,
                            "schema": { "type": "string" }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/CreateContentPayload" }
                            }
                        }
                    },
                    "responses": {
                        "201": {
                            "description": "Content successfully created",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/BaseContentItem" }
                                }
                            }
                        },
                        "400": { "description": "Invalid payload, validation failed or an on-before-save plugin hook failed" },
                        "404": { "description": "Schema content_type not registered" },
                        "422": { "description": "Rejected by an on-before-save plugin hook, or a JSON body that does not match the request type" }
                    }
                }
            }),
        );

        paths.insert(
            "/api/v1/contents/{content_type}/{slug}".to_string(),
            json!({
                "get": {
                    "tags": ["Contents"],
                    "summary": "Get content by slug",
                    "parameters": [
                        {
                            "name": "content_type",
                            "in": "path",
                            "required": true,
                            "schema": { "type": "string" }
                        },
                        {
                            "name": "slug",
                            "in": "path",
                            "required": true,
                            "schema": { "type": "string" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Content record details",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/BaseContentItem" }
                                }
                            }
                        },
                        "404": { "description": "Content not found" }
                    }
                }
            }),
        );

        // 2. Component Schemas
        let mut component_schemas = Map::new();

        component_schemas.insert(
            "ContentStatus".to_string(),
            json!({
                "type": "string",
                "enum": ["draft", "published", "archived"]
            }),
        );

        component_schemas.insert(
            "BlockNode".to_string(),
            json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "example": "core/heading" },
                    "attributes": {
                        "type": "object",
                        "additionalProperties": true
                    },
                    "children": {
                        "type": "array",
                        "items": { "$ref": "#/components/schemas/BlockNode" }
                    },
                    "text": { "type": "string" }
                },
                "required": ["name"]
            }),
        );

        component_schemas.insert(
            "FieldDefinition".to_string(),
            json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "type": {
                        "type": "string",
                        "enum": ["string", "text", "integer", "boolean", "timestamp", "relation", "jsonb"]
                    },
                    "required": { "type": "boolean" },
                    "nullable": { "type": "boolean" },
                    "unique": { "type": "boolean" },
                    "index": { "type": "boolean" },
                    "max_length": { "type": "integer" },
                    "target": { "type": "string" },
                    "indexed_paths": {
                        "type": "array",
                        "items": { "type": "string" }
                    }
                },
                "required": ["name", "type"]
            }),
        );

        component_schemas.insert(
            "SchemaDefinition".to_string(),
            json!({
                "type": "object",
                "properties": {
                    "content_type": { "type": "string" },
                    "label": { "type": "string" },
                    "description": { "type": "string" },
                    "fields": {
                        "type": "array",
                        "items": { "$ref": "#/components/schemas/FieldDefinition" }
                    }
                },
                "required": ["content_type", "label", "fields"]
            }),
        );

        component_schemas.insert(
            "BaseContentItem".to_string(),
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "format": "uuid" },
                    "content_type": { "type": "string" },
                    "title": { "type": "string" },
                    "slug": { "type": "string" },
                    "status": { "$ref": "#/components/schemas/ContentStatus" },
                    "blocks": {
                        "type": "array",
                        "items": { "$ref": "#/components/schemas/BlockNode" }
                    },
                    "rendered_html": { "type": "string" },
                    "author_id": { "type": "string", "nullable": true },
                    "published_at": { "type": "string", "format": "date-time", "nullable": true },
                    "created_at": { "type": "string", "format": "date-time" },
                    "updated_at": { "type": "string", "format": "date-time" }
                },
                "required": ["id", "content_type", "title", "slug", "status", "blocks", "rendered_html", "created_at", "updated_at"]
            }),
        );

        component_schemas.insert(
            "CreateContentPayload".to_string(),
            json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string" },
                    "slug": { "type": "string" },
                    "status": { "$ref": "#/components/schemas/ContentStatus" },
                    "blocks": {
                        "type": "array",
                        "items": { "$ref": "#/components/schemas/BlockNode" }
                    },
                    "author_id": { "type": "string", "nullable": true }
                },
                "required": ["title", "slug"]
            }),
        );

        // 3. Dynamic schema-specific components
        for schema in schemas {
            let pascal = to_pascal_case(&schema.content_type);
            let mut props = Map::new();
            let mut required_fields = Vec::new();

            for field in &schema.fields {
                let mut prop_schema = Self::field_to_openapi_property(field);
                if let Some(desc) = &schema.description {
                    if field.name == "title" {
                        prop_schema["description"] = json!(desc);
                    }
                }
                props.insert(field.name.clone(), prop_schema);

                if field.required && !field.nullable {
                    required_fields.push(json!(field.name));
                }
            }

            component_schemas.insert(
                format!("{}Content", pascal),
                json!({
                    "type": "object",
                    "description": format!("Content model for {}", schema.label),
                    "allOf": [
                        { "$ref": "#/components/schemas/BaseContentItem" },
                        {
                            "type": "object",
                            "properties": props,
                            "required": required_fields
                        }
                    ]
                }),
            );
        }

        // Return complete root OpenAPI Object
        json!({
            "openapi": "3.1.0",
            "info": {
                "title": "AegisCMS REST API",
                "version": "1.0.0",
                "description": "Zero-Trust Extensible Content Management System REST API specification."
            },
            "paths": paths,
            "components": {
                "schemas": component_schemas
            }
        })
    }

    /// Generate OpenAPI specification as a formatted JSON string.
    pub fn generate_json(schemas: &[SchemaDefinition]) -> String {
        serde_json::to_string_pretty(&Self::generate_value(schemas))
            .unwrap_or_else(|_| "{}".to_string())
    }

    fn field_to_openapi_property(field: &FieldDefinition) -> Value {
        let mut obj = Map::new();

        match &field.field_type {
            FieldType::String => {
                obj.insert("type".to_string(), json!("string"));
                if let Some(max) = field.max_length {
                    obj.insert("maxLength".to_string(), json!(max));
                }
            }
            FieldType::Text => {
                obj.insert("type".to_string(), json!("string"));
            }
            FieldType::Integer => {
                obj.insert("type".to_string(), json!("integer"));
            }
            FieldType::Boolean => {
                obj.insert("type".to_string(), json!("boolean"));
            }
            FieldType::Timestamp => {
                obj.insert("type".to_string(), json!("string"));
                obj.insert("format".to_string(), json!("date-time"));
            }
            FieldType::Relation => {
                obj.insert("type".to_string(), json!("string"));
                if let Some(target) = &field.target {
                    obj.insert(
                        "description".to_string(),
                        json!(format!("Foreign key relation to {}", target)),
                    );
                }
            }
            FieldType::Jsonb => {
                obj.insert("type".to_string(), json!("object"));
                obj.insert("additionalProperties".to_string(), json!(true));
            }
        }

        if field.nullable {
            obj.insert("nullable".to_string(), json!(true));
        }

        Value::Object(obj)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definition::{FieldDefinition, FieldType, SchemaDefinition};

    #[test]
    fn test_generate_openapi() {
        let schema = SchemaDefinition {
            content_type: "article".to_string(),
            label: "技術記事".to_string(),
            description: Some("技術ブログ".to_string()),
            fields: vec![
                FieldDefinition {
                    name: "title".to_string(),
                    field_type: FieldType::String,
                    required: true,
                    nullable: false,
                    unique: false,
                    index: true,
                    max_length: Some(255),
                    target: None,
                    indexed_paths: vec![],
                },
                FieldDefinition {
                    name: "published_at".to_string(),
                    field_type: FieldType::Timestamp,
                    required: false,
                    nullable: true,
                    unique: false,
                    index: true,
                    max_length: None,
                    target: None,
                    indexed_paths: vec![],
                },
            ],
        };

        let val = OpenApiGenerator::generate_value(&[schema]);
        assert_eq!(val["openapi"], "3.1.0");
        assert!(val["paths"]["/api/v1/health"].is_object());
        assert!(val["components"]["schemas"]["ArticleContent"].is_object());
        assert!(val["components"]["schemas"]["BlockNode"].is_object());

        let json_str = OpenApiGenerator::generate_json(&[]);
        assert!(json_str.contains("AegisCMS REST API"));
    }
}
