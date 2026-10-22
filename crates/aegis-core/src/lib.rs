pub mod audit;
pub mod auth;
pub mod cache;
pub mod config;
pub mod entity;
pub mod error;
pub mod manifest;
pub mod storage;

pub use audit::{
    format_audit_timestamp, normalize_audit_timestamp, AuditAction, AuditActor, AuditEvent,
    AuditOutcome, AuditQuery, AuditSink,
};
pub use auth::{AuthCredentials, AuthIdentity, AuthProvider, AuthRedirect, UserAuthContext};
pub use cache::{CacheBackend, CacheBackendKind};
pub use config::{
    AegisConfig, AuditConfig, AuditSinkKind, AuthConfig, CacheConfig, DatabaseBackendKind,
    DatabaseConfig, LocalStorageConfig, LoginRateLimitConfig, OidcConfig, PathsConfig,
    PluginsConfig, RedisCacheConfig, S3StorageConfig, SecurityConfig, StorageConfig, ThemeConfig,
};

pub use entity::{ContentSummary, PageRenderContext, Post, PostContext, PostId, PostStatus};

pub use error::{AegisError, Result};
pub use manifest::{
    is_valid_plugin_name, NetworkCapability, PackageAuthor, PluginCapabilities, PluginManifest,
    PluginResourceLimits, StorageCapability,
};
pub use storage::{
    guess_content_type, StorageBackend, StorageBackendKind, StorageMeta, StorageObject,
};
