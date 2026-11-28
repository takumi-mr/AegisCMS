[← Back to the documentation index](README.md)

**English** | [日本語](store.ja.md)

# Package store

> [!NOTE]
> How the package store works. Settings and APIs: [configuration.md 2.28](configuration.md#228-store--package-store); the Admin Studio screens: [admin-settings.md](admin-settings.md#store-on-the-themes-and-plugins-screens). [6](#6-later-accepting-third-party-packages) is a design for later (third-party packages), not implemented.

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

Packages are distributed from [AegisCMS-Store](https://github.com/takumi-mr/AegisCMS-Store), a repository separate from the core. The core's Releases are managed by release-please (tags and CHANGELOG), so package tags stay out of it. AegisCMS-Store builds the packages from the tags of its sources (`sources` in its `store.json`): the official plugins and themes from [AegisCMS-Plugin](https://github.com/takumi-mr/AegisCMS-Plugin), and the themes that stay in the core from its release tag ([7](#7-where-the-official-packages-live)).

```text
 AegisCMS-Store repository
   ├─ GitHub Pages:    https://takumi-mr.github.io/AegisCMS-Store/catalog.json
   │                   https://takumi-mr.github.io/AegisCMS-Store/catalog.json.sig
   └─ GitHub Releases: https://github.com/takumi-mr/AegisCMS-Store/releases/download/<tag>/<file>.zip
                         (downloads are redirected with 302 to release-assets.githubusercontent.com)

 AegisCMS (each site)
   fetch catalog.json → verify signature and rollback → show the list
   → fetch the ZIP → check sha256 → existing ZIP install (extraction limits, min_aegis_version, signature.json)
```

- **The catalog lives on Pages**: it keeps changing, so it is not a Releases asset (awkward to replace) or served from `raw.githubusercontent.com` (cached for minutes).
- **No GitHub API**: unauthenticated API calls are limited to 60 per hour, which many sites would hit. The list comes from the catalog, and ZIPs come from the direct URLs in it.
- **One tag name per package**: `<id>-v<version>` (for example `official-blocks-v0.5.0`, `roastery-v1.1.0`).
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
- Without `[[store.catalogs]]`, the official catalog (AegisCMS-Store; URL and public key are `OFFICIAL_CATALOG_*` in `store.rs`) is read. Writing some replaces it. `enabled` defaults to `true`; the catalog is fetched only when an administrator opens the Store tab, when the update check is due ([8](#8-update-notices)), or through the API (no requests at start-up, in the background, or for visitors).
- The official catalog key is built into the server, so after rotating it only servers updated to a version with the new key can read the catalog. Keep a copy of the catalog private key outside the GitHub secret.
- Accepted sequences are recorded in the DB (`store.catalog_sequences` in `_aegis_settings`), shared by all servers and never lowered. Restoring a backup brings the record back to its value at that time; that only loosens the check until the next accepted catalog.
- The key in the ZIP's `signature.json` must match the entry's `public_key`. `require_signatures` and `trusted_public_keys` under `[plugins]` / `[theme]` still apply; going through the store never relaxes them.

### 4.2 Flow

1. Fetch the catalog and `.sig`, then check the signature, `schema_version`, `sequence` and `expires_at`.
2. Show the list in Admin Studio. For installed packages from the same source, compare versions and offer updates ([8](#8-update-notices)). Incompatible ones (`min_aegis_version`) cannot be selected.
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
- How to rotate the catalog key without cutting off released servers is still open ([9](#9-open-questions)).

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

## 7. Where the official packages live

Since v0.13.0 the core ships no official plugin and only two themes. Upgrading from earlier versions: [upgrade.md](upgrade.md).

| What                                                                                             | Where                                                                                        | Why                                                                                                  |
| ------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| All official plugins (`official-*`)                                                              | [AegisCMS-Plugin](https://github.com/takumi-mr/AegisCMS-Plugin) `plugins/`, with their tests | Installed from the store. Releasing a change: that repository's README                               |
| Themes `app-shell`, `cyber-neon`, `emerald-luxury`, `minimal-editorial`, `roastery`, `trailhead` | AegisCMS-Plugin `themes/`, with their text in `locales/`                                     | Installed from the store                                                                             |
| Themes `default`, `wordpress-classic`                                                            | The core (`themes/`), also in the store                                                      | The theme engine falls back to `default`; the WordPress theme import starts from `wordpress-classic` |

- **Nothing is enabled by default.** The shipped configurations enable no plugin. The catalog marks recommended packages (`recommended` in AegisCMS-Store's `store.json`: code highlighting, Mermaid, PlantUML, content blocks), which the Store tab lists first until they are installed.
- **The core's tests use their own packages**: [`test-plugins/`](../test-plugins/README.md) and [`test-themes/`](../test-themes/README.md), never shipped or in the store. They cover what the core needs from plugins and themes (page hooks with inline scripts and CSP hosts, blocks with their own style, order calculation, notifications, declared settings, a theme declaring CSP hosts). The tests of the official packages themselves run in AegisCMS-Plugin against a core release tag.
- **The core does not depend on any official package.** Previous and next articles are a core setting ([`[adjacent_articles]`](configuration.md#229-adjacent_articles--previous-and-next-articles)), not `official-post-navigation`'s.
- **A theme carries its own text** (`locales/`, [theme authoring guide 2](theme_authoring_guide.md#2-directory-layout)), so it can be distributed apart from the core.

## 8. Update notices

Decided so that notices add no new kind of outgoing traffic and no new trust:

- **When**: only when an administrator opens Admin Studio, at most once per `update_check_hours` (default 24), plus "Check now". Never at start-up, in the background, or for visitors. `[store].check_updates = false` turns the automatic check off.
- **Cache**: the verified catalog (body and signature) is kept in the cache backend (shared between servers with Redis), and verified again (signature, expiry, rollback) every time it is read. The settings table is not used: a catalog can be up to 1 MiB, more than MySQL's `TEXT`.
- **What is trusted**: only verified catalogs. When a check fails, the notice says so with the time of the last verified result instead of "up to date". The installed list never leaves the server: the whole catalog is fetched and compared locally.
- **Same source only**: an update is offered only when the installed package is signed with a key the catalog lists for that package (`installed_from_catalog`). A package with the same ID from elsewhere (a fork, an upload) is never replaced by the store's version. Versions that do not run on this server are not offered.
- **No automatic updates**: an update goes through the same confirmation as an install, which highlights what the new version is allowed to do that the installed one was not (new hooks, hosts, scripts).

Later: a "security update" mark in the catalog, and keeping the previous version to roll back.

## 9. Open questions

- **Rotating the catalog key.** The official catalog's public key is built into the server (`OFFICIAL_CATALOG_PUBLIC_KEY`), so a catalog signed with a new key is refused by every released server. Today a rotation means releasing a core with the new key; until a site updates, it can write the new key in `[[store.catalogs]]` (AegisCMS-Store's README). One way to avoid the gap: let the catalog announce the next key, signed with the current one, so servers learn it before the switch.
- **Rotating the ZIP key.** The catalog lists each version's key, so new versions can simply be signed with a new key. But updates are offered only when the installed package's key appears among the catalog's versions of that package ([8](#8-update-notices)): as long as the old versions stay listed, updates keep showing; dropping them would hide updates from sites still on an old-key version. Decide together with the catalog key.
