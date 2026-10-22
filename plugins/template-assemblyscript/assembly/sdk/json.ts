/**
 * AegisCMS AssemblyScript Plugin SDK - Lightweight JSON Serialization & Deserialization
 */

import { PostContext, BlockNode, PageRenderContext } from "./types";

export function escapeJson(str: string): string {
  let out = "";
  for (let i = 0; i < str.length; ++i) {
    const c = str.charAt(i);
    if (c == "\"") out += "\\\"";
    else if (c == "\\") out += "\\\\";
    else if (c == "\n") out += "\\n";
    else if (c == "\r") out += "\\r";
    else if (c == "\t") out += "\\t";
    else out += c;
  }
  return out;
}

export function findStringField(json: string, key: string): string {
  const pattern = "\"" + key + "\":\"";
  const idx = json.indexOf(pattern);
  if (idx == -1) return "";

  const start = idx + pattern.length;
  let end = start;
  let escaped = false;

  while (end < json.length) {
    const c = json.charAt(end);
    if (escaped) {
      escaped = false;
    } else if (c == "\\") {
      escaped = true;
    } else if (c == "\"") {
      break;
    }
    end++;
  }

  const raw = json.substring(start, end);
  // Unescape standard sequences
  let unescaped = "";
  let i = 0;
  while (i < raw.length) {
    if (raw.charAt(i) == "\\" && i + 1 < raw.length) {
      const next = raw.charAt(i + 1);
      if (next == "\"") unescaped += "\"";
      else if (next == "\\") unescaped += "\\";
      else if (next == "n") unescaped += "\n";
      else if (next == "r") unescaped += "\r";
      else if (next == "t") unescaped += "\t";
      else unescaped += next;
      i += 2;
    } else {
      unescaped += raw.charAt(i);
      i++;
    }
  }
  return unescaped;
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
  let attrs = "{";
  const keys = node.attributes.keys();
  for (let i = 0; i < keys.length; ++i) {
    if (i > 0) attrs += ",";
    const k = keys[i];
    const v = node.attributes.get(k);
    attrs += "\"" + escapeJson(k) + "\":\"" + escapeJson(v) + "\"";
  }
  attrs += "}";

  let childrenStr = "[";
  for (let i = 0; i < node.children.length; ++i) {
    if (i > 0) childrenStr += ",";
    childrenStr += serializeBlockNode(node.children[i]);
  }
  childrenStr += "]";

  let textVal = node.text !== null ? "\"" + escapeJson(node.text!) + "\"" : "null";

  return "{"
    + "\"name\":\"" + escapeJson(node.name) + "\","
    + "\"attributes\":" + attrs + ","
    + "\"children\":" + childrenStr + ","
    + "\"text\":" + textVal
    + "}";
}

export function serializeBlockNodes(blocks: Array<BlockNode>): string {
  let res = "[";
  for (let i = 0; i < blocks.length; ++i) {
    if (i > 0) res += ",";
    res += serializeBlockNode(blocks[i]);
  }
  res += "]";
  return res;
}

export function parsePageRenderContext(json: string): PageRenderContext {
  const ctx = new PageRenderContext();
  ctx.site_title = findStringField(json, "site_title");
  ctx.page_title = findStringField(json, "page_title");
  ctx.slug = findStringField(json, "slug");
  ctx.template_name = findStringField(json, "template_name");
  ctx.extra_css = findStringField(json, "extra_css");
  ctx.extra_head = findStringField(json, "extra_head");
  return ctx;
}

export function serializePageRenderContext(ctx: PageRenderContext): string {
  return "{"
    + "\"site_title\":\"" + escapeJson(ctx.site_title) + "\","
    + "\"page_title\":\"" + escapeJson(ctx.page_title) + "\","
    + "\"slug\":\"" + escapeJson(ctx.slug) + "\","
    + "\"template_name\":\"" + escapeJson(ctx.template_name) + "\","
    + "\"extra_css\":\"" + escapeJson(ctx.extra_css) + "\","
    + "\"extra_head\":\"" + escapeJson(ctx.extra_head) + "\","
    + "\"variables\":{}"
    + "}";
}
