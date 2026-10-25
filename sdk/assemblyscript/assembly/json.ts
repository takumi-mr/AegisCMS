/**
 * AegisCMS AssemblyScript Plugin SDK - Lightweight JSON Serialization & Deserialization
 */

import { PostContext, BlockNode, PageRenderContext } from "./types";

/**
 * 1 文字ずつ `+=` で連結すると入力の大きさの二乗になる。区間ごとに `substring` して `join` しても
 * 確保の回数がエスケープの数に比例し、Wasmtime 上では 1 回の確保が重い（`blocks_json` は
 * JSON の中の JSON でエスケープが多い）ため、結果の長さを先に数えて 1 回だけ確保し、直接書き込む
 */
export function escapeJson(str: string): string {
  const len = str.length;
  let extra = 0;
  for (let i = 0; i < len; ++i) {
    if (escapeChar(str.charCodeAt(i)) != 0) extra++;
  }
  if (extra == 0) return str;

  const out = changetype<string>(__new((len + extra) << 1, idof<string>()));
  const src = changetype<usize>(str);
  let dst = changetype<usize>(out);
  for (let i = 0; i < len; ++i) {
    const c = load<u16>(src + ((i as usize) << 1));
    const e = escapeChar(c);
    if (e != 0) {
      store<u16>(dst, 0x5c);
      store<u16>(dst, e, 2);
      dst += 4;
    } else {
      store<u16>(dst, c);
      dst += 2;
    }
  }
  return out;
}

/** エスケープが要る文字なら `\` の後ろに置く文字、要らなければ 0 */
// @ts-ignore: decorator
@inline
function escapeChar(c: i32): u16 {
  if (c == 0x22) return 0x22; // "
  if (c == 0x5c) return 0x5c; // \
  if (c == 0x0a) return 0x6e; // \n
  if (c == 0x0d) return 0x72; // \r
  if (c == 0x09) return 0x74; // \t
  return 0;
}

export function findStringField(json: string, key: string): string {
  const pattern = "\"" + key + "\":\"";
  const idx = json.indexOf(pattern);
  if (idx == -1) return "";

  const start = idx + pattern.length;
  let end = start;
  let escaped = false;
  let pairs = 0;

  while (end < json.length) {
    const c = json.charCodeAt(end);
    if (escaped) {
      escaped = false;
    } else if (c == 0x5c) {
      escaped = true;
      pairs++;
    } else if (c == 0x22) {
      break;
    }
    end++;
  }

  if (pairs == 0) return json.substring(start, end);
  return unescapeJson(json, start, end, pairs);
}

/**
 * `json[start, end)` の標準のエスケープシーケンスを戻す。`pairs` は `\` で始まる組の数
 * （`escapeJson` と同じく 1 回だけ確保して直接書き込む）
 */
function unescapeJson(json: string, start: i32, end: i32, pairs: i32): string {
  // 末尾が `\` だけで終わる（閉じの `"` が無い）入力では、最後の `\` はそのまま残す
  if (json.charCodeAt(end - 1) == 0x5c && end == json.length) {
    let n = 0;
    for (let i = end - 1; i >= start && json.charCodeAt(i) == 0x5c; --i) n++;
    if (n & 1) pairs--;
  }
  const out = changetype<string>(__new((end - start - pairs) << 1, idof<string>()));
  const src = changetype<usize>(json);
  let dst = changetype<usize>(out);
  let i = start;
  while (i < end) {
    let c = load<u16>(src + ((i as usize) << 1));
    if (c == 0x5c && i + 1 < end) {
      const next = load<u16>(src + ((i as usize) << 1), 2);
      if (next == 0x6e) c = 0x0a;
      else if (next == 0x72) c = 0x0d;
      else if (next == 0x74) c = 0x09;
      else c = next;
      i += 2;
    } else {
      i++;
    }
    store<u16>(dst, c);
    dst += 2;
  }
  return out;
}

export function findIntField(json: string, key: string): i64 {
  const pattern = "\"" + key + "\":";
  const idx = json.indexOf(pattern);
  if (idx == -1) return 0;

  let start = idx + pattern.length;
  while (start < json.length && (json.charAt(start) == " " || json.charAt(start) == "\t")) {
    start++;
  }

  let end = start;
  while (end < json.length) {
    const c = json.charAt(end);
    if ((c >= "0" && c <= "9") || c == "-") {
      end++;
    } else {
      break;
    }
  }

  const numStr = json.substring(start, end);
  return I64.parseInt(numStr.length > 0 ? numStr : "0");
}

export function parsePostContext(json: string): PostContext {
  const post = new PostContext();
  post.id = findStringField(json, "id");
  post.title = findStringField(json, "title");
  post.slug = findStringField(json, "slug");
  post.content_type = findStringField(json, "content_type");
  post.status = findStringField(json, "status");
  post.author_id = findStringField(json, "author_id");
  post.blocks_json = findStringField(json, "blocks_json");
  post.created_at = findIntField(json, "created_at");
  post.updated_at = findIntField(json, "updated_at");
  return post;
}

export function serializePostContext(post: PostContext): string {
  return "{"
    + "\"id\":\"" + escapeJson(post.id) + "\","
    + "\"title\":\"" + escapeJson(post.title) + "\","
    + "\"slug\":\"" + escapeJson(post.slug) + "\","
    + "\"content_type\":\"" + escapeJson(post.content_type) + "\","
    + "\"status\":\"" + escapeJson(post.status) + "\","
    + "\"author_id\":\"" + escapeJson(post.author_id) + "\","
    + "\"blocks_json\":\"" + escapeJson(post.blocks_json) + "\","
    + "\"created_at\":" + post.created_at.toString() + ","
    + "\"updated_at\":" + post.updated_at.toString()
    + "}";
}

export function serializeBlockNode(node: BlockNode): string {
  const keys = node.attributes.keys();
  const attrParts = new Array<string>(keys.length);
  for (let i = 0; i < keys.length; ++i) {
    const k = keys[i];
    const v = node.attributes.get(k);
    attrParts[i] = "\"" + escapeJson(k) + "\":\"" + escapeJson(v) + "\"";
  }
  const attrs = "{" + attrParts.join(",") + "}";

  const childrenStr = serializeBlockNodes(node.children);

  let textVal = node.text !== null ? "\"" + escapeJson(node.text!) + "\"" : "null";

  return "{"
    + "\"name\":\"" + escapeJson(node.name) + "\","
    + "\"attributes\":" + attrs + ","
    + "\"children\":" + childrenStr + ","
    + "\"text\":" + textVal
    + "}";
}

export function serializeBlockNodes(blocks: Array<BlockNode>): string {
  const items = new Array<string>(blocks.length);
  for (let i = 0; i < blocks.length; ++i) {
    items[i] = serializeBlockNode(blocks[i]);
  }
  return "[" + items.join(",") + "]";
}

/**
 * ブロック配列の JSON（`transform_content_ast` の入力）の末尾にブロックを追加した JSON を返す。
 * 既存のブロックはパースせずにそのまま残す
 */
export function appendBlockNodes(blocksJson: string, blocks: Array<BlockNode>): string {
  const trimmed = blocksJson.trim();
  const end = trimmed.lastIndexOf("]");
  if (!trimmed.startsWith("[") || end < 0) return serializeBlockNodes(blocks);
  const items = serializeBlockNodes(blocks);
  const added = items.substring(1, items.length - 1);
  if (added.length == 0) return trimmed;
  const existing = trimmed.substring(1, end).trim();
  return "[" + existing + (existing.length > 0 ? "," : "") + added + "]";
}

export function parsePageRenderContext(json: string): PageRenderContext {
  const ctx = new PageRenderContext();
  ctx.site_title = findStringField(json, "site_title");
  ctx.site_description = findStringField(json, "site_description");
  ctx.template = findStringField(json, "template");
  ctx.content_type = findStringField(json, "content_type");
  ctx.title = findStringField(json, "title");
  ctx.slug = findStringField(json, "slug");
  ctx.extra_css = findStringField(json, "extra_css");
  ctx.extra_head = findStringField(json, "extra_head");
  return ctx;
}

/** 返したフィールドだけがホスト側で上書きされる（docs/plugin_abi_specification.md） */
export function serializePageRenderContext(ctx: PageRenderContext): string {
  return "{"
    + "\"site_title\":\"" + escapeJson(ctx.site_title) + "\","
    + "\"site_description\":\"" + escapeJson(ctx.site_description) + "\","
    + "\"template\":\"" + escapeJson(ctx.template) + "\","
    + "\"content_type\":\"" + escapeJson(ctx.content_type) + "\","
    + "\"title\":\"" + escapeJson(ctx.title) + "\","
    + "\"slug\":\"" + escapeJson(ctx.slug) + "\","
    + "\"extra_css\":\"" + escapeJson(ctx.extra_css) + "\","
    + "\"extra_head\":\"" + escapeJson(ctx.extra_head) + "\""
    + "}";
}
