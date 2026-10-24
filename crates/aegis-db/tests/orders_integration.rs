//! PostgreSQL / MySQL での注文・在庫の実機検証テスト。
//! コンテナ稼働時に実行され、スキーママイグレーション、商品登録、在庫確保、在庫不足ロールバック、
//! 状態遷移、キャンセル時の在庫返却、注文一覧・検索、期限切れ取得などを検証する。

use std::collections::HashMap;
use std::env;

use aegis_core::Result;
use aegis_db::{
    ContentRecord, DatabaseAdapter, MySqlAdapter, OrderListQuery, OrderRecord, OrderStorage,
    OrderTransition, OutOfStock, PostgresAdapter, StockReservation,
};
use aegis_schema::SchemaDefinition;
use serde_json::json;

fn product_schema() -> SchemaDefinition {
    serde_json::from_value(json!({
        "content_type": "product",
        "label": "商品",
        "fields": [
            { "name": "title", "type": "string", "required": true },
            { "name": "price", "type": "integer", "required": true },
            { "name": "stock", "type": "integer", "nullable": true }
        ]
    }))
    .unwrap()
}

async fn run_order_and_stock_lifecycle_test<A>(adapter: &A) -> Result<()>
where
    A: DatabaseAdapter + OrderStorage,
{
    // 1. 初期化とマイグレーション
    adapter.init().await?;
    let schema = product_schema();
    let schemas = HashMap::from([(schema.content_type.clone(), schema.clone())]);
    adapter.run_migrations(&schemas).await?;

    // 2. 商品データの投入
    // p-coffee: 在庫 5
    let coffee = ContentRecord {
        id: "p-coffee".to_string(),
        content_type: "product".to_string(),
        title: "Coffee Beans".to_string(),
        slug: "coffee-beans".to_string(),
        status: "published".to_string(),
        blocks_json: "[]".to_string(),
        author_id: None,
        published_at: Some("2026-10-25T00:00:00Z".to_string()),
        unpublish_at: None,
        created_at: "2026-10-25T00:00:00Z".to_string(),
        updated_at: "2026-10-25T00:00:00Z".to_string(),
        custom_fields: HashMap::from([
            ("price".to_string(), json!(1200)),
            ("stock".to_string(), json!(5)),
        ]),
    };
    adapter.insert_content(&schema, &coffee).await?;

    // p-infinite: 在庫 NULL（無限）
    let infinite = ContentRecord {
        id: "p-infinite".to_string(),
        content_type: "product".to_string(),
        title: "Digital Download".to_string(),
        slug: "digital-download".to_string(),
        status: "published".to_string(),
        blocks_json: "[]".to_string(),
        author_id: None,
        published_at: Some("2026-10-25T00:00:00Z".to_string()),
        unpublish_at: None,
        created_at: "2026-10-25T00:00:00Z".to_string(),
        updated_at: "2026-10-25T00:00:00Z".to_string(),
        custom_fields: HashMap::from([("price".to_string(), json!(2000))]),
    };
    adapter.insert_content(&schema, &infinite).await?;

    // p-rare: 在庫 1
    let rare = ContentRecord {
        id: "p-rare".to_string(),
        content_type: "product".to_string(),
        title: "Rare Vintage".to_string(),
        slug: "rare-vintage".to_string(),
        status: "published".to_string(),
        blocks_json: "[]".to_string(),
        author_id: None,
        published_at: Some("2026-10-25T00:00:00Z".to_string()),
        unpublish_at: None,
        created_at: "2026-10-25T00:00:00Z".to_string(),
        updated_at: "2026-10-25T00:00:00Z".to_string(),
        custom_fields: HashMap::from([
            ("price".to_string(), json!(10000)),
            ("stock".to_string(), json!(1)),
        ]),
    };
    adapter.insert_content(&schema, &rare).await?;

    // 3. 注文作成テスト（正常系: p-coffee x 2, p-infinite x 1）
    let order_id = "ord-001";
    let order1 = OrderRecord {
        id: order_id.to_string(),
        number: "261025-AA1111".to_string(),
        status: "pending_payment".to_string(),
        currency: "jpy".to_string(),
        subtotal: 4400,
        total: 4400,
        lines_json: json!([
            {"product_id": "p-coffee", "title": "Coffee Beans", "quantity": 2, "unit_price": 1200, "total": 2400},
            {"product_id": "p-infinite", "title": "Digital Download", "quantity": 1, "unit_price": 2000, "total": 2000}
        ]).to_string(),
        adjustments_json: "[]".to_string(),
        customer_email: "alice@example.com".to_string(),
        customer_name: "Alice Smith".to_string(),
        shipping_json: None,
        coupon_code: None,
        metadata_json: "{}".to_string(),
        payment_provider: "stripe".to_string(),
        payment_ref: None,
        access_token_hash: "hash001".to_string(),
        expires_at: Some("2026-10-25T01:00:00Z".to_string()),
        paid_at: None,
        created_at: "2026-10-25T00:05:00Z".to_string(),
        updated_at: "2026-10-25T00:05:00Z".to_string(),
        note: None,
    };
    let reservations = vec![
        StockReservation {
            table: "content_product".to_string(),
            product_id: "p-coffee".to_string(),
            quantity: 2,
        },
        StockReservation {
            table: "content_product".to_string(),
            product_id: "p-infinite".to_string(),
            quantity: 1,
        },
    ];
    let res = adapter.create_order(&order1, &reservations).await?;
    assert!(res.is_ok(), "Order creation should succeed");

    // 在庫の確認: p-coffee は 5 -> 3、p-infinite は NULL のまま
    let coffee_after = adapter.get_content(&schema, "p-coffee").await?.unwrap();
    assert_eq!(coffee_after.custom_fields.get("stock"), Some(&json!(3)));
    let infinite_after = adapter.get_content(&schema, "p-infinite").await?.unwrap();
    assert_eq!(infinite_after.custom_fields.get("stock"), None);

    // 4. 在庫不足テスト (p-coffee を残 3 なのに 5 注文しようとする)
    let order_fail = OrderRecord {
        id: "ord-fail-1".to_string(),
        number: "261025-FAIL01".to_string(),
        status: "pending_payment".to_string(),
        currency: "jpy".to_string(),
        subtotal: 6000,
        total: 6000,
        lines_json: "[]".to_string(),
        adjustments_json: "[]".to_string(),
        customer_email: "bob@example.com".to_string(),
        customer_name: "Bob".to_string(),
        shipping_json: None,
        coupon_code: None,
        metadata_json: "{}".to_string(),
        payment_provider: "stripe".to_string(),
        payment_ref: None,
        access_token_hash: "hash_fail".to_string(),
        expires_at: None,
        paid_at: None,
        created_at: "2026-10-25T00:06:00Z".to_string(),
        updated_at: "2026-10-25T00:06:00Z".to_string(),
        note: None,
    };
    let res_fail = adapter
        .create_order(
            &order_fail,
            &[StockReservation {
                table: "content_product".to_string(),
                product_id: "p-coffee".to_string(),
                quantity: 5,
            }],
        )
        .await?;
    assert_eq!(
        res_fail,
        Err(OutOfStock {
            product_id: "p-coffee".to_string()
        })
    );
    // 在庫が減っていないこと（3のまま）
    let coffee_still = adapter.get_content(&schema, "p-coffee").await?.unwrap();
    assert_eq!(coffee_still.custom_fields.get("stock"), Some(&json!(3)));
    // 注文が作成されていないこと
    assert!(adapter.get_order("ord-fail-1").await?.is_none());

    // 5. 複数商品のロールバック検証
    // p-coffee x 1 (在庫3あり) + p-rare x 5 (在庫1のみで不足)
    // p-coffee の引き落としがロールバックされて 3 のままであること
    let res_fail_multi = adapter
        .create_order(
            &order_fail,
            &[
                StockReservation {
                    table: "content_product".to_string(),
                    product_id: "p-coffee".to_string(),
                    quantity: 1,
                },
                StockReservation {
                    table: "content_product".to_string(),
                    product_id: "p-rare".to_string(),
                    quantity: 5,
                },
            ],
        )
        .await?;
    assert_eq!(
        res_fail_multi,
        Err(OutOfStock {
            product_id: "p-rare".to_string()
        })
    );
    let coffee_rb = adapter.get_content(&schema, "p-coffee").await?.unwrap();
    assert_eq!(
        coffee_rb.custom_fields.get("stock"),
        Some(&json!(3)),
        "p-coffee stock must be rolled back"
    );
    let rare_rb = adapter.get_content(&schema, "p-rare").await?.unwrap();
    assert_eq!(rare_rb.custom_fields.get("stock"), Some(&json!(1)));

    // 6. get_order による全フィールド取得検証
    let fetched = adapter
        .get_order(order_id)
        .await?
        .expect("Order must exist");
    assert_eq!(fetched.id, order1.id);
    assert_eq!(fetched.number, order1.number);
    assert_eq!(fetched.status, "pending_payment");
    assert_eq!(fetched.subtotal, 4400);
    assert_eq!(fetched.total, 4400);
    assert_eq!(fetched.customer_email, "alice@example.com");
    assert_eq!(fetched.customer_name, "Alice Smith");
    assert_eq!(fetched.payment_provider, "stripe");
    assert_eq!(fetched.expires_at, Some("2026-10-25T01:00:00Z".to_string()));

    // 7. set_order_payment による決済情報更新
    adapter
        .set_order_payment(
            order_id,
            "cs_test_session_12345",
            Some("2026-10-25T01:30:00Z"),
        )
        .await?;
    let updated_payment = adapter.get_order(order_id).await?.unwrap();
    assert_eq!(
        updated_payment.payment_ref.as_deref(),
        Some("cs_test_session_12345")
    );
    assert_eq!(
        updated_payment.expires_at.as_deref(),
        Some("2026-10-25T01:30:00Z")
    );

    // 8. transition_order による状態遷移（pending_payment -> paid）
    let transition_paid = OrderTransition {
        id: order_id.to_string(),
        from_status: "pending_payment".to_string(),
        to_status: "paid".to_string(),
        updated_at: "2026-10-25T00:10:00Z".to_string(),
        paid_at: Some("2026-10-25T00:10:00Z".to_string()),
        note: Some("Paid via Stripe webhook".to_string()),
        restock: vec![],
    };
    let ok = adapter.transition_order(&transition_paid).await?;
    assert!(ok, "Status transition to paid must succeed");

    let paid_order = adapter.get_order(order_id).await?.unwrap();
    assert_eq!(paid_order.status, "paid");
    assert_eq!(paid_order.paid_at.as_deref(), Some("2026-10-25T00:10:00Z"));
    assert_eq!(paid_order.note.as_deref(), Some("Paid via Stripe webhook"));

    // 二重遷移の防止: 再度同じ遷移を実行すると from_status 不一致で false
    let duplicate = adapter.transition_order(&transition_paid).await?;
    assert!(!duplicate, "Duplicate transition must return false");

    // 9. キャンセルと在庫返却（paid -> cancelled, restock: p-coffee x 2）
    let transition_cancel = OrderTransition {
        id: order_id.to_string(),
        from_status: "paid".to_string(),
        to_status: "cancelled".to_string(),
        updated_at: "2026-10-25T00:15:00Z".to_string(),
        paid_at: None,
        note: Some("Customer requested refund".to_string()),
        restock: vec![StockReservation {
            table: "content_product".to_string(),
            product_id: "p-coffee".to_string(),
            quantity: 2,
        }],
    };
    let cancelled = adapter.transition_order(&transition_cancel).await?;
    assert!(cancelled, "Transition to cancelled must succeed");

    let cancelled_order = adapter.get_order(order_id).await?.unwrap();
    assert_eq!(cancelled_order.status, "cancelled");
    assert_eq!(
        cancelled_order.note.as_deref(),
        Some("Customer requested refund")
    );

    // 在庫が返却されたこと: p-coffee は 3 + 2 = 5 に戻っている
    let coffee_restocked = adapter.get_content(&schema, "p-coffee").await?.unwrap();
    assert_eq!(
        coffee_restocked.custom_fields.get("stock"),
        Some(&json!(5)),
        "p-coffee stock must be restored to 5"
    );

    // 10. list_orders (一覧とカウント)
    let page_all = adapter
        .list_orders(&OrderListQuery {
            status: None,
            limit: 10,
            offset: 0,
        })
        .await?;
    assert_eq!(page_all.total, 1);
    assert_eq!(page_all.orders.len(), 1);

    let page_cancelled = adapter
        .list_orders(&OrderListQuery {
            status: Some("cancelled".to_string()),
            limit: 10,
            offset: 0,
        })
        .await?;
    assert_eq!(page_cancelled.total, 1);
    assert_eq!(page_cancelled.orders[0].id, order_id);

    let page_paid = adapter
        .list_orders(&OrderListQuery {
            status: Some("paid".to_string()),
            limit: 10,
            offset: 0,
        })
        .await?;
    assert_eq!(page_paid.total, 0);
    assert!(page_paid.orders.is_empty());

    // 11. list_expired_orders
    // 期限切れ注文を作成（status: pending_payment, expires_at: 2026-10-24T00:00:00Z）
    let expired_order = OrderRecord {
        id: "ord-expired".to_string(),
        number: "261024-EXP001".to_string(),
        status: "pending_payment".to_string(),
        currency: "jpy".to_string(),
        subtotal: 1000,
        total: 1000,
        lines_json: "[]".to_string(),
        adjustments_json: "[]".to_string(),
        customer_email: "expired@example.com".to_string(),
        customer_name: "Expired".to_string(),
        shipping_json: None,
        coupon_code: None,
        metadata_json: "{}".to_string(),
        payment_provider: "stripe".to_string(),
        payment_ref: None,
        access_token_hash: "hash_exp".to_string(),
        expires_at: Some("2026-10-24T00:00:00Z".to_string()),
        paid_at: None,
        created_at: "2026-10-24T00:00:00Z".to_string(),
        updated_at: "2026-10-24T00:00:00Z".to_string(),
        note: None,
    };
    adapter.create_order(&expired_order, &[]).await?.unwrap();

    let expired_list = adapter
        .list_expired_orders("2026-10-25T00:00:00Z", 10)
        .await?;
    assert_eq!(expired_list.len(), 1);
    assert_eq!(expired_list[0].id, "ord-expired");

    Ok(())
}

fn is_port_reachable(host_port: &str) -> bool {
    let Ok(addr) = host_port.parse() else {
        return false;
    };
    std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(150)).is_ok()
}

async fn connect_postgres_with_retry(url: &str) -> Option<PostgresAdapter> {
    for _ in 0..6 {
        if let Ok(adapter) = PostgresAdapter::connect(url, 5).await {
            return Some(adapter);
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    None
}

async fn connect_mysql_with_retry(url: &str) -> Option<MySqlAdapter> {
    for _ in 0..6 {
        if let Ok(adapter) = MySqlAdapter::connect(url, 5).await {
            return Some(adapter);
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    None
}

#[tokio::test]
async fn test_postgres_order_storage_real_db() {
    let custom_url = env::var("POSTGRES_URL").ok();
    if custom_url.is_none() && !is_port_reachable("127.0.0.1:5432") {
        eprintln!("Skipping PostgreSQL test: 127.0.0.1:5432 is not reachable (start with task docker:dbs:up)");
        return;
    }
    let url = custom_url
        .unwrap_or_else(|| "postgres://aegis:password@127.0.0.1:5432/aegis_test".to_string());
    println!("Connecting to PostgreSQL: {}", url);
    let adapter = match connect_postgres_with_retry(&url).await {
        Some(a) => a,
        None => {
            eprintln!("Skipping PostgreSQL test: failed to connect after retry");
            return;
        }
    };
    let _ = sqlx::query("DROP TABLE IF EXISTS content_product, content_article, _aegis_orders, _aegis_migrations, _aegis_users, _aegis_user_links, _aegis_settings, _aegis_audit CASCADE;")
        .execute(adapter.pool())
        .await;
    run_order_and_stock_lifecycle_test(&adapter)
        .await
        .expect("PostgreSQL order & stock integration test failed");
}

#[tokio::test]
async fn test_mysql_order_storage_real_db() {
    let custom_url = env::var("MYSQL_URL").ok();
    if custom_url.is_none() && !is_port_reachable("127.0.0.1:3306") {
        eprintln!(
            "Skipping MySQL test: 127.0.0.1:3306 is not reachable (start with task docker:dbs:up)"
        );
        return;
    }
    let url = custom_url
        .unwrap_or_else(|| "mysql://aegis:password@127.0.0.1:3306/aegis_test".to_string());
    println!("Connecting to MySQL: {}", url);
    let adapter = match connect_mysql_with_retry(&url).await {
        Some(a) => a,
        None => {
            eprintln!("Skipping MySQL test: failed to connect after retry");
            return;
        }
    };
    let _ = sqlx::query("DROP TABLE IF EXISTS content_product, content_article, _aegis_orders, _aegis_migrations, _aegis_users, _aegis_user_links, _aegis_settings, _aegis_audit;")
        .execute(adapter.pool())
        .await;
    run_order_and_stock_lifecycle_test(&adapter)
        .await
        .expect("MySQL order & stock integration test failed");
}
