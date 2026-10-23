//! 決済サービスの抽象（`[commerce].provider`）。決済ページへの誘導と、入金・期限切れの通知（Webhook）の解釈

use aegis_core::{AegisError, OrderContext, Result};
use async_trait::async_trait;
use http::HeaderMap;

use aegis_db::OrderRecord;

/// 決済後に戻る URL
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReturnUrls {
    /// 注文確認ページ（`/orders/{id}?token=...`）
    pub success_url: String,
    /// 決済をやめたときに戻るカートページ
    pub cancel_url: String,
}

/// 決済の開始結果
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PaymentStart {
    /// 顧客を移動させる決済ページ（無ければ注文確認ページへ）
    pub redirect_url: Option<String>,
    /// 決済サービス側の ID（Webhook で注文を見つけるのに使う）
    pub payment_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaymentEventKind {
    /// 入金された（金額・通貨は注文と照合する）
    Paid { amount: i64, currency: String },
    /// 決済ページは完了したが、支払いは後で行われる（コンビニ払いなど）
    AwaitingPayment,
    /// 決済ページの期限切れ
    Expired,
    /// 後払いの支払いが失敗した
    Failed,
}

/// Webhook から読み取った決済の出来事
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentEvent {
    /// 決済サービスのイベント ID（監査ログ用）
    pub event_id: String,
    /// 注文 ID（決済の開始時に渡したもの）
    pub order_id: String,
    pub payment_ref: String,
    pub kind: PaymentEventKind,
}

#[async_trait]
pub trait PaymentProvider: Send + Sync {
    /// `manual` / `stripe`（注文の `payment_provider` に保存する）
    fn id(&self) -> &'static str;

    /// 管理者が手動で「支払い済み」にできるか（入金を Webhook で確かめる決済では不可）
    fn allows_manual_confirmation(&self) -> bool;

    /// 決済を始める（決済ページの作成など）
    async fn start_payment(
        &self,
        order: &OrderRecord,
        ctx: &OrderContext,
        urls: &ReturnUrls,
    ) -> Result<PaymentStart>;

    /// Webhook の署名を確かめて内容を読む。注文に関係しないイベントは `None`
    fn parse_webhook(&self, headers: &HeaderMap, body: &[u8]) -> Result<Option<PaymentEvent>>;
}

/// 銀行振込・代引きなど。決済ページは無く、管理者が入金を確認する
pub struct ManualPaymentProvider;

#[async_trait]
impl PaymentProvider for ManualPaymentProvider {
    fn id(&self) -> &'static str {
        "manual"
    }

    fn allows_manual_confirmation(&self) -> bool {
        true
    }

    async fn start_payment(
        &self,
        _order: &OrderRecord,
        _ctx: &OrderContext,
        _urls: &ReturnUrls,
    ) -> Result<PaymentStart> {
        Ok(PaymentStart::default())
    }

    fn parse_webhook(&self, _headers: &HeaderMap, _body: &[u8]) -> Result<Option<PaymentEvent>> {
        Err(AegisError::NotFound(
            "The manual payment provider has no webhook".to_string(),
        ))
    }
}
