/**
 * AegisCMS AssemblyScript Plugin SDK - Hook input/output helpers
 *
 * フックのエクスポート関数 `(ptr: i32, len: i32): i64` の入出力を作るための関数。
 *
 * ```ts
 * export function calculate_order(ptr: i32, len: i32): i64 {
 *   const input = readHookJson(ptr, len);
 *   if (input === null) return writeError("Invalid OrderContext");
 *   const order = OrderContext.fromJson(input!);
 *   order.adjustments.push(new OrderAdjustment("shipping", "送料", 500));
 *   return writeOrderResult(order);
 * }
 * ```
 */

import { readString, writeString } from "./abi";
import { JsonValue, parseJson, quoteJson } from "./jsonvalue";
import { BlockNode } from "./types";
import { CommentVerdict, Delivery, OrderContext } from "./hook_types";

/** ホストが書き込んだ入力を JSON として読む。不正な JSON なら null */
export function readHookJson(ptr: i32, len: i32): JsonValue | null {
  return parseJson(readString(ptr, len));
}

/** 結果を返さないフック（`on-order-status-changed` / `on-content-published`）の成功 */
export function writeOk(): i64 {
  return 0;
}

/** JSON を結果として返す */
export function writeJson(value: JsonValue): i64 {
  return writeString(value.toString());
}

/** `calculate-order` の結果（`adjustments` と `metadata`）を返す */
export function writeOrderResult(order: OrderContext): i64 {
  return writeJson(order.toResultJson());
}

/** `check-comment` の審査結果を返す */
export function writeVerdict(verdict: CommentVerdict): i64 {
  return writeJson(verdict.toJson());
}

/**
 * `send-notification` の結果を返す。送ったら 0、扱わない通知は `{"skipped": true}`
 */
export function writeDelivery(delivery: Delivery): i64 {
  if (delivery == Delivery.Skipped) return writeString("{\"skipped\":true}");
  return 0;
}

/**
 * `send-notification` の失敗を返す。`permanent` が true なら再試行しても変わらない失敗
 * （設定の不備・相手の 4xx など）、false ならホストが間隔を空けて再試行する
 */
export function writeDeliveryError(message: string, permanent: bool = false): i64 {
  return writeString(
    "{\"error\":" + quoteJson(message) + ",\"permanent\":" + (permanent ? "true" : "false") + "}",
  );
}

/** `render-blocks` の入力 `{"blocks": [...]}` を読む。不正な入力なら null */
export function readBlocks(ptr: i32, len: i32): Array<BlockNode> | null {
  const input = readHookJson(ptr, len);
  if (input === null) return null;
  const list = input!.get("blocks");
  const blocks = new Array<BlockNode>();
  for (let i = 0; i < list.length; ++i) blocks.push(BlockNode.fromJson(list.at(i)));
  return blocks;
}

/**
 * `render-blocks` の結果を返す。入力と同じ数・同じ順の HTML で、null はそのブロックを出さない。
 * 返した HTML はホストがサニタイズする
 */
export function writeBlocksHtml(html: Array<string | null>): i64 {
  const parts = new Array<string>(html.length);
  for (let i = 0; i < html.length; ++i) {
    const item = html[i];
    parts[i] = item === null ? "null" : quoteJson(item!);
  }
  return writeString("{\"html\":[" + parts.join(",") + "]}");
}
