[← Back to the documentation index](README.md)

**English** | [日本語](security-assessment-report.ja.md)

# AegisCMS Security Assessment Report

## Assessment Summary

| Item | Description |
| --- | --- |
| Status | Dynamic testing and active API testing performed. Verification of the F-04 fix (Critical) on real servers (both Windows and Linux containers) is complete. Dynamic penetration testing of Web/REST API endpoints (all 225 groups and the OpenAPI specification) by unauthenticated and **administrator-logged-in (Authenticated) profiles** is complete |
| Target | AegisCMS local verification environment (Windows native process and Docker Compose isolated stack) |
| Attacker | Windows native (PowerShell / curl) and the OWASP ZAP (2.17.0) container on the Docker internal network |
| Assumed target | AegisCMS in Docker (`127.0.0.1:18080`, internal name `target:8080`) and local development processes |
| Environment | Windows 11 / Docker Desktop (WSL2 engine) |
| Recorded | 2026-10-26 (first edition), 2026-11-02 (revised: Windows native testing, unauthenticated Web/API scan, logged-in Authenticated scan), 2026-11-09 (retest: unauthenticated and authenticated scans rerun after introducing Strict CSP) |

---

## 2026-11-09 Retest (After Strict CSP, With Seeds and Target Features Reviewed)

After making the public pages' CSP Strict (no `'unsafe-inline'` or `'unsafe-eval'` in `script-src` / `style-src`; inline scripts and `<style>` are allowed by body hash, and `style=""` attributes are not used) ([csp.md](csp.md)), we retested in a disposable Docker environment (OWASP ZAP stable, an unauthenticated profile and an authenticated profile with an injected administrator session).

**Review of test scope**: In the previous test (11-02), some of the seed URLs did not match the current routes (such as `/api/v1/roles`, `/backup/list`, and `/forum/topics`), and in addition the forum, shop, contact, newsletter, comments, members, and analytics were disabled in the Docker settings, so those screens and APIs were not reached. So we (1) aligned the seeds with the current routes at about 60 URLs and put them into the site tree beforehand with a `requestor` job, and (2) layered the pentest-only settings [docker/pentest/pentest.toml](../docker/pentest/pentest.toml) (enabling the above features and 6 official plugins with no outbound traffic) onto `docker-compose.pentest.yml`. Both profiles were run in an initially set-up state (with sample articles).

| Profile | URLs scanned | High | Medium | Low | Info | Previous (11-02) Medium |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Unauthenticated (Low intensity) | 77 | 0 | 2 | 0 | 2 | 6 |
| Authenticated (Medium intensity) | 79 | 0 | 2 | 1 | 2 | 6 |

- **Resolved**: warnings about CSP `unsafe-inline` and `unsafe-eval`, and SRI attribute warnings (4). `'unsafe-inline'` is gone from the CSP of public pages and the API, and Admin Studio remains `script-src 'self'` / `style-src 'self'`.
- **Breakdown of the 2 Mediums** (the CORS finding disappeared in the rerun after narrowing CORS ([#312](https://github.com/takumi-mr/AegisCMS/pull/312))):
  1. **CSP: Wildcard Directive** — `https:` in `img-src` and `font-src`. A design to allow external images and fonts in the body.
  2. **Absence of Anti-CSRF Tokens (newly detected)** — the public forms at `/contact` (`POST /contact/default`) and `/newsletter`. Reached for the first time because the test scope was widened. Both are **anonymous submission forms** that do not depend on a login session or cookies and have no privileges to hijack (at worst, unwanted submissions). Unwanted submissions are suppressed with a honeypot, a per-IP submission limit, and optional CAPTCHA. **The risk is low and currently accepted** (introducing tokens will be reconsidered when the forms start doing privileged operations).
- **1 Low (authenticated only)**: Private IP Disclosure (the access source IP in the audit log; P-05).
- **2 Info**: Modern Web Application (SPA), and User Controllable HTML Element Attribute (the input of `/search?q=` goes into `<input value>`; ZAP only raises awareness, and the value is HTML-escaped).
- **High, SQL injection, command injection, authentication bypass, privilege escalation: 0.**
- **Limits of this test**: **Logical and design aspects that ZAP has difficulty checking automatically, such as member-only viewing restrictions, preview frames, and plugin output, were not examined in this scan** (they are confirmed by unit, integration, and E2E tests). Some of the seeds (`/api/v1/public/*`, `/api/v1/auth/registration`, `/api/v1/members/me`, `/api/v1/orders/report`) do not return 200 for GET (404/405/401/400). The OpenAPI definition (`docker/pentest/openapi.json`) has only 5 paths and does not cover the whole API.

## Status and Latest Summary (Revised 2026-11-02)

In this assessment, the scope was greatly expanded from the previous testing limited to public HTML pages, and **penetration testing covering the REST API in general (unauthenticated, viewer, editor, admin), the initial setup API (Web installer), and the OpenAPI definition** was performed in an isolated Docker environment. In addition, **we created a separate profile (`zap-authenticated.yaml`) with an injected administrator authentication session (Bearer token) and ran an authenticated scan at Medium attack intensity**.

### Main Findings
1. **OWASP ZAP Active Scan including the API (unauthenticated profile)**:
   - Imported the specification of `docker/pentest/openapi.json` and added the main API endpoints as targets.
   - The crawl and scan targets increased greatly from the previous 13 URLs to **75 URLs**.
   - Active scan result: **High: 0**, **Medium: 6** (a CSP note, wildcard CORS, missing SRI attributes), **Low: 0**, **Informational: 1**.
   - No critical vulnerabilities such as SQL injection, OS command injection, or remote code execution.
2. **Administrator-logged-in OWASP ZAP Active Scan (authenticated profile, Medium intensity)**:
   - Created `docker/pentest/zap-authenticated.yaml` and injected `Authorization: Bearer <session_id>` and `X-Session-Id` into all requests with the ZAP Replacer.
   - Scanned **86 URLs** covering the admin screen (Admin Studio), schema definitions, content management, storage listing, user management, plugins, themes, audit logs, header settings, and more.
   - Raised the attack intensity to **Medium** (expanded payload types and encodings) and ran active attacks.
   - Active scan result: **High: 0**, **Medium: 6**, **Low: 1 (new)**, **Informational: 1**.
   - The new Low: a **Private IP Disclosure** because the response of `/api/v1/audit-logs` records a Docker-internal private IP (`172.20.0.3`) as the client access source IP.
3. **Verification of the initial setup API (Web installer)**:
   - Before initialization: `/api/v1/setup/status` returns `{"initialized":false,"requires_setup":true}`. `/setup` serves the installer screen.
   - Input validation: an empty username and a password under 8 characters are properly rejected with `400 Bad Request` (`VALIDATION_FAILED`).
   - Guard against repeated initialization: confirmed on a real server that after initialization completes, all re-initialization requests to `POST /api/v1/setup` are reliably blocked with `403 Forbidden` (`ALREADY_INITIALIZED`). Also confirmed that `/setup` becomes a 303 redirect to `/admin/`.
4. **Verification of RBAC (role-based access control) boundaries**:
   - Unauthenticated requests: all protected APIs (schemas, content, storage, users, roles, plugins, themes, audit logs, backups, etc.) reliably return `401 Unauthorized`.
   - OpenAPI specification (`/api/v1/openapi.json`): corrected the statement "public" in the past report (P-01). Confirmed that a `ViewerSession` is actually required and that it is protected with 401 when unauthenticated.
   - `viewer` role: only read APIs succeed (200). Content creation (403), user management (403), and audit logs (403) are all rejected.
   - `editor` role: content creation and update, and normal storage upload and deletion succeed. User management (403) and audit logs (403) are all rejected.
   - `admin` role: legitimate access to all administration endpoints, including user management and audit logs, was confirmed.
5. **Regression verification of the F-04 path traversal vulnerability on a Linux container environment**:
   - For the drive letter / path traversal vulnerability (F-04) found and fixed on Windows native, the same verification was done on Docker (Linux).
   - Attacks specifying `/etc/passwd`, `C:\Windows\System32`, and `../../../escaped.txt` in the `key` field of `POST /api/v1/storage/upload` were all blocked immediately with `400 Bad Request` (`{"error":"Invalid storage key"}`).
   - `DELETE /api/v1/storage/objects/..%2F..%2Fescaped.txt` was rejected with `400 Bad Request` likewise.
6. **Re-verification of authentication brute-force defense (login lockout)**:
   - After sending a wrong password 5 times to `POST /api/v1/auth/session`, `429 Too Many Requests` (`Retry-After: 900`) is returned immediately from the 6th attempt, and it was confirmed that even the correct password is refused until the lockout period ends.
7. **Browser-facing defense headers**:
   - Confirmed that the security headers introduced for F-03 (`Content-Security-Policy`, `X-Frame-Options`, `X-Content-Type-Options`, `Referrer-Policy`, `Permissions-Policy`, `Cross-Origin-Opener-Policy`) are added uniformly to both API responses and HTML responses.

---

## Test Results and Findings List

| ID | Item | Basis / possible impact | Status |
| --- | --- | --- | --- |
| F-01 | Stored XSS on public pages | The default lol_html sanitization and opaque-origin iframe isolation in sandbox mode | Fixed |
| F-02 | CORS allowed all origins, all methods, and all headers (addressed on 2026-11-09: only public reads are `*`; APIs requiring authentication and writes are limited to the origins in `[server].cors_origins`. ZAP's Cross-Domain Misconfiguration is resolved) | Confirmed `Access-Control-Allow-Origin: *` in the response to `Origin: https://attacker.invalid`, and `Access-Control-Allow-Methods: *` and `Access-Control-Allow-Headers: *` on OPTIONS. `Access-Control-Allow-Credentials` is not returned, so cookie authentication cannot be hijacked, but it counts as an over-permissive setting of a public API. **Severity: Low** | Fixed (2026-11-09) |
| F-03 | Missing browser-facing defense headers | Introduced middleware that adds the default defense headers (`CSP`, `X-Content-Type-Options: nosniff`, `X-Frame-Options: SAMEORIGIN`, `Referrer-Policy: strict-origin-when-cross-origin`, `Permissions-Policy`, `Cross-Origin-Opener-Policy: same-origin`) to responses of the public top page, admin screen, and API. The ZAP Baseline warnings (10020, 10021) are completely resolved. | Fixed |
| F-04 | Path traversal and arbitrary file operations in the storage API | Introduced the shared validation `validate_storage_key`, which rejects absolute paths and relative path traversal outside `upload_dir`. Re-verified on both Windows native and Linux Docker containers, confirming all attack payloads are blocked with 400. | Fixed |
| P-01 | Unauthenticated access boundary | All protected APIs (schemas, content, storage, users, roles, plugins, themes, audit logs, backups, etc.) return 401. Only the public health and setup status return 200. Confirmed that the OpenAPI specification (`/api/v1/openapi.json`) also requires a `viewer` session and is 401 when unauthenticated. | As expected |
| P-02 | Login failure lockout | Confirmed that 5 wrong password attempts against the same account immediately trigger a 429 lockout (`Retry-After: 900`). | As expected |
| P-03 | Web installer repeated-initialization prevention | After a successful administrator registration via `/api/v1/setup` when uninitialized, a further initialization call is blocked with `403 Forbidden` (`ALREADY_INITIALIZED`). Confirmed that `/setup` redirects to `/admin/`. | As expected |
| P-04 | RBAC privilege escalation prevention | A `viewer` calling content creation/update, user management, or audit logs gets 403. An `editor` calling user management or audit logs is also blocked with 403. | As expected |
| P-05 | Private IP exposure in the audit log | The audit records of `/api/v1/audit-logs` (admin only) include the login client IP (`172.20.0.3`), detected by ZAP as Private IP Disclosure (Low). It is a legitimate feature as an audit trail, but counts as exposing internal container network information. | By design, low risk |
| F-05 | CSP `unsafe-inline` and `unsafe-eval` (ZAP Medium warnings) | The CSP of public pages, the API, and Admin Studio contains neither `'unsafe-inline'` nor `'unsafe-eval'` (Strict CSP; inline by hash, no `style` attributes). Confirmed in the 2026-11-09 retest that this warning and the SRI warning disappeared. Only `https:` in `img-src` / `font-src` remains | Fixed |
| P-06 | No CSRF token on anonymous forms (ZAP 10202) | `/contact` and `/newsletter` are anonymous submission forms that do not depend on sessions or cookies. Abuse is suppressed by a honeypot, submission limits, and (optional) CAPTCHA. | Low risk, accepted |
| N-01 | nmap service check | Only `127.0.0.1:18080` is open, and detailed versions are not disclosed thanks to `server_tokens off` on the nginx ingress. | Done |
| N-02 | metrics endpoint | `/metrics` is 404 in the default settings (when the Prometheus sink is not enabled). No unnecessary metrics exposure. | Done |

---

## OWASP ZAP Active Scan Results

### 1. Unauthenticated Profile (Web + API)
Plan: [docker/pentest/zap-active.yaml](../docker/pentest/zap-active.yaml)
- **Target**: public pages, the OpenAPI definition, unauthenticated API endpoints
- **Crawl result**: 75 URLs
- **Active scan settings**: Attack Strength: Low, 1–2 threads, 50 ms delay
- **Result**: High: 0, Medium: 6, Low: 0, Informational: 1
- **Detailed report**: [zap-active-report.html](security-reports/zap-active-report.html)

### 2. Logged-In Authenticated Profile (Medium Intensity)
Plan: [docker/pentest/zap-authenticated.yaml](../docker/pentest/zap-authenticated.yaml)
- **Target**: Admin Studio, all protected APIs, and the OpenAPI specification through an administrator session (Bearer token injection)
- **Crawl result**: 86 URLs (confirmed access to all protected endpoints that were 401 when unauthenticated)
- **Active scan settings**: Attack Strength: **Medium** (expanded payloads), 30 ms delay, up to 6 minutes
- **Result**:

| Risk level | Alerts | Main content |
| --- | ---: | --- |
| **High** | **0** | No serious injection, authentication bypass, privilege escalation, or code execution detected |
| **Medium** | **6** | Relaxed CSP settings (`unsafe-inline`, `unsafe-eval`, `img/font https:`), wildcard CORS setting (F-02), missing SRI attributes |
| **Low** | **1** | **Private IP Disclosure** (`172.20.0.3` included in the access source record of `/api/v1/audit-logs`) |
| **Informational** | **1** | Modern Web Application (SPA notice) |
| **False Positives** | **0** | - |

- **Detailed report**: [zap-authenticated-report.html](security-reports/zap-authenticated-report.html)

---

## Recommendations and Ongoing Issues

1. **Recommendation to do the initial setup (Web installer) before going public**:
   - If an uninitialized instance is exposed on a public network, there is a risk that a third party runs `/setup` and creates an administrator account before the legitimate administrator initializes it.
   - In production, either initialize beforehand with environment variables (such as `AEGIS_ADMIN_PASSWORD`) or the CLI (`aegis-server init`), or restrict access to `/setup` and `/api/v1/setup` to the internal network with a reverse proxy until setup completes.
2. **CORS settings (related to F-02; done on 2026-11-09)**:
   - By default, only public reads are open to all origins, and APIs requiring authentication and writes are same-origin only. Admin Studio on another domain or headless clients are allowed with `[server].cors_origins` ([configuration.md 4.3.1](configuration.md)).
3. **Tightening CSP (done as of 2026-11-09)**:
   - Removing `unsafe-eval`, making Admin Studio's `script-src` / `style-src` `'self'`, a hash-based Strict CSP for public pages (`strict_csp = "on"` is the default), and removing `style=""` attributes are all complete. What remains is `https:` in `img-src` / `font-src` (a design that allows external images and fonts). If you do not use external images, you can narrow the CSP in "Settings › Headers".
4. **Policy for internal IPs in the audit log (related to P-05)**:
   - When operating behind a reverse proxy, together with the trust settings of headers such as X-Forwarded-For, it is recommended to take care that internal network addresses do not leak outside.
