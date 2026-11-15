[← Back to the documentation index](README.md)

**English** | [日本語](publish.ja.md)

# Publishing Your Site on the Internet

AegisCMS serves plain HTTP. To publish it, put an HTTPS front end (a reverse proxy) in front of it. There are two ways.

| You want to                                                                                     | Use                                                 |
| ----------------------------------------------------------------------------------------------- | --------------------------------------------------- |
| Get HTTPS on a new Linux server with Docker, with certificates handled for you (multiple sites) | [Caddy setup (`deploy/`)](#1-caddy-setup-deploy)    |
| Use an existing nginx or other proxy, or publish under a subpath such as `/cms/`                | [Your own reverse proxy](#2-your-own-reverse-proxy) |

## 1. Caddy Setup (`deploy/`)

You need a Linux server with Docker and Docker Compose v2, ports 80 and 443 open, and a DNS record for each domain that points to the server.

```bash
git clone https://github.com/takumi-mr/aegiscms
cd aegiscms/deploy
cp sites.example.toml sites.toml
vi sites.toml          # your domains, site names, and theme
./deploy.sh up         # pull the image, generate the config, and start
./deploy.sh user main  # create the administrator of site "main"
```

`https://<domain>/` is the site and `https://<domain>/admin/` is Admin Studio. Caddy obtains and renews the certificates by itself. One site runs in one container, so you can add more sites by adding a `[[sites]]` block and running `./deploy.sh up` again.

Try it with `acme_staging = true` first so you do not hit Let's Encrypt's rate limits. Everything else (secrets, your own themes, updating, removing a site) is in [deploy/README.md](../deploy/README.md).

## 2. Your Own Reverse Proxy

Point your proxy at AegisCMS (`127.0.0.1:8080`) and follow these rules:

- Send `Host`, `X-Forwarded-For` (overwrite it, do not append) and `X-Forwarded-Proto`.
- Set `[audit].trust_forwarded_for = true` so logs and login limits see the visitor's IP. Do this **only** when AegisCMS cannot be reached except through the proxy.
- Allow uploads at least as large as `[storage].max_upload_mb` (nginx: `client_max_body_size`).
- Do not expose `/metrics`, `/health` and `/api/v1/health` to the internet.

Complete nginx examples (subdomain and subpath) and the other settings to check, such as single sign-on URLs and the canonical URL, are in [Configuration reference, section 4](configuration.md#4-operating-behind-a-reverse-proxy).

## After Publishing

- Set the canonical URL in Admin Studio under **Settings › SEO** so feeds and the sitemap use `https://`.
- [Set up backups](backup-restore.md).
