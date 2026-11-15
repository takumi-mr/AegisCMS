use std::sync::Arc;

use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;

use crate::auth::ViewerSession;
use crate::state::AppState;
use aegis_schema::OpenApiGenerator;

/// Handler for `GET /api/v1/openapi.json`. The spec lists every schema and endpoint, so it
/// needs the same `viewer` role as `GET /api/v1/schemas`
pub async fn get_openapi_spec(
    State(state): State<Arc<AppState>>,
    _session: ViewerSession,
) -> impl IntoResponse {
    let schemas = state.schemas.read().await;
    let schemas_vec: Vec<_> = schemas.values().cloned().collect();
    let spec = OpenApiGenerator::generate_value(&schemas_vec);
    Json(spec)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap};

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use serde_json::{json, Value};
    use tower::ServiceExt;

    use crate::handlers::content::{ContentResponse, ContentSummaryResponse, CreateContentRequest};

    fn resolve_schema<'a>(schema: &'a Value, components: &'a Value) -> &'a Value {
        if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
            let name = reference
                .strip_prefix("#/components/schemas/")
                .unwrap_or_else(|| panic!("unexpected $ref: {reference}"));
            components
                .get(name)
                .unwrap_or_else(|| panic!("missing component schema: {name}"))
        } else {
            schema
        }
    }

    fn collect_object_properties<'a>(
        schema: &'a Value,
        components: &'a Value,
        out: &mut BTreeSet<&'a str>,
        free_keys: &mut bool,
    ) {
        let schema = resolve_schema(schema, components);
        if let Some(branches) = schema.get("allOf").and_then(Value::as_array) {
            for branch in branches {
                collect_object_properties(branch, components, out, free_keys);
            }
            return;
        }
        if schema
            .get("additionalProperties")
            .map(|ap| ap == &Value::Bool(true) || ap.is_object())
            .unwrap_or(false)
        {
            *free_keys = true;
        }
        if let Some(props) = schema.get("properties").and_then(Value::as_object) {
            out.extend(props.keys().map(String::as_str));
        }
    }

    fn assert_matches_schema(value: &Value, schema: &Value, components: &Value, ctx: &str) {
        if let Err(err) = validate_schema(value, schema, components, ctx, true) {
            panic!("{err}");
        }
    }

    fn validate_schema(
        value: &Value,
        schema: &Value,
        components: &Value,
        ctx: &str,
        check_unknown: bool,
    ) -> Result<(), String> {
        let schema = resolve_schema(schema, components);

        if let Some(branches) = schema.get("oneOf").and_then(Value::as_array) {
            let mut errors = Vec::new();
            for branch in branches {
                match validate_schema(value, branch, components, ctx, check_unknown) {
                    Ok(()) => return Ok(()),
                    Err(e) => errors.push(e),
                }
            }
            return Err(format!(
                "{ctx}: value {value} did not match any oneOf branch: {}",
                errors.join("; ")
            ));
        }

        if let Some(branches) = schema.get("allOf").and_then(Value::as_array) {
            if check_unknown {
                if let Some(obj) = value.as_object() {
                    let mut allowed = BTreeSet::new();
                    let mut free_keys = false;
                    collect_object_properties(schema, components, &mut allowed, &mut free_keys);
                    if !free_keys && !allowed.is_empty() {
                        for key in obj.keys() {
                            if !allowed.contains(key.as_str()) {
                                return Err(format!(
                                    "{ctx}: undocumented field `{key}` in response {value}"
                                ));
                            }
                        }
                    }
                }
            }
            for branch in branches {
                validate_schema(value, branch, components, ctx, false)?;
            }
            return Ok(());
        }

        if value.is_null() {
            if schema.get("nullable").and_then(Value::as_bool) != Some(true) {
                return Err(format!("{ctx}: got null for non-nullable schema {schema}"));
            }
            return Ok(());
        }

        match schema.get("type").and_then(Value::as_str) {
            Some("object") => {
                let obj = value
                    .as_object()
                    .ok_or_else(|| format!("{ctx}: expected object, got {value}"))?;
                let props = schema.get("properties").and_then(Value::as_object);
                if let Some(required) = schema.get("required").and_then(Value::as_array) {
                    for req in required {
                        let key = req.as_str().unwrap();
                        let val = obj.get(key).ok_or_else(|| {
                            format!("{ctx}: missing required key `{key}` in {value}")
                        })?;
                        if val.is_null() {
                            let nullable = props
                                .and_then(|p| p.get(key))
                                .and_then(|p| resolve_schema(p, components).get("nullable"))
                                .and_then(Value::as_bool);
                            if nullable != Some(true) {
                                return Err(format!(
                                    "{ctx}: required key `{key}` is null without nullable: true"
                                ));
                            }
                        }
                    }
                }
                if let Some(props) = props {
                    let free_keys = schema
                        .get("additionalProperties")
                        .map(|ap| ap == &Value::Bool(true) || ap.is_object())
                        .unwrap_or(false);
                    if check_unknown && !free_keys {
                        for key in obj.keys() {
                            if !props.contains_key(key) {
                                return Err(format!(
                                    "{ctx}: undocumented field `{key}` in response {value}"
                                ));
                            }
                        }
                    }
                    for (key, val) in obj {
                        if let Some(prop_schema) = props.get(key) {
                            validate_schema(
                                val,
                                prop_schema,
                                components,
                                &format!("{ctx}.{key}"),
                                true,
                            )?;
                        } else if let Some(ap_schema) =
                            schema.get("additionalProperties").filter(|v| v.is_object())
                        {
                            validate_schema(
                                val,
                                ap_schema,
                                components,
                                &format!("{ctx}.{key}"),
                                true,
                            )?;
                        }
                    }
                }
            }
            Some("array") => {
                let arr = value
                    .as_array()
                    .ok_or_else(|| format!("{ctx}: expected array, got {value}"))?;
                if let Some(item_schema) = schema.get("items") {
                    for (idx, item) in arr.iter().enumerate() {
                        validate_schema(
                            item,
                            item_schema,
                            components,
                            &format!("{ctx}[{idx}]"),
                            true,
                        )?;
                    }
                }
            }
            Some("string") => {
                let s = value
                    .as_str()
                    .ok_or_else(|| format!("{ctx}: expected string, got {value}"))?;
                if let Some(variants) = schema.get("enum").and_then(Value::as_array) {
                    if !variants.iter().any(|v| v.as_str() == Some(s)) {
                        return Err(format!("{ctx}: `{s}` not in enum {variants:?}"));
                    }
                }
            }
            Some("integer") => {
                if !(value.is_i64() || value.is_u64()) {
                    return Err(format!("{ctx}: expected integer, got {value}"));
                }
            }
            Some("boolean") => {
                if !value.is_boolean() {
                    return Err(format!("{ctx}: expected boolean, got {value}"));
                }
            }
            other => {
                return Err(format!(
                    "{ctx}: unsupported schema type {other:?} in {schema}"
                ));
            }
        }
        Ok(())
    }

    async fn send_json(app: &axum::Router, req: Request<Body>) -> (StatusCode, Value) {
        let res = app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = axum::body::to_bytes(res.into_body(), 1 << 20)
            .await
            .unwrap();
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };
        (status, body)
    }

    #[tokio::test]
    async fn test_openapi_spec_matches_live_endpoints() {
        let mut config = crate::config::AegisConfig::default();
        config.tags.enabled = true;
        let article_schema: aegis_schema::SchemaDefinition = serde_json::from_value(json!({
            "content_type": "article",
            "label": "Articles",
            "fields": [
                { "name": "title", "type": "string", "required": true },
                { "name": "excerpt", "type": "string", "required": false, "nullable": true },
                { "name": "category_id", "type": "string", "required": false, "nullable": true }
            ]
        }))
        .unwrap();
        let state = crate::test_support::test_app_state(
            config,
            aegis_runtime::PluginSupervisor::new(),
            vec![article_schema],
        )
        .await;
        let tags = aegis_tags::TagService::new(state.db.clone(), state.config.tags.clone());
        let state = std::sync::Arc::new(state.with_tags(tags));
        let token = state
            .sessions
            .create_session("u1", "admin", Default::default(), None)
            .await
            .unwrap()
            .id;
        let auth = format!("Bearer {token}");
        let app = crate::app::router(&state.config.clone(), state);

        // Fetch live OpenAPI spec
        let (status, spec) = send_json(
            &app,
            Request::get("/api/v1/openapi.json")
                .header("authorization", &auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let components = &spec["components"]["schemas"];

        // 1. CreateContentRequest <-> CreateContentPayload
        let create_payload_props = components["CreateContentPayload"]["properties"]
            .as_object()
            .unwrap();
        let full_payload = json!({
            "title": "Hello",
            "slug": "hello-world",
            "status": "draft",
            "blocks": [{ "name": "core/paragraph", "attributes": {}, "children": [], "text": "Hi" }],
            "author_id": "u1",
            "published_at": null,
            "unpublish_at": null,
            "publish": false,
            "review_note": "Initial",
            "fields": { "excerpt": "Summary text" }
        });
        let payload_keys: BTreeSet<String> =
            full_payload.as_object().unwrap().keys().cloned().collect();
        let spec_payload_keys: BTreeSet<String> = create_payload_props.keys().cloned().collect();
        assert_eq!(payload_keys, spec_payload_keys);
        // Proves CreateContentRequest (#[serde(deny_unknown_fields)]) accepts all documented properties
        let parsed: CreateContentRequest = serde_json::from_value(full_payload.clone()).unwrap();
        let _exhaustive = CreateContentRequest {
            title: parsed.title,
            slug: parsed.slug,
            status: parsed.status,
            blocks: parsed.blocks,
            author_id: parsed.author_id,
            published_at: parsed.published_at,
            unpublish_at: parsed.unpublish_at,
            publish: parsed.publish,
            review_note: parsed.review_note,
            fields: parsed.fields,
        };

        // 2. ContentResponse <-> BaseContentItem & ContentSummaryResponse <-> ContentSummary
        let sample_relation = aegis_db::ResolvedRelation {
            id: "rel-1".to_string(),
            content_type: "page".to_string(),
            title: "Page".to_string(),
            slug: "page".to_string(),
        };
        let full_resp = ContentResponse {
            id: "id-1".to_string(),
            content_type: "article".to_string(),
            title: "T".to_string(),
            slug: "s".to_string(),
            status: "draft".to_string(),
            blocks: vec![],
            rendered_html: String::new(),
            author_id: None,
            published_at: None,
            unpublish_at: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            comments_open: Some(true),
            fields: HashMap::from([("excerpt".to_string(), json!("E"))]),
            relations: HashMap::from([("r".to_string(), sample_relation.clone())]),
        };
        let full_resp_keys: BTreeSet<String> = serde_json::to_value(&full_resp)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        let base_item_keys: BTreeSet<String> = components["BaseContentItem"]["properties"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert_eq!(full_resp_keys, base_item_keys);

        let summary_resp = ContentSummaryResponse {
            id: "id-1".to_string(),
            content_type: "article".to_string(),
            title: "T".to_string(),
            slug: "s".to_string(),
            status: "draft".to_string(),
            excerpt: "E".to_string(),
            author_id: None,
            published_at: None,
            unpublish_at: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            fields: HashMap::from([("excerpt".to_string(), json!("E"))]),
            relations: HashMap::from([("r".to_string(), sample_relation)]),
        };
        let summary_resp_keys: BTreeSet<String> = serde_json::to_value(&summary_resp)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        let summary_schema_keys: BTreeSet<String> = components["ContentSummary"]["properties"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert_eq!(summary_resp_keys, summary_schema_keys);

        // 3. GET /api/v1/health (anonymous & admin)
        let health_schema = &spec["paths"]["/api/v1/health"]["get"]["responses"]["200"]["content"]
            ["application/json"]["schema"];
        let (status, anon_health) = send_json(
            &app,
            Request::get("/api/v1/health").body(Body::empty()).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_matches_schema(
            &anon_health,
            health_schema,
            components,
            "GET /api/v1/health (anon)",
        );

        let (status, admin_health) = send_json(
            &app,
            Request::get("/api/v1/health")
                .header("authorization", &auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_matches_schema(
            &admin_health,
            health_schema,
            components,
            "GET /api/v1/health (admin)",
        );
        let health_prop_keys: BTreeSet<String> = health_schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        let admin_health_keys: BTreeSet<String> =
            admin_health.as_object().unwrap().keys().cloned().collect();
        assert_eq!(admin_health_keys, health_prop_keys);

        // 4. GET /api/v1/schemas and GET /api/v1/schemas/{content_type}
        let schemas_schema = &spec["paths"]["/api/v1/schemas"]["get"]["responses"]["200"]
            ["content"]["application/json"]["schema"];
        let (status, schemas_body) = send_json(
            &app,
            Request::get("/api/v1/schemas")
                .header("authorization", &auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_matches_schema(
            &schemas_body,
            schemas_schema,
            components,
            "GET /api/v1/schemas",
        );

        let schema_item_schema = &spec["paths"]["/api/v1/schemas/{content_type}"]["get"]
            ["responses"]["200"]["content"]["application/json"]["schema"];
        let (status, schema_body) = send_json(
            &app,
            Request::get("/api/v1/schemas/article")
                .header("authorization", &auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_matches_schema(
            &schema_body,
            schema_item_schema,
            components,
            "GET /api/v1/schemas/article",
        );

        let (status, _) = send_json(
            &app,
            Request::get("/api/v1/schemas/nonexistent")
                .header("authorization", &auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        // 5. POST /api/v1/contents/article
        let create_resp_schema = &spec["paths"]["/api/v1/contents/{content_type}"]["post"]
            ["responses"]["201"]["content"]["application/json"]["schema"];
        let (status, created) = send_json(
            &app,
            Request::post("/api/v1/contents/article")
                .header("authorization", &auth)
                .header("content-type", "application/json")
                .body(Body::from(full_payload.to_string()))
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{created}");
        assert_matches_schema(
            &created,
            create_resp_schema,
            components,
            "POST /api/v1/contents/article",
        );
        assert_matches_schema(
            &created,
            &components["ArticleContent"],
            components,
            "POST /api/v1/contents/article (ArticleContent)",
        );

        // 6. GET /api/v1/contents/article (full, summary, and tag/category filters)
        let list_resp_schema = &spec["paths"]["/api/v1/contents/{content_type}"]["get"]
            ["responses"]["200"]["content"]["application/json"]["schema"];
        for (query, ctx) in [
            (
                "limit=10&offset=0&status=draft&q=Hello&category=none&view=full",
                "list full",
            ),
            ("limit=10&offset=0&view=summary", "list summary"),
            (
                "limit=10&offset=0&category=none&tag=none&view=full",
                "list with tag+category",
            ),
        ] {
            let (status, list_body) = send_json(
                &app,
                Request::get(format!("/api/v1/contents/article?{query}"))
                    .header("authorization", &auth)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{ctx}: {list_body}");
            assert_matches_schema(&list_body, list_resp_schema, components, ctx);
        }

        // 7. GET, PUT, DELETE /api/v1/contents/article/hello-world
        let get_item_schema = &spec["paths"]["/api/v1/contents/{content_type}/{slug}"]["get"]
            ["responses"]["200"]["content"]["application/json"]["schema"];
        let (status, fetched) = send_json(
            &app,
            Request::get("/api/v1/contents/article/hello-world")
                .header("authorization", &auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_matches_schema(
            &fetched,
            get_item_schema,
            components,
            "GET /api/v1/contents/article/hello-world",
        );

        let put_item_schema = &spec["paths"]["/api/v1/contents/{content_type}/{slug}"]["put"]
            ["responses"]["200"]["content"]["application/json"]["schema"];
        let update_payload = json!({
            "title": "Updated Hello",
            "slug": "hello-world",
            "fields": { "excerpt": "Updated summary" }
        });
        let (status, updated) = send_json(
            &app,
            Request::put("/api/v1/contents/article/hello-world")
                .header("authorization", &auth)
                .header("content-type", "application/json")
                .body(Body::from(update_payload.to_string()))
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{updated}");
        assert_matches_schema(
            &updated,
            put_item_schema,
            components,
            "PUT /api/v1/contents/article/hello-world",
        );

        let (status, _) = send_json(
            &app,
            Request::delete("/api/v1/contents/article/hello-world")
                .header("authorization", &auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);

        let (status, _) = send_json(
            &app,
            Request::get("/api/v1/contents/article/hello-world")
                .header("authorization", &auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_openapi_spec_requires_a_session() {
        let state = crate::test_support::test_state(aegis_runtime::PluginSupervisor::new()).await;
        let token = state
            .sessions
            .create_session("u1", "viewer", Default::default(), None)
            .await
            .unwrap()
            .id;
        let app = crate::app::router(&state.config.clone(), state);

        let res = app
            .clone()
            .oneshot(
                Request::get("/api/v1/openapi.json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

        let res = app
            .oneshot(
                Request::get("/api/v1/openapi.json")
                    .header("authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }
}
