use std::collections::{BTreeMap, HashMap};
use std::io::{Seek, Write};
use std::path::Path;
use std::sync::Arc;

use aegis_core::{AegisError, Result, StorageBackend};
use aegis_db::{
    record_from_content_version, CommentListQuery, ContactListQuery, ContentRecord,
    ContentVersionRecord, DatabaseAdapter, ForumPostRecord, ForumThreadRecord, OrderListQuery,
    PostListQuery, ReviewListQuery, TagListQuery, ThreadListQuery,
};
use aegis_schema::SchemaDefinition;
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

use crate::manifest::{
    is_safe_relative_path, sha256_hex, BackupManifest, DatabaseSectionManifest, StorageFileEntry,
    StorageSectionManifest, TableManifestEntry, FORMAT_VERSION, MANIFEST_FILE_NAME,
};

/// Options controlling what is included in a backup dump.
#[derive(Debug, Clone)]
pub struct DumpOptions {
    /// Include all database tables (`_aegis_*` and `content_*`).
    pub include_database: bool,
    /// Include all uploaded objects from [`StorageBackend`].
    pub include_storage: bool,
    /// Maximum number of storage objects to scan and dump.
    pub max_storage_objects: usize,
}

impl Default for DumpOptions {
    fn default() -> Self {
        Self {
            include_database: true,
            include_storage: true,
            max_storage_objects: 100_000,
        }
    }
}

/// A content record bundled with its editorial version history and publication pointer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentBundle {
    pub record: ContentRecord,
    /// Immutable versions ordered oldest-first (`revision` ascending).
    pub versions: Vec<ContentVersionRecord>,
    /// Index into `versions` that `_aegis_content_publications` points to, if published.
    pub published_version_index: Option<usize>,
}

/// A forum thread bundled with all of its posts ordered by `post_number`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForumThreadBundle {
    pub thread: ForumThreadRecord,
    pub posts: Vec<ForumPostRecord>,
}

/// Write a backup archive to a file on disk.
///
/// Writes to a temporary sibling file first and renames on completion so an interrupted dump
/// never leaves a half-written archive at `output_path`.
pub async fn dump_to_file(
    db: &Arc<dyn DatabaseAdapter>,
    storage: &Arc<dyn StorageBackend>,
    schemas: &HashMap<String, SchemaDefinition>,
    options: &DumpOptions,
    output_path: &Path,
) -> Result<BackupManifest> {
    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                AegisError::Internal(format!(
                    "Failed to create parent directory '{}': {}",
                    parent.display(),
                    e
                ))
            })?;
        }
    }

    let tmp_path = output_path.with_extension("zip.part");
    let file = std::fs::File::create(&tmp_path).map_err(|e| {
        AegisError::Internal(format!(
            "Failed to create temporary backup file '{}': {}",
            tmp_path.display(),
            e
        ))
    })?;

    match dump_to_writer(db, storage, schemas, options, file).await {
        Ok(manifest) => {
            std::fs::rename(&tmp_path, output_path).map_err(|e| {
                AegisError::Internal(format!(
                    "Failed to move backup archive to '{}': {}",
                    output_path.display(),
                    e
                ))
            })?;
            Ok(manifest)
        }
        Err(err) => {
            let _ = std::fs::remove_file(&tmp_path);
            Err(err)
        }
    }
}

/// Write a backup archive to any seekable writer (`File`, `Cursor<Vec<u8>>`, etc.).
pub async fn dump_to_writer<W: Write + Seek>(
    db: &Arc<dyn DatabaseAdapter>,
    storage: &Arc<dyn StorageBackend>,
    schemas: &HashMap<String, SchemaDefinition>,
    options: &DumpOptions,
    writer: W,
) -> Result<BackupManifest> {
    let mut zip = zip::ZipWriter::new(writer);
    let mut db_section = DatabaseSectionManifest {
        included: options.include_database,
        tables: Vec::new(),
    };
    let mut storage_section = StorageSectionManifest {
        included: options.include_storage,
        files: Vec::new(),
        total_bytes: 0,
    };

    if options.include_database {
        dump_database_section(&mut zip, &mut db_section.tables, db, schemas).await?;
    }

    if options.include_storage {
        dump_storage_section(&mut zip, &mut storage_section, storage, options).await?;
    }

    let manifest = BackupManifest {
        format_version: FORMAT_VERSION,
        aegis_version: env!("CARGO_PKG_VERSION").to_string(),
        created_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        database: db_section,
        storage: storage_section,
    };

    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|e| AegisError::Internal(format!("Failed to serialize manifest: {}", e)))?;
    let file_opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file(MANIFEST_FILE_NAME, file_opts)
        .map_err(|e| AegisError::Internal(format!("Failed to write manifest entry: {}", e)))?;
    zip.write_all(&manifest_bytes)
        .map_err(|e| AegisError::Internal(format!("Failed to write manifest bytes: {}", e)))?;
    zip.finish()
        .map_err(|e| AegisError::Internal(format!("Failed to finalize backup archive: {}", e)))?;

    Ok(manifest)
}

async fn dump_database_section<W: Write + Seek>(
    zip: &mut zip::ZipWriter<W>,
    tables: &mut Vec<TableManifestEntry>,
    db: &Arc<dyn DatabaseAdapter>,
    schemas: &HashMap<String, SchemaDefinition>,
) -> Result<()> {
    // 1. Settings (_aegis_settings)
    let settings_map = db.list_settings().await?;
    let settings: BTreeMap<String, String> = settings_map.into_iter().collect();
    write_table_json(
        zip,
        tables,
        aegis_db::SETTINGS_TABLE,
        "database/settings.json",
        settings.len() as u64,
        &settings,
    )?;

    // 2. Users (_aegis_users)
    let users = db.list_users().await?;
    write_table_json(
        zip,
        tables,
        aegis_db::USER_TABLE,
        "database/users.json",
        users.len() as u64,
        &users,
    )?;

    // 3. Identity links (_aegis_user_identities)
    let identities = db.list_identity_links().await?;
    write_table_json(
        zip,
        tables,
        aegis_db::IDENTITY_TABLE,
        "database/identities.json",
        identities.len() as u64,
        &identities,
    )?;

    // 4. Customers (_aegis_customers)
    let mut customers = Vec::new();
    let mut offset = 0;
    let page_size = 500;
    loop {
        let (batch, total) = db.list_customers(page_size, offset).await?;
        let batch_len = batch.len() as u64;
        customers.extend(batch);
        offset += batch_len;
        if batch_len == 0 || offset >= total {
            break;
        }
    }
    write_table_json(
        zip,
        tables,
        aegis_db::CUSTOMER_TABLE,
        "database/customers.json",
        customers.len() as u64,
        &customers,
    )?;

    // 5. Dynamic content tables + versions + publication pointers
    let mut schema_names: Vec<String> = schemas.keys().cloned().collect();
    schema_names.sort();
    for schema_name in schema_names {
        let Some(schema) = schemas.get(&schema_name) else {
            continue;
        };
        let mut records = db.list_contents(schema).await?;
        records.sort_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });

        let mut bundles = Vec::with_capacity(records.len());
        for record in records {
            let mut versions = db
                .list_content_versions(&record.content_type, &record.id)
                .await?;
            versions.sort_by_key(|v| v.revision);

            let published = db.get_published_content(schema, &record.id).await?;
            let published_version_index = match published {
                Some(ref pub_rec) if !versions.is_empty() => versions
                    .iter()
                    .enumerate()
                    .rev()
                    .find(|(_, v)| {
                        record_from_content_version(v)
                            .map(|snap| {
                                snap.title == pub_rec.title
                                    && snap.slug == pub_rec.slug
                                    && snap.blocks_json == pub_rec.blocks_json
                                    && snap.custom_fields == pub_rec.custom_fields
                            })
                            .unwrap_or(false)
                    })
                    .map(|(idx, _)| idx)
                    .or_else(|| {
                        if record.status == "published" {
                            Some(versions.len() - 1)
                        } else {
                            None
                        }
                    }),
                _ => None,
            };

            bundles.push(ContentBundle {
                record,
                versions,
                published_version_index,
            });
        }

        let path = format!("database/contents/{}.json", schema.content_type);
        write_table_json(
            zip,
            tables,
            &schema.table_name(),
            &path,
            bundles.len() as u64,
            &bundles,
        )?;
    }

    // 6. Product variants (_aegis_product_variants)
    let variants = db.list_all_product_variants().await?;
    write_table_json(
        zip,
        tables,
        aegis_db::PRODUCT_VARIANT_TABLE,
        "database/variants.json",
        variants.len() as u64,
        &variants,
    )?;

    // 7. Orders (_aegis_orders)
    let mut orders = Vec::new();
    let mut offset = 0;
    loop {
        let page = db
            .list_orders(&OrderListQuery {
                status: None,
                limit: page_size,
                offset,
            })
            .await?;
        let batch_len = page.orders.len() as u64;
        orders.extend(page.orders);
        offset += batch_len;
        if batch_len == 0 || offset >= page.total {
            break;
        }
    }
    write_table_json(
        zip,
        tables,
        aegis_db::orders::ORDER_TABLE,
        "database/orders.json",
        orders.len() as u64,
        &orders,
    )?;

    // 8. Comments (_aegis_comments)
    let comments = db
        .list_comments(&CommentListQuery {
            oldest_first: true,
            limit: None,
            ..Default::default()
        })
        .await?
        .comments;
    write_table_json(
        zip,
        tables,
        aegis_db::COMMENT_TABLE,
        "database/comments.json",
        comments.len() as u64,
        &comments,
    )?;

    // 8b. Tags (_aegis_tags) and the tags on content (_aegis_content_tags)
    let tags: Vec<aegis_db::TagRecord> = db
        .list_tags(&TagListQuery::default())
        .await?
        .tags
        .into_iter()
        .map(|t| t.tag)
        .collect();
    write_table_json(
        zip,
        tables,
        aegis_db::TAG_TABLE,
        "database/tags.json",
        tags.len() as u64,
        &tags,
    )?;
    let content_tags = db.list_all_content_tags().await?;
    write_table_json(
        zip,
        tables,
        aegis_db::CONTENT_TAG_TABLE,
        "database/content_tags.json",
        content_tags.len() as u64,
        &content_tags,
    )?;

    // 9. Reviews (_aegis_reviews)
    let reviews = db
        .list_reviews(&ReviewListQuery {
            oldest_first: true,
            limit: None,
            ..Default::default()
        })
        .await?
        .reviews;
    write_table_json(
        zip,
        tables,
        aegis_db::REVIEW_TABLE,
        "database/reviews.json",
        reviews.len() as u64,
        &reviews,
    )?;

    // 10. Forum categories, threads + posts, reports
    let forum_categories = db.list_categories().await?;
    write_table_json(
        zip,
        tables,
        aegis_db::FORUM_CATEGORY_TABLE,
        "database/forum_categories.json",
        forum_categories.len() as u64,
        &forum_categories,
    )?;

    let mut threads = db
        .list_threads(&ThreadListQuery {
            category_id: None,
            status: None,
            before_last_post_at: None,
            before_id: None,
            limit: 100_000,
        })
        .await?;
    threads.sort_by(|a, b| {
        a.created_at
            .cmp(&b.created_at)
            .then_with(|| a.id.cmp(&b.id))
    });

    let mut thread_bundles = Vec::with_capacity(threads.len());
    for thread in threads {
        let mut posts = Vec::new();
        let mut after_post_number = 0;
        loop {
            let batch = db
                .list_posts(&PostListQuery {
                    thread_id: thread.id.clone(),
                    status: None,
                    after_post_number,
                    limit: 500,
                })
                .await?;
            let Some(last) = batch.last() else {
                break;
            };
            after_post_number = last.post_number;
            let batch_len = batch.len();
            posts.extend(batch);
            if batch_len < 500 {
                break;
            }
        }
        posts.sort_by_key(|p| p.post_number);
        thread_bundles.push(ForumThreadBundle { thread, posts });
    }
    write_table_json(
        zip,
        tables,
        aegis_db::FORUM_THREAD_TABLE,
        "database/forum_threads.json",
        thread_bundles.len() as u64,
        &thread_bundles,
    )?;

    let forum_reports = db.list_reports(None, 100_000).await?;
    write_table_json(
        zip,
        tables,
        aegis_db::FORUM_REPORT_TABLE,
        "database/forum_reports.json",
        forum_reports.len() as u64,
        &forum_reports,
    )?;

    // 11. Contact messages (_aegis_contact_messages)
    let contact_messages = db
        .list_contact_messages(&ContactListQuery {
            limit: None,
            ..Default::default()
        })
        .await?
        .messages;
    write_table_json(
        zip,
        tables,
        aegis_db::CONTACT_TABLE,
        "database/contact_messages.json",
        contact_messages.len() as u64,
        &contact_messages,
    )?;

    Ok(())
}

async fn dump_storage_section<W: Write + Seek>(
    zip: &mut zip::ZipWriter<W>,
    section: &mut StorageSectionManifest,
    storage: &Arc<dyn StorageBackend>,
    options: &DumpOptions,
) -> Result<()> {
    let mut objects = storage.list("", options.max_storage_objects).await?;
    objects.sort_by(|a, b| a.key.cmp(&b.key));

    let file_opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    for obj in objects {
        if !is_safe_relative_path(&obj.key) {
            tracing::warn!("Skipping storage object with unsafe key: '{}'", obj.key);
            continue;
        }
        let data = storage.get(&obj.key).await?;
        let size_bytes = data.len() as u64;
        let sha256 = sha256_hex(&data);
        let zip_path = format!("storage/{}", obj.key);

        zip.start_file(&zip_path, file_opts).map_err(|e| {
            AegisError::Internal(format!("Failed to add '{}' to ZIP: {}", zip_path, e))
        })?;
        zip.write_all(&data).map_err(|e| {
            AegisError::Internal(format!("Failed to write '{}' to ZIP: {}", zip_path, e))
        })?;

        section.total_bytes += size_bytes;
        section.files.push(StorageFileEntry {
            key: obj.key,
            path: zip_path,
            size_bytes,
            sha256,
        });
    }

    Ok(())
}

fn write_table_json<W: Write + Seek, T: Serialize>(
    zip: &mut zip::ZipWriter<W>,
    tables: &mut Vec<TableManifestEntry>,
    name: &str,
    path: &str,
    count: u64,
    value: &T,
) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|e| AegisError::Internal(format!("Failed to serialize {}: {}", name, e)))?;
    let sha256 = sha256_hex(&bytes);
    let file_opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file(path, file_opts)
        .map_err(|e| AegisError::Internal(format!("ZIP start_file '{}' failed: {}", path, e)))?;
    zip.write_all(&bytes)
        .map_err(|e| AegisError::Internal(format!("ZIP write '{}' failed: {}", path, e)))?;
    tables.push(TableManifestEntry {
        name: name.to_string(),
        path: path.to_string(),
        count,
        sha256,
    });
    Ok(())
}
