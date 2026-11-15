[← Back to the documentation index](README.md)

**English** | [日本語](troubleshooting.ja.md)

# Troubleshooting

First look at the log. Docker: `docker logs aegiscms`. systemd: `journalctl -u aegiscms`. The detail level follows the `RUST_LOG` environment variable (for example, `RUST_LOG=debug`).

## I cannot log in

| Symptom                                   | Cause and fix                                                                                                                                                                                                                                                                                                |
| ----------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| The log says `No user accounts exist`     | A new site has no accounts. Create one: `aegis-server user add admin --role admin` ([Installing](install.md#4-create-an-administrator)).                                                                                                                                                                     |
| `429` after several wrong passwords       | Logins are locked for 15 minutes after repeated failures, even with the right password. Wait, or release it with `aegis-server user unlock <username>`. The command works only with `[cache].backend = "redis"`; with the default in-memory cache, restart the server ([Authentication](authentication.md)). |
| I forgot the password                     | Run `aegis-server user passwd <username>` on the machine that runs AegisCMS.                                                                                                                                                                                                                                 |
| Everyone is locked at once behind a proxy | The proxy's IP is counted for everyone. Set `[audit].trust_forwarded_for = true` ([Publishing](publish.md#2-your-own-reverse-proxy)).                                                                                                                                                                        |

## The site does not open

| Symptom                                       | Cause and fix                                                                                                                                                                          |
| --------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Works on the server, not from another machine | The binary listens on `127.0.0.1` by default. Use a reverse proxy, or set `[server].host`. (The Docker image already listens on all addresses; check `-p 8080:8080` and the firewall.) |
| The port is already in use                    | Another program uses 8080. Start with `--port 8081` or set `PORT`.                                                                                                                     |
| Admin Studio is blank under a subpath         | `[server].base_path` and the proxy must agree ([Configuration reference 4.2](configuration.md#42-publishing-under-a-subpath)).                                                         |
| A theme or plugin is not applied              | Installed plugins are off until listed in `[plugins].enabled` (the log says `is disabled`). Check the spelling of the `[theme]` name too.                                              |

## Uploading, email and addresses

- **An upload fails for a large file**: raise `[storage].max_upload_mb` and the proxy's limit (nginx: `client_max_body_size`) to the same value or more.
- **Feeds or the sitemap show `http://`**: pass `X-Forwarded-Proto: https` from the proxy and set the canonical URL in **Settings › SEO**.
- **No email arrives**: check `[notifications.email]`, then run `aegis-server notify test --to you@example.com` ([Notifications](notifications.md)).

## Starting, updating and restoring

| Message                                                                               | Cause and fix                                                                                                                      |
| ------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| `The database has migration N applied, but this version of AegisCMS does not know it` | A newer version updated this database. Start that version, or install the old one and restore the backup ([Updating](upgrade.md)). |
| `Migration N was changed after it was applied (its checksum differs)`                 | A released migration file was modified. Use the original release files.                                                            |
| `already exists (use overwrite option to replace)` when restoring                     | The target has been started before. Restore into an empty folder or volume, or add `--overwrite` ([Restore](backup-restore.md)).   |
| `Warning:` lines when restoring                                                       | Custom fields that are not in the current schema were dropped. Put the right schemas in place first, and restore again.            |

If it is still unclear, [open an issue](https://github.com/takumi-mr/AegisCMS/issues) with the version (`aegis-server --version`), how you run it, and the relevant log lines. Remove passwords and tokens first.
