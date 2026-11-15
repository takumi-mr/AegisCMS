//! 配信 API の OpenAPI 3.1。公開する型と、公開してよいフィールドだけを載せる。
//!
//! 編集用の `/api/v1/openapi.json`（全スキーマ・全項目）とは別に、ログインなしで配信 API の
//! 利用者に渡せる仕様にする。型ごとの応答の形は [`crate::view::project`] と一致させる。

use std::collections::HashMap;

use aegis_schema::codegen::to_pascal_case;
use aegis_schema::{OpenApiGenerator, SchemaDefinition};
use serde_json::{json, Map, Value};

use crate::params::DEFAULT_PAGE_SIZE;
use crate::service::HeadlessConfig;
use crate::view::public_field_names;

fn schema_ref(name: &str) -> Value {
    json!({ "$ref": format!("#/components/schemas/{name}") })
}

fn error_response(description: &str) -> Value {
    json!({
        "description": description,
        "content": { "application/json": { "schema": schema_ref("Error") } }
    })
}

/// 型 1 つ分の `fields` の定義（公開してよいフィールドだけ）
fn fields_schema(schema: &SchemaDefinition) -> Value {
    let public: Vec<&str> = public_field_names(schema).collect();
    let properties: Map<String, Value> = schema
        .fields
        .iter()
        .filter(|field| public.contains(&field.name.as_str()))
        .map(|field| {
            (
                field.name.clone(),
                OpenApiGenerator::field_to_openapi_property(field),
            )
        })
        .collect();
    json!({ "type": "object", "properties": properties })
}

fn item_properties(schema: &SchemaDefinition) -> Map<String, Value> {
    let mut properties = Map::new();
    properties.insert("id".into(), json!({ "type": "string" }));
    properties.insert("content_type".into(), json!({ "type": "string" }));
    properties.insert("title".into(), json!({ "type": "string" }));
    properties.insert("slug".into(), json!({ "type": "string" }));
    properties.insert(
        "excerpt".into(),
        json!({
            "type": "string",
            "description": "The `excerpt` field when it is public, otherwise the start of the first paragraph"
        }),
    );
    properties.insert(
        "published_at".into(),
        json!({ "type": "string", "format": "date-time", "nullable": true }),
    );
    properties.insert(
        "updated_at".into(),
        json!({ "type": "string", "format": "date-time" }),
    );
    properties.insert("fields".into(), fields_schema(schema));
    properties.insert(
        "relations".into(),
        json!({
            "type": "object",
            "description": "Published content the relation fields point to (a relation to unpublished content is left out)",
            "additionalProperties": schema_ref("PublicRelation")
        }),
    );
    properties
}

const REQUIRED: [&str; 6] = [
    "id",
    "content_type",
    "title",
    "slug",
    "excerpt",
    "updated_at",
];

/// 公開用の OpenAPI。`config.content_types` にあり、`schemas` に定義がある型だけを載せる
pub fn public_openapi(
    schemas: &HashMap<String, SchemaDefinition>,
    config: &HeadlessConfig,
) -> Value {
    let mut types: Vec<&SchemaDefinition> = config
        .content_types
        .iter()
        .filter_map(|name| schemas.get(name))
        .collect();
    types.sort_by(|a, b| a.content_type.cmp(&b.content_type));
    types.dedup_by(|a, b| a.content_type == b.content_type);

    let mut paths = Map::new();
    let mut components = Map::new();

    components.insert(
        "Error".into(),
        json!({
            "type": "object",
            "properties": { "error": { "type": "string" } },
            "required": ["error"]
        }),
    );
    components.insert(
        "PublicRelation".into(),
        json!({
            "type": "object",
            "properties": {
                "id": { "type": "string" },
                "content_type": { "type": "string" },
                "title": { "type": "string" },
                "slug": { "type": "string" }
            },
            "required": ["id", "content_type", "title", "slug"]
        }),
    );
    components.insert(
        "PublicBlock".into(),
        json!({
            "type": "object",
            "description": "A block of the body. `core/html` blocks hold the stored HTML as it is (sanitize it before inserting it into a page, or use `?include=html`)",
            "properties": {
                "name": { "type": "string" },
                "attributes": { "type": "object", "additionalProperties": true },
                "children": { "type": "array", "items": schema_ref("PublicBlock") },
                "text": { "type": "string" }
            },
            "required": ["name"]
        }),
    );

    for schema in types {
        let name = to_pascal_case(&schema.content_type);
        let summary_name = format!("{name}Summary");
        let list_name = format!("{name}List");

        let summary_properties = item_properties(schema);
        let mut full_properties = item_properties(schema);
        full_properties.insert(
            "blocks".into(),
            json!({ "type": "array", "items": schema_ref("PublicBlock") }),
        );
        full_properties.insert(
            "html".into(),
            json!({
                "type": "string",
                "description": "The rendered body. Only with `include=html`"
            }),
        );
        components.insert(
            summary_name.clone(),
            json!({
                "type": "object",
                "description": "One item of a list (no body)",
                "properties": summary_properties,
                "required": REQUIRED
            }),
        );
        components.insert(
            name.clone(),
            json!({
                "type": "object",
                "properties": full_properties,
                "required": REQUIRED
            }),
        );
        components.insert(
            list_name.clone(),
            json!({
                "type": "object",
                "properties": {
                    "items": { "type": "array", "items": schema_ref(&summary_name) },
                    "next_cursor": {
                        "type": "string",
                        "nullable": true,
                        "description": "Pass it as `cursor` to get the next page. Null on the last page"
                    },
                    "total": {
                        "type": "integer",
                        "description": "The number of published items that match, not counting the cursor"
                    }
                },
                "required": ["items", "next_cursor", "total"]
            }),
        );

        let tag = json!([name]);
        paths.insert(
            format!("/api/v1/public/contents/{}", schema.content_type),
            json!({
                "get": {
                    "tags": tag,
                    "summary": format!("List published {} content", schema.content_type),
                    "description": "Newest first. Unpublished content and content past its unpublish date are never returned.",
                    "security": [],
                    "parameters": [
                        {
                            "name": "limit", "in": "query", "required": false,
                            "description": format!("Items per page (default {DEFAULT_PAGE_SIZE}, at most {})", config.max_page_size),
                            "schema": { "type": "integer", "minimum": 1, "maximum": config.max_page_size }
                        },
                        {
                            "name": "cursor", "in": "query", "required": false,
                            "description": "The `next_cursor` of the previous page",
                            "schema": { "type": "string" }
                        },
                        {
                            "name": "category", "in": "query", "required": false,
                            "description": "A category ID, or `none` for content without a category",
                            "schema": { "type": "string" }
                        },
                        {
                            "name": "tag", "in": "query", "required": false,
                            "description": "A tag slug",
                            "schema": { "type": "string" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "A page of published content",
                            "headers": { "ETag": { "schema": { "type": "string" } } },
                            "content": { "application/json": { "schema": schema_ref(&list_name) } }
                        },
                        "304": { "description": "Not modified (`If-None-Match` matched the `ETag`)" },
                        "400": error_response("Invalid `limit` or `cursor`"),
                        "429": error_response("Too many requests from this address")
                    }
                }
            }),
        );
        paths.insert(
            format!("/api/v1/public/contents/{}/{{slug}}", schema.content_type),
            json!({
                "get": {
                    "tags": tag,
                    "summary": format!("Get one published {} item", schema.content_type),
                    "security": [],
                    "parameters": [
                        {
                            "name": "slug", "in": "path", "required": true,
                            "description": "The slug (or the ID) of the content",
                            "schema": { "type": "string" }
                        },
                        {
                            "name": "include", "in": "query", "required": false,
                            "description": "`html` also returns the rendered body (`html`)",
                            "schema": { "type": "string", "enum": ["html"] }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "The published content, with its body",
                            "headers": { "ETag": { "schema": { "type": "string" } } },
                            "content": { "application/json": { "schema": schema_ref(&name) } }
                        },
                        "304": { "description": "Not modified (`If-None-Match` matched the `ETag`)" },
                        "404": error_response("Not published, past its unpublish date, or not found"),
                        "429": error_response("Too many requests from this address")
                    }
                }
            }),
        );
    }

    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "AegisCMS Public Content API",
            "version": "1.0.0",
            "description": "Read-only delivery of published content, without a login."
        },
        "paths": paths,
        "components": { "schemas": components }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_schema::SchemaLoader;

    fn schemas() -> HashMap<String, SchemaDefinition> {
        [
            "content_type: article\nlabel: A\nfields:\n  - name: excerpt\n    type: string\n  - name: internal_note\n    type: string\n    public: false\n",
            "content_type: page\nlabel: P\n",
            "content_type: product\nlabel: Pr\nfields:\n  - name: price\n    type: integer\n",
        ]
        .into_iter()
        .map(|yaml| {
            let schema = SchemaLoader::load_from_str(yaml).unwrap();
            (schema.content_type.clone(), schema)
        })
        .collect()
    }

    fn refs(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                for (key, v) in map {
                    if key == "$ref" {
                        out.push(v.as_str().unwrap().to_string());
                    } else {
                        refs(v, out);
                    }
                }
            }
            Value::Array(items) => items.iter().for_each(|v| refs(v, out)),
            _ => {}
        }
    }

    #[test]
    fn only_the_listed_types_and_public_fields_appear() {
        let spec = public_openapi(&schemas(), &HeadlessConfig::default());
        let paths: Vec<&str> = spec["paths"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            paths,
            vec![
                "/api/v1/public/contents/article",
                "/api/v1/public/contents/article/{slug}",
                "/api/v1/public/contents/page",
                "/api/v1/public/contents/page/{slug}",
            ],
            "product is not in the default list"
        );
        let text = spec.to_string();
        assert!(!text.contains("internal_note"));
        assert!(!text.contains("author_id"));
        // 商品の `price` は出ない（組み込みの `purchase_price` は公開フィールド）
        assert!(!text.contains("\"price\""));

        let article_fields =
            &spec["components"]["schemas"]["Article"]["properties"]["fields"]["properties"];
        assert!(article_fields.get("excerpt").is_some());
        assert!(article_fields.get("category_id").is_some());
        assert!(article_fields.get("internal_note").is_none());
    }

    #[test]
    fn every_reference_resolves() {
        let spec = public_openapi(&schemas(), &HeadlessConfig::default());
        let mut all = Vec::new();
        refs(&spec, &mut all);
        assert!(!all.is_empty());
        for reference in all {
            let name = reference.strip_prefix("#/components/schemas/").unwrap();
            assert!(
                spec["components"]["schemas"].get(name).is_some(),
                "{reference} is missing"
            );
        }
    }

    #[test]
    fn the_page_size_limit_follows_the_config() {
        let config = HeadlessConfig {
            content_types: vec!["product".to_string()],
            max_page_size: 50,
        };
        let spec = public_openapi(&schemas(), &config);
        let limit = &spec["paths"]["/api/v1/public/contents/product"]["get"]["parameters"][0];
        assert_eq!(limit["schema"]["maximum"], 50);
        // 設定にあっても、スキーマの無い型は載せない
        let none = public_openapi(
            &schemas(),
            &HeadlessConfig {
                content_types: vec!["nonexistent".to_string()],
                max_page_size: 10,
            },
        );
        assert!(none["paths"].as_object().unwrap().is_empty());
    }

    #[test]
    fn project_output_matches_openapi_schemas() {
        use std::collections::BTreeSet;

        fn key_set(obj: &Value) -> BTreeSet<String> {
            obj.as_object().unwrap().keys().cloned().collect()
        }

        let all_schemas = schemas();
        let article_schema = &all_schemas["article"];
        let spec = public_openapi(&all_schemas, &HeadlessConfig::default());
        let components = &spec["components"]["schemas"];

        let relation = aegis_db::ResolvedRelation {
            id: "rel-1".to_string(),
            content_type: "page".to_string(),
            title: "About".to_string(),
            slug: "about".to_string(),
        };
        let relations = HashMap::from([("related".to_string(), relation.clone())]);

        let record = aegis_db::ContentRecord {
            id: "art-1".to_string(),
            content_type: "article".to_string(),
            title: "Hello".to_string(),
            slug: "hello".to_string(),
            status: "published".to_string(),
            blocks_json: serde_json::to_string(&vec![aegis_ast::BlockNode {
                name: "core/paragraph".to_string(),
                attributes: Default::default(),
                children: vec![],
                text: Some("Body paragraph".to_string()),
            }])
            .unwrap(),
            author_id: Some("user-1".to_string()),
            published_at: Some("2026-01-01T00:00:00Z".to_string()),
            unpublish_at: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-02T00:00:00Z".to_string(),
            custom_fields: HashMap::from([
                ("excerpt".to_string(), json!("Summary")),
                ("internal_note".to_string(), json!("Secret")),
            ]),
        };

        // 1. ArticleSummary vs project(..., false)
        let summary = crate::view::project(article_schema, &record, &relations, false);
        let summary_schema = &components["ArticleSummary"];
        let summary_props = key_set(&summary_schema["properties"]);
        let summary_keys = key_set(&summary);
        assert_eq!(summary_keys, summary_props);
        for req in summary_schema["required"].as_array().unwrap() {
            assert!(summary.get(req.as_str().unwrap()).is_some());
        }

        // 2. Article vs project(..., true) (only `html` is added by ?include=html)
        let full = crate::view::project(article_schema, &record, &relations, true);
        let full_schema = &components["Article"];
        let full_props = key_set(&full_schema["properties"]);
        let mut full_keys = key_set(&full);
        assert!(!full_keys.contains("html"));
        full_keys.insert("html".to_string());
        assert_eq!(full_keys, full_props);
        for req in full_schema["required"].as_array().unwrap() {
            assert!(full.get(req.as_str().unwrap()).is_some());
        }

        // 3. PublicBlock vs serialized BlockNode
        let block_props = key_set(&components["PublicBlock"]["properties"]);
        for block in full["blocks"].as_array().unwrap() {
            for k in key_set(block) {
                assert!(block_props.contains(&k), "unexpected block key: {k}");
            }
        }

        // 4. PublicRelation vs serialized ResolvedRelation
        let rel_val = serde_json::to_value(&relation).unwrap();
        let rel_schema = &components["PublicRelation"];
        assert_eq!(key_set(&rel_val), key_set(&rel_schema["properties"]));
        let rel_required: BTreeSet<String> = rel_schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        assert_eq!(key_set(&rel_val), rel_required);

        // 5. ArticleList vs serialized ListResponse
        let list_val = serde_json::to_value(crate::service::ListResponse {
            items: vec![summary],
            next_cursor: None,
            total: 1,
        })
        .unwrap();
        let list_schema = &components["ArticleList"];
        assert_eq!(key_set(&list_val), key_set(&list_schema["properties"]));
        let list_required: BTreeSet<String> = list_schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        assert_eq!(key_set(&list_val), list_required);
    }
}
