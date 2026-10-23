#[cfg(target_arch = "wasm32")]
use std::alloc::{alloc, dealloc, Layout};

#[cfg(not(target_arch = "wasm32"))]
use std::sync::Mutex;

#[cfg(not(target_arch = "wasm32"))]
static NATIVE_ALLOCATIONS: Mutex<Option<std::collections::HashMap<i32, (usize, usize)>>> =
    Mutex::new(None);
#[cfg(not(target_arch = "wasm32"))]
static NEXT_ID: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(1);

/// ホストがゲストメモリにデータを書き込む領域を確保するアロケータ
pub fn allocate(len: usize) -> i32 {
    if len == 0 {
        return 0;
    }

    #[cfg(target_arch = "wasm32")]
    {
        let layout = match Layout::from_size_align(len, 8) {
            Ok(l) => l,
            Err(_) => return 0,
        };
        unsafe { alloc(layout) as i32 }
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let layout = match std::alloc::Layout::from_size_align(len, 8) {
            Ok(l) => l,
            Err(_) => return 0,
        };
        let ptr = unsafe { std::alloc::alloc(layout) };
        if ptr.is_null() {
            return 0;
        }
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut guard = NATIVE_ALLOCATIONS.lock().unwrap();
        let map = guard.get_or_insert_with(std::collections::HashMap::new);
        map.insert(id, (ptr as usize, len));
        id
    }
}

/// 確保されたメモリ領域を解放する
///
/// # Safety
/// `id_or_ptr` は `allocate` で確保された有効なポインタまたはハンドルであり、`len` は確保時のバイトサイズと一致する必要があります。
pub unsafe fn deallocate(id_or_ptr: i32, len: usize) {
    if id_or_ptr == 0 || len == 0 {
        return;
    }

    #[cfg(target_arch = "wasm32")]
    {
        if let Ok(layout) = Layout::from_size_align(len, 8) {
            dealloc(id_or_ptr as *mut u8, layout);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut guard = NATIVE_ALLOCATIONS.lock().unwrap();
        if let Some(map) = guard.as_mut() {
            if let Some((addr, alloc_len)) = map.remove(&id_or_ptr) {
                if let Ok(layout) = std::alloc::Layout::from_size_align(alloc_len, 8) {
                    std::alloc::dealloc(addr as *mut u8, layout);
                }
            }
        }
    }
}

/// ネイティブ環境でハンドルから実際のポインタを取得
#[cfg(not(target_arch = "wasm32"))]
pub fn get_native_ptr(id: i32) -> *mut u8 {
    let mut guard = NATIVE_ALLOCATIONS.lock().unwrap();
    let map = guard.get_or_insert_with(std::collections::HashMap::new);
    map.get(&id)
        .map(|(addr, _)| *addr as *mut u8)
        .unwrap_or(std::ptr::null_mut())
}

/// ホストから渡されたポインタと長さからバイト列を安全にコピーし、入力バッファを解放する
///
/// # Safety
/// `ptr` は `allocate` で確保された有効なメモリ領域であり、`len` バイト書き込まれている必要があります。
pub unsafe fn read_bytes_from_host(ptr: i32, len: i32) -> Vec<u8> {
    if ptr == 0 || len <= 0 {
        return Vec::new();
    }

    #[cfg(target_arch = "wasm32")]
    {
        let slice = std::slice::from_raw_parts(ptr as *const u8, len as usize);
        let bytes = slice.to_vec();
        deallocate(ptr, len as usize);
        bytes
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let raw_ptr = get_native_ptr(ptr);
        if raw_ptr.is_null() {
            return Vec::new();
        }
        let slice = std::slice::from_raw_parts(raw_ptr as *const u8, len as usize);
        let bytes = slice.to_vec();
        deallocate(ptr, len as usize);
        bytes
    }
}

/// ホストへ返却するためのメモリを確保・コピーし、(ptr, len) を packed i64 として返す
pub fn write_bytes_to_host(bytes: Vec<u8>) -> i64 {
    let len = bytes.len();
    if len == 0 {
        return 0;
    }
    let dest_id = allocate(len);
    if dest_id == 0 {
        return 0;
    }

    #[cfg(target_arch = "wasm32")]
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), dest_id as *mut u8, len);
    }

    #[cfg(not(target_arch = "wasm32"))]
    unsafe {
        let raw_ptr = get_native_ptr(dest_id);
        if raw_ptr.is_null() {
            return 0;
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), raw_ptr, len);
    }

    pack_ptr_len(dest_id, len as i32)
}

/// フックの失敗をホストへ返す。`{"error": "<message>"}` を書き込み、packed i64 を返す
///
/// `on_before_save` でこれを返すと、ホストはコンテンツの保存を中止します。
pub fn write_error_to_host(message: &str) -> i64 {
    let body = serde_json::json!({ "error": message });
    write_bytes_to_host(serde_json::to_vec(&body).unwrap_or_default())
}

/// ポインタ (上位32bit) と長さ (下位32bit) を 64bit 整数にパック
#[inline]
pub fn pack_ptr_len(ptr: i32, len: i32) -> i64 {
    ((ptr as i64) << 32) | ((len as u32) as i64)
}

/// 64bit 整数からポインタと長さをアンパック
#[inline]
pub fn unpack_ptr_len(packed: i64) -> (i32, i32) {
    let ptr = (packed >> 32) as i32;
    let len = (packed & 0xFFFF_FFFF) as i32;
    (ptr, len)
}

/// オブジェクトを受け取って返すフックの共通処理: 入力を読み、`f` の結果を書き戻す。
/// `Err` は `{"error": ...}`（ホストは処理を断る）
///
/// # Safety
/// `ptr` / `len` はホストが `allocate` で確保して書き込んだ領域であること
pub unsafe fn run_object_hook<I, O>(
    ptr: i32,
    len: i32,
    type_name: &str,
    f: impl FnOnce(I) -> crate::types::Result<O>,
) -> i64
where
    I: serde::de::DeserializeOwned,
    O: serde::Serialize,
{
    let in_bytes = read_bytes_from_host(ptr, len);
    let input: I = match serde_json::from_slice(&in_bytes) {
        Ok(input) => input,
        Err(e) => return write_error_to_host(&format!("Invalid {}: {}", type_name, e)),
    };
    match f(input) {
        Ok(output) => match serde_json::to_vec(&output) {
            Ok(bytes) => write_bytes_to_host(bytes),
            Err(e) => write_error_to_host(&format!("Failed to serialize {}: {}", type_name, e)),
        },
        Err(e) => write_error_to_host(&e.to_string()),
    }
}

/// 結果を返さない通知フックの共通処理（成功なら 0、`Err` なら `{"error": ...}`）
///
/// # Safety
/// `ptr` / `len` はホストが `allocate` で確保して書き込んだ領域であること
pub unsafe fn run_notification_hook<I>(
    ptr: i32,
    len: i32,
    type_name: &str,
    f: impl FnOnce(I) -> crate::types::Result<()>,
) -> i64
where
    I: serde::de::DeserializeOwned,
{
    let in_bytes = read_bytes_from_host(ptr, len);
    let input: I = match serde_json::from_slice(&in_bytes) {
        Ok(input) => input,
        Err(e) => return write_error_to_host(&format!("Invalid {}: {}", type_name, e)),
    };
    match f(input) {
        Ok(()) => 0,
        Err(e) => write_error_to_host(&e.to_string()),
    }
}
