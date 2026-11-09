/**
 * AegisCMS AssemblyScript Plugin SDK - JSON value tree
 *
 * 注文・通知・コメントのようにネストしたフックの入出力を扱うための、小さな JSON のパーサーとシリアライザー。
 * `json.ts` の `findStringField` は 1 階層の文字列フィールドしか読めないので、配列やオブジェクトを含む入力はこちらで読む
 */

export const JSON_NULL: i32 = 0;
export const JSON_BOOL: i32 = 1;
export const JSON_NUMBER: i32 = 2;
export const JSON_STRING: i32 = 3;
export const JSON_ARRAY: i32 = 4;
export const JSON_OBJECT: i32 = 5;

/** これより深くネストした入力は不正として扱う（再帰によるスタック枯渇を避ける） */
const MAX_DEPTH: i32 = 64;

export class JsonValue {
  kind: i32 = JSON_NULL;
  boolValue: bool = false;
  numberValue: f64 = 0;
  stringValue: string = "";
  items: Array<JsonValue> = new Array<JsonValue>();
  fields: Map<string, JsonValue> = new Map<string, JsonValue>();

  static null(): JsonValue {
    return new JsonValue();
  }

  static fromBool(value: bool): JsonValue {
    const v = new JsonValue();
    v.kind = JSON_BOOL;
    v.boolValue = value;
    return v;
  }

  static fromNumber(value: f64): JsonValue {
    const v = new JsonValue();
    v.kind = JSON_NUMBER;
    v.numberValue = value;
    return v;
  }

  static fromInt(value: i64): JsonValue {
    return JsonValue.fromNumber(value as f64);
  }

  static fromString(value: string): JsonValue {
    const v = new JsonValue();
    v.kind = JSON_STRING;
    v.stringValue = value;
    return v;
  }

  /** 文字列、`null` は JSON の null */
  static fromNullableString(value: string | null): JsonValue {
    return value === null ? JsonValue.null() : JsonValue.fromString(value);
  }

  static object(): JsonValue {
    const v = new JsonValue();
    v.kind = JSON_OBJECT;
    return v;
  }

  static array(): JsonValue {
    const v = new JsonValue();
    v.kind = JSON_ARRAY;
    return v;
  }

  isNull(): bool {
    return this.kind == JSON_NULL;
  }

  isObject(): bool {
    return this.kind == JSON_OBJECT;
  }

  isArray(): bool {
    return this.kind == JSON_ARRAY;
  }

  /** オブジェクトのフィールド。無い（またはオブジェクトでない）なら null の値 */
  get(key: string): JsonValue {
    if (this.kind == JSON_OBJECT && this.fields.has(key)) return this.fields.get(key);
    return JsonValue.null();
  }

  has(key: string): bool {
    return this.kind == JSON_OBJECT && this.fields.has(key);
  }

  /** 配列の要素。範囲外（または配列でない）なら null の値 */
  at(index: i32): JsonValue {
    if (this.kind == JSON_ARRAY && index >= 0 && index < this.items.length) return this.items[index];
    return JsonValue.null();
  }

  /** 配列の要素数（オブジェクトならフィールド数、それ以外は 0） */
  get length(): i32 {
    if (this.kind == JSON_ARRAY) return this.items.length;
    if (this.kind == JSON_OBJECT) return this.fields.size;
    return 0;
  }

  asString(fallback: string = ""): string {
    return this.kind == JSON_STRING ? this.stringValue : fallback;
  }

  /** 文字列なら値、null や他の型なら null */
  asNullableString(): string | null {
    return this.kind == JSON_STRING ? this.stringValue : null;
  }

  asInt(fallback: i64 = 0): i64 {
    return this.kind == JSON_NUMBER ? (this.numberValue as i64) : fallback;
  }

  asNumber(fallback: f64 = 0): f64 {
    return this.kind == JSON_NUMBER ? this.numberValue : fallback;
  }

  asBool(fallback: bool = false): bool {
    return this.kind == JSON_BOOL ? this.boolValue : fallback;
  }

  /** オブジェクトにフィールドを設定する（自身を返す） */
  set(key: string, value: JsonValue): JsonValue {
    this.fields.set(key, value);
    return this;
  }

  setString(key: string, value: string): JsonValue {
    return this.set(key, JsonValue.fromString(value));
  }

  setInt(key: string, value: i64): JsonValue {
    return this.set(key, JsonValue.fromInt(value));
  }

  /** 配列に要素を足す（自身を返す） */
  push(value: JsonValue): JsonValue {
    this.items.push(value);
    return this;
  }

  /** 文字列はそのまま、数値・真偽値・null などは JSON 表記にする（ブロックの属性を文字列で扱うときに使う） */
  asText(): string {
    return this.kind == JSON_STRING ? this.stringValue : this.toString();
  }

  toString(): string {
    if (this.kind == JSON_BOOL) return this.boolValue ? "true" : "false";
    if (this.kind == JSON_NUMBER) return formatNumber(this.numberValue);
    if (this.kind == JSON_STRING) return quoteJson(this.stringValue);
    if (this.kind == JSON_ARRAY) {
      const parts = new Array<string>(this.items.length);
      for (let i = 0; i < parts.length; ++i) parts[i] = this.items[i].toString();
      return "[" + parts.join(",") + "]";
    }
    if (this.kind == JSON_OBJECT) {
      const keys = this.fields.keys();
      const parts = new Array<string>(keys.length);
      for (let i = 0; i < parts.length; ++i) {
        parts[i] = quoteJson(keys[i]) + ":" + this.fields.get(keys[i]).toString();
      }
      return "{" + parts.join(",") + "}";
    }
    return "null";
  }
}

function formatNumber(value: f64): string {
  // 金額・件数は整数。2^53 未満の整数は小数点なしで出す
  if (value == Math.floor(value) && Math.abs(value) < 9007199254740992.0) {
    return (value as i64).toString();
  }
  return value.toString();
}

const HEX: string = "0123456789abcdef";

/** JSON の文字列リテラルにする（`"` `\` と制御文字をすべてエスケープする） */
export function quoteJson(str: string): string {
  const parts = new Array<string>();
  let start = 0;
  for (let i = 0; i < str.length; ++i) {
    const c = str.charCodeAt(i);
    if (c >= 0x20 && c != 0x22 && c != 0x5c) continue;
    if (i > start) parts.push(str.substring(start, i));
    start = i + 1;
    if (c == 0x22) parts.push("\\\"");
    else if (c == 0x5c) parts.push("\\\\");
    else if (c == 0x0a) parts.push("\\n");
    else if (c == 0x0d) parts.push("\\r");
    else if (c == 0x09) parts.push("\\t");
    else {
      parts.push("\\u00" + HEX.charAt((c >> 4) & 0xf) + HEX.charAt(c & 0xf));
    }
  }
  if (parts.length == 0) return "\"" + str + "\"";
  if (start < str.length) parts.push(str.substring(start));
  return "\"" + parts.join("") + "\"";
}

class JsonParser {
  pos: i32 = 0;
  failed: bool = false;

  constructor(public text: string) {}

  fail(): JsonValue {
    this.failed = true;
    return JsonValue.null();
  }

  skipWhitespace(): void {
    while (this.pos < this.text.length) {
      const c = this.text.charCodeAt(this.pos);
      if (c == 0x20 || c == 0x0a || c == 0x0d || c == 0x09) this.pos++;
      else break;
    }
  }

  parseValue(depth: i32): JsonValue {
    if (depth > MAX_DEPTH) return this.fail();
    this.skipWhitespace();
    if (this.pos >= this.text.length) return this.fail();
    const c = this.text.charCodeAt(this.pos);
    if (c == 0x7b) return this.parseObject(depth);
    if (c == 0x5b) return this.parseArray(depth);
    if (c == 0x22) {
      const s = this.parseString();
      return this.failed ? JsonValue.null() : JsonValue.fromString(s);
    }
    if (this.matchWord("true")) return JsonValue.fromBool(true);
    if (this.matchWord("false")) return JsonValue.fromBool(false);
    if (this.matchWord("null")) return JsonValue.null();
    if (c == 0x2d || (c >= 0x30 && c <= 0x39)) return this.parseNumber();
    return this.fail();
  }

  matchWord(word: string): bool {
    if (this.text.startsWith(word, this.pos)) {
      this.pos += word.length;
      return true;
    }
    return false;
  }

  parseNumber(): JsonValue {
    const start = this.pos;
    while (this.pos < this.text.length) {
      const c = this.text.charCodeAt(this.pos);
      const isNumberChar =
        (c >= 0x30 && c <= 0x39) || c == 0x2d || c == 0x2b || c == 0x2e || c == 0x65 || c == 0x45;
      if (!isNumberChar) break;
      this.pos++;
    }
    const value = parseFloat(this.text.substring(start, this.pos));
    if (isNaN(value)) return this.fail();
    return JsonValue.fromNumber(value);
  }

  /** `"` で始まる位置から文字列を読む */
  parseString(): string {
    this.pos++; // 開きの "
    const text = this.text;
    let start = this.pos;
    let parts: Array<string> | null = null;
    while (this.pos < text.length) {
      const c = text.charCodeAt(this.pos);
      if (c == 0x22) {
        const tail = text.substring(start, this.pos);
        this.pos++;
        if (parts === null) return tail;
        parts.push(tail);
        return parts.join("");
      }
      if (c == 0x5c) {
        if (parts === null) parts = new Array<string>();
        parts.push(text.substring(start, this.pos));
        this.pos++;
        if (this.pos >= text.length) break;
        const e = text.charCodeAt(this.pos);
        this.pos++;
        if (e == 0x6e) parts.push("\n");
        else if (e == 0x72) parts.push("\r");
        else if (e == 0x74) parts.push("\t");
        else if (e == 0x62) parts.push("\b");
        else if (e == 0x66) parts.push("\f");
        else if (e == 0x75) {
          if (this.pos + 4 > text.length) break;
          const code = I32.parseInt(text.substring(this.pos, this.pos + 4), 16);
          this.pos += 4;
          parts.push(String.fromCharCode(code));
        } else parts.push(String.fromCharCode(e)); // \" \\ \/
        start = this.pos;
        continue;
      }
      this.pos++;
    }
    this.failed = true;
    return "";
  }

  parseArray(depth: i32): JsonValue {
    const result = JsonValue.array();
    this.pos++; // [
    this.skipWhitespace();
    if (this.pos < this.text.length && this.text.charCodeAt(this.pos) == 0x5d) {
      this.pos++;
      return result;
    }
    while (!this.failed) {
      result.items.push(this.parseValue(depth + 1));
      this.skipWhitespace();
      if (this.pos >= this.text.length) return this.fail();
      const c = this.text.charCodeAt(this.pos);
      this.pos++;
      if (c == 0x5d) return result;
      if (c != 0x2c) return this.fail();
    }
    return result;
  }

  parseObject(depth: i32): JsonValue {
    const result = JsonValue.object();
    this.pos++; // {
    this.skipWhitespace();
    if (this.pos < this.text.length && this.text.charCodeAt(this.pos) == 0x7d) {
      this.pos++;
      return result;
    }
    while (!this.failed) {
      this.skipWhitespace();
      if (this.pos >= this.text.length || this.text.charCodeAt(this.pos) != 0x22) return this.fail();
      const key = this.parseString();
      this.skipWhitespace();
      if (this.failed || this.pos >= this.text.length || this.text.charCodeAt(this.pos) != 0x3a) {
        return this.fail();
      }
      this.pos++;
      result.fields.set(key, this.parseValue(depth + 1));
      this.skipWhitespace();
      if (this.pos >= this.text.length) return this.fail();
      const c = this.text.charCodeAt(this.pos);
      this.pos++;
      if (c == 0x7d) return result;
      if (c != 0x2c) return this.fail();
    }
    return result;
  }
}

/** JSON を読む。不正な JSON なら null */
export function parseJson(text: string): JsonValue | null {
  const parser = new JsonParser(text);
  const value = parser.parseValue(0);
  if (parser.failed) return null;
  parser.skipWhitespace();
  if (parser.pos != text.length) return null;
  return value;
}

/** `text` の `startPos` から始まる 1 つの JSON 値を読む（後ろに続く文字は無視する）。不正なら null */
export function parseJsonAt(text: string, startPos: i32): JsonValue | null {
  if (startPos < 0 || startPos >= text.length) return null;
  const parser = new JsonParser(text);
  parser.pos = startPos;
  const value = parser.parseValue(0);
  if (parser.failed) return null;
  return value;
}
