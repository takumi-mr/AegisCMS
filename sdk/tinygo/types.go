package sdk

// PostContext は保存直前にプラグインへ渡されるコンテンツコンテキスト
type PostContext struct {
	ID          string `json:"id"`
	Title       string `json:"title"`
	Slug        string `json:"slug"`
	ContentType string `json:"content_type"`
	Status      string `json:"status"`
	AuthorID    string `json:"author_id"`
	BlocksJSON  string `json:"blocks_json"`
	CreatedAt   int64  `json:"created_at"`
	UpdatedAt   int64  `json:"updated_at"`
}

// BlockNode は AST 内の個々のコンテンツブロックノード
type BlockNode struct {
	Name       string                 `json:"name"`
	Attributes map[string]interface{} `json:"attributes,omitempty"`
	Children   []BlockNode            `json:"children"`
	Text       *string                `json:"text,omitempty"`
}

// NewParagraph は段落ブロックノードを生成
func NewParagraph(text string) BlockNode {
	return BlockNode{
		Name:       "core/paragraph",
		Attributes: make(map[string]interface{}),
		Children:   make([]BlockNode, 0),
		Text:       &text,
	}
}

// NewHeading は見出しブロックノードを生成
func NewHeading(level int, text string) BlockNode {
	attrs := map[string]interface{}{
		"level": level,
	}
	return BlockNode{
		Name:       "core/heading",
		Attributes: attrs,
		Children:   make([]BlockNode, 0),
		Text:       &text,
	}
}

// PageRenderContext は公開ページ描画時にプラグインへ渡されるレンダリングコンテキスト
//
// ホストは返した JSON のフィールドだけを上書きするため、記事一覧 (items) やカスタムフィールドなど
// ここに無いフィールドはそのまま保たれる
type PageRenderContext struct {
	SiteTitle       string            `json:"site_title"`
	SiteDescription string            `json:"site_description"`
	Template        string            `json:"template"`
	ContentType     string            `json:"content_type"`
	Title           string            `json:"title"`
	Slug            string            `json:"slug"`
	ContentHTML     string            `json:"content_html"`
	ExtraCSS        string            `json:"extra_css"`
	ExtraHead       string            `json:"extra_head"`
	ThemeVars       map[string]string `json:"theme_vars,omitempty"`
}

// ErrorEnvelope はフック拒否時にホストへ返却するエラー構造体
type ErrorEnvelope struct {
	Error string `json:"error"`
}

// OrderLine は注文の 1 行 (商品 × 数量)。価格はホストが商品から決める (プラグインが変えても無視される)
type OrderLine struct {
	ProductID    string  `json:"product_id"`
	VariantID    *string `json:"variant_id,omitempty"`
	VariantTitle *string `json:"variant_title,omitempty"`
	Slug         string  `json:"slug"`
	Title        string  `json:"title"`
	SKU          *string `json:"sku,omitempty"`
	// UnitPrice は単価 (通貨の最小単位の整数。JPY なら円)
	UnitPrice int64 `json:"unit_price"`
	Quantity  int64 `json:"quantity"`
	LineTotal int64 `json:"line_total"`
}

// OrderAdjustment は送料・税・割引などの加算・減算 (割引は負の金額)
type OrderAdjustment struct {
	// Kind は shipping / tax / discount / fee など
	Kind string `json:"kind"`
	// Label は表示名 (カート・注文確認ページ・決済ページに出る)
	Label  string  `json:"label"`
	Amount int64   `json:"amount"`
	Code   *string `json:"code,omitempty"`
	// Source は追加したプラグイン (ホストが設定する)
	Source string `json:"source,omitempty"`
}

// NewOrderAdjustment は加算・減算を生成
func NewOrderAdjustment(kind, label string, amount int64) OrderAdjustment {
	return OrderAdjustment{Kind: kind, Label: label, Amount: amount}
}

// ShippingAddress は配送先
type ShippingAddress struct {
	Name       string `json:"name"`
	PostalCode string `json:"postal_code"`
	Region     string `json:"region"`
	City       string `json:"city"`
	Line1      string `json:"line1"`
	Line2      string `json:"line2"`
	Phone      string `json:"phone"`
	Country    string `json:"country"`
}

// OrderContext は見積もり・注文 (calculate-order / on-order-status-changed)
//
// calculate-order でホストが反映するのは Adjustments と Metadata だけ
type OrderContext struct {
	OrderID     *string `json:"order_id,omitempty"`
	OrderNumber *string `json:"order_number,omitempty"`
	// Currency は ISO 4217 の通貨コード (小文字)
	Currency    string            `json:"currency"`
	Lines       []OrderLine       `json:"lines"`
	Adjustments []OrderAdjustment `json:"adjustments"`
	// Subtotal は商品の合計
	Subtotal int64 `json:"subtotal"`
	// Total は支払う金額 (ホストが Subtotal + Adjustments で計算し直す)
	Total           int64            `json:"total"`
	CustomerEmail   string           `json:"customer_email"`
	CustomerName    string           `json:"customer_name"`
	ShippingAddress *ShippingAddress `json:"shipping_address,omitempty"`
	CouponCode      *string          `json:"coupon_code,omitempty"`
	// Status は pending_payment / paid / shipped / completed / cancelled (見積もり中は nil)
	Status *string `json:"status,omitempty"`
	// Metadata はプラグインが付ける任意の値 (注文に保存される)
	Metadata map[string]interface{} `json:"metadata,omitempty"`
}

// DiscountedSubtotal は送料・税などを除いた、割引を含む商品の金額 (Subtotal + 割引)
func (o *OrderContext) DiscountedSubtotal() int64 {
	total := o.Subtotal
	for _, a := range o.Adjustments {
		if a.Amount < 0 {
			total += a.Amount
		}
	}
	return total
}

// OrderStatusChange は注文の作成・状態の変化 (on-order-status-changed)
type OrderStatusChange struct {
	Order OrderContext `json:"order"`
	// FromStatus は変化前の状態 (注文の作成時は nil)
	FromStatus *string `json:"from_status,omitempty"`
	ToStatus   *string `json:"to_status,omitempty"`
	OccurredAt string  `json:"occurred_at"`
}

// PublishedContent はコンテンツ公開の通知 (on-content-published)
type PublishedContent struct {
	ID          string                 `json:"id"`
	ContentType string                 `json:"content_type"`
	Title       string                 `json:"title"`
	Slug        string                 `json:"slug"`
	PublishedAt string                 `json:"published_at"`
	AuthorID    *string                `json:"author_id,omitempty"`
	Fields      map[string]interface{} `json:"fields,omitempty"`
}

// CommentSubmission は公開サイトから投稿されたコメント (check-comment。保存する前)
type CommentSubmission struct {
	ContentType  string `json:"content_type"`
	ContentID    string `json:"content_id"`
	ContentTitle string `json:"content_title"`
	ContentSlug  string `json:"content_slug"`
	// ParentID は返信先のコメント
	ParentID    *string `json:"parent_id,omitempty"`
	AuthorName  string  `json:"author_name"`
	AuthorEmail string  `json:"author_email"`
	// Body は本文 (プレーンテキスト)
	Body      string  `json:"body"`
	IP        *string `json:"ip,omitempty"`
	UserAgent *string `json:"user_agent,omitempty"`
	// Status はホストの判定 (approved / pending)
	Status string `json:"status"`
}

// CommentVerdict はコメントの審査結果 (check-comment)
//
// Allow はホストの判定のまま、Hold は承認待ちに、Spam はスパムにする (ホストの判定より厳しくだけできる)。
// 投稿そのものを断るときは Reject を返す
type CommentVerdict struct {
	Verdict string `json:"verdict"`
	Reason  string `json:"reason,omitempty"`
}

// AllowComment はホストの判定のままにする
func AllowComment() CommentVerdict { return CommentVerdict{Verdict: "allow"} }

// HoldComment はコメントを承認待ちにする
func HoldComment(reason string) CommentVerdict { return CommentVerdict{Verdict: "hold", Reason: reason} }

// SpamComment はコメントをスパムにする
func SpamComment(reason string) CommentVerdict { return CommentVerdict{Verdict: "spam", Reason: reason} }

// Recipient は通知の宛先 (send-notification)。Type は "staff" (サイトの運営者) か "person" (特定の人)
type Recipient struct {
	Type  string `json:"type"`
	Email string `json:"email,omitempty"`
	Name  string `json:"name,omitempty"`
}

// NotificationMessage はテンプレートから作った、1 つの宛先への通知 (send-notification)
type NotificationMessage struct {
	// Kind は通知の種類 (order.placed / order.shipped / audit.session.create など)
	Kind      string    `json:"kind"`
	Recipient Recipient `json:"recipient"`
	Subject   string    `json:"subject"`
	// Body は本文 (プレーンテキスト)
	Body string `json:"body"`
	// HTML は HTML の本文 ([notifications].html が有効なときだけ入る)
	HTML *string `json:"html,omitempty"`
	// Data はテンプレートに渡した値 (order / audit / site_name など)
	Data map[string]interface{} `json:"data,omitempty"`
}

// Delivery は通知の配信結果 (send-notification)
type Delivery int

const (
	// DeliverySent は送った
	DeliverySent Delivery = iota
	// DeliverySkipped は扱わない通知 (種類・宛先)。失敗ではなく、ホストは監査ログにも残さない
	DeliverySkipped
)
