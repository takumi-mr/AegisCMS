//! 公式プラグイン: 送料・クーポン・税（`calculate-order`）。
//!
//! 設定（サーバーの `[plugins.settings.official-shop-rules]`、金額は通貨の最小単位）:
//!
//! | キー | 内容 |
//! | --- | --- |
//! | `shipping_fee` | 送料（0 または未設定なら送料の行を付けない） |
//! | `free_shipping_over` | この金額以上（割引後の商品の合計）で送料無料（0 なら無し） |
//! | `shipping_label` | 送料の表示名（既定 `Shipping`） |
//! | `coupons` | `CODE:10%` / `CODE:500` をカンマ区切り。`CODE:10%:100` のように 3 つ目で利用回数の上限 |
//! | `tax_rate_percent` | 税率（%）。0 または未設定なら価格は税込みとして税の行を付けない |
//! | `tax_label` | 税の表示名（既定 `Tax`） |
//!
//! 利用回数の上限のあるクーポンは、支払い済みになった注文の数を KV（`coupon:<CODE>:uses`）に数える

use aegis_plugin_sdk::prelude::*;

#[derive(Default)]
pub struct ShopRulesPlugin;

#[derive(Debug, Clone, PartialEq)]
enum Discount {
    Percent(i64),
    Amount(i64),
}

#[derive(Debug, Clone, PartialEq)]
struct Coupon {
    code: String,
    discount: Discount,
    max_uses: Option<u64>,
}

/// `CODE:10%,OTHER:500:20` を読む（読めない項目は無視して警告する）
fn parse_coupons(setting: &str) -> Vec<Coupon> {
    setting
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| {
            let mut parts = entry.split(':').map(str::trim);
            let code = parts.next()?.to_ascii_uppercase();
            let value = parts.next()?;
            let discount = match value.strip_suffix('%') {
                Some(percent) => {
                    Discount::Percent(percent.parse().ok().filter(|p| (1..=100).contains(p))?)
                }
                None => Discount::Amount(value.parse().ok().filter(|a: &i64| *a > 0)?),
            };
            let max_uses = match parts.next() {
                Some(limit) => Some(limit.parse().ok()?),
                None => None,
            };
            Some(Coupon {
                code,
                discount,
                max_uses,
            })
        })
        .collect()
}

fn coupons() -> Vec<Coupon> {
    let setting = config::get("coupons").unwrap_or_default();
    let parsed = parse_coupons(&setting);
    let entries = setting.split(',').filter(|e| !e.trim().is_empty()).count();
    if parsed.len() != entries {
        log_warn!(
            "Some entries of the 'coupons' setting could not be read: {}",
            setting
        );
    }
    parsed
}

fn uses_key(code: &str) -> String {
    format!("coupon:{}:uses", code)
}

fn uses(code: &str) -> u64 {
    kv::get(&uses_key(code))
        .ok()
        .flatten()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

impl Plugin for ShopRulesPlugin {
    fn calculate_order(&mut self, mut ctx: OrderContext) -> Result<OrderContext> {
        // 1. クーポン
        if let Some(code) = ctx.coupon_code.clone() {
            let code = code.trim().to_ascii_uppercase();
            let coupon = coupons()
                .into_iter()
                .find(|c| c.code == code)
                .ok_or_else(|| {
                    PluginError::Rejected(format!("Coupon code \"{}\" is not valid", code))
                })?;
            if let Some(max) = coupon.max_uses {
                if uses(&code) >= max {
                    return Err(PluginError::Rejected(format!(
                        "Coupon code \"{}\" has been used up",
                        code
                    )));
                }
            }
            let amount = match coupon.discount {
                Discount::Percent(percent) => ctx.subtotal * percent / 100,
                Discount::Amount(amount) => amount.min(ctx.subtotal),
            };
            let mut discount =
                OrderAdjustment::new("discount", format!("Coupon {}", code), -amount);
            discount.code = Some(code);
            ctx.adjustments.push(discount);
        }

        // 2. 送料
        let fee: i64 = config::get_or("shipping_fee", 0);
        let mut shipping = 0;
        if fee > 0 {
            let threshold: i64 = config::get_or("free_shipping_over", 0);
            let label = config::get("shipping_label").unwrap_or_else(|| "Shipping".to_string());
            if threshold > 0 && ctx.discounted_subtotal() >= threshold {
                ctx.adjustments.push(OrderAdjustment::new(
                    "shipping",
                    format!("{} (free)", label),
                    0,
                ));
            } else {
                shipping = fee;
                ctx.adjustments
                    .push(OrderAdjustment::new("shipping", label, fee));
            }
        }

        // 3. 税（価格が税抜きの場合）
        let rate: i64 = config::get_or("tax_rate_percent", 0);
        if rate > 0 {
            let base = (ctx.discounted_subtotal() + shipping).max(0);
            let label = config::get("tax_label").unwrap_or_else(|| "Tax".to_string());
            ctx.adjustments.push(OrderAdjustment::new(
                "tax",
                format!("{} ({}%)", label, rate),
                base * rate / 100,
            ));
        }
        Ok(ctx)
    }

    fn on_order_status_changed(&mut self, change: OrderStatusChange) -> Result<()> {
        // 利用回数の上限のあるクーポンは、支払い済みになった注文で数える
        if change.to_status.as_deref() != Some("paid") {
            return Ok(());
        }
        let Some(code) = change
            .order
            .coupon_code
            .as_ref()
            .map(|c| c.trim().to_ascii_uppercase())
        else {
            return Ok(());
        };
        if coupons()
            .iter()
            .any(|c| c.code == code && c.max_uses.is_some())
        {
            kv::set(&uses_key(&code), &(uses(&code) + 1).to_string())?;
        }
        Ok(())
    }
}

aegis_plugin_sdk::register_plugin!(ShopRulesPlugin);

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_plugin_sdk::host::mock;

    fn order(subtotal: i64, coupon: Option<&str>) -> OrderContext {
        OrderContext {
            currency: "jpy".to_string(),
            lines: vec![OrderLine {
                title: "Mug".to_string(),
                unit_price: subtotal,
                quantity: 1,
                line_total: subtotal,
                ..Default::default()
            }],
            subtotal,
            total: subtotal,
            coupon_code: coupon.map(str::to_string),
            ..Default::default()
        }
    }

    fn amounts(ctx: &OrderContext) -> Vec<(String, i64)> {
        ctx.adjustments
            .iter()
            .map(|a| (a.label.clone(), a.amount))
            .collect()
    }

    #[test]
    fn test_parse_coupons() {
        assert_eq!(
            parse_coupons(" save10:10% , YEN500:500:20, bad, ZERO:0%, NEG:-5 "),
            vec![
                Coupon {
                    code: "SAVE10".to_string(),
                    discount: Discount::Percent(10),
                    max_uses: None
                },
                Coupon {
                    code: "YEN500".to_string(),
                    discount: Discount::Amount(500),
                    max_uses: Some(20)
                },
            ]
        );
    }

    #[test]
    fn test_shipping_coupon_and_tax() {
        mock::reset();
        mock::set_config("shipping_fee", "500");
        mock::set_config("free_shipping_over", "5000");
        mock::set_config("coupons", "SAVE10:10%,YEN500:500");
        let mut plugin = ShopRulesPlugin;

        let quote = plugin.calculate_order(order(3000, None)).unwrap();
        assert_eq!(amounts(&quote), vec![("Shipping".to_string(), 500)]);

        // 10% off 6,000 → 5,400, still over the free-shipping threshold
        let quote = plugin.calculate_order(order(6000, Some("save10"))).unwrap();
        assert_eq!(
            amounts(&quote),
            vec![
                ("Coupon SAVE10".to_string(), -600),
                ("Shipping (free)".to_string(), 0)
            ]
        );
        // A fixed discount never exceeds the subtotal
        let quote = plugin.calculate_order(order(300, Some("YEN500"))).unwrap();
        assert_eq!(quote.adjustments[0].amount, -300);

        let error = plugin
            .calculate_order(order(3000, Some("NOPE")))
            .unwrap_err();
        assert!(matches!(error, PluginError::Rejected(_)));

        mock::set_config("tax_rate_percent", "10");
        let quote = plugin.calculate_order(order(3000, None)).unwrap();
        assert_eq!(amounts(&quote)[1], ("Tax (10%)".to_string(), 350));
    }

    #[test]
    fn test_coupon_usage_limit() {
        mock::reset();
        mock::set_config("coupons", "ONCE:10%:1");
        let mut plugin = ShopRulesPlugin;
        assert!(plugin.calculate_order(order(1000, Some("ONCE"))).is_ok());

        let paid = OrderStatusChange {
            order: order(1000, Some("once")),
            from_status: Some("pending_payment".to_string()),
            to_status: Some("paid".to_string()),
            occurred_at: String::new(),
        };
        plugin.on_order_status_changed(paid).unwrap();
        assert_eq!(uses("ONCE"), 1);
        let error = plugin
            .calculate_order(order(1000, Some("ONCE")))
            .unwrap_err();
        assert!(error.to_string().contains("used up"));
    }
}
