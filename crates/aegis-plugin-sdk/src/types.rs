use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// 記事保存時にプラグインへ渡されるコンテキスト
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PostContext {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    pub slug: String,
    #[serde(default)]
    pub author_id: String,
    #[serde(default)]
    pub blocks_json: String,
}

impl PostContext {
    pub fn new(title: impl Into<String>, slug: impl Into<String>) -> Self {
        Self {
            id: String::new(),
            title: title.into(),
            slug: slug.into(),
            author_id: String::new(),
            blocks_json: "[]".to_string(),
        }
    }

    /// blocks_json を BlockNode のベクタとしてパースする
    pub fn blocks(&self) -> Result<Vec<BlockNode>> {
        if self.blocks_json.is_empty() {
            return Ok(Vec::new());
        }
        serde_json::from_str(&self.blocks_json).map_err(Into::into)
    }

    /// BlockNode のベクタを JSON 文字列として blocks_json に格納する
    pub fn set_blocks(&mut self, blocks: &[BlockNode]) -> Result<()> {
        self.blocks_json = serde_json::to_string(blocks)?;
        Ok(())
    }
}

/// 構造化コンテンツの木構造ブロックノード (AST)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlockNode {
    pub name: String,
    /// ホストは属性の無いブロックを `"attributes": null` で渡すことがあるため、null は空として読む
    #[serde(default, deserialize_with = "null_as_empty_map")]
    pub attributes: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub children: Vec<BlockNode>,
}

fn null_as_empty_map<'de, D>(
    deserializer: D,
) -> std::result::Result<HashMap<String, serde_json::Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(
        Option::<HashMap<String, serde_json::Value>>::deserialize(deserializer)?
            .unwrap_or_default(),
    )
}

impl BlockNode {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            attributes: HashMap::new(),
            text: None,
            children: Vec::new(),
        }
    }

    pub fn paragraph(text: impl Into<String>) -> Self {
        Self {
            name: "core/paragraph".to_string(),
            attributes: HashMap::new(),
            text: Some(text.into()),
            children: Vec::new(),
        }
    }

    pub fn heading(level: u8, text: impl Into<String>) -> Self {
        let mut attributes = HashMap::new();
        attributes.insert("level".to_string(), serde_json::json!(level));
        Self {
            name: "core/heading".to_string(),
            attributes,
            text: Some(text.into()),
            children: Vec::new(),
        }
    }

    pub fn code(language: impl Into<String>, code: impl Into<String>) -> Self {
        let mut attributes = HashMap::new();
        attributes.insert("language".to_string(), serde_json::json!(language.into()));
        Self {
            name: "core/code".to_string(),
            attributes,
            text: Some(code.into()),
            children: Vec::new(),
        }
    }

    pub fn attribute(&self, key: &str) -> Option<&serde_json::Value> {
        self.attributes.get(key)
    }

    pub fn set_attribute(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.attributes.insert(key.into(), value);
    }
}

/// プラグイン処理中のエラー型
#[derive(Debug, Clone, PartialEq)]
pub enum PluginError {
    /// 入力を受け付けない（`on_before_save` で保存を拒否する）。メッセージはそのまま API の応答に出る
    Rejected(String),
    Execution(String),
    Serialization(String),
    Host(String),
}

impl fmt::Display for PluginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rejected(msg) => write!(f, "{}", msg),
            Self::Execution(msg) => write!(f, "Execution error: {}", msg),
            Self::Serialization(msg) => write!(f, "Serialization error: {}", msg),
            Self::Host(msg) => write!(f, "Host API error: {}", msg),
        }
    }
}

impl std::error::Error for PluginError {}

impl PluginError {
    /// 再試行しても同じ結果になる失敗か（`send-notification` でホストが再試行するかを決める）。
    /// `Rejected`（送らないと決めた）と `Serialization`（入力が読めない）は再試行しない
    pub fn is_permanent(&self) -> bool {
        matches!(self, Self::Rejected(_) | Self::Serialization(_))
    }
}

impl From<serde_json::Error> for PluginError {
    fn from(err: serde_json::Error) -> Self {
        Self::Serialization(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, PluginError>;

/// 認証後フック (`on-user-authenticated`) に渡されるログインユーザーの情報
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct UserAuthContext {
    /// 認証したプロバイダ (例: "local", "oidc")。変更してもホストは無視する
    pub provider: String,
    /// セッションの `user_id`。変更してもホストは無視する
    pub user_id: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub display_name: Option<String>,
    /// `admin` / `editor` / `viewer` のいずれか（それ以外を返すとログインは失敗する）
    pub role: String,
    /// プロバイダ固有の属性（IdP のクレーム・グループなど）。返した値はセッションの `metadata` に入る
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// ページレンダリング時にプラグインへ渡されるコンテキスト
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ContentSummary {
    pub id: String,
    pub content_type: String,
    pub title: String,
    pub slug: String,
    #[serde(default)]
    pub excerpt: String,
    pub author_id: Option<String>,
    pub published_at: Option<String>,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub fields: HashMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub relations: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PageRenderContext {
    pub site_title: String,
    pub site_description: String,
    pub template: String,
    pub content_type: String,
    pub title: String,
    pub slug: String,
    pub content_html: String,
    pub author: Option<String>,
    pub published_at: Option<String>,
    pub updated_at: Option<String>,
    #[serde(default)]
    pub fields: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub relations: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub items: Vec<ContentSummary>,
    #[serde(default)]
    pub extra_css: String,
    #[serde(default)]
    pub extra_head: String,
    #[serde(default)]
    pub theme_vars: HashMap<String, String>,
}

/// 注文の 1 行（商品 × 数量）。価格はホストが商品から決める（プラグインが変えても無視される）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct OrderLine {
    pub product_id: String,
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub sku: Option<String>,
    /// 単価（通貨の最小単位の整数。JPY なら円）
    pub unit_price: i64,
    pub quantity: u32,
    pub line_total: i64,
}

/// 送料・税・割引などの加算・減算（割引は負の金額）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct OrderAdjustment {
    /// `shipping` / `tax` / `discount` / `fee` など
    pub kind: String,
    /// 表示名（カート・注文確認ページ・決済ページに出る）
    pub label: String,
    pub amount: i64,
    #[serde(default)]
    pub code: Option<String>,
    /// 追加したプラグイン（ホストが設定する）
    #[serde(default)]
    pub source: String,
}

impl OrderAdjustment {
    pub fn new(kind: impl Into<String>, label: impl Into<String>, amount: i64) -> Self {
        Self {
            kind: kind.into(),
            label: label.into(),
            amount,
            ..Default::default()
        }
    }
}

/// 配送先
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct ShippingAddress {
    pub name: String,
    pub postal_code: String,
    pub region: String,
    pub city: String,
    pub line1: String,
    pub line2: String,
    pub phone: String,
    pub country: String,
}

/// 見積もり・注文（`calculate-order` / `on-order-status-changed`）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct OrderContext {
    #[serde(default)]
    pub order_id: Option<String>,
    #[serde(default)]
    pub order_number: Option<String>,
    /// ISO 4217 の通貨コード（小文字）
    pub currency: String,
    pub lines: Vec<OrderLine>,
    #[serde(default)]
    pub adjustments: Vec<OrderAdjustment>,
    /// 商品の合計
    pub subtotal: i64,
    /// 支払う金額（ホストが `subtotal` + `adjustments` で計算し直す）
    pub total: i64,
    #[serde(default)]
    pub customer_email: String,
    #[serde(default)]
    pub customer_name: String,
    #[serde(default)]
    pub shipping_address: Option<ShippingAddress>,
    #[serde(default)]
    pub coupon_code: Option<String>,
    /// `pending_payment` / `paid` / `shipped` / `completed` / `cancelled`（見積もり中は `None`）
    #[serde(default)]
    pub status: Option<String>,
    /// プラグインが付ける任意の値（注文に保存される）
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl OrderContext {
    /// 送料・税などを除いた、割引を含む商品の金額（`subtotal` + 割引）
    pub fn discounted_subtotal(&self) -> i64 {
        self.subtotal
            + self
                .adjustments
                .iter()
                .filter(|a| a.amount < 0)
                .map(|a| a.amount)
                .sum::<i64>()
    }
}

/// 注文の作成・状態の変化（`on-order-status-changed`）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct OrderStatusChange {
    pub order: OrderContext,
    /// 変化前の状態（注文の作成時は `None`）
    #[serde(default)]
    pub from_status: Option<String>,
    #[serde(default)]
    pub to_status: Option<String>,
    pub occurred_at: String,
}

/// 通知の宛先（`send-notification`）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Recipient {
    /// 特定の人（注文した顧客など）
    Person {
        email: String,
        #[serde(default)]
        name: Option<String>,
    },
    /// サイトの運営者（宛先はプラグインの設定で決める。Slack のチャンネルなど）
    Staff,
}

/// テンプレートから作った、1 つの宛先への通知（`send-notification`）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NotificationMessage {
    /// 通知の種類（`order.placed` / `order.shipped` / `audit.session.create` など）
    pub kind: String,
    pub recipient: Recipient,
    pub subject: String,
    /// 本文（プレーンテキスト）
    pub body: String,
    /// テンプレートに渡した値（`order` / `audit` / `site_name` など）
    #[serde(default)]
    pub data: serde_json::Value,
}

/// 通知の配信結果（`send-notification`）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    Sent,
    /// 扱わない通知（種類・宛先）。失敗ではなく、ホストは監査ログにも残さない
    Skipped,
}

/// AegisCMS プラグインが実装するコアトレイト
pub trait Plugin: Default {
    /// 記事保存前フック (`on-before-save`)
    /// 記事の検証、タイトルの整形、自動タグ付け、監査ログ送信などを実行可能
    fn on_before_save(&mut self, post: PostContext) -> Result<PostContext> {
        Ok(post)
    }

    /// コンテンツ AST 変換フック (`transform-content-ast`)
    /// 見出しへのアンカー自動付与、ショートコード展開、シンタックスハイライト加工などを実行可能
    fn transform_content_ast(&mut self, blocks: Vec<BlockNode>) -> Result<Vec<BlockNode>> {
        Ok(blocks)
    }

    /// ページ描画前フック (`on-render-page`)
    /// テーマの動的切り替え、カスタムCSS/メタタグのインジェクション、テーマ変数の書き換えなどを実行可能
    fn on_render_page(&mut self, ctx: PageRenderContext) -> Result<PageRenderContext> {
        Ok(ctx)
    }

    /// 認証後フック (`on-user-authenticated`)
    /// ログイン成功後・セッション発行前に呼ばれる。IdP のグループからのロールマッピング、
    /// 表示名や属性の補完、ログイン通知などを実行可能。`Err` を返すとログインを拒否する
    fn on_user_authenticated(&mut self, ctx: UserAuthContext) -> Result<UserAuthContext> {
        Ok(ctx)
    }

    /// 注文の計算フック (`calculate-order`)
    /// カートの見積もり・注文の確定時に呼ばれる。送料・税・割引を `adjustments` に追加する。
    /// 変えられるのは `adjustments` と `metadata` だけで、`Err`（`PluginError::Rejected`）を返すと
    /// 注文を断る（無効なクーポン、配送できない地域など。メッセージは顧客に表示される）
    fn calculate_order(&mut self, ctx: OrderContext) -> Result<OrderContext> {
        Ok(ctx)
    }

    /// 注文の通知フック (`on-order-status-changed`)
    /// 注文の作成（`from_status` が `None`）と状態の変化のたびに呼ばれる。通知・外部サービスへの連携用で、
    /// 結果は注文に影響しない（`Err` はログと監査ログに残る）
    fn on_order_status_changed(&mut self, change: OrderStatusChange) -> Result<()> {
        let _ = change;
        Ok(())
    }

    /// 通知の配信フック (`send-notification`)
    /// サーバーの `[notifications].channels` に `"plugin:<プラグイン名>"` と書くと、通知（注文・監査イベント）の
    /// たびに呼ばれる。件名・本文はホストがテンプレートから作ってある。`http::fetch` で Slack・SMS などへ送る。
    /// 扱わない通知は `Ok(Delivery::Skipped)`。`Err` はホストが間隔を空けて再試行する
    /// （`PluginError::Rejected` / `Serialization` は再試行しない）
    fn send_notification(&mut self, message: NotificationMessage) -> Result<Delivery> {
        let _ = message;
        Ok(Delivery::Skipped)
    }
}
