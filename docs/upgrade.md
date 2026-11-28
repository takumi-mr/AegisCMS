[← Back to the documentation index](README.md)

**English** | [日本語](upgrade.ja.md)

# Updating AegisCMS

## Before You Update

1. Read the release notes on [Releases](https://github.com/takumi-mr/AegisCMS/releases). A change marked **BREAKING** may need action on your side.
2. [Take a backup](backup-restore.md) and verify it.
3. Check the current version: `aegis-server --version` (or `docker exec aegiscms aegis-server --version`).

## Steps for Specific Versions

### Updating to v0.13.0: some official plugins come from the store

From v0.13.0, these official plugins are no longer bundled with AegisCMS. They are installed from the [package store](store.md) instead: `official-ads`, `official-back-to-top`, `official-charts`, `official-clarity`, `official-code-lines`, `official-embeds`, `official-external-links`, `official-lightbox`, `official-link-embed`, `official-reading-time`, `official-share-buttons`.

If you have none of them enabled, there is nothing to do. If you have any enabled, **the new version does not start** until they are installed again (the log says `Plugin '…' is enabled in [plugins].enabled but not installed`). Their settings are kept. Before updating:

1. Note which of them are enabled, and disable them (Admin Studio › Plugins, or remove them from `[plugins].enabled`).
2. Update as described below. Use the new shipped configuration for `[paths]` (or set `plugins_dir = "data/plugins"` and `bundled_plugins_dir = "plugins"` yourself, [configuration 2.5](configuration.md#25-paths--directories)), so that what you install stays across updates.
3. Install them again, either with `POST /api/v1/store/install` (`{"catalog": "official", "kind": "plugin", "id": "official-charts"}`, [configuration 2.28](configuration.md#228-store--package-store)), or by downloading the ZIP from [the store's Releases](https://github.com/takumi-mr/AegisCMS-Store/releases) and uploading it in Admin Studio › Plugins.
4. Enable them again.

## Update

Database changes are applied by themselves the first time the new version starts. You do not run a migration step, and updating from an older version straight to the latest is fine.

### Docker

```bash
docker pull ghcr.io/takumi-mr/aegiscms:latest
docker stop aegiscms && docker rm aegiscms
docker run -d --name aegiscms -p 8080:8080 -v aegis-data:/app/data ghcr.io/takumi-mr/aegiscms:latest
```

Use the same volume (`-v`) and the same options as before. Your data stays in the volume. Plugins and themes installed from Admin Studio or the store stay there too (`/app/data/plugins`, `/app/data/themes`), and the bundled ones are updated with the image. With `deploy/`, run `./deploy.sh pull && ./deploy.sh up`.

#### Pinning a version (Docker)

`latest` always points to the newest release, so `docker pull` can move you across a breaking change. For a production site, name the version you want:

| Tag                                 | Follows                                       |
| ----------------------------------- | --------------------------------------------- |
| `ghcr.io/takumi-mr/aegiscms:0.9.0`  | Nothing. This exact release only              |
| `ghcr.io/takumi-mr/aegiscms:0.9`    | Fixes (`0.9.1`, `0.9.2`, ...), but not `0.10` |
| `ghcr.io/takumi-mr/aegiscms:latest` | Every new release                             |

Before 1.0, a new minor version (`0.9` to `0.10`) can contain breaking changes, so `0.9` is a good choice: you get fixes by running `docker pull` again, and you move to the next minor version when you decide to, after reading its release notes. Version tags exist from `0.9.0` on.

### Binary

1. Stop the server.
2. Download the new archive and extract it into a **new folder**.
3. Copy your data and settings from the old folder: `aegis.db*`, `data/`, and your edited `config/` and `schemas/`. Compare the new `config/default.toml` and `schemas/` with yours, since new settings may have been added.
4. Use the bundled plugins and themes (`plugins/`, `themes/`) from the new folder (they are updated with the release). Plugins and themes installed from Admin Studio or the store are under `data/` with the shipped configuration (`[paths].plugins_dir = "data/plugins"`, `themes_dir = "data/themes"`), so they move with step 3. If you still have the old setting (`plugins_dir = "plugins"`), copy what you installed into the new `plugins/` and `themes/` ([configuration 2.5](configuration.md#25-paths--directories)).
5. Start the new binary, and keep the old folder until you are sure everything works.

If you run it as a service, point `WorkingDirectory` and `ExecStart` at the new folder.

## Check It Worked

- The log shows no errors, and `/` and `/admin/` open.
- Log in to Admin Studio, and open a few pages.

## Going Back

There is no downgrade step. A database that a newer version has updated is refused by an older version with a message like `The database has migration N applied, but this version of AegisCMS does not know it`. To go back, install the old version and [restore the backup](backup-restore.md) you took before updating. This is why the backup comes first.
