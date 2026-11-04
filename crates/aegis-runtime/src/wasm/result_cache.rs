//! `on-render-page` の結果のキャッシュ。
//!
//! Wasm のプラグインがホストの外の状態に触れるのは、ホスト API（`kv_get` / `kv_set` /
//! `http_fetch` / `config_get`）を呼ぶときだけである（時計も乱数も WASI も無い）。そのため、
//! 1 回の呼び出しで `kv_*` / `http_fetch` を呼ばず、`config_get` で読んだ設定値も変わっていなければ、
//! 出力は入力の JSON だけで決まる。この性質を呼び出しごとに実行時に確かめ、満たす呼び出しだけを
//! 入力のハッシュをキーにして覚える。プラグインの宣言は要らない。
//!
//! 覚えるのは出力の全体ではなく、入力から変わったキーだけ（本文を返すプラグインでも小さい）。
//! 入れ替えは 2 世代方式（若い世代がいっぱいになったら古い世代を丸ごと捨てる）で、
//! 走査をせず、全体で予算に収まる。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

/// キャッシュのキー（プラグイン・Wasm・入力の SHA-256）
pub type CacheKey = [u8; 32];

/// 呼び出しの間にプラグインが読んだ設定値（キー, 値）。キャッシュした時点と同じなら有効
pub type ConfigReads = Vec<(String, Option<String>)>;

/// 覚えた 1 回分の結果
pub struct CachedResult {
    /// 入力から変わったトップレベルのキー
    pub changes: Map<String, Value>,
    /// 結果を出すときに読んだ設定値
    pub config_reads: ConfigReads,
    size: usize,
}

impl CachedResult {
    pub fn new(changes: Map<String, Value>, config_reads: ConfigReads) -> Self {
        let size = changes
            .iter()
            .map(|(k, v)| k.len() + json_size(v))
            .sum::<usize>()
            + config_reads
                .iter()
                .map(|(k, v)| k.len() + v.as_ref().map_or(0, String::len))
                .sum::<usize>()
            + 128;
        Self {
            changes,
            config_reads,
            size,
        }
    }
}

fn json_size(value: &Value) -> usize {
    match value {
        Value::String(s) => s.len() + 2,
        Value::Array(a) => 2 + a.iter().map(json_size).sum::<usize>(),
        Value::Object(o) => {
            2 + o
                .iter()
                .map(|(k, v)| k.len() + 3 + json_size(v))
                .sum::<usize>()
        }
        other => other.to_string().len(),
    }
}

/// キャッシュの利用状況
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResultCacheStats {
    pub hits: u64,
    pub misses: u64,
    /// 設定値が変わっていて使えなかった回数（`misses` にも数える）
    pub stale: u64,
    pub stores: u64,
}

#[derive(Default)]
struct Generations {
    young: HashMap<CacheKey, Arc<CachedResult>>,
    old: HashMap<CacheKey, Arc<CachedResult>>,
    young_bytes: usize,
}

pub struct ResultCache {
    /// 全体の予算（バイト）。0 で無効
    budget: usize,
    inner: Mutex<Generations>,
    hits: AtomicU64,
    misses: AtomicU64,
    stale: AtomicU64,
    stores: AtomicU64,
}

impl ResultCache {
    pub fn new(budget_bytes: usize) -> Self {
        Self {
            budget: budget_bytes,
            inner: Mutex::new(Generations::default()),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            stale: AtomicU64::new(0),
            stores: AtomicU64::new(0),
        }
    }

    pub fn enabled(&self) -> bool {
        self.budget > 0
    }

    /// プラグインの識別（名前・版・Wasm のダイジェスト）と入力の JSON からキーを作る。
    /// 各要素に長さを付けて連結するので、境界をずらした別の組が同じキーにならない
    pub fn key(plugin: &str, version: &str, wasm_digest: &[u8; 32], input: &[u8]) -> CacheKey {
        let mut hasher = Sha256::new();
        for part in [plugin.as_bytes(), version.as_bytes(), wasm_digest] {
            hasher.update((part.len() as u64).to_le_bytes());
            hasher.update(part);
        }
        hasher.update((input.len() as u64).to_le_bytes());
        hasher.update(input);
        hasher.finalize().into()
    }

    /// 覚えた結果を返す。`settings` で今の設定値を引き、結果を出したときと違えば使えない
    pub fn get(
        &self,
        key: &CacheKey,
        settings: impl Fn(&str) -> Option<String>,
    ) -> Option<Arc<CachedResult>> {
        let found = {
            let mut inner = self.inner.lock().ok()?;
            if let Some(hit) = inner.young.get(key) {
                Some(hit.clone())
            } else if let Some(hit) = inner.old.remove(key) {
                // 使われたものは若い世代に移して残す
                inner.young_bytes += hit.size;
                inner.young.insert(*key, hit.clone());
                Some(hit)
            } else {
                None
            }
        };
        match found {
            Some(result) if result.config_reads.iter().all(|(k, v)| settings(k) == *v) => {
                self.hits.fetch_add(1, Ordering::Relaxed);
                Some(result)
            }
            Some(_) => {
                self.stale.fetch_add(1, Ordering::Relaxed);
                self.misses.fetch_add(1, Ordering::Relaxed);
                None
            }
            None => {
                self.misses.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    pub fn insert(&self, key: CacheKey, result: CachedResult) {
        // 1 件で予算の 1/4 を超えるものは覚えない（ほかの大勢を追い出すため）
        if self.budget == 0 || result.size > self.budget / 4 {
            return;
        }
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        if inner.young_bytes + result.size > self.budget / 2 {
            inner.old = std::mem::take(&mut inner.young);
            inner.young_bytes = 0;
        }
        inner.young_bytes += result.size;
        if let Some(replaced) = inner.young.insert(key, Arc::new(result)) {
            inner.young_bytes = inner.young_bytes.saturating_sub(replaced.size);
        }
        self.stores.fetch_add(1, Ordering::Relaxed);
    }

    pub fn stats(&self) -> ResultCacheStats {
        ResultCacheStats {
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            stale: self.stale.load(Ordering::Relaxed),
            stores: self.stores.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(text: &str, reads: ConfigReads) -> CachedResult {
        let mut changes = Map::new();
        changes.insert(
            "append_content".to_string(),
            Value::String(text.to_string()),
        );
        CachedResult::new(changes, reads)
    }

    fn key(n: u8) -> CacheKey {
        ResultCache::key("p", "1", &[0; 32], &[n])
    }

    #[test]
    fn keys_differ_by_every_part() {
        let base = ResultCache::key("p", "1", &[0; 32], b"in");
        assert_eq!(base, ResultCache::key("p", "1", &[0; 32], b"in"));
        assert_ne!(base, ResultCache::key("q", "1", &[0; 32], b"in"));
        assert_ne!(base, ResultCache::key("p", "2", &[0; 32], b"in"));
        assert_ne!(base, ResultCache::key("p", "1", &[1; 32], b"in"));
        assert_ne!(base, ResultCache::key("p", "1", &[0; 32], b"in2"));
        // 境界をずらしても別のキー
        assert_ne!(
            ResultCache::key("ab", "c", &[0; 32], b""),
            ResultCache::key("a", "bc", &[0; 32], b"")
        );
    }

    #[test]
    fn hit_requires_the_same_settings() {
        let cache = ResultCache::new(1 << 20);
        cache.insert(
            key(1),
            result("x", vec![("position".to_string(), Some("top".to_string()))]),
        );
        let top = |k: &str| (k == "position").then(|| "top".to_string());
        assert!(cache.get(&key(1), top).is_some());
        // 設定が変わったら使えない
        let bottom = |k: &str| (k == "position").then(|| "bottom".to_string());
        assert!(cache.get(&key(1), bottom).is_none());
        assert!(cache.get(&key(1), |_| None).is_none());
        assert!(cache.get(&key(2), top).is_none());
        assert_eq!(
            cache.stats(),
            ResultCacheStats {
                hits: 1,
                misses: 3,
                stale: 2,
                stores: 1
            }
        );
    }

    #[test]
    fn generations_bound_the_memory_and_keep_what_is_used() {
        let one = result("x", vec![]).size;
        // 若い世代は予算の半分（= 3 件）でいっぱいになる
        let cache = ResultCache::new(one * 6);
        for n in 0..3 {
            cache.insert(key(n), result("x", vec![]));
        }
        // 4 件目で若い世代が古い世代に回る。ここで 0 を使うと若い世代に戻る
        cache.insert(key(3), result("x", vec![]));
        assert!(cache.get(&key(0), |_| None).is_some());
        // さらに入れて世代が 2 回回ると、使われなかったものは消える
        for n in 4..12 {
            cache.insert(key(n), result("x", vec![]));
        }
        assert!(cache.get(&key(1), |_| None).is_none());
        assert!(cache.get(&key(11), |_| None).is_some());
    }

    #[test]
    fn oversized_results_and_a_zero_budget_are_not_kept() {
        let cache = ResultCache::new(1024);
        cache.insert(key(1), result(&"x".repeat(1000), vec![]));
        assert!(cache.get(&key(1), |_| None).is_none());
        let off = ResultCache::new(0);
        assert!(!off.enabled());
        off.insert(key(1), result("x", vec![]));
        assert!(off.get(&key(1), |_| None).is_none());
    }
}
