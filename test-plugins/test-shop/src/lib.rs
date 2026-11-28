//! AegisCMS 本体のテストのためのプラグイン（同梱しない・ストアに載せない）。
//!
//! 設定（マニフェストでは宣言しない。宣言しないキーがそのまま届くことも確かめる）:
//! - `shipping_fee`: 0 より大きければ、送料（`Shipping`）を足す
//! - `coupon`: `CODE:percent[:max_uses]`。1 つだけのクーポン。上限があれば、支払い済みになった注文で
//!   KV の `coupon:CODE:uses` を数え、上限に達したら断る

use aegis_plugin_sdk::prelude::*;

struct Coupon {
    code: String,
    percent: i64,
    max_uses: Option<u64>,
}

fn coupon() -> Option<Coupon> {
    let setting = config::get("coupon")?;
    let mut parts = setting.split(':').map(str::trim);
    let code = parts.next()?.to_ascii_uppercase();
    let percent = parts.next()?.trim_end_matches('%').parse().ok()?;
    let max_uses = parts.next().and_then(|m| m.parse().ok());
    (!code.is_empty()).then_some(Coupon {
        code,
        percent,
        max_uses,
    })
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

#[derive(Default)]
pub struct TestShopPlugin;

impl Plugin for TestShopPlugin {
    fn calculate_order(&mut self, mut ctx: OrderContext) -> Result<OrderContext> {
        if let Some(code) = ctx
            .coupon_code
            .as_deref()
            .map(|c| c.trim().to_ascii_uppercase())
        {
            let coupon = coupon().filter(|c| c.code == code).ok_or_else(|| {
                PluginError::Rejected(format!("Coupon code \"{}\" is not valid", code))
            })?;
            if coupon.max_uses.is_some_and(|max| uses(&code) >= max) {
                return Err(PluginError::Rejected(format!(
                    "Coupon code \"{}\" has been used up",
                    code
                )));
            }
            let mut discount = OrderAdjustment::new(
                "discount",
                format!("Coupon {}", code),
                -(ctx.subtotal * coupon.percent / 100),
            );
            discount.code = Some(code);
            ctx.adjustments.push(discount);
        }
        let fee: i64 = config::get_or("shipping_fee", 0);
        if fee > 0 {
            ctx.adjustments
                .push(OrderAdjustment::new("shipping", "Shipping", fee));
        }
        Ok(ctx)
    }

    fn on_order_status_changed(&mut self, change: OrderStatusChange) -> Result<()> {
        if change.to_status.as_deref() != Some("paid") {
            return Ok(());
        }
        let code = change
            .order
            .coupon_code
            .as_deref()
            .map(|c| c.trim().to_ascii_uppercase());
        if let (Some(code), Some(coupon)) = (code, coupon()) {
            if coupon.code == code && coupon.max_uses.is_some() {
                kv::set(&uses_key(&code), &(uses(&code) + 1).to_string())?;
            }
        }
        Ok(())
    }
}

aegis_plugin_sdk::register_plugin!(TestShopPlugin);

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_plugin_sdk::host::mock;

    fn order(coupon: Option<&str>) -> OrderContext {
        OrderContext {
            currency: "jpy".to_string(),
            subtotal: 8000,
            total: 8000,
            coupon_code: coupon.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn adds_the_coupon_and_the_shipping_fee() {
        mock::reset();
        mock::set_config("shipping_fee", "600");
        mock::set_config("coupon", "WELCOME:10%:1");
        let ctx = TestShopPlugin
            .calculate_order(order(Some("welcome")))
            .unwrap();
        let amounts: Vec<_> = ctx
            .adjustments
            .iter()
            .map(|a| (a.label.as_str(), a.amount))
            .collect();
        assert_eq!(amounts, [("Coupon WELCOME", -800), ("Shipping", 600)]);

        let err = TestShopPlugin
            .calculate_order(order(Some("NOPE")))
            .unwrap_err();
        assert!(err.to_string().contains("\"NOPE\" is not valid"), "{err}");
    }

    #[test]
    fn counts_uses_when_paid_and_refuses_after_the_limit() {
        mock::reset();
        mock::set_config("coupon", "WELCOME:10:1");
        let paid = OrderStatusChange {
            order: order(Some("WELCOME")),
            to_status: Some("paid".to_string()),
            ..Default::default()
        };
        TestShopPlugin.on_order_status_changed(paid).unwrap();
        let err = TestShopPlugin
            .calculate_order(order(Some("WELCOME")))
            .unwrap_err();
        assert!(err.to_string().contains("used up"), "{err}");
    }
}
