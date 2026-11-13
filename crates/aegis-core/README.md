# aegis-core

**English** | [日本語](README.ja.md)

The lowest-level crate of AegisCMS: shared interfaces, core data types, and contract definitions.

## Overview

`aegis-core` provides the abstract interfaces, common domain models, manifest definitions, and unified error type shared by AegisCMS's domain crates (`aegis-db`, `aegis-storage`, `aegis-auth`, etc.) and the WebAssembly plugin runtime.

The crate itself has no implementation that depends on external systems such as concrete storage or databases; it works purely as a shared contract.

## Responsibilities and Features

- **Abstract traits**:
    - `StorageBackend`: abstraction for saving, retrieving, and deleting media and assets, and for generating public URLs.
    - `CacheBackend`: abstraction for getting, storing, and deleting cache entries.
    - `AuthProvider`: abstraction for integrating external authentication providers.
    - `AuditSink`: abstraction for where audit events are recorded.
    - `Notifier`: abstraction for delivering notifications such as email.
- **Plugin integration types**:
    - Context types exchanged between Wasm plugins and the host, such as `PostContext`, `PageRenderContext`, `OrderContext`, and `CommentSubmission`.
- **Plugin manifest**:
    - `PluginManifest`: definition and validation of plugin metadata, requested permissions (network domains, KVS namespace), and resource limits (fuel / memory caps).
- **Packaging and signature verification**:
    - Safe ZIP extraction for plugins and themes (path traversal prevention), and Ed25519 digital signature verification and signing utilities.
- **Unified error**:
    - `AegisError` and the `Result<T>` type alias.

## Module Layout

| Module     | Description                                                                                     |
| :--------- | :---------------------------------------------------------------------------------------------- |
| `audit`    | Audit log data model (`AuditEvent`, `AuditAction`, etc.) and the `AuditSink` trait              |
| `auth`     | External auth provider abstraction (`AuthProvider`) and `UserAuthContext`                       |
| `cache`    | Cache abstraction trait (`CacheBackend`)                                                        |
| `comments` | Comment moderation context (`CommentSubmission`, `CommentVerdict`)                              |
| `commerce` | E-commerce, order, and currency calculation types (`OrderContext`, `OrderLine`, `format_money`) |
| `entity`   | Content model (`PostContext`, `PageRenderContext`, `PublishedContent`)                          |
| `error`    | System-wide error (`AegisError`)                                                                |
| `manifest` | Plugin manifest definitions (`PluginManifest`, `PluginCapabilities`)                            |
| `notify`   | Notification abstraction (`Notifier`, `NotificationMessage`, `Recipient`)                       |
| `package`  | Safe ZIP extraction and Ed25519 signature verification (`verify_package_signature`)             |
| `storage`  | Object storage abstraction (`StorageBackend`, `StorageMeta`)                                    |
