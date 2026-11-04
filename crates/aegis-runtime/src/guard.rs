use aegis_core::{AegisError, PluginManifest, Result};

/// サイトの設定（`[plugins.settings.<name>]`）でプラグインに足す、通信を許可するドメインの設定キー。
/// 値はカンマ区切りの文字列か、文字列の配列（マニフェストの `allowed_domains` に足して使う）
pub const EXTRA_ALLOWED_DOMAINS_SETTING: &str = "extra_allowed_domains";

/// [`EXTRA_ALLOWED_DOMAINS_SETTING`] の値（カンマ区切り、または JSON の文字列の配列）をドメインの一覧にする
pub fn parse_extra_domains(value: &str) -> Vec<String> {
    let value = value.trim();
    let items: Vec<String> = match serde_json::from_str::<Vec<String>>(value) {
        Ok(list) => list,
        Err(_) => value.split(',').map(str::to_string).collect(),
    };
    items
        .into_iter()
        .map(|d| d.trim().trim_end_matches('.').to_ascii_lowercase())
        .filter(|d| !d.is_empty())
        .collect()
}

pub struct CapabilityGuard<'a> {
    manifest: &'a PluginManifest,
    /// サイトの設定で足した許可ドメイン
    extra_domains: Vec<String>,
}

impl<'a> CapabilityGuard<'a> {
    pub fn new(manifest: &'a PluginManifest) -> Self {
        Self {
            manifest,
            extra_domains: Vec::new(),
        }
    }

    pub fn with_extra_domains(mut self, domains: Vec<String>) -> Self {
        self.extra_domains = domains;
        self
    }

    /// 指定URLへのHTTPフェッチ権限があるかを検証
    pub fn check_network_access(&self, url_str: &str) -> Result<()> {
        let net_cap = self.manifest.capabilities.network.as_ref().ok_or_else(|| {
            AegisError::PermissionDenied(format!(
                "Plugin '{}' has no network capability declared",
                self.manifest.name
            ))
        })?;

        // ホスト名は HTTP クライアントと同じ URL パーサーで取り出す（`https://evil.com#.allowed.com`
        // のような URL で判定をすり抜けさせない）。ユーザー情報を含む URL は拒否する
        let deny = |reason: &str| AegisError::PermissionDenied(format!("{}: {}", reason, url_str));
        let url = url::Url::parse(url_str).map_err(|_| deny("Invalid URL format"))?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(deny("Only http and https URLs are allowed"));
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(deny("URLs with user information are not allowed"));
        }
        let host = url
            .host_str()
            .ok_or_else(|| deny("The URL has no host"))?
            .trim_end_matches('.')
            .to_ascii_lowercase();
        let host = host.as_str();

        let is_allowed = net_cap
            .allowed_domains
            .iter()
            .chain(self.extra_domains.iter())
            .map(|domain| domain.trim().trim_end_matches('.').to_ascii_lowercase())
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
