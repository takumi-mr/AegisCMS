pub mod auth;
pub mod cache;
pub mod config;
pub mod entity;
pub mod error;
pub mod manifest;
pub mod storage;

pub use auth::{AuthCredentials, AuthIdentity, AuthProvider, AuthRedirect};
pub use cache::{CacheBackend, CacheBackendKind};
pub use config::{
    AegisConfig, CacheConfig, DatabaseBackendKind, DatabaseConfig, LocalStorageConfig,
    RedisCacheConfig, S3StorageConfig, SecurityConfig, StorageConfig,
};
pub use entity::{Post, PostContext, PostId, PostStatus};
pub use error::{AegisError, Result};
pub use manifest::{
    NetworkCapability, PluginCapabilities, PluginManifest, PluginResourceLimits, StorageCapability,
};
pub use storage::{StorageBackend, StorageBackendKind, StorageMeta};
