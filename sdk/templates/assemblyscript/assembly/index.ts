/**
 * AegisCMS AssemblyScript Plugin Implementation Template
 */

import {
  allocate as sdkAllocate,
  readString,
  writeString,
  writeError,
  packPtrLen,
  log,
  net,
  kv,
  PostContext,
  BlockNode,
  PageRenderContext,
  parsePostContext,
  serializePostContext,
  appendBlockNodes,
  parsePageRenderContext,
  serializePageRenderContext,
} from "@aegiscms/plugin-sdk/assembly/index";

// --- Required Export: Memory Allocator ---
export function allocate(len: i32): i32 {
  return sdkAllocate(len);
}

// --- Lifecycle Hook: on_before_save ---
export function on_before_save(ptr: i32, len: i32): i64 {
  const inJson = readString(ptr, len);
  const post = parsePostContext(inJson);

  log.info("[AssemblyScript] on_before_save called for: " + post.title);

  // タイトル検証: 空の場合は保存を拒否 (422 Unprocessable Entity)
  if (post.title.trim().length == 0) {
    return writeError("Post title cannot be empty");
  }

  // 外部ネットワーク権限チェック (マニフェストで許可されたドメインか)
  if (net.isAllowed("https://api.example.com/webhook")) {
    log.info("[AssemblyScript] Network access to api.example.com is permitted");
  }

  // KVS への状態保存
  kv.set("last_saved_slug", post.slug);

  // タイトルにプラグイン処理済みの接頭辞を付与
  if (!post.title.startsWith("[AS-Plugin] ")) {
    post.title = "[AS-Plugin] " + post.title;
  }

  const outJson = serializePostContext(post);
  return writeString(outJson);
}

const SIGNATURE = "Transformed by AssemblyScript Plugin";

// --- Lifecycle Hook: transform_content_ast ---
export function transform_content_ast(ptr: i32, len: i32): i64 {
  const inJson = readString(ptr, len);
  log.info("[AssemblyScript] transform_content_ast called");

  // 保存のたびに呼ばれるため、追加済みなら何もしない（0 を返すと「変更なし」）
  if (inJson.includes(SIGNATURE)) return 0;

  // サンプル: 既存のブロックを残したまま、末尾にプラグイン署名ブロックを追加
  const blocks = new Array<BlockNode>();
  blocks.push(BlockNode.paragraph(SIGNATURE));

  return writeString(appendBlockNodes(inJson, blocks));
}

// --- Lifecycle Hook: on_render_page ---
export function on_render_page(ptr: i32, len: i32): i64 {
  const inJson = readString(ptr, len);
  const ctx = parsePageRenderContext(inJson);

  log.info("[AssemblyScript] on_render_page called for: " + ctx.site_title);

  // テーマ修飾
  ctx.extra_css +=
    "\n/* Injected by AssemblyScript Plugin */\n.as-badge { color: #2b7489; font-weight: bold; }";

  const outJson = serializePageRenderContext(ctx);
  return writeString(outJson);
}
