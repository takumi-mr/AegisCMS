use std::collections::HashMap;
use std::path::PathBuf;

use aegis_backup::{
    dump_to_file, restore_from_file, verify_backup_file, DumpOptions, RestoreOptions,
};
use aegis_schema::SchemaLoader;
use clap::Args;

use crate::app;
use crate::config::AegisConfig;

#[derive(Args, Debug, Clone)]
pub struct DumpArgs {
    /// Output backup archive path (e.g. backup.zip)
    #[arg(short, long)]
    pub output: PathBuf,

    /// Skip database tables and only export uploaded storage objects
    #[arg(long)]
    pub no_database: bool,

    /// Skip uploaded storage objects and only export database tables
    #[arg(long)]
    pub no_storage: bool,
}

#[derive(Args, Debug, Clone)]
pub struct RestoreArgs {
    /// Input backup archive path (e.g. backup.zip)
    #[arg(short, long)]
    pub input: PathBuf,

    /// Overwrite existing records and storage objects when IDs or keys collide
    #[arg(long)]
    pub overwrite: bool,

    /// Only verify the archive's manifest and SHA-256 checksums without restoring
    #[arg(long)]
    pub verify_only: bool,

    /// Skip restoring database tables
    #[arg(long)]
    pub no_database: bool,

    /// Skip restoring uploaded storage objects
    #[arg(long)]
    pub no_storage: bool,
}

pub async fn run_dump(config: &AegisConfig, args: DumpArgs) -> anyhow::Result<()> {
    if args.no_database && args.no_storage {
        anyhow::bail!("Nothing to dump: both --no-database and --no-storage were specified");
    }

    let base_path = config.server.normalized_base_path()?;
    let schemas_vec = if config.paths.schemas_dir.exists() {
        SchemaLoader::load_dir(&config.paths.schemas_dir)?
    } else {
        Vec::new()
    };
    let schemas: HashMap<String, _> = schemas_vec
        .into_iter()
        .map(|s| (s.content_type.clone(), s))
        .collect();

    let db = aegis_db::create_database_adapter(&config.database).await?;
    db.run_migrations(&schemas).await?;

    let storage = aegis_storage::create_storage_backend(
        &config.storage,
        &app::under_base_path(&base_path, &config.storage.local.public_url_prefix),
    )?;

    let options = DumpOptions {
        include_database: !args.no_database,
        include_storage: !args.no_storage,
        ..Default::default()
    };

    let manifest = dump_to_file(&db, &storage, &schemas, &options, &args.output).await?;
    let total_records: u64 = manifest.database.tables.iter().map(|t| t.count).sum();
    println!(
        "Backup created at {} ({} tables / {} records, {} storage files / {} bytes)",
        args.output.display(),
        manifest.database.tables.len(),
        total_records,
        manifest.storage.files.len(),
        manifest.storage.total_bytes
    );
    Ok(())
}

pub async fn run_restore(config: &AegisConfig, args: RestoreArgs) -> anyhow::Result<()> {
    if args.verify_only {
        let manifest = verify_backup_file(&args.input)?;
        let total_records: u64 = manifest.database.tables.iter().map(|t| t.count).sum();
        println!(
            "Verified backup {} (format v{}, created {}, {} tables / {} records, {} storage files / {} bytes)",
            args.input.display(),
            manifest.format_version,
            manifest.created_at,
            manifest.database.tables.len(),
            total_records,
            manifest.storage.files.len(),
            manifest.storage.total_bytes
        );
        return Ok(());
    }

    if args.no_database && args.no_storage {
        anyhow::bail!("Nothing to restore: both --no-database and --no-storage were specified");
    }

    let base_path = config.server.normalized_base_path()?;
    let schemas_vec = if config.paths.schemas_dir.exists() {
        SchemaLoader::load_dir(&config.paths.schemas_dir)?
    } else {
        Vec::new()
    };
    let schemas: HashMap<String, _> = schemas_vec
        .into_iter()
        .map(|s| (s.content_type.clone(), s))
        .collect();

    let db = aegis_db::create_database_adapter(&config.database).await?;
    let storage = aegis_storage::create_storage_backend(
        &config.storage,
        &app::under_base_path(&base_path, &config.storage.local.public_url_prefix),
    )?;

    let options = RestoreOptions {
        include_database: !args.no_database,
        include_storage: !args.no_storage,
        overwrite: args.overwrite,
    };

    // 宣言的クエリの定義は、データを書き換える前に検証する（誤りがあれば何も変えずに止める）
    let query_service =
        crate::queries::QueryService::load(&config.queries, &config.paths.queries_dir, &schemas)?;

    let summary = restore_from_file(&db, &storage, &schemas, &options, &args.input).await?;
    // 公開側の列（副表）は公開の記録から導ける派生データなので、復元したデータに合わせて作り直す
    if !args.no_database {
        let rebuilt =
            crate::queries::prepare_columns_of(&query_service, db.as_ref(), &schemas, true).await?;
        if rebuilt > 0 {
            println!("Rebuilt the published columns of declarative queries ({rebuilt} tables)");
        }
    }
    println!(
        "Restored from {} ({} tables / {} records, {} storage files / {} bytes)",
        args.input.display(),
        summary.tables_restored,
        summary.records_restored,
        summary.storage_files_restored,
        summary.storage_bytes_restored
    );
    for warning in &summary.warnings {
        eprintln!("Warning: {}", warning);
    }
    if !summary.warnings.is_empty() {
        eprintln!(
            "{} item(s) of the backup were dropped because they do not match the current schemas.",
            summary.warnings.len()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cli_dump_verify_and_restore() {
        let dir = tempfile::tempdir().unwrap();
        let db_file = dir.path().join("src.db");
        let upload_dir = dir.path().join("uploads");
        let archive_path = dir.path().join("backup.zip");

        let mut src_cfg = AegisConfig::default();
        src_cfg.database.url = format!("sqlite://{}?mode=rwc", db_file.to_string_lossy());
        src_cfg.storage.local.upload_dir = upload_dir.clone();
        src_cfg.paths.schemas_dir = dir.path().join("schemas");
        std::fs::create_dir_all(&src_cfg.paths.schemas_dir).unwrap();

        let db = aegis_db::create_database_adapter(&src_cfg.database)
            .await
            .unwrap();
        db.set_setting("site.title", "CLI Backup Site")
            .await
            .unwrap();

        run_dump(
            &src_cfg,
            DumpArgs {
                output: archive_path.clone(),
                no_database: false,
                no_storage: false,
            },
        )
        .await
        .unwrap();
        assert!(archive_path.exists());

        // Verify-only succeeds
        run_restore(
            &src_cfg,
            RestoreArgs {
                input: archive_path.clone(),
                overwrite: false,
                verify_only: true,
                no_database: false,
                no_storage: false,
            },
        )
        .await
        .unwrap();

        // Restore into a second SQLite DB
        let dst_db_file = dir.path().join("dst.db");
        let mut dst_cfg = src_cfg.clone();
        dst_cfg.database.url = format!("sqlite://{}?mode=rwc", dst_db_file.to_string_lossy());
        dst_cfg.storage.local.upload_dir = dir.path().join("dst_uploads");

        run_restore(
            &dst_cfg,
            RestoreArgs {
                input: archive_path,
                overwrite: false,
                verify_only: false,
                no_database: false,
                no_storage: false,
            },
        )
        .await
        .unwrap();

        let dst_db = aegis_db::create_database_adapter(&dst_cfg.database)
            .await
            .unwrap();
        assert_eq!(
            dst_db.list_settings().await.unwrap().get("site.title"),
            Some(&"CLI Backup Site".to_string())
        );
    }
    fn product(id: &str) -> aegis_db::ContentRecord {
        aegis_db::ContentRecord {
            id: id.to_string(),
            content_type: "product".to_string(),
            title: format!("Product {id}"),
            slug: format!("p-{id}"),
            status: "published".to_string(),
            blocks_json: "[]".to_string(),
            author_id: None,
            published_at: Some("2026-03-01T00:00:00Z".to_string()),
            unpublish_at: None,
            created_at: "2026-03-01T00:00:00Z".to_string(),
            updated_at: "2026-03-01T00:00:00Z".to_string(),
            custom_fields: HashMap::from([("price".to_string(), serde_json::json!(100))]),
        }
    }

    async fn publish(
        db: &dyn aegis_db::DatabaseAdapter,
        schema: &aegis_schema::SchemaDefinition,
        record: &aegis_db::ContentRecord,
    ) {
        let version = aegis_db::ContentVersionRecord {
            content_type: record.content_type.clone(),
            content_id: record.id.clone(),
            revision: 0,
            snapshot_json: serde_json::to_string(record).unwrap(),
            created_at: record.updated_at.clone(),
            created_by: None,
            change_note: None,
        };
        db.insert_content_with_versions(
            schema,
            record,
            &[version],
            aegis_db::ContentPublicationChange::PublishVersion(0),
        )
        .await
        .unwrap();
    }

    /// 復元のあと、宣言的クエリが読む公開側の列（副表）は、復元したデータに合わせて作り直される
    /// （定義が同じなので、通常の起動時の用意では古いまま残る）。
    #[tokio::test]
    async fn test_restore_resets_the_published_columns_of_queries() {
        let dir = tempfile::tempdir().unwrap();
        let schemas_dir = dir.path().join("schemas");
        let queries_dir = dir.path().join("queries");
        std::fs::create_dir_all(&schemas_dir).unwrap();
        std::fs::create_dir_all(&queries_dir).unwrap();
        std::fs::write(
            schemas_dir.join("product.yaml"),
            "content_type: product
label: P
fields:
  - name: price
    type: integer
    required: true
",
        )
        .unwrap();
        std::fs::write(
            queries_dir.join("how_many.yaml"),
            "name: how_many
from: product
select: [{ fn: count, as: n }]
",
        )
        .unwrap();

        let mut cfg = AegisConfig::default();
        cfg.paths.schemas_dir = schemas_dir.clone();
        cfg.paths.queries_dir = queries_dir.clone();
        let config_for = |name: &str| {
            let mut c = cfg.clone();
            c.database.url = format!(
                "sqlite://{}?mode=rwc",
                dir.path().join(name).to_string_lossy()
            );
            c.storage.local.upload_dir = dir.path().join(format!("{name}-uploads"));
            c
        };
        let schema = SchemaLoader::load_dir(&schemas_dir).unwrap().remove(0);
        let schemas: HashMap<String, _> = HashMap::from([("product".to_string(), schema.clone())]);

        // 元の DB: 公開中の商品 p1・p2
        let src_cfg = config_for("src.db");
        let src = aegis_db::create_database_adapter(&src_cfg.database)
            .await
            .unwrap();
        src.run_migrations(&schemas).await.unwrap();
        publish(src.as_ref(), &schema, &product("p1")).await;
        publish(src.as_ref(), &schema, &product("p2")).await;
        let archive = dir.path().join("backup.zip");
        run_dump(
            &src_cfg,
            DumpArgs {
                output: archive.clone(),
                no_database: false,
                no_storage: true,
            },
        )
        .await
        .unwrap();

        // 復元先: すでに副表があり、別の商品 p0 が入っている
        let dst_cfg = config_for("dst.db");
        let dst = aegis_db::create_database_adapter(&dst_cfg.database)
            .await
            .unwrap();
        dst.run_migrations(&schemas).await.unwrap();
        let prepared = crate::queries::prepare_columns(
            dst.as_ref(),
            &dst_cfg.queries,
            &queries_dir,
            &schemas,
            false,
        )
        .await
        .unwrap();
        assert_eq!(prepared, 1);
        publish(dst.as_ref(), &schema, &product("p0")).await;

        run_restore(
            &dst_cfg,
            RestoreArgs {
                input: archive,
                overwrite: true,
                verify_only: false,
                no_database: false,
                no_storage: true,
            },
        )
        .await
        .unwrap();

        // 新しい接続で、通常の起動時と同じ用意（定義が同じなら作り直さない）をして数える
        let after = aegis_db::create_database_adapter(&dst_cfg.database)
            .await
            .unwrap();
        crate::queries::prepare_columns(
            after.as_ref(),
            &dst_cfg.queries,
            &queries_dir,
            &schemas,
            false,
        )
        .await
        .unwrap();
        let service =
            crate::queries::QueryService::load(&dst_cfg.queries, &queries_dir, &schemas).unwrap();
        let rows = after
            .run_query(&service.definitions()[0], &schema, 100)
            .await
            .unwrap();
        match rows {
            aegis_db::published_exec::QueryRows::Rows(rows) => {
                // 復元は同じキーを上書きするだけで、復元先の p0 は消えない: 公開の記録は p0・p1・p2。
                // 作り直さなければ、副表は復元前の p0 の 1 件のまま
                assert_eq!(rows[0]["n"], serde_json::json!(3), "{rows:?}");
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
