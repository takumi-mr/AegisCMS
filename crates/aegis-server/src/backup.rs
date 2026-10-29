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

    let summary = restore_from_file(&db, &storage, &schemas, &options, &args.input).await?;
    println!(
        "Restored from {} ({} tables / {} records, {} storage files / {} bytes)",
        args.input.display(),
        summary.tables_restored,
        summary.records_restored,
        summary.storage_files_restored,
        summary.storage_bytes_restored
    );
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
}
