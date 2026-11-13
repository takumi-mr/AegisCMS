# aegis-backup

**English** | [日本語](README.ja.md)

AegisCMS's crate for one-shot backup and restore (dump / restore) of the database and upload storage (`feature = "backup"`).

## Overview

`aegis-backup` provides one-shot export and import of all the site's data (the `_aegis_*` management tables and the `content_*` content tables) and the uploaded files stored in the `StorageBackend`, as a single portable ZIP archive.

It is used for environment migration (between development, staging, and production), disaster recovery (DR), and creating and restoring periodic backups.

## Responsibilities and Features

- **One-shot dump (`dump`)**:
    - Serializes the data of all tables in a dialect-independent JSON format.
    - Bundles the uploaded media files into the ZIP archive.
    - Generates a `manifest.json` that records each file's SHA-256 hash and file size.
- **Restore with integrity verification (`restore`)**:
    - Based on `manifest.json`, verifies the SHA-256 checksums of all files before extraction.
    - Path traversal prevention (Zip Slip vulnerability protection): strictly checks that relative paths in the archive are safe.
    - Restores table data within a transaction and places media files in storage.
- **CLI integration**:
    - `aegis-server dump -o backup.zip`
    - `aegis-server restore backup.zip`

## Module Layout

| Module     | Description                                                                                  |
| :--------- | :------------------------------------------------------------------------------------------- |
| `dump`     | Creates a ZIP archive from the database and storage files (`dump_to_file`, `dump_to_writer`) |
| `restore`  | Verifies a ZIP archive and restores it to the database and storage                           |
| `manifest` | Backup metadata and SHA-256 checksum management (`BackupManifest`, `TableManifestEntry`)     |
