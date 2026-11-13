// この SDK で `unsafe` を許すのは、Wasm の ABI を扱うこのファイルだけ（Cargo.toml の [lints] を参照）。
// どの `unsafe` ブロックにも、成り立つ理由を `// SAFETY:` で書く
#![allow(unsafe_code)]

use std::alloc::{alloc, dealloc, Layout};
use std::collections::HashMap;
use std::sync::Mutex;

/// 確保した領域の登録簿: ハンドル → (アドレス, 確保したバイト数)
///
/// `allocate` が確保した領域だけをここに載せる。`deallocate` と `read_bytes_from_host` は、呼び出し元が渡す
/// `ptr` / `len` をそのまま使わず、必ずこの登録簿と照らす。載っていない値（未確保・解放済み・でたらめ）は
/// 何もしないか空を返し、確保したバイト数を超える長さは確保したバイト数に切り詰める。
/// これで、二重解放・確保時と違う大きさでの解放・領域の外の読み取りが起きない
///
/// ハンドルは、Wasm ではゲストのアドレスそのもの、ネイティブ（`cargo test`）では連番の ID
static ALLOCATIONS: Mutex<Option<HashMap<i32, (usize, usize)>>> = Mutex::new(None);

#[cfg(not(target_arch = "wasm32"))]
static NEXT_ID: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(1);

/// 登録簿を操作する。別のスレッドが panic して Mutex が毒されていても、登録簿の中身は壊れていない
/// （挿入と削除だけ）ので、そのまま使う
fn with_allocations<T>(f: impl FnOnce(&mut HashMap<i32, (usize, usize)>) -> T) -> T {
    let mut guard = ALLOCATIONS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    f(guard.get_or_insert_with(HashMap::new))
}

/// ホストがゲストメモリにデータを書き込む領域を確保するアロケータ
pub fn allocate(len: usize) -> i32 {
    // 2 GiB 以上は、`i32` のハンドル・長さで表せない
    if len == 0 || len > i32::MAX as usize {
        return 0;
    }
    let Ok(layout) = Layout::from_size_align(len, 8) else {
        return 0;
    };
    // SAFETY: `layout` の大きさは 0 ではない
    let ptr = unsafe { alloc(layout) };
    if ptr.is_null() {
        return 0;
    }

    #[cfg(target_arch = "wasm32")]
    let id = ptr as usize as i32;

    #[cfg(not(target_arch = "wasm32"))]
    let id = loop {
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        // 一周して 0 や使用中の ID に当たったら、次の ID にする
        if id != 0 && with_allocations(|map| !map.contains_key(&id)) {
            break id;
        }
    };

    with_allocations(|map| map.insert(id, (ptr as usize, len)));
    id
}

/// 確保された領域を解放する
///
/// 解放するのは、`allocate` が確保して、まだ解放していない領域だけ。それ以外の `id_or_ptr`（0・未確保・
/// 解放済み・でたらめな値）は何もしない。解放の大きさは、確保したときの大きさを使う（`len` は使わない）
///
/// # Safety
/// 関数の中で `id_or_ptr` と `len` を検証するので、どんな値でも安全に呼べる。`unsafe` のままなのは、
/// 既存のプラグインのコードとの互換のため
pub unsafe fn deallocate(id_or_ptr: i32, _len: usize) {
    if id_or_ptr == 0 {
        return;
    }
    if let Some((addr, alloc_len)) = with_allocations(|map| map.remove(&id_or_ptr)) {
        if let Ok(layout) = Layout::from_size_align(alloc_len, 8) {
            // SAFETY: 登録簿から取り出した `(addr, alloc_len)` は `allocate` が同じ `layout` で確保したもので、
            // 取り出したので二重には解放されない
            dealloc(addr as *mut u8, layout);
        }
    }
}

/// ネイティブ環境でハンドルから実際のポインタを取得
#[cfg(not(target_arch = "wasm32"))]
pub fn get_native_ptr(id: i32) -> *mut u8 {
    with_allocations(|map| map.get(&id).map(|(addr, _)| *addr as *mut u8))
        .unwrap_or(std::ptr::null_mut())
}

/// ホストから渡されたポインタと長さからバイト列を安全にコピーし、入力バッファを解放する
///
/// `allocate` が確保した領域でなければ空を返す。`len` が確保した大きさより大きければ、確保した大きさまで読む
///
/// # Safety
/// 関数の中で `ptr` と `len` を検証するので、どんな値でも安全に呼べる。`unsafe` のままなのは、
/// 既存のプラグインのコードとの互換のため
pub unsafe fn read_bytes_from_host(ptr: i32, len: i32) -> Vec<u8> {
    if ptr == 0 || len <= 0 {
        return Vec::new();
    }
    // 登録簿から取り出す（これで、同じ領域を読む呼び出しが 2 つ重なっても、解放は 1 回だけ）
    let Some((addr, alloc_len)) = with_allocations(|map| map.remove(&ptr)) else {
        return Vec::new();
    };
    let n = (len as usize).min(alloc_len);
    // SAFETY: `addr` は `allocate` が `alloc_len` バイトで確保した領域で、`n` はそれを超えない。
    // 登録簿から取り出したので、読んでいる間に他から解放されない
    let bytes = std::slice::from_raw_parts(addr as *const u8, n).to_vec();
    if let Ok(layout) = Layout::from_size_align(alloc_len, 8) {
        dealloc(addr as *mut u8, layout);
    }
    bytes
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
    let Some(addr) = with_allocations(|map| map.get(&dest_id).map(|(addr, _)| *addr)) else {
        return 0;
    };
    // SAFETY: `addr` は今 `allocate(len)` で確保した `len` バイトの領域で、`bytes` とは重ならない
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), addr as *mut u8, len);
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

/// カスタムブロックの描画フック（`render-blocks`）の共通処理: 入力 `{"blocks": [...]}` を読み、
/// 結果を `{"html": [...]}` で書き戻す。`Err` は `{"error": ...}`（ホストはそのブロックを出さない）
///
/// # Safety
/// `ptr` / `len` はホストが `allocate` で確保して書き込んだ領域であること
pub unsafe fn run_render_blocks_hook(
    ptr: i32,
    len: i32,
    f: impl FnOnce(
        Vec<crate::types::BlockNode>,
    ) -> crate::types::Result<Vec<Option<std::string::String>>>,
) -> i64 {
    #[derive(serde::Deserialize)]
    struct Input {
        #[serde(default)]
        blocks: Vec<crate::types::BlockNode>,
    }
    let in_bytes = read_bytes_from_host(ptr, len);
    let input: Input = match serde_json::from_slice(&in_bytes) {
        Ok(input) => input,
        Err(e) => return write_error_to_host(&format!("Invalid render-blocks input: {}", e)),
    };
    match f(input.blocks) {
        Ok(html) => match serde_json::to_vec(&serde_json::json!({ "html": html })) {
            Ok(bytes) => write_bytes_to_host(bytes),
            Err(e) => write_error_to_host(&format!("Failed to serialize the HTML: {}", e)),
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

/// 通知の配信フック（`send-notification`）の共通処理。送ったら 0、扱わない通知は
/// `{"skipped": true}`、失敗は `{"error": ..., "permanent": ...}`
///
/// # Safety
/// `ptr` / `len` はホストが `allocate` で確保して書き込んだ領域であること
pub unsafe fn run_delivery_hook(
    ptr: i32,
    len: i32,
    f: impl FnOnce(crate::types::NotificationMessage) -> crate::types::Result<crate::types::Delivery>,
) -> i64 {
    let in_bytes = read_bytes_from_host(ptr, len);
    let message = match serde_json::from_slice(&in_bytes) {
        Ok(message) => message,
        Err(e) => {
            let body = serde_json::json!({
                "error": format!("Invalid NotificationMessage: {}", e),
                "permanent": true,
            });
            return write_bytes_to_host(serde_json::to_vec(&body).unwrap_or_default());
        }
    };
    match f(message) {
        Ok(crate::types::Delivery::Sent) => 0,
        Ok(crate::types::Delivery::Skipped) => write_bytes_to_host(br#"{"skipped":true}"#.to_vec()),
        Err(e) => {
            let body = serde_json::json!({ "error": e.to_string(), "permanent": e.is_permanent() });
            write_bytes_to_host(serde_json::to_vec(&body).unwrap_or_default())
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    fn filled(bytes: &[u8]) -> i32 {
        let id = allocate(bytes.len());
        assert_ne!(id, 0);
        // SAFETY: `id` はいま確保した `bytes.len()` バイトの領域
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), get_native_ptr(id), bytes.len()) };
        id
    }

    #[test]
    fn test_read_returns_the_bytes_and_frees_the_buffer() {
        let id = filled(b"hello");
        assert_eq!(unsafe { read_bytes_from_host(id, 5) }, b"hello");
        // 読むと解放されるので、2 回目は空
        assert!(unsafe { read_bytes_from_host(id, 5) }.is_empty());
        assert!(get_native_ptr(id).is_null());
    }

    #[test]
    fn test_read_never_goes_past_the_allocation() {
        let id = filled(b"abcd");
        // 確保した 4 バイトより長い `len` は、確保した大きさまでで切る
        assert_eq!(unsafe { read_bytes_from_host(id, i32::MAX) }, b"abcd");
    }

    #[test]
    fn test_bogus_handles_and_lengths_do_nothing() {
        for (ptr, len) in [(0, 8), (-1, 8), (i32::MAX, 8), (12345, 0), (12345, -4)] {
            assert!(
                unsafe { read_bytes_from_host(ptr, len) }.is_empty(),
                "{ptr}/{len}"
            );
            unsafe { deallocate(ptr, len.max(0) as usize) };
        }
    }

    #[test]
    fn test_double_free_and_wrong_size_free_are_harmless() {
        let id = allocate(64);
        // 確保した大きさと違う `len` でも、確保したときの大きさで解放する
        unsafe { deallocate(id, 1) };
        unsafe { deallocate(id, 64) };
        unsafe { deallocate(id, usize::MAX) };
        assert!(get_native_ptr(id).is_null());
    }

    #[test]
    fn test_allocate_rejects_sizes_that_do_not_fit_in_i32() {
        assert_eq!(allocate(0), 0);
        assert_eq!(allocate(i32::MAX as usize + 1), 0);
        assert_eq!(allocate(usize::MAX), 0);
    }

    #[test]
    fn test_write_bytes_to_host_round_trips() {
        let packed = write_bytes_to_host(b"payload".to_vec());
        let (ptr, len) = unpack_ptr_len(packed);
        assert_eq!(unsafe { read_bytes_from_host(ptr, len) }, b"payload");
        assert_eq!(write_bytes_to_host(Vec::new()), 0);
    }
}
