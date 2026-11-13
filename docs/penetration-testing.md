[← Back to the documentation index](README.md)

**English** | [日本語](penetration-testing.ja.md)

# Local Penetration Testing Environment

This procedure is only for a local environment you own and control, targeting AegisCMS. Do not point scans at external hosts or production environments.

## Layout

- Defender: the AegisCMS container started by `docker-compose.pentest.yml`
- Attacker: a WSL distribution such as Kali Linux
- Target: `http://127.0.0.1:18080`
- Settings: `docker/pentest/pentest.toml` (layered on `config/docker.toml`). It enables members, shop, forum, contact, newsletter, comments, analytics, and six official plugins so that ZAP can reach them (no outbound traffic)
- The ZAP plans (`zap-active.yaml` / `zap-authenticated.yaml`) use a `requestor` job to send GETs to the seed URLs of public pages and the API first, and then crawl. When you add a route, add a seed too (it is not scanned if it stays a 404). The `Authorization` of the authenticated plan is replaced with a session ID obtained anew on each run (the value in the repository is invalid)
- DB and upload area: a 512 MiB tmpfs. Erased on stop
- Docker network: AegisCMS is on an internal network only. The nginx ingress that publishes a loopback port connects to both the internal network and a standard bridge, and the target itself is given no outbound route
- Containers: non-root, read-only root filesystem, all Linux capabilities dropped, with CPU/memory/PID limits

## Starting

Run from the project root in Windows PowerShell:

```powershell
docker compose -f docker-compose.pentest.yml up --build -d --wait
curl.exe -i http://127.0.0.1:18080/api/v1/health
```

Check connectivity from Kali on WSL:

```bash
curl -i http://127.0.0.1:18080/api/v1/health
```

In environments where WSL cannot connect to `127.0.0.1`, enable WSL's localhost forwarding. Do not widen the port exposure to the LAN.

## Initial Checks

From the Kali side, work with the target fixed to the URL above. First do light enumeration and save the output under a place such as `artifacts/`.

```bash
nmap -Pn -sT -sV -p 18080 --script http-title,http-headers 127.0.0.1
whatweb http://127.0.0.1:18080
```

Then perform an OWASP ZAP passive scan, manual authentication and authorization tests, and input validation tests. Limit active scans and load tests to this disposable environment only, and set the concurrency and request rate low before starting. Record authentication lockout, input size limits, and access control, and for each finding record the reproduction steps, expected result, actual result, and severity.

ZAP's Baseline Scan can be run with the target left on the internal network. It is spider and passive scan only.

```powershell
docker compose -f docker-compose.pentest.yml up -d --wait target
docker run --rm --network aegiscms_lab --memory 1g --cpus 2 --pids-limit 256 `
  ghcr.io/zaproxy/zaproxy:stable zap-baseline.py -t http://target:8080 -m 1
```

## Running an Active Scan

Active scans can also be run, limited to the disposable environment. Two profiles are provided: an unauthenticated scan and a logged-in authenticated scan.

### 1. Unauthenticated Scan (Web + API)
The plan in the repository (`docker/pentest/zap-active.yaml`) targets public web pages and the main API endpoints (including the OpenAPI definition `docker/pentest/openapi.json`) and scans at Low intensity. The result HTML is saved to `docs/security-reports/zap-active-report.html`.

```powershell
docker compose -f docker-compose.pentest.yml up -d --wait target
New-Item -ItemType Directory -Force -Path docs/security-reports | Out-Null
docker run --rm --network aegiscms_lab --memory 1g --cpus 1 --pids-limit 256 `
  -v "${PWD}/docker/pentest:/zap/wrk:ro" `
  -v "${PWD}/docs/security-reports:/zap/out:rw" `
  ghcr.io/zaproxy/zaproxy:stable zap.sh -cmd -autorun /zap/wrk/zap-active.yaml
docker compose -f docker-compose.pentest.yml down
```

### 2. Logged-In Authenticated Scan (Medium Intensity)
A separate profile (`docker/pentest/zap-authenticated.yaml`) injects an administrator session (Bearer token / X-Session-Id) into every request with the ZAP Replacer, and runs an active scan at Medium intensity against protected APIs and Admin Studio. The result is saved to `docs/security-reports/zap-authenticated-report.html`.

```powershell
# 1. Start the target and the ingress
docker compose -f docker-compose.pentest.yml up -d --wait target
docker compose -f docker-compose.pentest.yml up -d ingress

# 2. Do the initial setup and obtain an administrator session
Invoke-RestMethod -Uri "http://127.0.0.1:18080/api/v1/setup" -Method Post -ContentType "application/json" -Body '{"admin_username":"admin","admin_password":"AdminPassword123!"}'
$login = Invoke-RestMethod -Uri "http://127.0.0.1:18080/api/v1/auth/session" -Method Post -ContentType "application/json" -Body '{"username":"admin","password":"AdminPassword123!"}'

# 3. Run the authenticated scan
New-Item -ItemType Directory -Force -Path docs/security-reports | Out-Null
docker run --rm --network aegiscms_lab --memory 1g --cpus 1 --pids-limit 256 `
  -v "${PWD}/docker/pentest:/zap/wrk:ro" `
  -v "${PWD}/docs/security-reports:/zap/out:rw" `
  ghcr.io/zaproxy/zaproxy:stable zap.sh -cmd -autorun /zap/wrk/zap-authenticated.yaml

# 4. Tear down
docker compose -f docker-compose.pentest.yml down
```

Key items to check:

- Unauthenticated APIs and public pages do not contain unintended admin data
- Permission boundaries between `viewer` / `editor` / `admin`, IDOR, and access to drafts, history, and previews
- Per-IP/per-user rate limiting on login failures and session invalidation
- Input handling of HTML blocks, uploads, file names, redirects, and Host / forwarded headers
- CSRF/CORS, security headers, and information exposure in error responses and OpenAPI/metrics
- Plugin/theme signature settings and admin-only operations

## Stopping and Discarding

```powershell
docker compose -f docker-compose.pentest.yml down
```

The data is on tmpfs, so it is discarded when the container is removed. Do not add separate volumes.
