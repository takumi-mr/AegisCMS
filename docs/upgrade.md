[← Back to the documentation index](README.md)

**English** | [日本語](upgrade.ja.md)

# Updating AegisCMS

## Before You Update

1. Read the release notes on [Releases](https://github.com/takumi-mr/AegisCMS/releases). A change marked **BREAKING** may need action on your side.
2. [Take a backup](backup-restore.md) and verify it.
3. Check the current version: `aegis-server --version` (or `docker exec aegiscms aegis-server --version`).

## Update

Database changes are applied by themselves the first time the new version starts. You do not run a migration step, and updating from an older version straight to the latest is fine.

### Docker

```bash
docker pull ghcr.io/takumi-mr/aegiscms:latest
docker stop aegiscms && docker rm aegiscms
docker run -d --name aegiscms -p 8080:8080 -v aegis-data:/app/data ghcr.io/takumi-mr/aegiscms:latest
```

Use the same volume (`-v`) and the same options as before. Your data stays in the volume. With `deploy/`, run `./deploy.sh pull && ./deploy.sh up`.

### Binary

1. Stop the server.
2. Download the new archive and extract it into a **new folder**.
3. Copy your data and settings from the old folder: `aegis.db*`, `data/`, and your edited `config/` and `schemas/`. Compare the new `config/default.toml` and `schemas/` with yours, since new settings may have been added.
4. Take the new `plugins/` and `themes/` (they are updated with the release), plus any of your own.
5. Start the new binary, and keep the old folder until you are sure everything works.

If you run it as a service, point `WorkingDirectory` and `ExecStart` at the new folder.

## Check It Worked

- The log shows no errors, and `/` and `/admin/` open.
- Log in to Admin Studio, and open a few pages.

## Going Back

There is no downgrade step. A database that a newer version has updated is refused by an older version with a message like `The database has migration N applied, but this version of AegisCMS does not know it`. To go back, install the old version and [restore the backup](backup-restore.md) you took before updating. This is why the backup comes first.
