//! Core types, traits, and utilities for AegisCMS.

pub mod audit;
pub mod auth;
pub mod cache;
pub mod comments;
pub mod commerce;
pub mod entity;
pub mod error;
pub mod manifest;
pub mod notify;
pub mod package;
pub mod storage;

pub use audit::{
    format_audit_timestamp, normalize_audit_timestamp, AuditAction, AuditActor, AuditEvent,
    AuditOutcome, AuditQuery, AuditSink,
};
pub use auth::{AuthCredentials, AuthIdentity, AuthProvider, AuthRedirect, UserAuthContext};
pub use cache::CacheBackend;
pub use comments::{
    CommentSection, CommentSubmission, CommentVerdict, ModerationStatus, PublicComment,
};
pub use commerce::{
    convert_currency, currency_decimals, format_money, is_zero_decimal_currency, OrderAdjustment,
    OrderContext, OrderLine, OrderRefundRecord, OrderStatus, OrderStatusChange, ProductVariant,
    ShippingAddress,
};

pub use entity::{ContentSummary, PageRenderContext, Pagination, PostContext, PublishedContent};

pub use error::{AegisError, Result};
pub use manifest::{
    is_valid_frame_domain, is_valid_plugin_name, BlockDefinition, BlockField, BlockFieldOption,
    BlockFieldType, EmbedCapability, LocalizedText, NetworkCapability, PackageAuthor,
    PluginCapabilities, PluginManifest, PluginResourceLimits, StorageCapability,
};
pub use notify::{
    Delivered, NotificationEvent, NotificationMessage, Notifier, Recipient, SendError,
};
pub use package::{
    check_min_aegis_version, compute_package_entries_digest, compute_package_files_digest,
    create_test_signature, extract_zip_safely, validate_signature_policy, verify_package_signature,
    PackageSignature, ZipExtractOptions, VERSION,
};
pub use storage::{guess_content_type, StorageBackend, StorageMeta, StorageObject};
