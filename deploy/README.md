# Publishing AegisCMS over HTTPS with Caddy (Multiple Sites Supported)

**English** | [日本語](README.ja.md)

Write your domains and site settings in `sites.toml`, and a single `./deploy.sh up` starts AegisCMS and Caddy (which obtains and renews HTTPS certificates from Let's Encrypt automatically). To add a site, just add a `[[sites]]` to `sites.toml` and run `./deploy.sh up` again.

```text
                    ┌────────────── Server (Docker) ──────────────┐
 example.com ──┐    │  Caddy ──┬─ main  (AegisCMS) ── main-data   │
 blog.example ─┼─▶ │  :80/443 └─ blog  (AegisCMS) ── blog-data   │
 www.example ──┘    │   (www redirects to example.com)            │
                    └─────────────────────────────────────────────┘
```

- **One site = one container.** The database (SQLite), uploads, administrators, and plugin enablement are separate per site, so a bug or load in one site is less likely to affect the others.
- The sites' containers are placed on separate networks, and only Caddy's ports 80 / 443 are reachable from outside.
- Caddy redirects HTTP to HTTPS and adds HSTS (`max-age=31536000`) to each site.
- Each site runs with `[audit].trust_forwarded_for = true` (the viewer's IP is used for audit logs and login attempt limits). Caddy ignores any `X-Forwarded-For` that arrives from an untrusted source and sets it again, and the sites' containers cannot be reached directly from outside, so the IP cannot be spoofed ([Configuration reference 4.4](../docs/configuration.md#44-other-settings-to-check)).

## What You Need

- A Linux server with Docker and Docker Compose v2 (`docker compose`) installed
- The server's ports 80 / 443 open (UDP 443 is for HTTP/3; optional)
- DNS (A / AAAA records) for each domain pointing to the server

## 1. First Start

```bash
git clone https://github.com/takumi-mr/aegiscms
cd aegiscms/deploy
cp sites.example.toml sites.toml
vi sites.toml          # write the domains, site names, themes, etc.
./deploy.sh up         # pull the public image, generate the config, and start
./deploy.sh user main  # create the administrator admin for site main (you are asked for a password)
```

`https://<domain>/` is the site, and `https://<domain>/admin/` is Admin Studio.

The first `./deploy.sh up` automatically pulls (`docker pull`) the official public image from the GitHub Container Registry (`ghcr.io/takumi-mr/aegiscms:latest`), generates the config, and starts (no Rust build environment is needed, and it starts immediately).

> [!TIP]
> - To build the image yourself from the repository source, run `./deploy.sh build`, or specify a local image name like `AEGIS_IMAGE=aegiscms:latest ./deploy.sh up`.
> - When trying it for the first time, set `acme_staging = true` to try without worrying about Let's Encrypt's rate limits (the certificates are not trusted by browsers). Once it works, set it back to `false` and run `./deploy.sh up`. To try it locally, use a `.localhost` domain such as `domains = ["main.localhost"]`, and Caddy creates a certificate with its own certificate authority.

## 2. `sites.toml`

```toml
[deploy]
acme_email = "admin@example.com"   # Let's Encrypt contact
acme_staging = false

[[sites]]
name = "main"                      # service name and data volume name
domains = ["example.com", "www.example.com"]   # the first is the canonical domain; the rest redirect to it
site_title = "Example"
site_description = "..."
theme = "minimal-editorial"
plugins = ["official-blocks"]
memory = "512m"                    # container memory limit (optional)

[sites.config.commerce]            # settings layered onto this site's aegis.toml (optional)
enabled = true
```

| Key | Description |
| --- | --- |
| `[deploy].acme_email` | Contact registered with Let's Encrypt. Notifications arrive here, for example when a certificate renewal fails |
| `[deploy].acme_staging` | `true` uses Let's Encrypt's staging environment (for trials) |
| `[deploy].project` | Docker Compose project name (default `aegiscms`). The prefix for container and volume names. Change it when running a different setup on the same server |
| `[deploy].caddy_image` | The Caddy image (default `caddy:2.10-alpine`) |
| `name` | Starts with a lowercase letter, then lowercase letters, digits, and `-` (up to 40 characters). The data is in the volume `<project>_<name>-data` |
| `domains` | Lowercase host names. `https://`, ports, paths, wildcards, and IP addresses cannot be written. The same domain cannot be written for multiple sites |
| `site_title` / `site_description` / `theme` / `plugins` | That site's `[theme]` and `[plugins].enabled` |
| `memory` | Container memory limit (`512m`, `1g`, etc.) |
| `public_health` | `true` exposes the health checks (`/health`, `/api/v1/health`) to the internet (default `false`. [5. Operations](#5-operations)) |
| `[sites.config.*]` | Arbitrary settings layered onto that site's `aegis.toml` ([Configuration reference](../docs/configuration.md)). `[sites.config.xxx]` attaches to the preceding `[[sites]]` |

Mistakes (unknown keys, invalid domains, duplicates, themes or plugins that do not exist, wrong setting types) cause an error at generation time, and the running sites keep running as they are.

### Generated Files

`./deploy.sh up` (or `./deploy.sh generate`) creates the following files with `aegis-server deploy generate` inside the AegisCMS image. Do not edit them by hand; fix `sites.toml` and regenerate (none of them are tracked by Git).

| File | Description |
| --- | --- |
| `compose.yaml` | Caddy and each site's container, network, and volume |
| `Caddyfile` | Per-domain routing, redirects, HSTS, compression, and blocking of internal paths |
| `sites/<name>/aegis.toml` | The site's settings ([`config/docker.toml`](../config/docker.toml) with the values from `sites.toml` layered on top; `[audit].trust_forwarded_for = true`) |

When you change the settings and run `./deploy.sh up`, only the containers of sites whose settings changed are recreated, and Caddy reloads its config without stopping.

## 3. Secret Values (SMTP Password, Payment Keys, etc.)

Do not write them in `sites.toml`; write them as environment variables in `secrets/<site name>.env`. If the file exists, it is passed to that site's container (not tracked by Git).

```bash
# secrets/shop.env
AEGIS_SMTP_PASSWORD=...
AEGIS_STRIPE_SECRET_KEY=sk_live_...
AEGIS_STRIPE_WEBHOOK_SECRET=whsec_...
```

## 4. Your Own Themes and Plugins

Place them in `themes/<theme ID>/` and `plugins/<plugin name>/`, and they are mounted read-only into every site's container. Use them by writing the names in `theme` / `plugins`, or by switching in Admin Studio.

## 5. Operations

| What you want to do | Command |
| --- | --- |
| Check the status | `./deploy.sh ps` |
| View logs | `./deploy.sh logs` (to narrow to a site, `./deploy.sh logs main`) |
| Add a site or change settings | Fix `sites.toml` and run `./deploy.sh up` |
| Pull the latest image | `./deploy.sh pull` |
| Update AegisCMS | `./deploy.sh pull && ./deploy.sh up` (if you build from source yourself, `git pull && ./deploy.sh build && ./deploy.sh up`) |
| Restart | `./deploy.sh restart main` |
| Stop everything | `./deploy.sh down` (the data volumes remain) |

- **Removing a site**: if you delete it from `sites.toml` and run `./deploy.sh up`, the container is stopped and removed, but the data volume (`<project>_<name>-data`) remains. To delete it completely, use `docker volume rm aegiscms_<name>-data`.
- **Backups**: a site's data is all in its volume. Example: `docker run --rm -v aegiscms_main-data:/data -v "$PWD:/backup" alpine tar czf /backup/main-$(date +%F).tar.gz -C /data .`. To reliably capture data that is being written, first stop that site with `docker compose -f compose.yaml stop main`, and bring it back with `docker compose -f compose.yaml start main` after taking the backup.
- **Feed and sitemap URLs**: Caddy adds `X-Forwarded-Proto: https`, so they become `https://<domain>/...`. Setting the canonical URL in Admin Studio's **Settings › SEO** makes it more reliable.
- **Internal paths (`/metrics`, `/health`)**: Caddy returns 404 for `/metrics`, `/health`, and `/api/v1/health` (and under `[server].base_path` if set), so they are not visible from the internet (to make it harder to identify which CMS is in use; [Configuration reference 4.6](../docs/configuration.md#46-making-it-harder-to-identify-the-cms-in-use)).
  - Prometheus reads `http://<name>:8080/metrics` from inside Docker's network, not through Caddy (join the Prometheus container to the site's network `<project>_site-<name>`. `/metrics` responds only when the `prometheus` output is enabled in `[sites.config.audit]`).
  - Container health checks read `127.0.0.1:8080` from inside the container, so they are unaffected. To use the health check from an external uptime monitoring service, write `public_health = true` for that site (the product name and version are returned only to an `admin` session).
- **Upload limit**: no limit is set on the Caddy side. It is determined by AegisCMS's `[storage].max_upload_mb` (default 10MB; change it in `[sites.config.storage]`).
