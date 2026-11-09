// AegisCMS TinyGo example: commerce, notification, comment and block hooks
//
// ホスト API (ConfigGet / KVGet / HTTPFetch) と、calculate-order / on-order-status-changed /
// send-notification / on-content-published / check-comment / render-blocks の実装例。
// ABI は docs/plugin_abi_specification.md を参照
package main

import (
	"strconv"
	"strings"

	"github.com/takumi-mr/AegisCMS/sdk/tinygo"
)

var htmlEscaper = strings.NewReplacer("&", "&amp;", "<", "&lt;", ">", "&gt;", "\"", "&quot;")

//export allocate
func allocate(size int32) uintptr {
	return sdk.Allocate(size)
}

// calculate-order: 送料と割引
// 設定: [plugins.settings.example-tinygo-hooks] free_shipping_threshold (既定 5000)
//
//export calculate_order
func calculate_order(ptr int32, length int32) int64 {
	return sdk.RunHook(ptr, length, "OrderContext", func(order *sdk.OrderContext) error {
		// 顧客に見せる理由でそのまま断れる
		if order.CouponCode != nil && *order.CouponCode == "EXPIRED" {
			return sdk.Reject("This coupon has expired")
		}

		threshold, err := strconv.ParseInt(sdk.ConfigGetOr("free_shipping_threshold", "5000"), 10, 64)
		if err != nil {
			threshold = 5000
		}
		if order.DiscountedSubtotal() < threshold {
			order.Adjustments = append(order.Adjustments, sdk.NewOrderAdjustment("shipping", "送料", 500))
		}
		if order.Metadata == nil {
			order.Metadata = map[string]interface{}{}
		}
		order.Metadata["calculated_by"] = "example-tinygo-hooks"
		return nil
	})
}

// on-order-status-changed: 件数を KVS に数え、Webhook に知らせる
//
//export on_order_status_changed
func on_order_status_changed(ptr int32, length int32) int64 {
	return sdk.RunNotificationHook(ptr, length, "OrderStatusChange", func(change *sdk.OrderStatusChange) error {
		count := int64(0)
		if previous, found, err := sdk.KVGet("order_events"); err == nil && found {
			count, _ = strconv.ParseInt(previous, 10, 64)
		}
		sdk.KVSet("order_events", strconv.FormatInt(count+1, 10))

		to := "created"
		if change.ToStatus != nil {
			to = *change.ToStatus
		}
		number := "?"
		if change.Order.OrderNumber != nil {
			number = *change.Order.OrderNumber
		}
		request, err := sdk.NewHTTPPostJSON("https://hooks.example.com/orders", map[string]string{
			"text": "Order " + number + ": " + to,
		})
		if err != nil {
			return err
		}
		response, err := sdk.HTTPFetch(request)
		if err != nil {
			return &sdk.HookError{Message: "Webhook failed: " + err.Error()}
		}
		sdk.LogInfo("[TinyGo] webhook answered " + strconv.Itoa(response.Status))
		return nil
	})
}

// send-notification: 運営者向けの通知だけ Webhook に送る
//
//export send_notification
func send_notification(ptr int32, length int32) int64 {
	return sdk.RunDeliveryHook(ptr, length, func(message *sdk.NotificationMessage) (sdk.Delivery, error) {
		// 顧客宛ては扱わない (失敗ではない)
		if message.Recipient.Type != "staff" {
			return sdk.DeliverySkipped, nil
		}

		request, err := sdk.NewHTTPPostJSON("https://hooks.example.com/notify", map[string]string{
			"text": message.Subject + "\n" + message.Body,
		})
		if err != nil {
			return sdk.DeliverySent, err
		}
		response, err := sdk.HTTPFetch(request)
		if err != nil {
			return sdk.DeliverySent, err // 通信の失敗は再試行される
		}
		// 4xx は再試行しても変わらない
		if response.Status >= 400 && response.Status < 500 {
			return sdk.DeliverySent, sdk.Reject("Rejected with " + strconv.Itoa(response.Status))
		}
		if !response.IsSuccess() {
			return sdk.DeliverySent, &sdk.HookError{Message: "Failed with " + strconv.Itoa(response.Status)}
		}
		return sdk.DeliverySent, nil
	})
}

// on-content-published: 公開を記録する
//
//export on_content_published
func on_content_published(ptr int32, length int32) int64 {
	return sdk.RunNotificationHook(ptr, length, "PublishedContent", func(content *sdk.PublishedContent) error {
		sdk.KVSet("last_published", content.ContentType+"/"+content.Slug)
		sdk.LogInfo("[TinyGo] published: " + content.Title)
		return nil
	})
}

// check-comment: リンクが多いコメントは承認待ち、NG ワードはスパム
//
//export check_comment
func check_comment(ptr int32, length int32) int64 {
	return sdk.RunCheckCommentHook(ptr, length, func(comment *sdk.CommentSubmission) (sdk.CommentVerdict, error) {
		if strings.TrimSpace(comment.Body) == "" {
			return sdk.CommentVerdict{}, sdk.Reject("The comment is empty")
		}
		if strings.Contains(strings.ToLower(comment.Body), "viagra") {
			return sdk.SpamComment("blocked word"), nil
		}
		if strings.Count(comment.Body, "http") > 2 {
			return sdk.HoldComment("too many links"), nil
		}
		return sdk.AllowComment(), nil
	})
}

// render-blocks: <プラグイン名>/callout ブロックの HTML
//
//export render_blocks
func render_blocks(ptr int32, length int32) int64 {
	return sdk.RunRenderBlocksHook(ptr, length, func(blocks []sdk.BlockNode) ([]*string, error) {
		html := make([]*string, len(blocks))
		for i, block := range blocks {
			// 入力が足りないブロックは出さない (nil)
			if message, ok := block.Attributes["message"].(string); ok {
				html[i] = sdk.HTML("<aside class=\"callout\">" + htmlEscaper.Replace(message) + "</aside>")
			}
		}
		return html, nil
	})
}

// on-render-page: 本文の前後の枠・CSS・構造化したスクリプトの注入 (ABI 2)
//
//export on_render_page
func on_render_page(ptr int32, length int32) int64 {
	return sdk.RunHook(ptr, length, "PageRenderContext", func(ctx *sdk.PageRenderContext) error {
		ctx.ExtraCSS += ".callout { border-left: 3px solid #2563eb; }"
		if ctx.ContentLength > 0 {
			ctx.PrependContent = "<div class=\"example-banner\">Reading</div>"
			ctx.AppendContent = "<footer class=\"example-footer\">End</footer>"
		}
		if rows, ok := ctx.Queries["featured_count"].([]interface{}); ok && len(rows) > 0 {
			if row, ok := rows[0].(map[string]interface{}); ok {
				if n, ok := row["n"].(float64); ok {
					ctx.ExtraHead += `<meta name="featured-count" content="` + strconv.FormatInt(int64(n), 10) + `">`
				}
			}
		}
		ctx.Scripts = append(
			ctx.Scripts,
			sdk.InlineScript("window.__aegisExample = 1;"),
			sdk.ExternalScript("https://cdn.example.com/analytics.js").
				Async().
				Deferred().
				WithCrossorigin().
				WithConsent("analytics"),
		)
		return nil
	})
}

// main は TinyGo Wasm ターゲットで必要とされる空のエントリポイントです
func main() {}
