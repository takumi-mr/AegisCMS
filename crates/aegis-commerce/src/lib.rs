//! EC（`[commerce]`）: 見積もり・注文・在庫の確保・決済・注文の状態管理。
//! HTTP 層（API・公開ページ）はサーバー側にあり、ここは HTTP に依存しない
//!
//! 価格は必ずサーバーが商品（`product_type` のコンテンツ）から決め、クライアントから送られた値は
//! 使わない。送料・税・割引は `calculate-order` プラグインが加え、合計はホストが計算し直す。
//! 在庫は注文の作成と同じトランザクションで減らし（`stock >= 数量` のときだけ）、取り消し・期限切れで戻す

pub mod config;
pub mod payment;
pub mod stripe;

use std::collections::HashMap;
use std::sync::Arc;

use aegis_core::{
    AegisError, AuditAction, AuditOutcome, NotificationEvent, Notifier, OrderAdjustment,
    OrderContext, OrderLine, OrderRefundRecord, OrderStatus, OrderStatusChange, Recipient,
    ShippingAddress,
};
use aegis_schema::{FieldType, SchemaDefinition};
use chrono::{SecondsFormat, Utc};
pub use config::{CommerceConfig, ManualPaymentConfig, PaymentProviderKind, StripeConfig};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use aegis_audit::{AuditOrigin, AuditService};
use aegis_core::CacheBackend;
use aegis_db::{
    DatabaseAdapter, OrderRecord, OrderRefundUpdate, OrderTransition, StockReservation,
};
use aegis_runtime::PluginSupervisor;
use payment::{
    ManualPaymentProvider, PaymentEvent, PaymentEventKind, PaymentProvider, RefundRequest,
    ReturnUrls,
};
use tokio::sync::RwLock;

/// 期限切れの判定に足す猶予（秒）。決済サービス側の期限と同時に支払われた通知の到着を待つ
const EXPIRY_GRACE_SECS: i64 = 300;
/// 1 回の期限切れ処理で取り消す注文数の上限
const EXPIRY_BATCH: u64 = 100;

#[derive(Debug, thiserror::Error)]
pub enum CommerceError {
    /// 入力の不備（400）
    #[error("{0}")]
    Invalid(String),
    /// 商品・注文が無い（404）
    #[error("{0}")]
    NotFound(String),
    /// 在庫不足（409）
    #[error("Not enough stock for '{title}'")]
    OutOfStock { product_id: String, title: String },
    /// 状態が変わっていて操作できない（409）
    #[error("{0}")]
    Conflict(String),
    /// `calculate-order` プラグインが注文を断った（422。メッセージは顧客に見せる）
    #[error("{0}")]
    Rejected(String),
    /// 同じ IP からの注文が多すぎる（429）
    #[error("Too many orders from this address. Try again later.")]
    RateLimited,
    /// 決済サービスの失敗（502）
    #[error("Payment provider error: {0}")]
    Payment(String),
    /// それ以外のサーバー側の失敗（500）
    #[error("{0}")]
    Internal(String),
}

impl From<AegisError> for CommerceError {
    fn from(e: AegisError) -> Self {
        match e {
            AegisError::PluginRejected(message) => Self::Rejected(message),
            other => Self::Internal(other.to_string()),
        }
    }
}

pub type CommerceResult<T> = std::result::Result<T, CommerceError>;

/// カートの 1 行（商品の ID か slug と数量）
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CartItem {
    pub product: String,
    #[serde(default)]
    pub variant: Option<String>,
    pub quantity: u32,
}

/// `POST /api/v1/shop/quote` / `checkout` の本文
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CartRequest {
    pub items: Vec<CartItem>,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub shipping_address: Option<ShippingAddress>,
    #[serde(default)]
    pub coupon_code: Option<String>,
}

/// 注文の作成結果
pub struct CheckoutResult {
    pub order: OrderRecord,
    /// 注文確認ページの URL に付けるトークン（平文はここでしか得られない）
    pub access_token: String,
    /// 決済ページ（無ければ注文確認ページへ）
    pub redirect_url: Option<String>,
}

/// 注文の処理が使うサーバーの部品（商品・注文の DB、IP ごとの注文数を数えるキャッシュ、スキーマ、
/// プラグイン、監査ログ、顧客・運営者への通知）
#[derive(Clone)]
pub struct CommerceDeps {
    pub db: Arc<dyn DatabaseAdapter>,
    pub cache: Arc<dyn CacheBackend>,
    pub schemas: Arc<RwLock<HashMap<String, SchemaDefinition>>>,
    pub supervisor: Arc<RwLock<PluginSupervisor>>,
    pub audit: Arc<AuditService>,
    /// 注文の受付・入金確認・発送・取り消しの通知（`[notifications]`）。無効なら `None`
    pub notifier: Option<Arc<dyn Notifier>>,
}

pub struct CommerceService {
    pub config: CommerceConfig,
    provider: Arc<dyn PaymentProvider>,
    deps: CommerceDeps,
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn after(seconds: i64) -> String {
    (Utc::now() + chrono::Duration::seconds(seconds)).to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// 顧客に見せる注文番号（日付 + 紛らわしい文字を除いた 6 文字）
fn order_number() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let random = Uuid::new_v4();
    let suffix: String = random.as_bytes()[..6]
        .iter()
        .map(|b| ALPHABET[(*b as usize) % ALPHABET.len()] as char)
        .collect();
    format!("{}-{}", Utc::now().format("%y%m%d"), suffix)
}

fn valid_email(email: &str) -> bool {
    let email = email.trim();
    match email.split_once('@') {
        Some((local, domain)) => {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
                && email.len() <= 254
                && !email.chars().any(char::is_whitespace)
        }
        None => false,
    }
}

/// 商品フィールドの整数（数値でも文字列でもよい）。無い・空なら `None`
fn integer_field(fields: &HashMap<String, serde_json::Value>, name: &str) -> Option<i64> {
    match fields.get(name)? {
        serde_json::Value::Number(n) => n.as_i64(),
        serde_json::Value::String(s) if !s.trim().is_empty() => s.trim().parse().ok(),
        _ => None,
    }
}

impl CommerceService {
    pub fn new(config: CommerceConfig, deps: CommerceDeps) -> aegis_core::Result<Self> {
        let provider: Arc<dyn PaymentProvider> = match config.provider {
            PaymentProviderKind::Manual => Arc::new(ManualPaymentProvider),
            PaymentProviderKind::Stripe => {
                Arc::new(stripe::StripePaymentProvider::new(&config.stripe)?)
            }
        };
        Ok(Self {
            config,
            provider,
            deps,
        })
    }

    pub fn provider(&self) -> &dyn PaymentProvider {
        self.provider.as_ref()
    }

    /// 商品スキーマが `price`（integer）を持つかを起動時に確かめる
    pub fn check_product_schema(
        &self,
        schemas: &HashMap<String, SchemaDefinition>,
    ) -> aegis_core::Result<()> {
        let schema = schemas.get(&self.config.product_type).ok_or_else(|| {
            AegisError::Config(format!(
                "[commerce].product_type: schema '{}' is not defined in the schemas directory",
                self.config.product_type
            ))
        })?;
        let field_type = |name: &str| {
            schema
                .fields
                .iter()
                .find(|f| f.name == name)
                .map(|f| f.field_type)
        };
        if field_type("price") != Some(FieldType::Integer) {
            return Err(AegisError::Config(format!(
                "Schema '{}' needs an integer 'price' field for [commerce]",
                schema.content_type
            )));
        }
        if matches!(field_type("stock"), Some(t) if t != FieldType::Integer) {
            return Err(AegisError::Config(format!(
                "Schema '{}': 'stock' must be an integer field",
                schema.content_type
            )));
        }
        Ok(())
    }

    async fn product_schema(&self) -> CommerceResult<SchemaDefinition> {
        self.deps
            .schemas
            .read()
            .await
            .get(&self.config.product_type)
            .cloned()
            .ok_or_else(|| CommerceError::Internal("Product schema is missing".to_string()))
    }

    /// カートの検証と、商品から価格を引いた明細・在庫の確保内容を作る
    async fn build_lines(
        &self,
        items: &[CartItem],
    ) -> CommerceResult<(Vec<OrderLine>, Vec<StockReservation>)> {
        if items.is_empty() {
            return Err(CommerceError::Invalid("The cart is empty".to_string()));
        }
        // 同じ商品・バリエーションは 1 行にまとめる（並び順はカートの順）
        let mut merged: Vec<((String, Option<String>), u32)> = Vec::new();
        for item in items {
            let prod_key = item.product.trim().to_string();
            let var_key = item
                .variant
                .as_ref()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty());
            if prod_key.is_empty() || item.quantity == 0 {
                return Err(CommerceError::Invalid(
                    "Each cart item needs a product and a quantity of at least 1".to_string(),
                ));
            }
            let key = (prod_key, var_key);
            match merged.iter_mut().find(|(k, _)| *k == key) {
                Some((_, quantity)) => *quantity = quantity.saturating_add(item.quantity),
                None => merged.push((key, item.quantity)),
            }
        }
        if merged.len() > self.config.max_items {
            return Err(CommerceError::Invalid(format!(
                "An order can have at most {} different products",
                self.config.max_items
            )));
        }

        let schema = self.product_schema().await?;
        let table = schema.table_name();
        let mut lines = Vec::with_capacity(merged.len());
        let mut reservations = Vec::with_capacity(merged.len());
        for ((prod_key, var_key), quantity) in merged {
            if quantity > self.config.max_quantity_per_item {
                return Err(CommerceError::Invalid(format!(
                    "At most {} of each product per order",
                    self.config.max_quantity_per_item
                )));
            }
            let product = self
                .deps
                .db
                .get_content(&schema, &prod_key)
                .await
                .map_err(|e| CommerceError::Internal(e.to_string()))?
                .filter(|p| p.status == "published")
                .ok_or_else(|| {
                    CommerceError::NotFound(format!("Product '{}' is not available", prod_key))
                })?;

            let (variant, unit_price, sku, title) = if let Some(ref vk) = var_key {
                let v = self
                    .deps
                    .db
                    .get_product_variant(vk)
                    .await
                    .map_err(|e| CommerceError::Internal(e.to_string()))?;
                let v = match v {
                    Some(v) => v,
                    None => self
                        .deps
                        .db
                        .get_product_variant_by_sku(vk)
                        .await
                        .map_err(|e| CommerceError::Internal(e.to_string()))?
                        .ok_or_else(|| {
                            CommerceError::NotFound(format!(
                                "Product variant '{}' is not available",
                                vk
                            ))
                        })?,
                };
                if v.product_id != product.id {
                    return Err(CommerceError::Invalid(format!(
                        "Variant '{}' does not belong to product '{}'",
                        vk, product.slug
                    )));
                }
                if lines.iter().any(|l: &OrderLine| {
                    l.product_id == product.id && l.variant_id.as_deref() == Some(&v.id)
                }) {
                    return Err(CommerceError::Invalid(format!(
                        "Product variant '{}' is listed twice",
                        v.title
                    )));
                }
                let base_price = integer_field(&product.custom_fields, "price");
                let price = v
                    .price_override
                    .or(base_price)
                    .filter(|price| *price >= 0)
                    .ok_or_else(|| {
                        CommerceError::NotFound(format!(
                            "Product variant '{}' has no price",
                            v.title
                        ))
                    })?;
                if let Some(stock) = v.stock {
                    if stock < i64::from(quantity) {
                        return Err(CommerceError::OutOfStock {
                            product_id: product.id.clone(),
                            title: format!("{} ({})", product.title, v.title),
                        });
                    }
                }
                let var_sku = v.sku.clone().or_else(|| {
                    product
                        .custom_fields
                        .get("sku")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                });
                let full_title = format!("{} - {}", product.title, v.title);
                (Some(v), price, var_sku, full_title)
            } else {
                if lines
                    .iter()
                    .any(|l: &OrderLine| l.product_id == product.id && l.variant_id.is_none())
                {
                    // ID と slug で同じ商品を指定した
                    return Err(CommerceError::Invalid(format!(
                        "Product '{}' is listed twice",
                        product.slug
                    )));
                }
                let unit_price = integer_field(&product.custom_fields, "price")
                    .filter(|price| *price >= 0)
                    .ok_or_else(|| {
                        CommerceError::NotFound(format!("Product '{}' has no price", product.slug))
                    })?;
                // 見積もりの時点でも在庫切れを知らせる（確定は注文作成時の UPDATE で行う）
                if let Some(stock) = integer_field(&product.custom_fields, "stock") {
                    if stock < i64::from(quantity) {
                        return Err(CommerceError::OutOfStock {
                            product_id: product.id.clone(),
                            title: product.title.clone(),
                        });
                    }
                }
                let sku = product
                    .custom_fields
                    .get("sku")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(str::to_string);
                (None, unit_price, sku, product.title.clone())
            };

            let line_total = unit_price
                .checked_mul(i64::from(quantity))
                .ok_or_else(|| CommerceError::Invalid("Amount is too large".to_string()))?;
            let (variant_id, variant_title) = match variant {
                Some(v) => (Some(v.id), Some(v.title)),
                None => (None, None),
            };
            reservations.push(StockReservation {
                table: table.clone(),
                product_id: product.id.clone(),
                variant_id: variant_id.clone(),
                quantity: i64::from(quantity),
            });
            lines.push(OrderLine {
                product_id: product.id,
                slug: product.slug,
                title,
                sku,
                unit_price,
                quantity,
                line_total,
                variant_id,
                variant_title,
            });
        }
        Ok((lines, reservations))
    }

    /// 明細にプラグインの `calculate-order`（送料・税・割引）を適用した見積もり
    async fn calculate(
        &self,
        request: &CartRequest,
    ) -> CommerceResult<(OrderContext, Vec<StockReservation>)> {
        let (lines, reservations) = self.build_lines(&request.items).await?;
        let ctx = OrderContext {
            currency: self.config.currency.clone(),
            lines,
            customer_email: request.email.trim().to_string(),
            customer_name: request.name.trim().to_string(),
            shipping_address: request.shipping_address.clone(),
            coupon_code: request
                .coupon_code
                .as_ref()
                .map(|c| c.trim().to_string())
                .filter(|c| !c.is_empty()),
            ..Default::default()
        };
        let ctx = {
            let supervisor = self.deps.supervisor.read().await;
            match supervisor.apply_calculate_order(ctx).await {
                Ok(ctx) => ctx,
                // 断った理由は顧客に見せるため、Wasm ホストが付けるプラグイン名（`name: `）を外す
                Err(AegisError::PluginRejected(message)) => {
                    tracing::info!("calculate-order refused the order: {}", message);
                    let reason = supervisor
                        .plugins()
                        .iter()
                        .find_map(|p| message.strip_prefix(&format!("{}: ", p.manifest().name)))
                        .unwrap_or(&message)
                        .to_string();
                    return Err(CommerceError::Rejected(reason));
                }
                Err(e) => return Err(e.into()),
            }
        };
        if ctx.total < 0 {
            return Err(CommerceError::Rejected(
                "The order total cannot be negative".to_string(),
            ));
        }
        Ok((ctx, reservations))
    }

    /// 見積もり（何も保存しない）
    pub async fn quote(&self, request: &CartRequest) -> CommerceResult<OrderContext> {
        Ok(self.calculate(request).await?.0)
    }

    /// 注文を作成し、在庫を確保して決済を始める
    pub async fn checkout(
        &self,
        audit: &AuditOrigin,
        request: &CartRequest,
        client_ip: Option<&str>,
        urls: impl FnOnce(&OrderRecord, &str) -> ReturnUrls,
    ) -> CommerceResult<CheckoutResult> {
        if !valid_email(&request.email) {
            return Err(CommerceError::Invalid(
                "A valid email address is required".to_string(),
            ));
        }
        if request.name.trim().is_empty() {
            return Err(CommerceError::Invalid("Your name is required".to_string()));
        }
        if let (Some(ip), max) = (client_ip, self.config.max_orders_per_ip_per_hour) {
            if max > 0 {
                let count = self
                    .deps
                    .cache
                    .increment(&format!("shop:checkout:{}", ip), Some(3600))
                    .await
                    .map_err(|e| CommerceError::Internal(e.to_string()))?;
                if count > max {
                    return Err(CommerceError::RateLimited);
                }
            }
        }

        let (mut ctx, reservations) = self.calculate(request).await?;
        let access_token = Uuid::new_v4().simple().to_string();
        let created_at = now();
        let free = ctx.total == 0;
        let hold_secs = match self.config.provider {
            PaymentProviderKind::Manual => self.config.manual.payment_due_hours as i64 * 3600,
            PaymentProviderKind::Stripe => self.config.reservation_minutes as i64 * 60,
        };
        let status = if free {
            OrderStatus::Paid
        } else {
            OrderStatus::PendingPayment
        };
        let order = OrderRecord {
            id: Uuid::new_v4().to_string(),
            number: order_number(),
            status: status.as_str().to_string(),
            currency: ctx.currency.clone(),
            subtotal: ctx.subtotal,
            total: ctx.total,
            lines_json: serde_json::to_string(&ctx.lines).unwrap_or_else(|_| "[]".to_string()),
            adjustments_json: serde_json::to_string(&ctx.adjustments)
                .unwrap_or_else(|_| "[]".to_string()),
            customer_email: ctx.customer_email.clone(),
            customer_name: ctx.customer_name.clone(),
            shipping_json: ctx
                .shipping_address
                .as_ref()
                .and_then(|a| serde_json::to_string(a).ok()),
            coupon_code: ctx.coupon_code.clone(),
            metadata_json: serde_json::to_string(&ctx.metadata)
                .unwrap_or_else(|_| "{}".to_string()),
            payment_provider: self.provider.id().to_string(),
            payment_ref: None,
            access_token_hash: hash_token(&access_token),
            expires_at: (!free).then(|| after(hold_secs)),
            paid_at: free.then(|| created_at.clone()),
            created_at: created_at.clone(),
            updated_at: created_at,
            note: None,
            transaction_id: None,
            refunded_amount: 0,
            refunds_json: "[]".to_string(),
        };

        let stored = self
            .deps
            .db
            .create_order(&order, &reservations)
            .await
            .map_err(|e| CommerceError::Internal(e.to_string()))?;
        if let Err(out) = stored {
            let title = ctx
                .lines
                .iter()
                .find(|l| l.product_id == out.product_id)
                .map(|l| l.title.clone())
                .unwrap_or_default();
            return Err(CommerceError::OutOfStock {
                product_id: out.product_id,
                title,
            });
        }
        ctx.order_id = Some(order.id.clone());
        ctx.order_number = Some(order.number.clone());
        ctx.status = Some(status);
        self.deps
            .audit
            .record(
                audit
                    .event(AuditAction::OrderCreate, AuditOutcome::Success)
                    .with_resource("order", Some(order.id.clone()))
                    .with_details(json!({
                        "number": order.number,
                        "total": order.total,
                        "currency": order.currency,
                        "items": ctx.lines.len(),
                        "provider": order.payment_provider,
                    })),
            )
            .await;

        let mut order = order;
        let mut redirect_url = None;
        let return_urls = urls(&order, &access_token);
        if !free {
            match self
                .provider
                .start_payment(&order, &ctx, &return_urls)
                .await
            {
                Ok(start) => {
                    if let Some(payment_ref) = &start.payment_ref {
                        self.deps
                            .db
                            .set_order_payment(&order.id, payment_ref, order.expires_at.as_deref())
                            .await
                            .map_err(|e| CommerceError::Internal(e.to_string()))?;
                        order.payment_ref = Some(payment_ref.clone());
                    }
                    redirect_url = start.redirect_url;
                }
                Err(e) => {
                    // 決済を始められなかった注文は取り消して在庫を戻す
                    tracing::error!(
                        "Failed to start the payment of order {}: {}",
                        order.number,
                        e
                    );
                    let _ = self
                        .transition(
                            audit,
                            &order,
                            OrderStatus::Cancelled,
                            Some("Payment could not be started".to_string()),
                            "payment_start_failed",
                        )
                        .await;
                    return Err(CommerceError::Payment(e.to_string()));
                }
            }
        }

        self.notify(&order, None, status);
        // Stripe などの決済ページで支払う注文は、支払いが済んでから（Webhook で）受付を知らせる
        if free || self.config.provider == PaymentProviderKind::Manual {
            let mut extra = json!({ "order_url": return_urls.success_url });
            if status == OrderStatus::PendingPayment {
                extra["payment_instructions"] = json!(self.config.manual.instructions);
                extra["payment_due"] = json!(order.expires_at);
            }
            self.send_notification("order.placed", &order, true, extra);
        }
        Ok(CheckoutResult {
            order,
            access_token,
            redirect_url,
        })
    }

    /// 注文の状態を変える（`from` の状態のときだけ）。取り消しなら在庫を戻す。
    /// `source` は監査ログに残す変更のきっかけ（`admin` / `payment_webhook` / `expired` など）
    pub async fn transition(
        &self,
        audit: &AuditOrigin,
        order: &OrderRecord,
        to: OrderStatus,
        note: Option<String>,
        source: &str,
    ) -> CommerceResult<OrderRecord> {
        let from: OrderStatus = order.status.parse().map_err(CommerceError::Internal)?;
        if !from.can_transition_to(to) {
            return Err(CommerceError::Conflict(format!(
                "An order cannot go from {} to {}",
                from, to
            )));
        }
        let restock = if to == OrderStatus::Cancelled && from.holds_stock() {
            let lines: Vec<OrderLine> = serde_json::from_str(&order.lines_json).unwrap_or_default();
            let table = self.product_schema().await?.table_name();
            lines
                .iter()
                .map(|line| StockReservation {
                    table: table.clone(),
                    product_id: line.product_id.clone(),
                    variant_id: line.variant_id.clone(),
                    quantity: i64::from(line.quantity),
                })
                .collect()
        } else {
            Vec::new()
        };
        let updated_at = now();
        let change = OrderTransition {
            id: order.id.clone(),
            from_status: from.as_str().to_string(),
            to_status: to.as_str().to_string(),
            updated_at: updated_at.clone(),
            paid_at: (to == OrderStatus::Paid).then(|| updated_at.clone()),
            note: note.clone(),
            restock,
        };
        let changed = self
            .deps
            .db
            .transition_order(&change)
            .await
            .map_err(|e| CommerceError::Internal(e.to_string()))?;
        if !changed {
            return Err(CommerceError::Conflict(
                "The order was changed by someone else. Reload and try again.".to_string(),
            ));
        }
        self.deps
            .audit
            .record(
                audit
                    .event(AuditAction::OrderUpdate, AuditOutcome::Success)
                    .with_resource("order", Some(order.id.clone()))
                    .with_details(json!({
                        "number": order.number,
                        "from": from.as_str(),
                        "to": to.as_str(),
                        "source": source,
                        "restocked": !change.restock.is_empty(),
                        "note": note,
                    })),
            )
            .await;
        let mut updated = order.clone();
        updated.status = to.as_str().to_string();
        updated.updated_at = updated_at;
        if change.paid_at.is_some() {
            updated.paid_at = change.paid_at;
        }
        if note.is_some() {
            updated.note = note;
        }
        self.notify(&updated, Some(from), to);
        self.notify_status_change(&updated, from, to, source);
        Ok(updated)
    }

    /// 注文の返金を行う（管理者操作）
    /// プロバイダがオンライン返金をサポートしていればプロバイダの `refund` を呼び出し、
    /// 結果を注文の返金履歴に記録する。手動決済の場合は手動返金として記録する。
    pub async fn refund_order(
        &self,
        audit: &AuditOrigin,
        order: &OrderRecord,
        amount: Option<i64>,
        reason: Option<String>,
        restock: bool,
    ) -> CommerceResult<OrderRecord> {
        let current_status: OrderStatus = order.status.parse().map_err(CommerceError::Internal)?;
        if current_status == OrderStatus::PendingPayment {
            return Err(CommerceError::Conflict(
                "Cannot refund an unpaid order".to_string(),
            ));
        }

        let refundable_balance = order.total.saturating_sub(order.refunded_amount);
        if refundable_balance <= 0 {
            return Err(CommerceError::Conflict(
                "Order is already fully refunded".to_string(),
            ));
        }

        let refund_amount = match amount {
            Some(a) if a <= 0 => {
                return Err(CommerceError::Invalid(
                    "Refund amount must be greater than zero".to_string(),
                ));
            }
            Some(a) if a > refundable_balance => {
                return Err(CommerceError::Invalid(format!(
                    "Refund amount ({}) exceeds refundable balance ({})",
                    a, refundable_balance
                )));
            }
            Some(a) => a,
            None => refundable_balance,
        };

        // 決済プロバイダでの返金処理（オンライン返金対応プロバイダの場合）
        let refund_ref = if self.provider.supports_refund() {
            let req = RefundRequest {
                order,
                amount: refund_amount,
                reason: reason.clone(),
            };
            let res = self
                .provider
                .refund(req)
                .await
                .map_err(|e| CommerceError::Payment(e.to_string()))?;
            res.refund_ref
        } else {
            // 手動決済など
            format!("manual-{}", Uuid::new_v4().simple())
        };

        let now = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
        let mut refunds: Vec<OrderRefundRecord> =
            serde_json::from_str(&order.refunds_json).unwrap_or_default();
        let refund_entry_id = format!("ref-{}", Uuid::new_v4().simple());
        refunds.push(OrderRefundRecord {
            id: refund_entry_id.clone(),
            amount: refund_amount,
            currency: order.currency.clone(),
            refund_ref: refund_ref.clone(),
            reason: reason.clone(),
            created_at: now.clone(),
        });
        let refunds_json =
            serde_json::to_string(&refunds).map_err(|e| CommerceError::Internal(e.to_string()))?;

        let new_refunded_amount = order.refunded_amount + refund_amount;
        let is_full = new_refunded_amount >= order.total;
        let new_status = if is_full {
            Some(OrderStatus::Refunded.as_str().to_string())
        } else {
            None
        };

        let restock_reservations = if restock {
            let product_table = self.product_schema().await?.table_name();
            let lines: Vec<OrderLine> = serde_json::from_str(&order.lines_json).unwrap_or_default();
            lines
                .into_iter()
                .map(|l| StockReservation {
                    table: product_table.clone(),
                    product_id: l.product_id,
                    variant_id: l.variant_id,
                    quantity: l.quantity as i64,
                })
                .collect()
        } else {
            Vec::new()
        };

        let update = OrderRefundUpdate {
            id: order.id.clone(),
            refunded_amount: new_refunded_amount,
            refunds_json: refunds_json.clone(),
            new_status: new_status.clone(),
            updated_at: now.clone(),
            note: reason.clone(),
            restock: restock_reservations,
        };

        let ok = self
            .deps
            .db
            .record_order_refund(&update)
            .await
            .map_err(|e| CommerceError::Internal(e.to_string()))?;

        if !ok {
            return Err(CommerceError::Conflict(
                "Failed to update order for refund. It may have been updated concurrently."
                    .to_string(),
            ));
        }

        self.deps
            .audit
            .record(
                audit
                    .event(AuditAction::OrderUpdate, AuditOutcome::Success)
                    .with_resource("order", Some(order.id.clone()))
                    .with_details(json!({
                        "number": order.number,
                        "action": "refund",
                        "refund_id": refund_entry_id,
                        "refund_ref": refund_ref,
                        "amount": refund_amount,
                        "currency": order.currency,
                        "total_refunded": new_refunded_amount,
                        "status": new_status.as_deref().unwrap_or(&order.status),
                        "restocked": restock,
                        "reason": reason,
                    })),
            )
            .await;

        let mut updated = order.clone();
        updated.refunded_amount = new_refunded_amount;
        updated.refunds_json = refunds_json;
        if let Some(status) = new_status {
            let to = OrderStatus::Refunded;
            updated.status = status;
            self.notify(&updated, Some(current_status), to);
            self.notify_status_change(&updated, current_status, to, "admin_refund");
        }
        updated.updated_at = now;
        if reason.is_some() {
            updated.note = reason;
        }

        Ok(updated)
    }

    /// 決済サービスからの通知（入金・期限切れ・失敗・返金）を注文に反映する。重複した通知は何もしない
    pub async fn apply_payment_event(
        &self,
        audit: &AuditOrigin,
        event: &PaymentEvent,
    ) -> CommerceResult<()> {
        let order = self
            .deps
            .db
            .get_order(&event.order_id)
            .await
            .map_err(|e| CommerceError::Internal(e.to_string()))?
            .filter(|o| o.payment_ref.as_deref() == Some(event.payment_ref.as_str()))
            .ok_or_else(|| {
                CommerceError::NotFound(format!(
                    "No order {} with payment {}",
                    event.order_id, event.payment_ref
                ))
            })?;

        // 決済サービスから確定取引IDが通知された場合は保存
        if let Some(tx_id) = &event.transaction_id {
            if order.transaction_id.as_deref() != Some(tx_id) {
                let _ = self
                    .deps
                    .db
                    .set_order_transaction_id(&order.id, tx_id)
                    .await;
            }
        }

        let status: OrderStatus = order.status.parse().map_err(CommerceError::Internal)?;
        let record_problem = |problem: &'static str, extra: serde_json::Value| {
            let mut details = json!({
                "number": order.number,
                "problem": problem,
                "event_id": event.event_id,
                "status": order.status,
            });
            if let (Some(map), serde_json::Value::Object(extra)) = (details.as_object_mut(), extra)
            {
                map.extend(extra);
            }
            audit
                .event(AuditAction::OrderUpdate, AuditOutcome::Failure)
                .with_resource("order", Some(order.id.clone()))
                .with_details(details)
        };
        match &event.kind {
            PaymentEventKind::Paid { amount, currency } => {
                if *amount != order.total || !currency.eq_ignore_ascii_case(&order.currency) {
                    tracing::error!(
                        "Payment for order {} does not match its total ({} {} paid, {} {} due)",
                        order.number,
                        amount,
                        currency,
                        order.total,
                        order.currency
                    );
                    self.deps
                        .audit
                        .record(record_problem(
                            "amount_mismatch",
                            json!({ "paid": amount, "paid_currency": currency, "total": order.total }),
                        ))
                        .await;
                    return Ok(());
                }
                match status {
                    OrderStatus::PendingPayment => {
                        self.transition(audit, &order, OrderStatus::Paid, None, "payment_webhook")
                            .await?;
                    }
                    OrderStatus::Cancelled => {
                        // 期限切れで取り消した後に支払われた。返金が必要
                        tracing::error!(
                            "Order {} was paid after it was cancelled; refund it from the payment dashboard",
                            order.number
                        );
                        self.deps
                            .audit
                            .record(record_problem("paid_after_cancel", json!({})))
                            .await;
                    }
                    // 重複した通知
                    _ => {}
                }
            }
            PaymentEventKind::Refunded {
                amount,
                currency: _,
                refund_ref,
            } => {
                let mut refunds: Vec<OrderRefundRecord> =
                    serde_json::from_str(&order.refunds_json).unwrap_or_default();
                if !refunds.iter().any(|r| r.refund_ref == *refund_ref) {
                    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
                    let refund_entry_id = format!("ref-{}", Uuid::new_v4().simple());
                    refunds.push(OrderRefundRecord {
                        id: refund_entry_id,
                        amount: *amount,
                        currency: order.currency.clone(),
                        refund_ref: refund_ref.clone(),
                        reason: Some("External webhook refund".to_string()),
                        created_at: now.clone(),
                    });
                    let refunds_json =
                        serde_json::to_string(&refunds).unwrap_or_else(|_| "[]".to_string());
                    let new_refunded_amount = order.refunded_amount + amount;
                    let is_full = new_refunded_amount >= order.total;
                    let new_status = if is_full {
                        Some(OrderStatus::Refunded.as_str().to_string())
                    } else {
                        None
                    };
                    let update = OrderRefundUpdate {
                        id: order.id.clone(),
                        refunded_amount: new_refunded_amount,
                        refunds_json,
                        new_status,
                        updated_at: now,
                        note: Some(format!("Refunded via webhook: {}", refund_ref)),
                        restock: vec![],
                    };
                    let _ = self.deps.db.record_order_refund(&update).await;
                }
            }
            PaymentEventKind::AwaitingPayment => {
                if status == OrderStatus::PendingPayment {
                    // 後払いの支払い期限まで在庫を確保したままにする
                    let hours = self.config.stripe.async_payment_due_hours.max(1) as i64;
                    self.deps
                        .db
                        .set_order_payment(
                            &order.id,
                            &event.payment_ref,
                            Some(&after(hours * 3600)),
                        )
                        .await
                        .map_err(|e| CommerceError::Internal(e.to_string()))?;
                }
            }
            PaymentEventKind::Expired | PaymentEventKind::Failed => {
                if status == OrderStatus::PendingPayment {
                    let source = if event.kind == PaymentEventKind::Expired {
                        "payment_expired"
                    } else {
                        "payment_failed"
                    };
                    self.transition(audit, &order, OrderStatus::Cancelled, None, source)
                        .await?;
                }
            }
        }
        Ok(())
    }

    /// 期限切れの支払い待ちの注文を取り消して在庫を戻す。取り消した数を返す
    pub async fn expire_orders(&self) -> CommerceResult<usize> {
        let threshold = after(-EXPIRY_GRACE_SECS);
        let expired = self
            .deps
            .db
            .list_expired_orders(&threshold, EXPIRY_BATCH)
            .await
            .map_err(|e| CommerceError::Internal(e.to_string()))?;
        let mut cancelled = 0;
        for order in expired {
            match self
                .transition(
                    &AuditOrigin::default(),
                    &order,
                    OrderStatus::Cancelled,
                    None,
                    "expired",
                )
                .await
            {
                Ok(_) => cancelled += 1,
                // 同時に支払われた・取り消された
                Err(CommerceError::Conflict(_)) => {}
                Err(e) => tracing::warn!("Failed to expire order {}: {}", order.number, e),
            }
        }
        Ok(cancelled)
    }

    /// 期限切れの注文を定期的に取り消すタスクを起動する
    pub fn spawn_expiry_task(self: &Arc<Self>) {
        let commerce = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                match commerce.expire_orders().await {
                    Ok(0) => {}
                    Ok(n) => {
                        tracing::info!("Cancelled {} expired order(s) and returned their stock", n)
                    }
                    Err(e) => tracing::warn!("Order expiry failed: {}", e),
                }
            }
        });
    }

    /// 状態の変化を顧客（と運営者）に知らせる。
    /// 決済ページで支払う注文は支払いが済んだ時点を「受付」とし、支払われずに期限切れ・失敗で
    /// 取り消された注文（受付を知らせていない）は知らせない
    fn notify_status_change(
        &self,
        order: &OrderRecord,
        from: OrderStatus,
        to: OrderStatus,
        source: &str,
    ) {
        let manual = order.payment_provider == PaymentProviderKind::Manual.as_str();
        match to {
            OrderStatus::Paid if !manual && from == OrderStatus::PendingPayment => {
                self.send_notification("order.placed", order, true, json!({}));
            }
            OrderStatus::Paid => self.send_notification("order.paid", order, false, json!({})),
            OrderStatus::Shipped => {
                self.send_notification("order.shipped", order, false, json!({}))
            }
            OrderStatus::Cancelled if manual || from != OrderStatus::PendingPayment => {
                let was_paid = from != OrderStatus::PendingPayment;
                self.send_notification(
                    "order.cancelled",
                    order,
                    true,
                    json!({ "reason": source, "was_paid": was_paid }),
                );
            }
            _ => {}
        }
    }

    /// 注文の通知を送る（`[notifications]` が無効なら何もしない）。`extra` はテンプレートに渡す値
    fn send_notification(
        &self,
        kind: &str,
        order: &OrderRecord,
        to_staff: bool,
        extra: serde_json::Value,
    ) {
        let Some(notifier) = &self.deps.notifier else {
            return;
        };
        let mut recipients = vec![Recipient::person(
            order.customer_email.clone(),
            order.customer_name.clone(),
        )];
        if to_staff {
            recipients.push(Recipient::Staff);
        }
        let mut data = json!({ "order": order_notification_data(order) });
        if let (Some(map), serde_json::Value::Object(extra)) = (data.as_object_mut(), extra) {
            map.extend(extra);
        }
        notifier.notify(NotificationEvent::new(kind, recipients, data));
    }

    /// `on-order-status-changed` プラグインへの通知（応答を待たない）
    fn notify(&self, order: &OrderRecord, from: Option<OrderStatus>, to: OrderStatus) {
        let change = OrderStatusChange {
            order: order_context(order),
            from_status: from,
            to_status: Some(to),
            occurred_at: now(),
        };
        let supervisor = self.deps.supervisor.clone();
        let audit = self.deps.audit.clone();
        let order_id = order.id.clone();
        tokio::spawn(async move {
            let failures = {
                let supervisor = supervisor.read().await;
                supervisor.notify_order_status_changed(&change).await
            };
            for (plugin, error) in failures {
                tracing::warn!(
                    "Plugin '{}' on-order-status-changed failed: {}",
                    plugin,
                    error
                );
                audit
                    .record(
                        AuditOrigin::default()
                            .event(AuditAction::PluginError, AuditOutcome::Failure)
                            .with_resource("order", Some(order_id.clone()))
                            .with_details(json!({
                                "hook": "on-order-status-changed",
                                "plugin": plugin,
                                "error": error.to_string(),
                            })),
                    )
                    .await;
            }
        });
    }
}

/// 通知のテンプレートに渡す注文（`order_context` + 注文番号・状態・日時・メモ）
fn order_notification_data(order: &OrderRecord) -> serde_json::Value {
    let mut data = serde_json::to_value(order_context(order)).unwrap_or_else(|_| json!({}));
    data["number"] = json!(order.number);
    data["status"] = json!(order.status);
    data["created_at"] = json!(order.created_at);
    data["paid_at"] = json!(order.paid_at);
    data["note"] = json!(order.note);
    data
}

/// 保存済みの注文をプラグイン・API に渡す形にする
pub fn order_context(order: &OrderRecord) -> OrderContext {
    OrderContext {
        order_id: Some(order.id.clone()),
        order_number: Some(order.number.clone()),
        currency: order.currency.clone(),
        lines: serde_json::from_str(&order.lines_json).unwrap_or_default(),
        adjustments: serde_json::from_str::<Vec<OrderAdjustment>>(&order.adjustments_json)
            .unwrap_or_default(),
        subtotal: order.subtotal,
        total: order.total,
        customer_email: order.customer_email.clone(),
        customer_name: order.customer_name.clone(),
        shipping_address: order
            .shipping_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok()),
        coupon_code: order.coupon_code.clone(),
        status: order.status.parse().ok(),
        metadata: serde_json::from_str(&order.metadata_json).unwrap_or_default(),
    }
}
