/**
 * AegisCMS AssemblyScript Plugin SDK - Low-level ABI and Memory Protocol
 */

/**
 * ゲストメモリ領域を確保するアロケータ
 * ホストからエクスポート関数として直接呼び出されます。
 */
export function allocate(len: i32): i32 {
  if (len <= 0) return 0;
  return heap.alloc(len as usize) as i32;
}

/**
 * 確保されたメモリ領域を解放
 */
export function deallocate(ptr: i32): void {
  if (ptr != 0) {
    heap.free(ptr as usize);
  }
}

/**
 * ポインタと長さから UTF-8 文字列をデコード
 */
export function readString(ptr: i32, len: i32): string {
  if (ptr == 0 || len <= 0) return "";
  return String.UTF8.decodeUnsafe(ptr as usize, len as usize);
}

/**
 * 文字列をゲストメモリに書き込み、(ptr, len) をパックした i64 を返却
 */
export function writeString(str: string): i64 {
  const buf = String.UTF8.encode(str);
  const len = buf.byteLength as i32;
  if (len == 0) return 0;

  const ptr = allocate(len);
  if (ptr == 0) return 0;

  memory.copy(ptr as usize, changetype<usize>(buf), len as usize);
  return packPtrLen(ptr, len);
}

/**
 * ポインタ (上位32bit) と長さ (下位32bit) を 64bit 整数にパック
 */
export function packPtrLen(ptr: i32, len: i32): i64 {
  return ((ptr as i64) << 32) | ((len as i64) & 0xFFFFFFFF);
}

/**
 * フックの拒否・エラーをホストへ返却 ({"error": "message"})
 */
export function writeError(message: string): i64 {
  // JSON 文字列エスケープ処理
  let escaped = "";
  for (let i = 0; i < message.length; ++i) {
    const char = message.charAt(i);
    if (char == "\"") escaped += "\\\"";
    else if (char == "\\") escaped += "\\\\";
    else if (char == "\n") escaped += "\\n";
    else if (char == "\r") escaped += "\\r";
    else if (char == "\t") escaped += "\\t";
    else escaped += char;
  }
  const json = "{\"error\":\"" + escaped + "\"}";
  return writeString(json);
}
