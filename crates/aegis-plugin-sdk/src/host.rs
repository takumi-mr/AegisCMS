use crate::types::Result;
use serde::Serialize;

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "host")]
extern "C" {
    fn log(level: i32, ptr: i32, len: i32);
    fn http_check_allowed(ptr: i32, len: i32) -> i32;
    fn kv_set(k_ptr: i32, k_len: i32, v_ptr: i32, v_len: i32) -> i32;
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
            let _ = (key, value);
            Ok(())
        }
    }

    /// KVS にシリアライズ可能な JSON オブジェクトを保存
    pub fn set_json<T: Serialize>(key: &str, value: &T) -> Result<()> {
        let json_str = serde_json::to_string(value)?;
        set(key, &json_str)
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
