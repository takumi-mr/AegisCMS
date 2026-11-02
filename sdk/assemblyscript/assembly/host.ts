/**
 * AegisCMS AssemblyScript Plugin SDK - Host Functions
 */

import { parseJson, quoteJson } from "./jsonvalue";

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

// @ts-ignore
@external("host", "kv_get")
declare function host_kv_get(k_ptr: i32, k_len: i32): i64;

// @ts-ignore
@external("host", "http_fetch")
declare function host_http_fetch(ptr: i32, len: i32): i64;

// @ts-ignore
@external("host", "config_get")
declare function host_config_get(k_ptr: i32, k_len: i32): i64;

/**
 * ホストが `allocate` で確保して書き込んだ結果 ((ptr << 32) | len) を文字列として読み、領域を解放する。
 * 0 以下（値なし・エラー）なら null
 */
function takeHostString(packed: i64): string | null {
  if (packed <= 0) return null;
  const ptr = (packed >> 32) as i32;
  const len = (packed & 0xffffffff) as i32;
  if (ptr == 0 || len <= 0) return null;
  const text = String.UTF8.decodeUnsafe(ptr as usize, len as usize);
  heap.free(ptr as usize);
  return text;
}

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

export namespace kv {
  /**
   * KVS から値を読む。未設定なら null。`capabilities.storage` が必要で、
   * 未宣言・ホストのエラーでも null を返す（警告ログを残す）
   */
  export function get(key: string): string | null {
    const kBuf = String.UTF8.encode(key);
    const packed = host_kv_get(changetype<i32>(kBuf), kBuf.byteLength as i32);
    if (packed == -1) {
      log.warn("KVS permission denied for key '" + key + "'");
      return null;
    }
    if (packed < 0) {
      log.warn("KVS error for key '" + key + "'");
      return null;
    }
    return takeHostString(packed);
  }
}

// --- High-level Host Config ---
export namespace config {
  /**
   * サーバーの `[plugins.settings.<プラグイン名>]` の値。未設定なら null。
   * 文字列以外の値（数値・配列など）は JSON 文字列で返る。空文字列も未設定として扱われる
   */
  export function get(key: string): string | null {
    const kBuf = String.UTF8.encode(key);
    return takeHostString(host_config_get(changetype<i32>(kBuf), kBuf.byteLength as i32));
  }

  export function getOr(key: string, fallback: string): string {
    const value = get(key);
    return value === null ? fallback : value!;
  }
}

// --- High-level Host HTTP ---
export class HttpRequest {
  method: string = "GET";
  url: string = "";
  headers: Map<string, string> = new Map<string, string>();
  body: string | null = null;

  constructor(url: string = "", method: string = "GET") {
    this.url = url;
    this.method = method;
  }

  static get(url: string): HttpRequest {
    return new HttpRequest(url, "GET");
  }

  /** JSON 本文の POST（`Content-Type: application/json`）。`json` は JSON のテキスト */
  static postJson(url: string, json: string): HttpRequest {
    const request = new HttpRequest(url, "POST");
    request.headers.set("Content-Type", "application/json");
    request.body = json;
    return request;
  }

  header(name: string, value: string): HttpRequest {
    this.headers.set(name, value);
    return this;
  }
}

export class HttpResponse {
  status: i32 = 0;
  headers: Map<string, string> = new Map<string, string>();
  body: string = "";
  /** 通信の失敗・拒否（許可されていないドメイン、タイムアウトなど）の理由。成功なら空 */
  error: string = "";

  /** 通信に成功したか（ステータスが 4xx / 5xx でも true） */
  get ok(): bool {
    return this.error.length == 0;
  }

  get isSuccess(): bool {
    return this.error.length == 0 && this.status >= 200 && this.status < 300;
  }
}

export namespace http {
  /**
   * 許可されたドメイン（`capabilities.network.allowed_domains`）へ HTTP リクエストを送る。
   * リダイレクトは追わない。1 回のフック呼び出しで 8 回まで、本文は送受信とも 1 MiB まで。
   * 通信の時間もフックの `execution_timeout_ms` に含まれる。失敗は `error` に入る
   */
  export function fetch(request: HttpRequest): HttpResponse {
    const response = new HttpResponse();
    const headerKeys = request.headers.keys();
    const headerParts = new Array<string>(headerKeys.length);
    for (let i = 0; i < headerKeys.length; ++i) {
      headerParts[i] =
        quoteJson(headerKeys[i]) + ":" + quoteJson(request.headers.get(headerKeys[i]));
    }
    let json =
      "{\"method\":" + quoteJson(request.method) +
      ",\"url\":" + quoteJson(request.url) +
      ",\"headers\":{" + headerParts.join(",") + "}";
    const body = request.body;
    if (body !== null) json += ",\"body\":" + quoteJson(body!);
    json += "}";

    const buf = String.UTF8.encode(json);
    const text = takeHostString(host_http_fetch(changetype<i32>(buf), buf.byteLength as i32));
    if (text === null) {
      response.error = "http_fetch returned nothing";
      return response;
    }
    const parsed = parseJson(text!);
    if (parsed === null) {
      response.error = "http_fetch returned invalid JSON";
      return response;
    }
    const value = parsed!;
    if (value.has("error")) {
      response.error = value.get("error").asString("http_fetch failed");
      return response;
    }
    response.status = value.get("status").asInt() as i32;
    response.body = value.get("body").asString();
    const headers = value.get("headers");
    if (headers.isObject()) {
      const names = headers.fields.keys();
      for (let i = 0; i < names.length; ++i) {
        response.headers.set(names[i], headers.fields.get(names[i]).asString());
      }
    }
    return response;
  }
}

/**
 * AssemblyScript の実行時エラー（境界外アクセス・assert など）のハンドラー。
 * 既定のコンパイラ設定は `env.abort` をインポートするが、ホストは `host` モジュールしか
 * 提供しないため、asconfig.json の `"use": ["abort=@aegiscms/plugin-sdk/assembly/host/abort"]` でこの関数に差し替える
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
