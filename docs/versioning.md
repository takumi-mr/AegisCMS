# Versioning and Releases

**English** | [日本語](versioning.ja.md)

AegisCMS follows [Semantic Versioning 2.0.0](https://semver.org/). Raising versions and creating tags and the CHANGELOG are done automatically by [release-please](https://github.com/googleapis/release-please) from Conventional Commits.

## 1. What Gets a Version

| Target                                                                                                         | Version                    | Where it lives                                                          | Tag                          |
| -------------------------------------------------------------------------------------------------------------- | -------------------------- | ----------------------------------------------------------------------- | ---------------------------- |
| **Core**: all `aegis-*` crates, the Rust SDK (`aegis-plugin-sdk`), Admin Studio, Theme Builder, Plugin Builder | Together (the same number) | `[workspace.package]` in the root `Cargo.toml`, and each `package.json` | `v0.2.0`                     |
| **AssemblyScript SDK** `@takumi-mr/plugin-sdk`                                                                 | Individual                 | `sdk/assemblyscript/package.json`                                       | `sdk-assemblyscript-v0.2.0`  |
| **TinyGo SDK**                                                                                                 | Individual                 | The tag itself (Go module convention)                                   | `sdk/tinygo/v0.2.0`          |
| **Official plugins** `plugins/official-*`                                                                      | Individual per plugin      | `version` in `Cargo.toml` and `plugin.manifest.json`                    | `plugin-official-ads-v0.1.1` |

- The core crates share one number with `version.workspace = true`. Do not give them individual numbers.
- `sdk/templates/` and `sdk/examples/` are templates to copy from and are not released on their own (their version can stay `0.1.0`). However, the version requirement on `aegis-plugin-sdk` in the Rust template follows the core.
- Until 1.0, both feature additions (`feat`) and breaking changes raise the minor (`0.x`), and fixes (`fix`, `perf`) raise the patch.

## 2. Plugin Compatibility

Compatibility between a plugin and the core is declared with two items (`plugin.manifest.json`).

- **`min_aegis_version`**: the minimum core version it can run on (SemVer). If the core is older, the plugin is not loaded.
- **`abi_version`**: the **plugin ABI version** (an integer) the plugin assumes. `aegis_core::PLUGIN_ABI_VERSION` is raised only when an incompatible change is made to the host API or the hooks' input and output. If it exceeds the range the host supports (1 or more, up to `PLUGIN_ABI_VERSION`), the plugin is not loaded. Plugins that omit it are not checked.

The contents of the ABI are in [plugin_abi_specification.md](plugin_abi_specification.md). **ABI 2**: scripts in `on-render-page` can only be added through structured `scripts`, not HTML strings (the host strips `<script>` and `on*` attributes inside `extra_head`, `prepend_content`, and `append_content`). Plugins that use `scripts` write `abi_version: 2`. ABI 1 plugins can still be loaded, but their `<script>` no longer runs. For the background, see [csp.md](csp.md).

Additions that do not break compatibility (adding a new hook or host API) do not raise the ABI version. Plugins that use such an addition declare it by raising `min_aegis_version`.

The server returns `version` (the core) and `plugin_abi_version` in `GET /api/v1/health` for an administrator's session (not on the public side). You can check the core version in the tooltip shown on the gateway indicator in Admin Studio's header.

## 3. Commit Messages and How Versions Go Up

The Conventional Commits in [CLAUDE.md 4](../CLAUDE.md) are reflected in the version as they are.

| Commit                                     | Version raised                                    |
| ------------------------------------------ | ------------------------------------------------- |
| `fix:` / `perf:`                           | Patch                                             |
| `feat:`                                    | Minor (the same before 1.0)                       |
| `feat!:` or `BREAKING CHANGE:` in the body | Minor before 1.0, major from 1.0                  |
| `docs` / `style` / `test` / `chore`        | Not raised (and does not appear in the CHANGELOG) |

> [!WARNING]
> **Do not write raw HTML (especially `<script>` and `<style>`) in commit messages or in PR titles and bodies.** The release PR body is built from the commit messages, and release-please reads it as HTML. If parsing breaks, for example when there are two or more `<script>`, then even if you merge the release PR, **neither a tag nor a GitHub Release is created** and it stays `autorelease: pending`. Later release PRs also stop being created with "untagged, merged release PRs outstanding" (the workflow shows success, so it is hard to notice). Wrapping it in backticks does not prevent it. Write it in words, like "script element". It happens especially often in the explanation of a breaking change (`BREAKING CHANGE:`). For PRs, `scripts/check-release-notes-html.mjs` (`commit-lint.yml`) checks it.
>
> If you already merged a release PR and it stalled, fix the `<` in that release PR's body to `&amp;lt;` (`<script` → `&amp;lt;script`) and rerun the Release Please workflow on main (with `&lt;`, release-please reverts it when rereading the body, so it does not get fixed).

Which target a commit counts toward is decided by the **path of the changed files** (not the scope). Changes in `plugins/official-ads/` raise only that plugin, changes in `sdk/assemblyscript/` raise only that SDK, and everything else (outside `plugins/` and `sdk/`) raises the core.

## 4. The Release Flow

1. When changes are merged to `main`, the `Release Please` workflow creates or updates a release PR (version updates + each `CHANGELOG.md`).
2. Check the release PR's versions and CHANGELOG and merge it. The workflow makes `Cargo.lock` follow in the same PR.
3. Merging creates tags and GitHub Releases. In addition, based on the `paths_released` output of `release-please.yml`, the publishing workflows for the released target paths are triggered automatically:
    - **AssemblyScript SDK** (`sdk/assemblyscript`): the npm package is published automatically to GitHub Packages (`@takumi-mr/plugin-sdk`) (`release-sdk.yml`).
    - **Core** (`.`): the official Docker image is built and pushed automatically to the GitHub Container Registry (`ghcr.io/takumi-mr/aegiscms`) (`release-docker.yml`).
    - **TinyGo SDK** (`sdk/tinygo`): the created tag (`sdk/tinygo/vX.Y.Z`) itself is the distribution as a Go module.

So that CI also runs on release PRs (CI does not run on PRs created with `GITHUB_TOKEN`), `RELEASE_PLEASE_TOKEN` (a PAT) is registered in the repository secrets. A fine-grained PAT needs Read and write for **Contents** and **Pull requests** on the target repository (without them, creating the PR's branch fails). It also works with `GITHUB_TOKEN` if unregistered, but CI does not run on release PRs.

When the first release PR is created, all paths changed up to then are targets. Right after introducing the mechanism, all plugins and the TinyGo SDK go up at the same time for this reason (from next time, only the changed paths).

## 5. Consistency Checks

`task version:check` (`node scripts/check-versions.mjs`) checks the following. It also runs in `task check` and CI.

- `release-please-config.json`, `.release-please-manifest.json`, and `plugins/official-*` on disk agree (when you add a plugin, register it in both)
- The root `Cargo.toml`, each crate's `version.workspace = true`, the `package.json` / `package-lock.json` of the three frontends, and the SDK version requirements agree with the core version
- Each official plugin's `Cargo.toml` and `plugin.manifest.json` `version` agree
- Every `plugin.manifest.json` has an `abi_version` within the range the host handles. `min_aegis_version` is not newer than the core

You basically never raise versions by hand (leave it to release PRs). Even when fixing by hand, use this command to check for omissions.

## 6. Policy for Updating Table Definitions

The definitions of the system tables (`_aegis_*`) are created and changed with sqlx migrate (numbered SQL files). **The steps only move forward**, and are not reorganized even at a major version. From a DB of any version, starting a newer version runs the steps not yet applied, in order.

- **Where they are**: `sqlite/`, `postgres/`, and `mysql/` under [`crates/aegis-db/migrations/`](../crates/aegis-db/migrations). SQL is written by hand per dialect (because type and index syntax differ). File names are `NNNN_description.sql` (a 4-digit sequence number), and the **same number and the same name across the 3 dialects** (tests fail if they do not match).
- **`0001_baseline`**: the definition of v0.4's system tables. Everything is `IF NOT EXISTS`, so a DB created by v0.4 is adopted as "already applied" without changing anything (no backup → restore needed).
- **Records and checks**: applied steps remain in the DB's `_sqlx_migrations` with checksums. The following two stop startup.
    - **A published step's file was rewritten** (the checksum differs). If you want to fix it, add a new step.
    - **The DB has applied steps the current binary does not know** (a DB updated by a newer version was opened by an older one). Do not run it with the older version. To go back, restore from the backup taken before the update.
- **Run automatically at startup** (`init`, `run`, `task migrate`). Even when several instances start at the same time, sqlx takes an exclusive lock on PostgreSQL and MySQL.
- **No rollback (down) is created.** To go back, restore from the backup taken before the update (`task dump`).

### 6.1 How to Write Steps

- **Basically add columns, tables, and indexes.** Add columns that are nullable or have a default value, so that when updating several instances one by one, an instance that has not been updated yet still works if it hits the new column.
- **Split renames, type changes, and column drops into stages.** Add the new column and copy, and drop the old column in the **next step** after the version in which the app uses only the new column (SQLite's `DROP COLUMN` is 3.35 and later).
- **Write one change per file**, in a form that gives the same result however many times it is run (such as `IF NOT EXISTS`). **MySQL commits DDL one statement at a time and cannot roll back.** A step that fails partway is recorded as "failed" by sqlx and stops startup. Fix the cause, delete that row in `_sqlx_migrations`, and start again.
- When rewriting data inside a step (`UPDATE`), make it give the same result on all 3 DBs.

### 6.2 Content Tables Declared in YAML Schemas

`content_*` (tables declared in YAML schemas) is not covered by this mechanism. At startup, it is still the existing mechanism that adds only columns missing from the existing table with `ALTER TABLE ... ADD COLUMN` ([architecture.md "Schema evolution"](architecture.md)).

### 6.3 Relation to Backup and Restore

Since the steps only move forward, no backup → restore is needed for updates across versions. Backups are used as insurance before an update (for going back) and for moving to another DB or server. Restore drops custom fields that do not fit the current schema, with a warning ([architecture.md 17](architecture.md#17-backup-and-restore-aegis-backup)).

### 6.4 DBs Older Than v0.4

For a DB up to 0.3.x, `0001_baseline` does nothing, treating the tables as "already created", so it cannot add the missing columns. `init` checks for the columns added in v0.4 (`REQUIRED_COLUMNS`), and if they are absent it stops startup and shows the procedure of taking a backup on 0.3.x and restoring it into the new version.
