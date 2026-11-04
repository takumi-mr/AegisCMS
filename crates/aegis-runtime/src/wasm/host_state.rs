use std::sync::{Arc, Mutex};

use crate::guard::CapabilityGuard;
use crate::services::PluginServices;
use aegis_core::PluginManifest;
use serde::{Deserialize, Serialize};
use wasmtime::{StoreLimits, StoreLimitsBuilder};

/// 1 回のフック呼び出しで `host.http_fetch` を呼べる回数
pub const MAX_FETCHES_PER_CALL: u32 = 8;

/// プラグイン実行中に `host.log` 経由でキャプチャされたログエントリ
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginLogEntry {
    /// ログレベル ("debug", "info", "warn", "error")
    pub level: String,
    /// ログ本文
    pub message: String,
    /// ISO 8601 (RFC 3339) タイムスタンプ
    pub timestamp: String,
}

/// 1 回の呼び出しでプラグインがホストの外の状態に触れたか（結果をキャッシュしてよいかの判定用）
#[derive(Debug, Default, Clone)]
pub struct HostUsage {
    /// `kv_get` / `kv_set` / `http_fetch` を呼んだ。出力が入力だけでは決まらない
    pub impure: bool,
    /// `config_get` で読んだ設定値（キー, 値）。同じ値なら同じ出力になる
    pub config_reads: Vec<(String, Option<String>)>,
}

/// WasmtimeのStore内に保持されるホスト状態
pub struct PluginHostState {
    pub manifest: PluginManifest,
    /// KV ストア・HTTP・設定値（サーバーが実装する）
    pub services: PluginServices,
    /// この呼び出しで `host.http_fetch` を呼んだ回数
    pub fetch_count: u32,
    pub last_error: Option<String>,
    /// Linear memory cap from `resource_limits.max_memory_mb` (installed with `Store::limiter`)
    pub limits: StoreLimits,
    /// ログのリアルタイム収集用コレクタ（開発サーバー・テスト向け）
    pub log_collector: Option<Arc<Mutex<Vec<PluginLogEntry>>>>,
    /// この呼び出しで触れた外部の状態（呼び出しのあとにホストが読む）
    pub usage: Arc<Mutex<HostUsage>>,
}

impl PluginHostState {
    pub fn new(manifest: PluginManifest, services: PluginServices) -> Self {
        let max_memory_bytes = (manifest.resource_limits.max_memory_mb.max(1) as usize) << 20;
        let limits = StoreLimitsBuilder::new()
            .memory_size(max_memory_bytes)
            // Growing past the cap traps (reported as a plugin error) instead of quietly
            // returning -1 from memory.grow
            .trap_on_grow_failure(true)
            .build();
        Self {
            manifest,
            services,
            fetch_count: 0,
            last_error: None,
            limits,
            log_collector: None,
            usage: Arc::new(Mutex::new(HostUsage::default())),
        }
    }

    pub fn with_log_collector(mut self, collector: Arc<Mutex<Vec<PluginLogEntry>>>) -> Self {
        self.log_collector = Some(collector);
        self
    }

    /// 外部の状態に触れたことを記録する（`kv_get` / `kv_set` / `http_fetch`）
    pub fn mark_impure(&self) {
        if let Ok(mut usage) = self.usage.lock() {
            usage.impure = true;
        }
    }

    /// `config_get` で読んだ値を記録する
    pub fn record_config_read(&self, key: &str, value: &Option<String>) {
        if let Ok(mut usage) = self.usage.lock() {
            usage.config_reads.push((key.to_string(), value.clone()));
        }
    }

    pub fn guard(&self) -> CapabilityGuard<'_> {
        CapabilityGuard::new(&self.manifest)
    }
}
