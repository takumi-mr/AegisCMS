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
type PageRenderContext struct {
	SiteTitle    string                 `json:"site_title"`
	PageTitle    string                 `json:"page_title"`
	Slug         string                 `json:"slug"`
	TemplateName string                 `json:"template_name"`
	ExtraCSS     string                 `json:"extra_css"`
	ExtraHead    string                 `json:"extra_head"`
	Variables    map[string]interface{} `json:"variables"`
}

// ErrorEnvelope はフック拒否時にホストへ返却するエラー構造体
type ErrorEnvelope struct {
	Error string `json:"error"`
}
