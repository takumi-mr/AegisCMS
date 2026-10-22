package main

import (
	"encoding/json"
	"strings"

	"aegis-plugin-template-tinygo/sdk"
)

//export allocate
func allocate(size int32) uintptr {
	return sdk.Allocate(size)
}

//export on_before_save
func on_before_save(ptr int32, length int32) int64 {
	inBytes := sdk.ReadBytes(uintptr(ptr), length)

	var post sdk.PostContext
	if err := json.Unmarshal(inBytes, &post); err != nil {
		sdk.LogError("[TinyGo] Failed to deserialize PostContext: " + err.Error())
		return sdk.WriteError("Invalid PostContext payload")
	}

	sdk.LogInfo("[TinyGo] on_before_save called for: " + post.Title)

	// タイトル検証: 空なら保存拒否 (422 Unprocessable Entity)
	if strings.TrimSpace(post.Title) == "" {
		return sdk.WriteError("Post title cannot be empty")
	}

	// 外部ネットワーク権限チェック
	if sdk.NetIsAllowed("https://api.example.com/webhook") {
		sdk.LogInfo("[TinyGo] Network access to api.example.com is permitted")
	}

	// KVS 永続化
	sdk.KVSet("last_saved_slug", post.Slug)

	// タイトル加工
	if !strings.HasPrefix(post.Title, "[TinyGo] ") {
		post.Title = "[TinyGo] " + post.Title
	}

	return sdk.WriteJSON(&post)
}

//export transform_content_ast
func transform_content_ast(ptr int32, length int32) int64 {
	inBytes := sdk.ReadBytes(uintptr(ptr), length)

	var blocks []sdk.BlockNode
	if err := json.Unmarshal(inBytes, &blocks); err != nil {
		sdk.LogError("[TinyGo] Failed to deserialize BlockNode list: " + err.Error())
		return sdk.PackPtrLen(uintptr(ptr), length)
	}

	sdk.LogInfo("[TinyGo] transform_content_ast called")

	// サンプル: 末尾にプラグイン段落を追加
	blocks = append(blocks, sdk.NewParagraph("Transformed by TinyGo Plugin"))

	return sdk.WriteJSON(&blocks)
}

//export on_render_page
func on_render_page(ptr int32, length int32) int64 {
	inBytes := sdk.ReadBytes(uintptr(ptr), length)

	var ctx sdk.PageRenderContext
	if err := json.Unmarshal(inBytes, &ctx); err != nil {
		sdk.LogError("[TinyGo] Failed to deserialize PageRenderContext: " + err.Error())
		return sdk.PackPtrLen(uintptr(ptr), length)
	}

	sdk.LogInfo("[TinyGo] on_render_page called for: " + ctx.SiteTitle)

	// テーマ修飾
	ctx.ExtraCSS += "\n/* Injected by TinyGo Plugin */\n.tinygo-badge { color: #00add8; font-weight: bold; }"

	return sdk.WriteJSON(&ctx)
}

// main は TinyGo Wasm ターゲットで必要とされる空のエントリポイントです
func main() {}
