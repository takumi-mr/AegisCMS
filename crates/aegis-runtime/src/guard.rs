use aegis_core::{AegisError, PluginManifest, Result};

pub struct CapabilityGuard<'a> {
    manifest: &'a PluginManifest,
}

impl<'a> CapabilityGuard<'a> {
    pub fn new(manifest: &'a PluginManifest) -> Self {
        Self { manifest }
    }

    /// 指定URLへのHTTPフェッチ権限があるかを検証
    pub fn check_network_access(&self, url_str: &str) -> Result<()> {
        let net_cap = self.manifest.capabilities.network.as_ref().ok_or_else(|| {
            AegisError::PermissionDenied(format!(
                "Plugin '{}' has no network capability declared",
                self.manifest.name
            ))
        })?;

        // 簡易URLパースでホスト名を抽出
        let host = if let Some(stripped) = url_str.strip_prefix("https://") {
            stripped.split(['/', ':', '?']).next().unwrap_or("")
        } else if let Some(stripped) = url_str.strip_prefix("http://") {
            stripped.split(['/', ':', '?']).next().unwrap_or("")
        } else {
            return Err(AegisError::PermissionDenied(format!(
                "Invalid URL format: {}",
                url_str
            )));
        };

        let is_allowed = net_cap
            .allowed_domains
            .iter()
            .any(|domain| host == domain || host.ends_with(&format!(".{}", domain)));

        if is_allowed {
            Ok(())
        } else {
            Err(AegisError::PermissionDenied(format!(
                "Access to domain '{}' is not permitted by plugin '{}' manifest",
                host, self.manifest.name
            )))
        }
    }

    /// フック実行が許可されているかを検証
    pub fn check_hook(&self, hook_name: &str) -> Result<()> {
        if self
            .manifest
            .capabilities
            .hooks
            .iter()
            .any(|h| h == hook_name)
        {
            Ok(())
        } else {
            Err(AegisError::PermissionDenied(format!(
                "Plugin '{}' is not registered for hook '{}'",
                self.manifest.name, hook_name
            )))
        }
    }

    /// KVストレージのネームスペースアクセス権を検証
    pub fn check_storage_access(&self, namespace: &str) -> Result<()> {
        let storage_cap = self.manifest.capabilities.storage.as_ref().ok_or_else(|| {
            AegisError::PermissionDenied(format!(
                "Plugin '{}' has no storage capability declared",
                self.manifest.name
            ))
        })?;

        if storage_cap.kv_namespace == namespace {
            Ok(())
        } else {
            Err(AegisError::PermissionDenied(format!(
                "Plugin '{}' cannot access namespace '{}' (allowed: '{}')",
                self.manifest.name, namespace, storage_cap.kv_namespace
            )))
        }
    }
}
