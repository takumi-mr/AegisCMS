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

/// 決済プロバイダに送る返金リクエスト
#[derive(Debug, Clone)]
pub struct RefundRequest<'a> {
    pub order: &'a OrderRecord,
    /// 返金する金額（通貨の最小単位。JPYなら円）
    pub amount: i64,
    /// 返金理由
    pub reason: Option<String>,
}

/// 決済プロバイダから返る返金結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefundResult {
    /// 決済サービス側の返金 ID（Stripe の re_xxx など）
    pub refund_ref: String,
    /// 返金額
    pub amount: i64,
    /// 通貨コード
    pub currency: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaymentEventKind {
    /// 入金された（金額・通貨は注文と照合する）
    Paid { amount: i64, currency: String },
    /// 返金された（外部ダッシュボードや Webhook での返金検知）
    Refunded {
        amount: i64,
        currency: String,
        refund_ref: String,
    },
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
    /// 確定取引ID（Stripe payment_intent 等）
    pub transaction_id: Option<String>,
    pub kind: PaymentEventKind,
}

#[async_trait]
pub trait PaymentProvider: Send + Sync {
    /// `manual` / `stripe`（注文の `payment_provider` に保存する）
    fn id(&self) -> &'static str;

    /// 管理者が手動で「支払い済み」にできるか（入金を Webhook で確かめる決済では不可）
    fn allows_manual_confirmation(&self) -> bool;

    /// オンライン返金 API をサポートしているか（外部連携決済は true、manual は false）
    fn supports_refund(&self) -> bool {
        false
    }

    /// 決済を始める（決済ページの作成など）
    async fn start_payment(
        &self,
        order: &OrderRecord,
        ctx: &OrderContext,
        urls: &ReturnUrls,
    ) -> Result<PaymentStart>;

    /// 返金を処理する（外部連携決済プロバイダが実装する）
    async fn refund(&self, req: RefundRequest<'_>) -> Result<RefundResult> {
        let _ = req;
        Err(AegisError::Validation(format!(
            "Payment provider '{}' does not support online refunds",
            self.id()
        )))
    }

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
