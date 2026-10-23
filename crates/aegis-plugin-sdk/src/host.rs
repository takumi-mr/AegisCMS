use crate::types::Result;
use serde::Serialize;

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "host")]
extern "C" {
    fn log(level: i32, ptr: i32, len: i32);
    fn http_check_allowed(ptr: i32, len: i32) -> i32;
    fn kv_set(k_ptr: i32, k_len: i32, v_ptr: i32, v_len: i32) -> i32;
    fn kv_get(k_ptr: i32, k_len: i32) -> i64;
    fn http_fetch(ptr: i32, len: i32) -> i64;
    fn config_get(k_ptr: i32, k_len: i32) -> i64;
}

/// ホストが `allocate` で確保して書き込んだ `ptr << 32 | len` の領域を読む（0 以下なら `None`）
#[cfg(target_arch = "wasm32")]
fn take_host_bytes(packed: i64) -> Option<Vec<u8>> {
    if packed <= 0 {
        return None;
    }
    let (ptr, len) = crate::abi::unpack_ptr_len(packed);
    Some(unsafe { crate::abi::read_bytes_from_host(ptr, len) })
}

/// ネイティブ（`cargo test`）で `kv` / `config` / `http` の代わりに使う値
#[cfg(not(target_arch = "wasm32"))]
pub mod mock {
    use std::cell::RefCell;
    use std::collections::HashMap;

    thread_local! {
        pub(crate) static KV: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
        pub(crate) static CONFIG: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
        pub(crate) static HTTP: RefCell<Vec<super::http::Request>> = const { RefCell::new(Vec::new()) };
    }

    /// テストで `config::get(key)` が返す値を設定する
    pub fn set_config(key: &str, value: &str) {
        CONFIG.with(|c| c.borrow_mut().insert(key.to_string(), value.to_string()));
    }

    /// テスト中に `http::fetch` に渡されたリクエスト（取り出すと空になる）
    pub fn take_http_requests() -> Vec<super::http::Request> {
        HTTP.with(|h| std::mem::take(&mut *h.borrow_mut()))
    }

    /// `kv` / `config` / `http` のモックを空にする
    pub fn reset() {
        KV.with(|k| k.borrow_mut().clear());
        CONFIG.with(|c| c.borrow_mut().clear());
        HTTP.with(|h| h.borrow_mut().clear());
    }
}

pub mod log {
    /// デバッグログを出力
    pub fn debug(msg: &str) {
        log_internal(0, msg);
    }

    /// 情報ログを出力
    pub fn info(msg: &str) {
        log_internal(1, msg);
    }

    /// 警告ログを出力
    pub fn warn(msg: &str) {
        log_internal(2, msg);
    }

    /// エラーログを出力
    pub fn error(msg: &str) {
        log_internal(3, msg);
    }

    fn log_internal(level: i32, msg: &str) {
        #[cfg(target_arch = "wasm32")]
        unsafe {
            super::log(level, msg.as_ptr() as i32, msg.len() as i32);
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let level_str = match level {
                0 => "DEBUG",
                1 => "INFO",
                2 => "WARN",
                _ => "ERROR",
            };
            println!("[WasmMock:{}] {}", level_str, msg);
        }
    }
}

pub mod net {
    /// URL がマニフェストの `allowed_domains` で許可されているかをホストに問い合わせる
    pub fn is_allowed(url: &str) -> bool {
        #[cfg(target_arch = "wasm32")]
        unsafe {
            super::http_check_allowed(url.as_ptr() as i32, url.len() as i32) == 1
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = url;
            true
        }
    }
}

pub mod kv {
    use super::*;

    /// KVS に文字列値を保存
    pub fn set(key: &str, value: &str) -> Result<()> {
        #[cfg(target_arch = "wasm32")]
        {
            use crate::types::PluginError;
            let res = unsafe {
                super::kv_set(
                    key.as_ptr() as i32,
                    key.len() as i32,
                    value.as_ptr() as i32,
                    value.len() as i32,
                )
            };
            match res {
                0 => Ok(()),
                1 => Err(PluginError::Host(format!(
                    "KVS permission denied for key '{}'",
                    key
                ))),
                _ => Err(PluginError::Host(format!("KVS error for key '{}'", key))),
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            super::mock::KV.with(|kv| kv.borrow_mut().insert(key.to_string(), value.to_string()));
            Ok(())
        }
    }

    /// KVS にシリアライズ可能な JSON オブジェクトを保存
    pub fn set_json<T: Serialize>(key: &str, value: &T) -> Result<()> {
        let json_str = serde_json::to_string(value)?;
        set(key, &json_str)
    }

    /// KVS から値を読む（無ければ `None`）。`capabilities.storage` が必要
    pub fn get(key: &str) -> Result<Option<String>> {
        #[cfg(target_arch = "wasm32")]
        {
            use crate::types::PluginError;
            let packed = unsafe { super::kv_get(key.as_ptr() as i32, key.len() as i32) };
            match packed {
                -1 => Err(PluginError::Host(format!(
                    "KVS permission denied for key '{}'",
                    key
                ))),
                p if p < 0 => Err(PluginError::Host(format!("KVS error for key '{}'", key))),
                p => Ok(super::take_host_bytes(p)
                    .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())),
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            Ok(super::mock::KV.with(|kv| kv.borrow().get(key).cloned()))
        }
    }

    /// KVS から JSON の値を読む
    pub fn get_json<T: serde::de::DeserializeOwned>(key: &str) -> Result<Option<T>> {
        match get(key)? {
            Some(text) => Ok(Some(serde_json::from_str(&text)?)),
            None => Ok(None),
        }
    }
}

pub mod config {
    /// プラグインの設定値（サーバーの `[plugins.settings.<プラグイン名>]`）。未設定なら `None`。
    /// 文字列以外の値（数値・配列など）は JSON で返る。空文字列も `None`（ABI では長さ 0 が「値なし」のため）。
    /// 「何もしない」を設定で表すなら `none` などの値を決めておく
    pub fn get(key: &str) -> Option<String> {
        #[cfg(target_arch = "wasm32")]
        {
            let packed = unsafe { super::config_get(key.as_ptr() as i32, key.len() as i32) };
            super::take_host_bytes(packed).map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            super::mock::CONFIG.with(|c| c.borrow().get(key).cloned())
        }
    }

    /// 設定値を `T` として読む（無い・読めなければ `default`）
    pub fn get_or<T: std::str::FromStr>(key: &str, default: T) -> T {
        get(key)
            .and_then(|value| value.trim().trim_matches('"').parse().ok())
            .unwrap_or(default)
    }
}

pub mod http {
    #[cfg(target_arch = "wasm32")]
    use crate::types::PluginError;
    use crate::types::Result;
    use serde::{Deserialize, Serialize};
    use std::collections::BTreeMap;

    /// `http::fetch` のリクエスト
    #[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
    pub struct Request {
        pub method: String,
        pub url: String,
        #[serde(default)]
        pub headers: BTreeMap<String, String>,
        #[serde(default)]
        pub body: Option<String>,
    }

    impl Request {
        pub fn get(url: impl Into<String>) -> Self {
            Self {
                method: "GET".to_string(),
                url: url.into(),
                ..Default::default()
            }
        }

        /// JSON 本文の POST（`Content-Type: application/json`）
        pub fn post_json<T: Serialize>(url: impl Into<String>, body: &T) -> Result<Self> {
            Ok(Self {
                method: "POST".to_string(),
                url: url.into(),
                headers: BTreeMap::from([(
                    "Content-Type".to_string(),
                    "application/json".to_string(),
                )]),
                body: Some(serde_json::to_string(body)?),
            })
        }

        pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
            self.headers.insert(name.into(), value.into());
            self
        }
    }

    /// `http::fetch` の応答
    #[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
    pub struct Response {
        pub status: u16,
        #[serde(default)]
        pub headers: BTreeMap<String, String>,
        #[serde(default)]
        pub body: String,
    }

    impl Response {
        pub fn is_success(&self) -> bool {
            (200..300).contains(&self.status)
        }
    }

    /// HTTP リクエストを送る。宛先はマニフェストの `capabilities.network.allowed_domains` に
    /// 含まれていること（リダイレクトは追わない。1 回のフック呼び出しで 8 回まで、本文は 1 MiB まで）。
    /// フックの `execution_timeout_ms` に通信の時間も含まれる
    pub fn fetch(request: &Request) -> Result<Response> {
        #[cfg(target_arch = "wasm32")]
        {
            let body = serde_json::to_vec(request)?;
            let packed = unsafe { super::http_fetch(body.as_ptr() as i32, body.len() as i32) };
            let bytes = super::take_host_bytes(packed)
                .ok_or_else(|| PluginError::Host("http_fetch returned nothing".to_string()))?;
            let value: serde_json::Value = serde_json::from_slice(&bytes)?;
            if let Some(error) = value.get("error").and_then(|e| e.as_str()) {
                return Err(PluginError::Host(error.to_string()));
            }
            Ok(serde_json::from_value(value)?)
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            super::mock::HTTP.with(|h| h.borrow_mut().push(request.clone()));
            Ok(Response {
                status: 200,
                ..Default::default()
            })
        }
    }
}

#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {
        $crate::host::log::debug(&format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        $crate::host::log::info(&format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        $crate::host::log::warn(&format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        $crate::host::log::error(&format!($($arg)*))
    };
}
