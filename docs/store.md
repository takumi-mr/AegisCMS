[← Back to the documentation index](README.md)

**English** | [日本語](store.ja.md)

# Package store (design, not implemented)

> [!NOTE]
> This is a design note. Implemented so far: fetching and verifying catalogs, the listing API (`GET /api/v1/store/packages`) and installing from the store (`POST /api/v1/store/install`); settings and APIs in [configuration.md 2.28](configuration.md#228-store--package-store). and the Admin Studio screen (a **Store** tab on the Themes and Plugins screens, [admin-settings.md](admin-settings.md#store-on-the-themes-and-plugins-screens)). Once the open questions are settled, this will be rewritten as a specification.

Lets admins pick published plugins and themes in Admin Studio and install or update them. The distribution side is **static files only** (a signed catalog and ZIP files); there is no dedicated HTTP server. AegisCMS reads the catalog, downloads a ZIP, and hands it to the existing ZIP installation ([installing packages and signatures](plugin_hooks.md#74-installing-from-a-zip-package-and-signatures)).

## 1. Scope

- **Stage 1 (for now)**: only our own (official) plugins and themes. No review process, no author accounts, no paid items.
- **Stage 2 (when needed)**: accept third-party plugins ([6](#6-later-accepting-third-party-packages)). Only then consider a dynamic server.

So that stage 2 does not force a redesign, the catalog format and the CMS settings meet these from stage 1:

- The catalog carries a format version (`schema_version`)
- Every entry has a `publisher` field (only ourselves for now)
- AegisCMS assumes nothing about GitHub (it reads only the catalog URL and the ZIP URLs written in the catalog)
- The settings accept more than one catalog URL

## 2. Layout

Packages are distributed from a repository separate from the main one (for example `aegis-store`). The main repository's Releases are managed by release-please (tags and CHANGELOG), so plugin tags stay out of it.

```text
 aegis-store repository
   ├─ GitHub Pages:    https://<owner>.github.io/aegis-store/catalog.json
   │                   https://<owner>.github.io/aegis-store/catalog.json.sig
   └─ GitHub Releases: https://github.com/<owner>/aegis-store/releases/download/<tag>/<file>.zip
                         (downloads are redirected with 302 to release-assets.githubusercontent.com)

 AegisCMS (each site)
   fetch catalog.json → verify signature and rollback → show the list
   → fetch the ZIP → check sha256 → existing ZIP install (extraction limits, min_aegis_version, signature.json)
```

- **The catalog lives on Pages**: it keeps changing, so it is not a Releases asset (awkward to replace) or served from `raw.githubusercontent.com` (cached for minutes).
- **No GitHub API**: unauthenticated API calls are limited to 60 per hour, which many sites would hit. The list comes from the catalog, and ZIPs come from the direct URLs in it.
- **One tag name per package**: `<id>-v<version>` (for example `seo-tools-v1.2.0`, `theme-minimal-v0.3.1`).
- **Released assets are never replaced**: a fix is a new version, so the catalog's sha256 never disagrees with the file.

## 3. Catalog format

```json
{
  "schema_version": 1,
  "sequence": 42,
  "generated_at": "2026-11-28T00:00:00Z",
  "expires_at": "2027-02-28T00:00:00Z",
  "packages": [
    {
      "id": "seo-tools",
      "kind": "plugin",
      "name": "SEO Tools",
      "description": "…",
      "publisher": "official",
      "versions": [
        {
          "version": "1.2.0",
          "min_aegis_version": "0.9.0",
          "capabilities": { "local_scripts": false },
          "url": "https://github.com/<owner>/aegis-store/releases/download/seo-tools-v1.2.0/seo-tools-1.2.0.zip",
          "sha256": "<64 hex characters>",
          "size": 123456,
          "public_key": "<Ed25519 public key of the ZIP's signature.json, 32 bytes hex>"
        }
      ]
    }
  ]
}
```

- `catalog.json.sig` is an Ed25519 signature (64 bytes hex) over the exact bytes of `catalog.json`. The catalog key and the ZIP key are different keys ([5](#5-key-management)).
- `sequence` increases with every publish. AegisCMS remembers the last value it accepted and rejects a catalog with a smaller one (against rollbacks that present an old, vulnerable version as the latest).
- A catalog past `expires_at` is rejected (against freezing the site on an old list). CI regenerates the catalog regularly to extend it.
- `capabilities` is copied from the manifest so the confirmation screen can show it before installing. Installation fails if it disagrees with the manifest in the ZIP.
- A catalog with an unknown `schema_version` is not read. New fields are added under the same `schema_version`, and AegisCMS ignores fields it does not know.

## 4. On the AegisCMS side

### 4.1 Settings

```toml
[store]
enabled = true

[[store.catalogs]]
name = "official"
url = "https://<owner>.github.io/aegis-store/catalog.json"
public_key = "<Ed25519 public key for the catalog signature, 32 bytes hex>"
```

- Catalogs are an array; each key is used for its own catalog only.
- Without `[[store.catalogs]]`, the official catalog (AegisCMS-Store; URL and public key are `OFFICIAL_CATALOG_*` in `store.rs`) is read. Writing some replaces it. `enabled` defaults to `true`; the catalog is fetched only when an administrator opens the Store tab or calls the API (no requests at start-up or in the background).
- The official catalog key is built into the server, so after rotating it only servers updated to a version with the new key can read the catalog. Keep a copy of the catalog private key outside the GitHub secret.
- Accepted sequences are recorded in the DB (`store.catalog_sequences` in `_aegis_settings`), shared by all servers and never lowered. Restoring a backup brings the record back to its value at that time; that only loosens the check until the next accepted catalog.
- The key in the ZIP's `signature.json` must match the entry's `public_key`. `require_signatures` and `trusted_public_keys` under `[plugins]` / `[theme]` still apply; going through the store never relaxes them.

### 4.2 Flow

1. Fetch the catalog and `.sig`, then check the signature, `schema_version`, `sequence` and `expires_at`.
2. Show the list in Admin Studio. For installed packages, compare versions and offer updates. Incompatible ones (`min_aegis_version`) cannot be selected.
3. Before installing, show `capabilities` and ask for confirmation.
4. Fetch the ZIP and check `size` and `sha256`.
5. Hand it to the existing steps (`extract_zip_safely` → `check_min_aegis_version` → `verify_package_signature`). Installing does not enable it (same as today). The audit log records the catalog and the URL it came from.

### 4.3 Limits on outgoing requests

- `https://` only. Few redirects, and only to `https://`. Correctness comes from sha256 and signatures, so the host that served the bytes is not trusted for anything.
- Size limits (separately for the catalog and ZIPs) and timeouts.
- The URL is written by an admin in the configuration file, so there is little room for SSRF, but these limits still apply.

## 5. Key management

- Private keys never go into a repository. When CI signs, the key lives in an Actions secret.
- The **catalog key** and the **ZIP key** are separate. A leaked ZIP key cannot ship a ZIP that is not in the catalog; a leaked catalog key cannot forge ZIP signatures.
- To rotate a key, first make AegisCMS accept both old and new keys (`store.catalogs[].public_key`), then switch (open question, [8](#8-open-questions)).

Publishing (CI example): push a tag → build → sign the ZIP → create the Release → regenerate and sign `catalog.json` → deploy to Pages. Pushing the tag is the only manual step. Download counts are available from the Releases assets.

## 6. Later: accepting third-party packages

Authors submit, the store checks, then signs and publishes. But what authors submit is **source, not a ZIP (a built artifact)**.

**This does not require open source.** The source is handed over for review and building, not for publication.

- The source can stay private (give only the store's CI read access to the author's private repository, or send an archive through a private submission channel). The store never redistributes it.
- Users receive only the built ZIP (Wasm, templates and so on), never the source.
- The author chooses the license (proprietary or paid is fine). Catalog entries get a `license` field (an SPDX identifier or `proprietary`) shown in the list.

The details of the source submission flow:

- **Take the source and build it in the store's CI**: Wasm is a binary, so a submitted ZIP cannot be checked against its source. With the source (repository and commit) built by the store on a pinned toolchain, the reviewed source and the shipped binary match, and review only needs to read source diffs.
- **Two signatures**: the author's key ("the author released this") and the store's key ("the store reviewed this"). The catalog entry carries the author's public key (the `publisher` and `public_key` fields that exist from stage 1).
- **What review looks at**: `capabilities` (especially `local_scripts` and external hosts), manifest changes, and the diff from the previous version.
- **Withdrawal**: remove a vulnerable version from the catalog or mark it `yanked`; AegisCMS warns about installed copies.
- Only then is a dynamic server needed for author accounts, submissions and review state (or a process that uses GitHub pull requests as the submission point). Paid items are considered at this stage too.

### 6.1 Two tiers: reviewed and binary-only (draft)

For authors who do not want to hand source to review at all, a tier that accepts only a ZIP:

| Tier        | Submitted               | Signatures             | Permissions allowed                                                                                                  |
| ----------- | ----------------------- | ---------------------- | -------------------------------------------------------------------------------------------------------------------- |
| Reviewed    | Source (may be private) | Author key + store key | Unrestricted (`local_scripts`, external hosts and so on, if review approves)                                         |
| Binary-only | ZIP                     | Author key only        | Weak permissions only (anything declaring `local_scripts`, external hosts or other strong permissions is not listed) |

- **Only reviewed packages get the store's signature.** The store never signs what it has not checked. For binary-only packages it checks only that the author's key signed them and that the declared permissions are weak.
- **The Wasm sandbox is what makes binary-only acceptable.** Undeclared permissions are stopped at run time, so with narrow permissions the damage is bounded without reading the source. Conversely, anything with strong permissions is never listed without reading its source.
- Catalog entries carry the tier (`review`: `"reviewed"` / `"unreviewed"`), shown in the Admin Studio list and on the confirmation screen. Operators can refuse the binary-only tier in settings (for example `[store] allow_unreviewed = false`).

## 7. Moving official packages to another repository (plan)

The goal is that **every official plugin is distributed through the store and the core ships none**. Plugins live in [AegisCMS-Plugin](https://github.com/takumi-mr/AegisCMS-Plugin) with their own tests, and AegisCMS-Store builds them from that repository's tags (`sources` in `store.json`). The core's tests stop using official plugins and use small test plugins instead.

| Group              | Packages                                                                                                                                                                                                                                          | What it needs                                                                                                                                                            |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Moved (v0.13.0)    | `official-ads`, `official-clarity`, `official-embeds`, `official-link-embed`, `official-charts`, `official-share-buttons`, `official-reading-time`, `official-back-to-top`, `official-code-lines`, `official-lightbox`, `official-external-links` | Done                                                                                                                                                                     |
| Used by core tests | `official-blocks`, `official-page-toc`, `official-code-enhancer`, `official-shop-rules`, `official-order-notifier`                                                                                                                                | The server tests (plugin switching, commerce, notifications), the E2E tests (`plugins.spec.ts`, `csp.spec.ts`) and the pentest load them. Replace them with test plugins |
| Enabled by default | `official-code-enhancer`, `official-mermaid`, `official-plantuml`                                                                                                                                                                                 | `config/default.toml` and the bench configurations enable them, and the server does not start when an enabled plugin is missing                                          |
| Read by the core   | `official-post-navigation`                                                                                                                                                                                                                        | The core picks the previous and next articles from this plugin's settings (`mode`, `same_category`, `fallback_to_all`, `order_by`; `handlers/public.rs`, `related.rs`)   |
| Themes             | themes other than `default`                                                                                                                                                                                                                       | Theme strings live in the core's `aegis-theme/src/messages.rs`; themes need a way to carry their own                                                                     |

Order (1 to 7 are done):

1. Let the store build from several sources (`sources` in AegisCMS-Store's `store.json`).
2. Build the store install API (`POST /api/v1/store/install`) and the Store tab in Admin Studio.
3. Move the plugins the core did not need to AegisCMS-Plugin, with their tests (taken from core v0.12.0).
4. Remove them from the core, released as `feat!:` in v0.13.0 with migration steps ([upgrade.md](upgrade.md)).
5. **Test plugins** (done: [`test-plugins/`](../test-plugins/README.md)). Add small plugins built with the Rust SDK for the core's tests only (not shipped, not in the store). They cover what the tests need: a page hook that adds an inline script and a CSP host, blocks with their own style, order calculation, a notification that sends a request, and declared settings. Switch the server tests, the E2E tests and the pentest to them. The tests of the real plugins (`aegis-runtime/tests/sdk_plugins.rs`) move with the plugins.
6. **Previous and next articles become a core setting** (done: [`[adjacent_articles]`](configuration.md#229-adjacent_articles--previous-and-next-articles)). Move the four settings into a core section, editable in Admin Studio. For a while, when the new section is not set, the core still reads `[plugins.settings.official-post-navigation]` and logs a warning. The plugin then only draws the links.
7. **Nothing is enabled by default** (done: `config/default.toml`, and `recommended` in AegisCMS-Store's `store.json`). Stop enabling code highlighting, Mermaid and PlantUML in the shipped configurations. The Store tab shows a short list of recommended packages (marked in the catalog), each installed with one click. No request is made at start-up.
8. Move the remaining plugins to AegisCMS-Plugin with their tests, and switch their source in `store.json`.
9. Remove them from the core as `feat!:`, with migration steps. The Docker image and the release archives then ship no plugins; `bundled_plugins_dir` stays for anyone who builds their own image.
10. Move themes once they can carry their own strings.

## 8. Open questions

- How often to check for updates, and where to cache the fetched catalog. Today the catalogs are fetched each time the Store tab opens, and updates are shown only there (not yet on the Installed tab or elsewhere).
- How to rotate keys (whether the catalog should announce the next key).
