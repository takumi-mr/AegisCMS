//! 宣言的クエリ（docs/queries.md）: 公開・取り下げに追従して結果が変わり、テーマの `queries` に出る

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use aegis_core::PageRenderContext;
use aegis_db::{ContentPublicationChange, ContentRecord, ContentVersionRecord};
use aegis_runtime::PluginSupervisor;
use aegis_schema::SchemaLoader;
use serde_json::json;

use crate::config::AegisConfig;
use crate::queries::QueryService;
use crate::state::AppState;
use crate::test_support::test_app_state;

fn product(id: &str, brand: &str, price: i64, on_sale: bool, day: u32) -> ContentRecord {
    ContentRecord {
        id: id.to_string(),
        content_type: "product".to_string(),
        title: format!("Product {id}"),
        slug: format!("p-{id}"),
        status: "published".to_string(),
        blocks_json: "[]".to_string(),
        author_id: None,
        published_at: Some(format!("2026-03-{day:02}T00:00:00Z")),
        unpublish_at: None,
        created_at: format!("2026-03-{day:02}T00:00:00Z"),
        updated_at: format!("2026-03-{day:02}T00:00:00Z"),
        custom_fields: HashMap::from([
            ("brand".to_string(), json!(brand)),
            ("price".to_string(), json!(price)),
            ("on_sale".to_string(), json!(on_sale)),
        ]),
    }
}

async fn render(state: &AppState) -> String {
    state
        .theme
        .render("q.html", &PageRenderContext::default())
        .unwrap()
        .trim()
        .to_string()
}

/// 期待する出力になるまで待つ（知らせはバス経由で届くため）
async fn render_until(state: &AppState, expected: &str) -> String {
    let mut last = String::new();
    for _ in 0..100 {
        last = render(state).await;
        if last == expected {
            return last;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    last
}

#[tokio::test]
async fn queries_follow_publication_and_render_in_the_theme() {
    let schema = SchemaLoader::load_from_str(
        "content_type: product\nlabel: P\nfields:\n  - name: price\n    type: integer\n    required: true\n  - name: brand\n    type: string\n  - name: on_sale\n    type: boolean\n",
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let queries_dir = dir.path().join("queries");
    std::fs::create_dir_all(&queries_dir).unwrap();
    std::fs::write(
        queries_dir.join("by_brand.yaml"),
        "name: by_brand\nfrom: product\ngroup_by: [{ field: brand }]\nselect:\n  - { fn: count, as: n }\n  - { fn: sum, field: price, as: total }\norder_by: [{ field: total, dir: desc }]\n",
    )
    .unwrap();
    std::fs::write(
        queries_dir.join("latest_sale.yaml"),
        "name: latest_sale\nfrom: product\nwhere: { on_sale: { eq: true } }\norder_by: [{ field: published_at, dir: desc }]\nlimit: 2\n",
    )
    .unwrap();

    let mut state = test_app_state(
        AegisConfig::default(),
        PluginSupervisor::new(),
        vec![schema.clone()],
    )
    .await;
    let schemas = state.schemas.read().await.clone();
    let service = Arc::new(
        QueryService::load(&state.config.queries, &queries_dir, &schemas).expect("queries load"),
    );
    state.queries = service.clone();
    state.theme = Arc::new(
        aegis_theme::ThemeEngine::new(std::path::Path::new("themes"), "default")
            .with_queries(service.handle()),
    );
    state.theme.register_template(
        "q.html",
        "{% for r in queries.by_brand %}{{ r.brand }}={{ r.n }}/{{ r.total }};{% endfor %}|{% for p in queries.latest_sale %}{{ p.title }};{% endfor %}",
    );
    let state = Arc::new(state);
    crate::cluster::spawn_listener(state.clone(), state.bus.subscribe());
    state.queries.prepare(&state).await.unwrap();
    assert_eq!(render(&state).await, "|");

    let publish = |record: ContentRecord, change: ContentPublicationChange| {
        let state = state.clone();
        let schema = schema.clone();
        async move {
            let version = ContentVersionRecord {
                content_type: record.content_type.clone(),
                content_id: record.id.clone(),
                revision: 0,
                snapshot_json: serde_json::to_string(&record).unwrap(),
                created_at: record.updated_at.clone(),
                created_by: None,
                change_note: None,
            };
            if state
                .db
                .get_content(&schema, &record.id)
                .await
                .unwrap()
                .is_some()
            {
                state
                    .db
                    .update_content_with_versions(&schema, &record, &[version], change)
                    .await
                    .unwrap();
            } else {
                state
                    .db
                    .insert_content_with_versions(&schema, &record, &[version], change)
                    .await
                    .unwrap();
            }
            // 本番の書き込み経路と同じ、変更の知らせ
            crate::cdn::purge_content(&state, "product", &record.id, &record.slug, None).await;
        }
    };

    publish(
        product("a1", "A", 1000, true, 1),
        ContentPublicationChange::PublishVersion(0),
    )
    .await;
    publish(
        product("a2", "A", 3000, false, 2),
        ContentPublicationChange::PublishVersion(0),
    )
    .await;
    publish(
        product("b1", "B", 500, true, 3),
        ContentPublicationChange::PublishVersion(0),
    )
    .await;
    assert_eq!(
        render_until(&state, "A=2/4000;B=1/500;|Product b1;Product a1;").await,
        "A=2/4000;B=1/500;|Product b1;Product a1;"
    );

    // 取り下げると結果から消える
    publish(
        product("a2", "A", 3000, false, 2),
        ContentPublicationChange::Unpublish,
    )
    .await;
    assert_eq!(
        render_until(&state, "A=1/1000;B=1/500;|Product b1;Product a1;").await,
        "A=1/1000;B=1/500;|Product b1;Product a1;"
    );

    // 別の種類のコンテンツの変更は、このクエリを計算し直さない（知らせも出さない）
    let before = state.queries.handle().read().unwrap().clone();
    crate::queries::content_changed(&state, "article").await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(Arc::ptr_eq(
        &before,
        &state.queries.handle().read().unwrap()
    ));
}

#[tokio::test]
async fn a_broken_query_stops_the_server_from_starting() {
    let schema = SchemaLoader::load_from_str(
        "content_type: product\nlabel: P\nfields:\n  - name: price\n    type: integer\n",
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("bad.yaml"),
        "name: bad\nfrom: product\nwhere: { price: { eq: null } }\n",
    )
    .unwrap();
    let schemas = HashMap::from([("product".to_string(), schema)]);
    let err = QueryService::load(&AegisConfig::default().queries, dir.path(), &schemas)
        .err()
        .expect("must be refused")
        .to_string();
    assert!(err.contains("use 'is_null'"), "{err}");
}
