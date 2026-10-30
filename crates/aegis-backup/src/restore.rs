use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Seek};
use std::path::Path;
use std::sync::Arc;

use aegis_core::{guess_content_type, AegisError, Result, StorageBackend};
use aegis_db::{
    CommentRecord, ContactMessageRecord, ContentPublicationChange, ContentTagRecord,
    CustomerRecord, DatabaseAdapter, ForumCategoryRecord, ForumPostRecord, ForumReportRecord,
    OrderRecord, ProductVariantRecord, ReviewRecord, TagRecord, UserIdentityRecord, UserRecord,
};
use aegis_schema::SchemaDefinition;
use serde::de::DeserializeOwned;

use crate::dump::{ContentBundle, ForumThreadBundle};
use crate::manifest::{
    is_safe_relative_path, sha256_hex, BackupManifest, FORMAT_VERSION, MANIFEST_FILE_NAME,
};

/// Options controlling how a backup archive is restored.
#[derive(Debug, Clone)]
pub struct RestoreOptions {
    /// Restore database tables (`_aegis_*` and `content_*`).
    pub include_database: bool,
    /// Restore uploaded objects into [`StorageBackend`].
    pub include_storage: bool,
    /// Overwrite existing database records and storage objects when IDs/keys collide.
    /// When `false`, colliding records return a validation error so existing data is not
    /// overwritten accidentally.
    pub overwrite: bool,
}

impl Default for RestoreOptions {
    fn default() -> Self {
        Self {
            include_database: true,
            include_storage: true,
            overwrite: false,
        }
    }
}

/// Summary of a completed backup restore operation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RestoreSummary {
    pub manifest: Option<BackupManifest>,
    pub tables_restored: usize,
    pub records_restored: u64,
    pub storage_files_restored: usize,
    pub storage_bytes_restored: u64,
}

/// Inspect and verify the integrity of a backup archive on disk without modifying the database or storage.
pub fn verify_backup_file(input_path: &Path) -> Result<BackupManifest> {
    let file = std::fs::File::open(input_path).map_err(|e| {
        AegisError::Validation(format!(
            "Failed to open backup file '{}': {}",
            input_path.display(),
            e
        ))
    })?;
    verify_backup_reader(file)
}

/// Inspect and verify the integrity of a backup archive from any seekable reader.
pub fn verify_backup_reader<R: Read + Seek>(reader: R) -> Result<BackupManifest> {
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|e| AegisError::Validation(format!("Invalid backup ZIP archive: {}", e)))?;
    let manifest = read_and_validate_manifest(&mut archive)?;
    verify_archive_checksums(&mut archive, &manifest)?;
    Ok(manifest)
}

/// Restore a backup archive from a file on disk.
pub async fn restore_from_file(
    db: &Arc<dyn DatabaseAdapter>,
    storage: &Arc<dyn StorageBackend>,
    schemas: &HashMap<String, SchemaDefinition>,
    options: &RestoreOptions,
    input_path: &Path,
) -> Result<RestoreSummary> {
    let file = std::fs::File::open(input_path).map_err(|e| {
        AegisError::Validation(format!(
            "Failed to open backup file '{}': {}",
            input_path.display(),
            e
        ))
    })?;
    restore_from_reader(db, storage, schemas, options, file).await
}

/// Restore a backup archive from any seekable reader.
///
/// All manifest entries and SHA-256 digests are verified before any database or storage mutation
/// takes place.
pub async fn restore_from_reader<R: Read + Seek>(
    db: &Arc<dyn DatabaseAdapter>,
    storage: &Arc<dyn StorageBackend>,
    schemas: &HashMap<String, SchemaDefinition>,
    options: &RestoreOptions,
    reader: R,
) -> Result<RestoreSummary> {
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|e| AegisError::Validation(format!("Invalid backup ZIP archive: {}", e)))?;

    let manifest = read_and_validate_manifest(&mut archive)?;
    verify_archive_checksums(&mut archive, &manifest)?;

    let mut summary = RestoreSummary {
        manifest: Some(manifest.clone()),
        ..Default::default()
    };

    if options.include_database && manifest.database.included {
        db.run_migrations(schemas).await?;
        restore_database_section(&mut archive, &manifest, db, schemas, options, &mut summary)
            .await?;
    }

    if options.include_storage && manifest.storage.included {
        restore_storage_section(&mut archive, &manifest, storage, options, &mut summary).await?;
    }

    Ok(summary)
}

fn read_and_validate_manifest<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
) -> Result<BackupManifest> {
    let bytes = read_zip_entry_bytes(archive, MANIFEST_FILE_NAME)?;
    let manifest: BackupManifest = serde_json::from_slice(&bytes)
        .map_err(|e| AegisError::Validation(format!("Invalid manifest.json: {}", e)))?;

    if manifest.format_version != FORMAT_VERSION {
        return Err(AegisError::Validation(format!(
            "Unsupported backup format_version {} (expected {})",
            manifest.format_version, FORMAT_VERSION
        )));
    }

    Ok(manifest)
}

fn verify_archive_checksums<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    manifest: &BackupManifest,
) -> Result<()> {
    for table in &manifest.database.tables {
        if !is_safe_relative_path(&table.path) {
            return Err(AegisError::Validation(format!(
                "Unsafe path in database manifest: '{}'",
                table.path
            )));
        }
        let bytes = read_zip_entry_bytes(archive, &table.path)?;
        let actual_sha = sha256_hex(&bytes);
        if actual_sha != table.sha256 {
            return Err(AegisError::Validation(format!(
                "Checksum mismatch for '{}': expected {}, got {}",
                table.path, table.sha256, actual_sha
            )));
        }
    }

    for file in &manifest.storage.files {
        if !is_safe_relative_path(&file.key) || !is_safe_relative_path(&file.path) {
            return Err(AegisError::Validation(format!(
                "Unsafe path in storage manifest: '{}' ('{}')",
                file.key, file.path
            )));
        }
        let bytes = read_zip_entry_bytes(archive, &file.path)?;
        if bytes.len() as u64 != file.size_bytes {
            return Err(AegisError::Validation(format!(
                "Size mismatch for '{}': expected {} bytes, got {}",
                file.path,
                file.size_bytes,
                bytes.len()
            )));
        }
        let actual_sha = sha256_hex(&bytes);
        if actual_sha != file.sha256 {
            return Err(AegisError::Validation(format!(
                "Checksum mismatch for '{}': expected {}, got {}",
                file.path, file.sha256, actual_sha
            )));
        }
    }

    Ok(())
}

async fn restore_database_section<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    manifest: &BackupManifest,
    db: &Arc<dyn DatabaseAdapter>,
    schemas: &HashMap<String, SchemaDefinition>,
    options: &RestoreOptions,
    summary: &mut RestoreSummary,
) -> Result<()> {
    let has_path = |p: &str| manifest.database.tables.iter().any(|t| t.path == p);

    // 1. Settings (_aegis_settings)
    if has_path("database/settings.json") {
        let settings: BTreeMap<String, String> =
            read_json_entry(archive, "database/settings.json")?;
        let existing = if !options.overwrite {
            db.list_settings().await?
        } else {
            HashMap::new()
        };
        for (name, value) in &settings {
            if !options.overwrite && existing.contains_key(name) {
                return Err(AegisError::Validation(format!(
                    "Setting '{}' already exists (use overwrite option to replace)",
                    name
                )));
            }
            db.set_setting(name, value).await?;
            summary.records_restored += 1;
        }
        summary.tables_restored += 1;
    }

    // 2. Users (_aegis_users)
    if has_path("database/users.json") {
        let users: Vec<UserRecord> = read_json_entry(archive, "database/users.json")?;
        for user in &users {
            if let Some(existing) = db.get_user_by_username(&user.username).await? {
                if options.overwrite {
                    db.delete_user(&existing).await?;
                } else {
                    return Err(AegisError::Validation(format!(
                        "User '{}' already exists (use overwrite option to replace)",
                        user.username
                    )));
                }
            }
            db.insert_user(user).await?;
            summary.records_restored += 1;
        }
        summary.tables_restored += 1;
    }

    // 3. Identity links (_aegis_user_identities)
    if has_path("database/identities.json") {
        let links: Vec<UserIdentityRecord> = read_json_entry(archive, "database/identities.json")?;
        for link in &links {
            if db
                .get_identity_link(&link.provider, &link.subject)
                .await?
                .is_none()
            {
                db.insert_identity_link(&link.provider, &link.subject, &link.username)
                    .await?;
                summary.records_restored += 1;
            } else if !options.overwrite {
                return Err(AegisError::Validation(format!(
                    "Identity link '{}:{}' already exists",
                    link.provider, link.subject
                )));
            }
        }
        summary.tables_restored += 1;
    }

    // 4. Customers (_aegis_customers)
    if has_path("database/customers.json") {
        let customers: Vec<CustomerRecord> = read_json_entry(archive, "database/customers.json")?;
        for customer in &customers {
            if db.get_customer_by_id(&customer.id).await?.is_some() {
                if options.overwrite {
                    db.delete_customer(&customer.id).await?;
                } else {
                    return Err(AegisError::Validation(format!(
                        "Customer '{}' already exists (use overwrite option to replace)",
                        customer.id
                    )));
                }
            }
            db.insert_customer(customer).await?;
            summary.records_restored += 1;
        }
        summary.tables_restored += 1;
    }

    // 5. Dynamic content tables + versions + publication pointers
    let mut schema_names: Vec<String> = schemas.keys().cloned().collect();
    schema_names.sort();
    for schema_name in schema_names {
        let Some(schema) = schemas.get(&schema_name) else {
            continue;
        };
        let path = format!("database/contents/{}.json", schema.content_type);
        if !has_path(&path) {
            continue;
        }
        let bundles: Vec<ContentBundle> = read_json_entry(archive, &path)?;
        for bundle in &bundles {
            if db.get_content(schema, &bundle.record.id).await?.is_some() {
                if options.overwrite {
                    db.delete_content(schema, &bundle.record.id).await?;
                } else {
                    return Err(AegisError::Validation(format!(
                        "Content '{}/{}' already exists (use overwrite option to replace)",
                        schema.content_type, bundle.record.id
                    )));
                }
            }

            if bundle.versions.is_empty() {
                db.insert_content(schema, &bundle.record).await?;
            } else {
                let pub_change = match bundle.published_version_index {
                    Some(idx) if idx < bundle.versions.len() => {
                        ContentPublicationChange::PublishVersion(idx)
                    }
                    _ => ContentPublicationChange::Keep,
                };
                db.insert_content_with_versions(
                    schema,
                    &bundle.record,
                    &bundle.versions,
                    pub_change,
                )
                .await?;
            }
            summary.records_restored += 1;
        }
        summary.tables_restored += 1;
    }

    // 6. Product variants (_aegis_product_variants)
    if has_path("database/variants.json") {
        let variants: Vec<ProductVariantRecord> =
            read_json_entry(archive, "database/variants.json")?;
        for variant in &variants {
            if !options.overwrite && db.get_product_variant(&variant.id).await?.is_some() {
                return Err(AegisError::Validation(format!(
                    "Product variant '{}' already exists (use overwrite option to replace)",
                    variant.id
                )));
            }
            db.upsert_product_variant(variant).await?;
            summary.records_restored += 1;
        }
        summary.tables_restored += 1;
    }

    // 7. Orders (_aegis_orders)
    if has_path("database/orders.json") {
        let orders: Vec<OrderRecord> = read_json_entry(archive, "database/orders.json")?;
        for order in &orders {
            if db.get_order(&order.id).await?.is_some() {
                if options.overwrite {
                    continue;
                }
                return Err(AegisError::Validation(format!(
                    "Order '{}' already exists",
                    order.id
                )));
            }
            db.create_order(order, &[])
                .await?
                .map_err(|_| AegisError::Internal("Unexpected stock check failure".to_string()))?;
            summary.records_restored += 1;
        }
        summary.tables_restored += 1;
    }

    // 8. Comments (_aegis_comments)
    if has_path("database/comments.json") {
        let comments: Vec<CommentRecord> = read_json_entry(archive, "database/comments.json")?;
        for comment in &comments {
            if db.get_comment(&comment.id).await?.is_some() {
                if options.overwrite {
                    db.delete_comments(std::slice::from_ref(&comment.id))
                        .await?;
                } else {
                    return Err(AegisError::Validation(format!(
                        "Comment '{}' already exists (use overwrite option to replace)",
                        comment.id
                    )));
                }
            }
            db.insert_comment(comment).await?;
            summary.records_restored += 1;
        }
        summary.tables_restored += 1;
    }

    // 8b. Tags (_aegis_tags) and the tags on content (_aegis_content_tags)
    if has_path("database/tags.json") {
        let tags: Vec<TagRecord> = read_json_entry(archive, "database/tags.json")?;
        for tag in &tags {
            // The slug is unique: a different tag with the same slug is a conflict, not an overwrite
            if let Some(other) = db.get_tag_by_slug(&tag.slug).await? {
                if other.id != tag.id {
                    return Err(AegisError::Validation(format!(
                        "Tag slug '{}' is already used by another tag",
                        tag.slug
                    )));
                }
            }
            if db.get_tag(&tag.id).await?.is_some() {
                if options.overwrite {
                    db.update_tag(tag).await?;
                    summary.records_restored += 1;
                    continue;
                }
                return Err(AegisError::Validation(format!(
                    "Tag '{}' already exists (use overwrite option to replace)",
                    tag.id
                )));
            }
            db.insert_tag(tag).await?;
            summary.records_restored += 1;
        }
        summary.tables_restored += 1;
    }
    if has_path("database/content_tags.json") {
        let links: Vec<ContentTagRecord> = read_json_entry(archive, "database/content_tags.json")?;
        for link in &links {
            db.insert_content_tag(link).await?;
            summary.records_restored += 1;
        }
        summary.tables_restored += 1;
    }

    // 9. Reviews (_aegis_reviews)
    if has_path("database/reviews.json") {
        let reviews: Vec<ReviewRecord> = read_json_entry(archive, "database/reviews.json")?;
        for review in &reviews {
            if db.get_review(&review.id).await?.is_some() {
                if options.overwrite {
                    db.delete_reviews(std::slice::from_ref(&review.id)).await?;
                } else {
                    return Err(AegisError::Validation(format!(
                        "Review '{}' already exists (use overwrite option to replace)",
                        review.id
                    )));
                }
            }
            db.insert_review(review).await?;
            summary.records_restored += 1;
        }
        summary.tables_restored += 1;
    }

    // 10. Forum categories, threads + posts, reports
    if has_path("database/forum_categories.json") {
        let categories: Vec<ForumCategoryRecord> =
            read_json_entry(archive, "database/forum_categories.json")?;
        for cat in &categories {
            if db.get_category(&cat.id).await?.is_some() {
                if options.overwrite {
                    db.delete_category(&cat.id).await?;
                } else {
                    return Err(AegisError::Validation(format!(
                        "Forum category '{}' already exists (use overwrite option to replace)",
                        cat.id
                    )));
                }
            }
            db.insert_category(cat).await?;
            summary.records_restored += 1;
        }
        summary.tables_restored += 1;
    }

    if has_path("database/forum_threads.json") {
        let thread_bundles: Vec<ForumThreadBundle> =
            read_json_entry(archive, "database/forum_threads.json")?;
        for bundle in &thread_bundles {
            if db.get_thread(&bundle.thread.id).await?.is_some() {
                if options.overwrite {
                    db.delete_thread(&bundle.thread.id).await?;
                } else {
                    return Err(AegisError::Validation(format!(
                        "Forum thread '{}' already exists (use overwrite option to replace)",
                        bundle.thread.id
                    )));
                }
            }

            let mut posts = bundle.posts.clone();
            posts.sort_by_key(|p| p.post_number);

            if let Some((first, rest)) = posts.split_first() {
                let mut initial_thread = bundle.thread.clone();
                initial_thread.post_count = 1;
                db.insert_thread(&initial_thread, first).await?;
                for post in rest {
                    let mut p = post.clone();
                    db.insert_post(&mut p).await?;
                }
            } else {
                let placeholder_post = ForumPostRecord {
                    id: format!("{}-p1", bundle.thread.id),
                    thread_id: bundle.thread.id.clone(),
                    post_number: 1,
                    author_id: bundle.thread.author_id.clone(),
                    author_name: bundle.thread.author_name.clone(),
                    body: String::new(),
                    status: bundle.thread.status.clone(),
                    reply_to_post_id: None,
                    reply_to_number: None,
                    is_deleted: true,
                    ip: None,
                    user_agent: None,
                    created_at: bundle.thread.created_at.clone(),
                    updated_at: bundle.thread.updated_at.clone(),
                };
                db.insert_thread(&bundle.thread, &placeholder_post).await?;
            }
            summary.records_restored += 1;
        }
        summary.tables_restored += 1;
    }

    if has_path("database/forum_reports.json") {
        let reports: Vec<ForumReportRecord> =
            read_json_entry(archive, "database/forum_reports.json")?;
        for report in &reports {
            if db.get_report(&report.id).await?.is_some() {
                if options.overwrite {
                    continue;
                }
                return Err(AegisError::Validation(format!(
                    "Forum report '{}' already exists",
                    report.id
                )));
            }
            db.insert_report(report).await?;
            summary.records_restored += 1;
        }
        summary.tables_restored += 1;
    }

    // 11. Contact messages (_aegis_contact_messages)
    if has_path("database/contact_messages.json") {
        let messages: Vec<ContactMessageRecord> =
            read_json_entry(archive, "database/contact_messages.json")?;
        for msg in &messages {
            if db.get_contact_message(&msg.id).await?.is_some() {
                if options.overwrite {
                    db.delete_contact_messages(std::slice::from_ref(&msg.id))
                        .await?;
                } else {
                    return Err(AegisError::Validation(format!(
                        "Contact message '{}' already exists (use overwrite option to replace)",
                        msg.id
                    )));
                }
            }
            db.insert_contact_message(msg).await?;
            summary.records_restored += 1;
        }
        summary.tables_restored += 1;
    }

    Ok(())
}

async fn restore_storage_section<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    manifest: &BackupManifest,
    storage: &Arc<dyn StorageBackend>,
    options: &RestoreOptions,
    summary: &mut RestoreSummary,
) -> Result<()> {
    for file in &manifest.storage.files {
        if !options.overwrite && storage.exists(&file.key).await? {
            return Err(AegisError::Validation(format!(
                "Storage object '{}' already exists (use overwrite option to replace)",
                file.key
            )));
        }
        let bytes = read_zip_entry_bytes(archive, &file.path)?;
        let content_type = guess_content_type(&file.key);
        storage.put(&file.key, &bytes, content_type).await?;
        summary.storage_files_restored += 1;
        summary.storage_bytes_restored += bytes.len() as u64;
    }
    Ok(())
}

fn read_json_entry<R: Read + Seek, T: DeserializeOwned>(
    archive: &mut zip::ZipArchive<R>,
    path: &str,
) -> Result<T> {
    let bytes = read_zip_entry_bytes(archive, path)?;
    serde_json::from_slice(&bytes)
        .map_err(|e| AegisError::Validation(format!("Failed to parse '{}': {}", path, e)))
}

fn read_zip_entry_bytes<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    path: &str,
) -> Result<Vec<u8>> {
    let mut entry = archive.by_name(path).map_err(|e| {
        AegisError::Validation(format!("Missing entry '{}' in backup archive: {}", path, e))
    })?;
    let mut buf = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut buf).map_err(|e| {
        AegisError::Validation(format!(
            "Failed to read entry '{}' from backup archive: {}",
            path, e
        ))
    })?;
    Ok(buf)
}
