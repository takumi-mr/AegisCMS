/**
 * AegisCMS AssemblyScript example: commerce, notification, comment and block hooks
 *
 * ホスト API（`config` / `kv` / `http`）と、`calculate-order` / `on-order-status-changed` /
 * `send-notification` / `on-content-published` / `check-comment` / `render-blocks` の実装例。
 * ABI は docs/plugin_abi_specification.md を参照
 */

import {
  allocate as sdkAllocate,
  readString,
  writeString,
  writeError,
  log,
  kv,
  config,
  http,
  HttpRequest,
  JsonValue,
  readHookJson,
  readBlocks,
  writeOk,
  writeOrderResult,
  writeDelivery,
  writeDeliveryError,
  writeVerdict,
  writeBlocksHtml,
  parsePageRenderContext,
  serializePageRenderContext,
  InjectedScript,
  Delivery,
  OrderContext,
  OrderAdjustment,
  OrderStatusChange,
  PublishedContent,
  CommentSubmission,
  CommentVerdict,
  NotificationMessage,
} from "@takumi-mr/plugin-sdk/assembly/index";

// --- Required Export: Memory Allocator ---
export function allocate(len: i32): i32 {
  return sdkAllocate(len);
}

// --- calculate-order: 送料と割引 ---
// 設定: [plugins.settings.aegis-example-as-hooks] free_shipping_threshold（既定 5000）
export function calculate_order(ptr: i32, len: i32): i64 {
  const input = readHookJson(ptr, len);
  if (input === null) return writeError("Invalid OrderContext");
  const order = OrderContext.fromJson(input!);

  // 顧客に見せる理由でそのまま断れる
  if (order.coupon_code !== null && order.coupon_code! == "EXPIRED") {
    return writeError("This coupon has expired");
  }

  const threshold = I64.parseInt(config.getOr("free_shipping_threshold", "5000"));
  if (order.discountedSubtotal() < threshold) {
    order.adjustments.push(new OrderAdjustment("shipping", "送料", 500));
  }
  order.metadata.setString("calculated_by", "example-as-hooks");
  return writeOrderResult(order);
}

// --- on-order-status-changed: 件数を KVS に数え、Webhook に知らせる ---
export function on_order_status_changed(ptr: i32, len: i32): i64 {
  const input = readHookJson(ptr, len);
  if (input === null) return writeError("Invalid OrderStatusChange");
  const change = OrderStatusChange.fromJson(input!);

  const previous = kv.get("order_events");
  const count = (previous === null ? 0 : I64.parseInt(previous!)) + 1;
  kv.set("order_events", count.toString());

  const to = change.to_status === null ? "created" : change.to_status!;
  const body = JsonValue.object()
    .setString("text", "Order " + (change.order.order_number === null ? "?" : change.order.order_number!) + ": " + to);
  const response = http.fetch(HttpRequest.postJson("https://hooks.example.com/orders", body.toString()));
  if (!response.ok) return writeError("Webhook failed: " + response.error);
  log.info("[AssemblyScript] webhook answered " + response.status.toString());
  return writeOk();
}

// --- send-notification: 運営者向けの通知だけ Webhook に送る ---
export function send_notification(ptr: i32, len: i32): i64 {
  const input = readHookJson(ptr, len);
  if (input === null) return writeDeliveryError("Invalid NotificationMessage", true);
  const message = NotificationMessage.fromJson(input!);

  // 顧客宛ては扱わない（失敗ではない）
  if (message.recipient.type != "staff") return writeDelivery(Delivery.Skipped);

  const payload = JsonValue.object().setString("text", message.subject + "\n" + message.body);
  const response = http.fetch(HttpRequest.postJson("https://hooks.example.com/notify", payload.toString()));
  if (!response.ok) return writeDeliveryError(response.error, false);
  // 4xx は再試行しても変わらない
  if (response.status >= 400 && response.status < 500) {
    return writeDeliveryError("Rejected with " + response.status.toString(), true);
  }
  if (!response.isSuccess) return writeDeliveryError("Failed with " + response.status.toString(), false);
  return writeDelivery(Delivery.Sent);
}

// --- on-content-published: 公開を記録する ---
export function on_content_published(ptr: i32, len: i32): i64 {
  const input = readHookJson(ptr, len);
  if (input === null) return writeError("Invalid PublishedContent");
  const content = PublishedContent.fromJson(input!);
  kv.set("last_published", content.content_type + "/" + content.slug);
  log.info("[AssemblyScript] published: " + content.title);
  return writeOk();
}

// --- check-comment: リンクが多いコメントは承認待ち、NG ワードはスパム ---
export function check_comment(ptr: i32, len: i32): i64 {
  const input = readHookJson(ptr, len);
  if (input === null) return writeError("Invalid CommentSubmission");
  const comment = CommentSubmission.fromJson(input!);

  if (comment.body.trim().length == 0) return writeError("The comment is empty");
  if (comment.body.toLowerCase().includes("viagra")) {
    return writeVerdict(CommentVerdict.spam("blocked word"));
  }
  if (comment.body.split("http").length > 3) {
    return writeVerdict(CommentVerdict.hold("too many links"));
  }
  return writeVerdict(CommentVerdict.allow());
}

// --- render-blocks: <プラグイン名>/callout ブロックの HTML ---
export function render_blocks(ptr: i32, len: i32): i64 {
  const blocks = readBlocks(ptr, len);
  if (blocks === null) return writeError("Invalid render-blocks input");
  const html = new Array<string | null>();
  for (let i = 0; i < blocks!.length; ++i) {
    const block = blocks![i];
    if (block.attributes.has("message")) {
      html.push("<aside class=\"callout\">" + escapeHtml(block.attributes.get("message")) + "</aside>");
    } else {
      // 入力が足りないブロックは出さない
      html.push(null);
    }
  }
  return writeBlocksHtml(html);
}

// --- on-render-page: 本文の前後の枠・CSS・構造化したスクリプトの注入 (ABI 2) ---
export function on_render_page(ptr: i32, len: i32): i64 {
  const ctx = parsePageRenderContext(readString(ptr, len));
  ctx.extra_css += ".callout { border-left: 3px solid #2563eb; }";
  if (ctx.content_length > 0) {
    ctx.prepend_content = "<div class=\"example-banner\">Reading</div>";
    ctx.append_content = "<footer class=\"example-footer\">End</footer>";
  }
  const featured = ctx.queries.get("featured_count");
  if (featured.isArray() && featured.length > 0) {
    const n = featured.at(0).get("n").asInt();
    ctx.extra_head += "<meta name=\"featured-count\" content=\"" + n.toString() + "\">";
  }
  ctx.scripts.push(InjectedScript.inline("window.__aegisExample = 1;"));
  ctx.scripts.push(
    InjectedScript.external("https://cdn.example.com/analytics.js")
      .asyncLoad()
      .deferred()
      .withCrossorigin()
      .withConsent("analytics"),
  );
  return writeString(serializePageRenderContext(ctx));
}

function escapeHtml(text: string): string {
  return text.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;").replaceAll("\"", "&quot;");
}
