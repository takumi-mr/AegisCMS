[← Back to the documentation index](README.md)

**English** | [日本語](backup-restore.ja.md)

# Backup and Restore

## What Is Backed Up

`aegis-server dump` writes one ZIP file with:

- the database: content and its version history, settings, and accounts (including password hashes)
- the uploaded files

It does **not** include your configuration files, schemas, themes, or plugins. Keep those in Git. The ZIP contains accounts, so store it as a secret.

The format does not depend on the database, so you can also use a backup to move from SQLite to PostgreSQL or MySQL.

## Take a Backup

It is safe to run while the server is running.

```bash
# Binary (in the extracted folder)
./aegis-server dump --output backup.zip

# Docker: write inside the container, then copy it out
docker exec aegiscms aegis-server dump --output /app/data/backup.zip
docker cp aegiscms:/app/data/backup.zip ./backup-$(date +%F).zip
docker exec aegiscms rm /app/data/backup.zip
```

Use `--no-storage` for the database only, or `--no-database` for the uploads only.

Take a backup **before every [update](upgrade.md)**, and on a schedule (for example, daily with cron) for a live site. Keep copies on another machine.

For multi-site `deploy/` setups, see [Operations in deploy/README.md](../deploy/README.md#5-operations) for backing up a site's data volume.

## Check a Backup

```bash
./aegis-server restore --input backup.zip --verify-only
```

This checks the file's checksums without changing anything. A backup you have never verified is not a backup.

## Restore

Restore into a new, empty data folder (or a new Docker volume) when you can:

```bash
# Binary: an empty folder with the same config, schemas, themes and plugins
./aegis-server restore --input backup.zip

# Docker: a new volume that has never been started
docker run --rm -v aegis-data:/app/data -v "$PWD:/backup" \
  ghcr.io/takumi-mr/aegiscms:latest restore --input /backup/backup.zip
```

Then start the server as usual.

### Restoring into a site that has already started

A server that has been started once already holds some settings, so a plain restore stops with an error like `already exists (use overwrite option to replace)`. This is a safety check against destroying data by mistake. If you really want to replace what is there, add `--overwrite`:

```bash
./aegis-server restore --input backup.zip --overwrite
```

### When the schema changed

Custom fields are restored to the **current** schemas in the `schemas` folder, not the ones at backup time. Put the schemas you want in place **before** restoring. Fields that no longer exist in the schema are dropped, and a `Warning:` line says which. Standard fields (title, body, dates) are never dropped silently.

## Try the Restore Before You Need It

Once, restore a backup into a spare folder or volume and open it. Then you know the procedure works and how long it takes.
