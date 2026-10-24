//! EC（`[commerce]`）で共有する型と金額の扱い。金額はすべて通貨の最小単位の整数
//! （JPY なら円、USD ならセント）で、Stripe の `unit_amount` と同じ単位

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 注文の状態。遷移は `can_transition_to` のとおり
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    /// 在庫を確保し、支払いを待っている
    PendingPayment,
    Paid,
    Shipped,
    Completed,
    /// 取り消し（期限切れ・支払いの失敗を含む）。在庫は戻してある
    Cancelled,
    /// 全額返金済み
    Refunded,
}

impl OrderStatus {
    pub const ALL: [OrderStatus; 6] = [
        Self::PendingPayment,
        Self::Paid,
        Self::Shipped,
        Self::Completed,
        Self::Cancelled,
        Self::Refunded,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PendingPayment => "pending_payment",
            Self::Paid => "paid",
            Self::Shipped => "shipped",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::Refunded => "refunded",
        }
    }

    /// `self` から `next` に移れるか。発送後の取り消し・完了後の変更はできない（返金は可能）
    pub fn can_transition_to(&self, next: OrderStatus) -> bool {
        use OrderStatus::*;
        matches!(
            (self, next),
            (PendingPayment, Paid)
                | (PendingPayment, Cancelled)
                | (Paid, Shipped)
                | (Paid, Cancelled)
                | (Paid, Refunded)
                | (Shipped, Completed)
                | (Shipped, Refunded)
                | (Completed, Refunded)
                | (Cancelled, Refunded)
        )
    }

    /// 取り消すときに在庫を戻す状態か（発送前）
    pub fn holds_stock(&self) -> bool {
        matches!(self, Self::PendingPayment | Self::Paid)
    }
}

impl std::str::FromStr for OrderStatus {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|status| status.as_str() == s)
            .ok_or_else(|| format!("Unknown order status: {}", s))
    }
}

impl std::fmt::Display for OrderStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 商品のバリエーション（サイズ・色・SKUごとの価格上書きと在庫）
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductVariant {
    pub id: String,
    pub product_id: String,
    #[serde(default)]
    pub sku: Option<String>,
    pub title: String,
    #[serde(default)]
    pub price_override: Option<i64>,
    #[serde(default)]
    pub stock: Option<i64>,
    #[serde(default)]
    pub attributes: HashMap<String, String>,
    #[serde(default)]
    pub position: i32,
    pub created_at: String,
    pub updated_at: String,
}

/// 注文の 1 行（商品 × 数量）。単価はサーバーが商品から決める
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderLine {
    pub product_id: String,
    #[serde(default)]
    pub variant_id: Option<String>,
    #[serde(default)]
    pub variant_title: Option<String>,
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub sku: Option<String>,
    pub unit_price: i64,
    pub quantity: u32,
    pub line_total: i64,
}

/// 送料・税・割引などの加算・減算（割引は負の金額）。`calculate-order` フックが追加する
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderAdjustment {
    /// `shipping` / `tax` / `discount` / `fee` など
    pub kind: String,
    /// 表示名（例: `送料`、`クーポン WELCOME10`）
    pub label: String,
    pub amount: i64,
    /// クーポンコードなど
    #[serde(default)]
    pub code: Option<String>,
    /// 追加したプラグイン（ホストが設定する）
    #[serde(default)]
    pub source: String,
}

/// 配送先
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShippingAddress {
    pub name: String,
    pub postal_code: String,
    /// 都道府県・州
    pub region: String,
    pub city: String,
    pub line1: String,
    pub line2: String,
    pub phone: String,
    /// ISO 3166-1 alpha-2（例: `JP`）
    pub country: String,
}

/// 返金の記録（`_aegis_orders.refunds_json` に格納）
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderRefundRecord {
    pub id: String,
    pub amount: i64,
    pub currency: String,
    pub refund_ref: String,
    #[serde(default)]
    pub reason: Option<String>,
    pub created_at: String,
}

/// 見積もり・注文の内容。`calculate-order` / `on-order-status-changed` フックに渡す
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OrderContext {
    /// 注文 ID（見積もり中は `None`）
    #[serde(default)]
    pub order_id: Option<String>,
    /// 注文番号（顧客に見せる番号。見積もり中は `None`）
    #[serde(default)]
    pub order_number: Option<String>,
    pub currency: String,
    pub lines: Vec<OrderLine>,
    #[serde(default)]
    pub adjustments: Vec<OrderAdjustment>,
    /// 商品の合計（`lines` の `line_total` の和）
    pub subtotal: i64,
    /// 支払う金額（`subtotal` + `adjustments` の和）
    pub total: i64,
    #[serde(default)]
    pub customer_email: String,
    #[serde(default)]
    pub customer_name: String,
    #[serde(default)]
    pub shipping_address: Option<ShippingAddress>,
    #[serde(default)]
    pub coupon_code: Option<String>,
    /// 注文の状態（見積もり中は `None`）
    #[serde(default)]
    pub status: Option<OrderStatus>,
    /// プラグインが付ける任意の値（注文に保存される）
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl OrderContext {
    /// `lines` から `subtotal`、`adjustments` を足して `total` を計算し直す
    pub fn recompute_totals(&mut self) {
        self.subtotal = self.lines.iter().map(|l| l.line_total).sum();
        self.total = self.subtotal + self.adjustments.iter().map(|a| a.amount).sum::<i64>();
    }
}

/// `on-order-status-changed` フックに渡す、注文の作成・状態の変化
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OrderStatusChange {
    pub order: OrderContext,
    /// 変化前の状態（注文の作成時は `None`）
    #[serde(default)]
    pub from_status: Option<OrderStatus>,
    pub to_status: Option<OrderStatus>,
    /// RFC 3339
    pub occurred_at: String,
}

/// 小数部を持たない通貨（Stripe の zero-decimal currencies）。それ以外は最小単位が 1/100
const ZERO_DECIMAL_CURRENCIES: [&str; 16] = [
    "bif", "clp", "djf", "gnf", "jpy", "kmf", "krw", "mga", "pyg", "rwf", "ugx", "vnd", "vuv",
    "xaf", "xof", "xpf",
];

/// 小数部を持たない通貨か（`jpy` など。大文字小文字は区別しない）
pub fn is_zero_decimal_currency(currency: &str) -> bool {
    ZERO_DECIMAL_CURRENCIES.contains(&currency.to_ascii_lowercase().as_str())
}

fn currency_symbol(currency: &str) -> Option<&'static str> {
    match currency.to_ascii_lowercase().as_str() {
        "jpy" => Some("¥"),
        "usd" => Some("$"),
        "eur" => Some("€"),
        "gbp" => Some("£"),
        "krw" => Some("₩"),
        "cny" => Some("CN¥"),
        _ => None,
    }
}

fn group_thousands(digits: &str) -> String {
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// 最小単位の金額を表示用に整形する（`1200, "jpy"` → `¥1,200`、`1250, "usd"` → `$12.50`、
/// 記号の無い通貨は `CHF 12.50`）
pub fn format_money(amount: i64, currency: &str) -> String {
    let sign = if amount < 0 { "-" } else { "" };
    let abs = amount.unsigned_abs();
    let number = if is_zero_decimal_currency(currency) {
        group_thousands(&abs.to_string())
    } else {
        format!(
            "{}.{:02}",
            group_thousands(&(abs / 100).to_string()),
            abs % 100
        )
    };
    match currency_symbol(currency) {
        Some(symbol) => format!("{}{}{}", sign, symbol, number),
        None => format!("{}{} {}", sign, currency.to_ascii_uppercase(), number),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_status_transitions() {
        use OrderStatus::*;
        assert!(PendingPayment.can_transition_to(Paid));
        assert!(Paid.can_transition_to(Cancelled));
        assert!(Paid.can_transition_to(Refunded));
        assert!(Shipped.can_transition_to(Refunded));
        assert!(Completed.can_transition_to(Refunded));
        assert!(Cancelled.can_transition_to(Refunded));
        assert!(!Shipped.can_transition_to(Cancelled));
        assert!(!Cancelled.can_transition_to(Paid));
        assert!(!Completed.can_transition_to(Shipped));
        assert_eq!("pending_payment".parse::<OrderStatus>(), Ok(PendingPayment));
        assert_eq!("refunded".parse::<OrderStatus>(), Ok(Refunded));
        assert_eq!(
            serde_json::to_value(PendingPayment).unwrap(),
            serde_json::json!("pending_payment")
        );
        assert_eq!(
            serde_json::to_value(Refunded).unwrap(),
            serde_json::json!("refunded")
        );
    }

    #[test]
    fn test_format_money() {
        assert_eq!(format_money(0, "jpy"), "¥0");
        assert_eq!(format_money(1_234_567, "JPY"), "¥1,234,567");
        assert_eq!(format_money(1250, "usd"), "$12.50");
        assert_eq!(format_money(100_005, "eur"), "€1,000.05");
        assert_eq!(format_money(-500, "jpy"), "-¥500");
        assert_eq!(format_money(990, "chf"), "CHF 9.90");
        assert!(is_zero_decimal_currency("KRW"));
        assert!(!is_zero_decimal_currency("usd"));
    }
}
