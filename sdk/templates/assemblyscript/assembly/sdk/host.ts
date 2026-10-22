/**
 * AegisCMS AssemblyScript Plugin SDK - Host Functions
 */

// --- Low-level host imports ---
// @ts-ignore
@external("host", "log")
declare function host_log(level: i32, ptr: i32, len: i32): void;

// @ts-ignore
@external("host", "http_check_allowed")
declare function host_http_check_allowed(ptr: i32, len: i32): i32;

// @ts-ignore
@external("host", "kv_set")
declare function host_kv_set(k_ptr: i32, k_len: i32, v_ptr: i32, v_len: i32): i32;

// --- High-level Host Logging ---
export namespace log {
  export function debug(msg: string): void {
    const buf = String.UTF8.encode(msg);
    host_log(0, changetype<i32>(buf), buf.byteLength as i32);
  }

  export function info(msg: string): void {
    const buf = String.UTF8.encode(msg);
    host_log(1, changetype<i32>(buf), buf.byteLength as i32);
  }

  export function warn(msg: string): void {
    const buf = String.UTF8.encode(msg);
    host_log(2, changetype<i32>(buf), buf.byteLength as i32);
  }

  export function error(msg: string): void {
    const buf = String.UTF8.encode(msg);
    host_log(3, changetype<i32>(buf), buf.byteLength as i32);
  }
}

// --- High-level Host Network ---
export namespace net {
  export function isAllowed(url: string): bool {
    const buf = String.UTF8.encode(url);
    const res = host_http_check_allowed(changetype<i32>(buf), buf.byteLength as i32);
    return res === 1;
  }
}

// --- High-level Host Storage (KVS) ---
export namespace kv {
  export function set(key: string, value: string): bool {
    const kBuf = String.UTF8.encode(key);
    const vBuf = String.UTF8.encode(value);
    const res = host_kv_set(
      changetype<i32>(kBuf),
      kBuf.byteLength as i32,
      changetype<i32>(vBuf),
      vBuf.byteLength as i32
    );
    return res === 0;
  }
}

/**
 * AssemblyScript の実行時エラー（境界外アクセス・assert など）のハンドラー。
 * 既定のコンパイラ設定は `env.abort` をインポートするが、ホストは `host` モジュールしか
 * 提供しないため、asconfig.json の `"use": ["abort=assembly/sdk/host/abort"]` でこの関数に差し替える
 * （差し替えないとプラグインを読み込めない）。ログを出してから実行を中止（トラップ）する。
 */
export function abort(
  message: string | null,
  fileName: string | null,
  lineNumber: u32,
  columnNumber: u32,
): void {
  log.error(
    "abort: " +
      (message ? message : "") +
      " at " +
      (fileName ? fileName : "?") +
      ":" +
      lineNumber.toString() +
      ":" +
      columnNumber.toString(),
  );
  unreachable();
}
