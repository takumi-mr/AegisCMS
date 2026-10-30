//! Database and upload storage dump/restore for AegisCMS (`[feature = "backup"]`).
//!
//! Exports all database tables (`_aegis_*` and `content_*`) as dialect-neutral JSON alongside
//! uploaded objects from [`aegis_core::StorageBackend`] into a single ZIP archive with a
//! SHA-256-verified `manifest.json`.

pub mod dump;
pub mod manifest;
pub mod restore;

pub use dump::{dump_to_file, dump_to_writer, ContentBundle, DumpOptions, ForumThreadBundle};
pub use manifest::{
    is_safe_relative_path, sha256_hex, BackupManifest, DatabaseSectionManifest, StorageFileEntry,
    StorageSectionManifest, TableManifestEntry, FORMAT_VERSION, MANIFEST_FILE_NAME,
};
pub use restore::{
    restore_from_file, restore_from_reader, verify_backup_file, verify_backup_reader,
    RestoreOptions, RestoreSummary,
};

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io::{Cursor, Read, Write};
    use std::sync::Arc;

    use aegis_core::StorageBackend;
    use aegis_db::{
        CommentListQuery, CommentRecord, ContactListQuery, ContactMessageRecord,
        ContentPublicationChange, ContentRecord, ContentVersionRecord, CustomerRecord,
        DatabaseAdapter, ForumCategoryRecord, ForumPostRecord, ForumReportRecord,
        ForumThreadRecord, OrderRecord, PostListQuery, ProductVariantRecord, ReviewListQuery,
        ReviewRecord, SqliteAdapter, UserRecord,
    };
    use aegis_schema::SchemaDefinition;
    use aegis_storage::LocalStorageBackend;
    use serde_json::json;

    use super::*;

    fn test_schemas() -> HashMap<String, SchemaDefinition> {
        let article: SchemaDefinition = serde_json::from_value(json!({
            "content_type": "article",
            "label": "Article",
            "fields": [
                { "name": "excerpt", "type": "string", "required": false },
                { "name": "views", "type": "integer", "required": false }
            ]
        }))
        .unwrap();
        let mut map = HashMap::new();
        map.insert("article".to_string(), article);
        map
    }

    async fn setup_db(schemas: &HashMap<String, SchemaDefinition>) -> Arc<dyn DatabaseAdapter> {
        let adapter = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
        adapter.init().await.unwrap();
        adapter.run_migrations(schemas).await.unwrap();
        Arc::new(adapter)
    }

    #[tokio::test]
    async fn test_dump_and_restore_roundtrip() {
        let schemas = test_schemas();
        let src_db = setup_db(&schemas).await;
        let src_dir = tempfile::tempdir().unwrap();
        let src_storage: Arc<dyn StorageBackend> = Arc::new(LocalStorageBackend::new(
            src_dir.path(),
            "/uploads".to_string(),
        ));

        // 1. Settings
        src_db
            .set_setting("site.title", "Aegis Demo")
            .await
            .unwrap();
        src_db
            .set_setting("seo.canonical_base_url", "https://example.com")
            .await
            .unwrap();

        // 2. Users + Identity
        let user = UserRecord {
            id: "u-1".to_string(),
            username: "alice".to_string(),
            email: Some("alice@example.com".to_string()),
            display_name: Some("Alice".to_string()),
            role: "admin".to_string(),
            password_hash: "$argon2id$v=19$m=19456,t=2,p=1$hash".to_string(),
            disabled: false,
            avatar_key: Some("avatars/alice.png".to_string()),
            created_at: "2026-10-29T00:00:00Z".to_string(),
            updated_at: "2026-10-29T00:00:00Z".to_string(),
            pending_approval: false,
            totp_secret: Some("JBSWY3DPEHPK3PXP".to_string()),
            totp_enabled: true,
            backup_codes: Some(r#"["hash1"]"#.to_string()),
        };
        src_db.insert_user(&user).await.unwrap();
        src_db
            .insert_identity_link("oidc", "sub-123", "alice")
            .await
            .unwrap();

        // 3. Customer
        let customer = CustomerRecord {
            id: "cust-1".to_string(),
            email: "bob@example.com".to_string(),
            password_hash: "hash".to_string(),
            name: "Bob".to_string(),
            phone: None,
            shipping_address_json: Some("{}".to_string()),
            status: "active".to_string(),
            created_at: "2026-10-29T00:00:00Z".to_string(),
            updated_at: "2026-10-29T00:00:00Z".to_string(),
            last_login_at: None,
        };
        src_db.insert_customer(&customer).await.unwrap();

        // 4. Content with versions (v1 published, v2 working copy)
        let article_schema = &schemas["article"];
        let mut custom = HashMap::new();
        custom.insert("excerpt".to_string(), json!("Published excerpt"));
        custom.insert("views".to_string(), json!(42));

        let v1_rec = ContentRecord {
            id: "art-1".to_string(),
            content_type: "article".to_string(),
            title: "Published V1".to_string(),
            slug: "hello-world".to_string(),
            status: "published".to_string(),
            blocks_json: "[]".to_string(),
            author_id: Some("alice".to_string()),
            published_at: Some("2026-10-29T01:00:00Z".to_string()),
            unpublish_at: None,
            created_at: "2026-10-29T01:00:00Z".to_string(),
            updated_at: "2026-10-29T01:00:00Z".to_string(),
            custom_fields: custom.clone(),
        };
        let mut v2_rec = v1_rec.clone();
        v2_rec.title = "Working Copy V2".to_string();
        v2_rec.updated_at = "2026-10-29T02:00:00Z".to_string();

        let versions = vec![
            ContentVersionRecord {
                content_type: "article".to_string(),
                content_id: "art-1".to_string(),
                revision: 1,
                snapshot_json: serde_json::to_string(&v1_rec).unwrap(),
                created_at: "2026-10-29T01:00:00Z".to_string(),
                created_by: Some("alice".to_string()),
                change_note: Some("Initial publish".to_string()),
            },
            ContentVersionRecord {
                content_type: "article".to_string(),
                content_id: "art-1".to_string(),
                revision: 2,
                snapshot_json: serde_json::to_string(&v2_rec).unwrap(),
                created_at: "2026-10-29T02:00:00Z".to_string(),
                created_by: Some("alice".to_string()),
                change_note: Some("Draft edit".to_string()),
            },
        ];
        src_db
            .insert_content_with_versions(
                article_schema,
                &v2_rec,
                &versions,
                ContentPublicationChange::PublishVersion(0),
            )
            .await
            .unwrap();

        // 5. Product Variant & Order
        let variant = ProductVariantRecord {
            id: "var-1".to_string(),
            sku: Some("SKU-RED-M".to_string()),
            product_id: "prod-1".to_string(),
            title: "Red / M".to_string(),
            price_override: Some(2500),
            stock: Some(8),
            attributes_json: r#"{"color":"Red"}"#.to_string(),
            position: 0,
            created_at: "2026-10-29T01:00:00Z".to_string(),
            updated_at: "2026-10-29T01:00:00Z".to_string(),
        };
        src_db.upsert_product_variant(&variant).await.unwrap();

        let order = OrderRecord {
            id: "ord-1".to_string(),
            number: "ORD-2026-0001".to_string(),
            status: "paid".to_string(),
            currency: "JPY".to_string(),
            subtotal: 2500,
            total: 3250,
            coupon_code: None,
            customer_email: "bob@example.com".to_string(),
            customer_name: "Bob".to_string(),
            shipping_json: Some("{}".to_string()),
            lines_json: "[]".to_string(),
            adjustments_json: "[]".to_string(),
            metadata_json: "{}".to_string(),
            payment_provider: "manual".to_string(),
            payment_ref: None,
            access_token_hash: "hash123".to_string(),
            note: None,
            created_at: "2026-10-29T01:00:00Z".to_string(),
            updated_at: "2026-10-29T01:00:00Z".to_string(),
            paid_at: Some("2026-10-29T01:05:00Z".to_string()),
            expires_at: None,
            customer_id: Some("cust-1".to_string()),
            transaction_id: Some("tx-999".to_string()),
            refunded_amount: 0,
            refunds_json: "[]".to_string(),
        };
        src_db.create_order(&order, &[]).await.unwrap().unwrap();

        // 6. Comment & Review & Contact
        let comment = CommentRecord {
            id: "cmt-1".to_string(),
            content_type: "article".to_string(),
            content_id: "art-1".to_string(),
            parent_id: None,
            author_name: "Reader".to_string(),
            author_email: "reader@example.com".to_string(),
            author_user_id: None,
            body: "Great post!".to_string(),
            status: "approved".to_string(),
            ip: None,
            user_agent: None,
            notify_replies: false,
            notify_token: Some("tok-1".to_string()),
            created_at: "2026-10-29T01:10:00Z".to_string(),
            updated_at: "2026-10-29T01:10:00Z".to_string(),
        };
        src_db.insert_comment(&comment).await.unwrap();

        // Tags and the tags on content
        let tag = aegis_db::TagRecord {
            id: "tag-1".to_string(),
            slug: "rust".to_string(),
            name: "Rust".to_string(),
            description: Some("The Rust language".to_string()),
            created_at: "2026-10-29T01:11:00Z".to_string(),
            updated_at: "2026-10-29T01:11:00Z".to_string(),
        };
        src_db.insert_tag(&tag).await.unwrap();
        src_db
            .set_content_tags(
                "article",
                "art-1",
                &["tag-1".to_string()],
                "2026-10-29T01:12:00Z",
            )
            .await
            .unwrap();

        let review = ReviewRecord {
            id: "rev-1".to_string(),
            product_id: "prod-1".to_string(),
            order_id: "ord-1".to_string(),
            customer_id: Some("cust-1".to_string()),
            author_name: "Bob".to_string(),
            author_email: "bob@example.com".to_string(),
            rating: 5,
            title: "Awesome".to_string(),
            body: "Works great".to_string(),
            status: "approved".to_string(),
            verified_purchase: 1,
            reply: None,
            replied_at: None,
            rejection_reason: None,
            ip: None,
            user_agent: None,
            created_at: "2026-10-29T01:15:00Z".to_string(),
            updated_at: "2026-10-29T01:15:00Z".to_string(),
        };
        src_db.insert_review(&review).await.unwrap();

        let contact = ContactMessageRecord {
            id: "cnt-1".to_string(),
            form: "default".to_string(),
            name: "Visitor".to_string(),
            email: "visitor@example.com".to_string(),
            subject: "Hello".to_string(),
            fields_json: "[]".to_string(),
            status: "unread".to_string(),
            ip: None,
            user_agent: None,
            created_at: "2026-10-29T01:20:00Z".to_string(),
            updated_at: "2026-10-29T01:20:00Z".to_string(),
        };
        src_db.insert_contact_message(&contact).await.unwrap();

        // 7. Forum Category, Thread + 2 Posts, Report
        let fcat = ForumCategoryRecord {
            id: "fcat-1".to_string(),
            slug: "general".to_string(),
            name: "General".to_string(),
            description: "General talk".to_string(),
            position: 1,
            is_private: false,
            created_at: "2026-10-29T01:00:00Z".to_string(),
            updated_at: "2026-10-29T01:00:00Z".to_string(),
        };
        src_db.insert_category(&fcat).await.unwrap();

        let fthread = ForumThreadRecord {
            id: "fth-1".to_string(),
            category_id: "fcat-1".to_string(),
            title: "Welcome thread".to_string(),
            author_id: Some("u-1".to_string()),
            author_name: "Alice".to_string(),
            status: "approved".to_string(),
            is_pinned: true,
            is_locked: false,
            post_count: 1,
            last_post_at: "2026-10-29T01:00:00Z".to_string(),
            created_at: "2026-10-29T01:00:00Z".to_string(),
            updated_at: "2026-10-29T01:00:00Z".to_string(),
        };
        let fpost1 = ForumPostRecord {
            id: "fp-1".to_string(),
            thread_id: "fth-1".to_string(),
            post_number: 1,
            author_id: Some("u-1".to_string()),
            author_name: "Alice".to_string(),
            body: "First post".to_string(),
            status: "approved".to_string(),
            reply_to_post_id: None,
            reply_to_number: None,
            is_deleted: false,
            ip: None,
            user_agent: None,
            created_at: "2026-10-29T01:00:00Z".to_string(),
            updated_at: "2026-10-29T01:00:00Z".to_string(),
        };
        src_db.insert_thread(&fthread, &fpost1).await.unwrap();

        let mut fpost2 = ForumPostRecord {
            id: "fp-2".to_string(),
            thread_id: "fth-1".to_string(),
            post_number: 2,
            author_id: None,
            author_name: "Guest".to_string(),
            body: "Second post".to_string(),
            status: "approved".to_string(),
            reply_to_post_id: Some("fp-1".to_string()),
            reply_to_number: Some(1),
            is_deleted: false,
            ip: None,
            user_agent: None,
            created_at: "2026-10-29T01:05:00Z".to_string(),
            updated_at: "2026-10-29T01:05:00Z".to_string(),
        };
        src_db.insert_post(&mut fpost2).await.unwrap();

        let freport = ForumReportRecord {
            id: "frep-1".to_string(),
            target_type: "post".to_string(),
            target_id: "fp-2".to_string(),
            thread_id: "fth-1".to_string(),
            reason: "Test report".to_string(),
            reporter_id: None,
            reporter_ip: None,
            status: "open".to_string(),
            resolved_by: None,
            created_at: "2026-10-29T01:06:00Z".to_string(),
            updated_at: "2026-10-29T01:06:00Z".to_string(),
        };
        src_db.insert_report(&freport).await.unwrap();

        // 8. Storage objects
        src_storage
            .put("2026/10/banner.png", b"PNGDATA123", "image/png")
            .await
            .unwrap();
        src_storage
            .put("avatars/alice.png", b"AVATARDATA", "image/png")
            .await
            .unwrap();

        // Dump to memory buffer
        let mut zip_buf = Vec::new();
        let manifest = dump_to_writer(
            &src_db,
            &src_storage,
            &schemas,
            &DumpOptions::default(),
            Cursor::new(&mut zip_buf),
        )
        .await
        .unwrap();

        assert_eq!(manifest.format_version, FORMAT_VERSION);
        assert!(manifest.database.included);
        assert!(manifest.storage.included);
        assert_eq!(manifest.storage.files.len(), 2);

        // Verify archive without restoring
        let verified = verify_backup_reader(Cursor::new(&zip_buf)).unwrap();
        assert_eq!(verified, manifest);

        // Restore into a fresh database and storage
        let dst_db = setup_db(&schemas).await;
        let dst_dir = tempfile::tempdir().unwrap();
        let dst_storage: Arc<dyn StorageBackend> = Arc::new(LocalStorageBackend::new(
            dst_dir.path(),
            "/uploads".to_string(),
        ));

        let summary = restore_from_reader(
            &dst_db,
            &dst_storage,
            &schemas,
            &RestoreOptions::default(),
            Cursor::new(&zip_buf),
        )
        .await
        .unwrap();

        assert_eq!(summary.storage_files_restored, 2);
        assert!(summary.records_restored >= 14);

        // Verify restored settings, users, identity
        assert_eq!(
            dst_db.list_settings().await.unwrap().get("site.title"),
            Some(&"Aegis Demo".to_string())
        );
        let restored_user = dst_db.get_user_by_username("alice").await.unwrap().unwrap();
        assert_eq!(restored_user, user);
        assert_eq!(
            dst_db.get_identity_link("oidc", "sub-123").await.unwrap(),
            Some("alice".to_string())
        );

        // Verify working copy vs published version separation!
        let working = dst_db
            .get_content(article_schema, "art-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(working.title, "Working Copy V2");
        let published = dst_db
            .get_published_content(article_schema, "hello-world")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(published.title, "Published V1");
        let restored_versions = dst_db
            .list_content_versions("article", "art-1")
            .await
            .unwrap();
        assert_eq!(restored_versions.len(), 2);

        // The tags and the tags on content come back
        assert_eq!(dst_db.get_tag("tag-1").await.unwrap().unwrap(), tag);
        assert_eq!(
            dst_db
                .list_content_tags("article", "art-1")
                .await
                .unwrap()
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            vec!["tag-1"]
        );

        // Verify variant, order, comment, review, contact, forum
        assert_eq!(
            dst_db.get_product_variant("var-1").await.unwrap().unwrap(),
            variant
        );
        assert_eq!(dst_db.get_order("ord-1").await.unwrap().unwrap(), order);
        assert_eq!(
            dst_db
                .list_comments(&CommentListQuery::default())
                .await
                .unwrap()
                .comments
                .len(),
            1
        );
        assert_eq!(
            dst_db
                .list_reviews(&ReviewListQuery::default())
                .await
                .unwrap()
                .reviews
                .len(),
            1
        );
        assert_eq!(
            dst_db
                .list_contact_messages(&ContactListQuery::default())
                .await
                .unwrap()
                .messages
                .len(),
            1
        );
        let restored_thread = dst_db.get_thread("fth-1").await.unwrap().unwrap();
        assert_eq!(restored_thread.post_count, 2);
        let restored_posts = dst_db
            .list_posts(&PostListQuery {
                thread_id: "fth-1".to_string(),
                status: None,
                after_post_number: 0,
                limit: 10,
            })
            .await
            .unwrap();
        assert_eq!(restored_posts.len(), 2);
        assert_eq!(restored_posts[1].body, "Second post");

        // Verify storage objects
        assert_eq!(
            dst_storage.get("2026/10/banner.png").await.unwrap(),
            b"PNGDATA123"
        );
        assert_eq!(
            dst_storage.get("avatars/alice.png").await.unwrap(),
            b"AVATARDATA"
        );

        // Restoring again with overwrite = false fails with Validation error
        let err = restore_from_reader(
            &dst_db,
            &dst_storage,
            &schemas,
            &RestoreOptions {
                overwrite: false,
                ..Default::default()
            },
            Cursor::new(&zip_buf),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, aegis_core::AegisError::Validation(_)));

        // Restoring again with overwrite = true succeeds
        restore_from_reader(
            &dst_db,
            &dst_storage,
            &schemas,
            &RestoreOptions {
                overwrite: true,
                ..Default::default()
            },
            Cursor::new(&zip_buf),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_tampered_archive_is_rejected_before_mutation() {
        let schemas = test_schemas();
        let src_db = setup_db(&schemas).await;
        src_db.set_setting("site.title", "Original").await.unwrap();
        let src_dir = tempfile::tempdir().unwrap();
        let src_storage: Arc<dyn StorageBackend> = Arc::new(LocalStorageBackend::new(
            src_dir.path(),
            "/uploads".to_string(),
        ));

        let mut zip_buf = Vec::new();
        dump_to_writer(
            &src_db,
            &src_storage,
            &schemas,
            &DumpOptions::default(),
            Cursor::new(&mut zip_buf),
        )
        .await
        .unwrap();

        // Rebuild the ZIP with tampered settings.json while keeping the original manifest.json
        let mut orig = zip::ZipArchive::new(Cursor::new(&zip_buf)).unwrap();
        let mut tampered_buf = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(Cursor::new(&mut tampered_buf));
            let opts = zip::write::SimpleFileOptions::default();
            for i in 0..orig.len() {
                let mut file = orig.by_index(i).unwrap();
                let name = file.name().unwrap().to_string();
                let mut data = Vec::new();
                file.read_to_end(&mut data).unwrap();
                writer.start_file(&name, opts).unwrap();
                if name == "database/settings.json" {
                    writer.write_all(b"{\"site.title\":\"Tampered\"}").unwrap();
                } else {
                    writer.write_all(&data).unwrap();
                }
            }
            writer.finish().unwrap();
        }

        let dst_db = setup_db(&schemas).await;
        let dst_dir = tempfile::tempdir().unwrap();
        let dst_storage: Arc<dyn StorageBackend> = Arc::new(LocalStorageBackend::new(
            dst_dir.path(),
            "/uploads".to_string(),
        ));

        let res = restore_from_reader(
            &dst_db,
            &dst_storage,
            &schemas,
            &RestoreOptions::default(),
            Cursor::new(&tampered_buf),
        )
        .await;
        assert!(res.is_err());
        assert!(dst_db.list_settings().await.unwrap().is_empty());
    }
}
