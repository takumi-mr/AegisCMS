use aegis_plugin_sdk::prelude::*;

/// 末尾に追加する署名段落
const FOOTER_TEXT: &str = "— Enhanced by AegisCMS Rust Wasm Plugin —";

#[derive(Default)]
pub struct TemplatePlugin;

#[aegis_plugin]
impl Plugin for TemplatePlugin {
    /// 記事が保存される直前に呼ばれるフック
    fn on_before_save(&mut self, mut post: PostContext) -> Result<PostContext> {
        log_info!("Executing on_before_save hook for post: '{}'", post.title);

        // 1. タイトルの自動トリムとフォーマット
        post.title = post.title.trim().to_string();

        // 2. 外部API通信の権限チェック (Manifest の allowed_domains と照合)
        let endpoint = "https://hooks.slack.com/services/xxx";
        if net::is_allowed(endpoint) {
            log_info!("Network access to '{}' is authorized.", endpoint);
        } else {
            log_warn!(
                "Network access to '{}' is denied by capability guard.",
                endpoint
            );
        }

        // 3. プラグイン専用 KVS への状態保存
        let save_record = serde_json::json!({
            "last_slug": post.slug,
            "title": post.title,
            "timestamp": "2026-10-22T00:00:00Z"
        });
        if let Err(e) = kv::set_json("last_saved_post", &save_record) {
            log_warn!("Failed to save state to KVS: {}", e);
        } else {
            log_info!("Successfully recorded last saved post state to KVS.");
        }

        Ok(post)
    }

    /// コンテンツの AST をレンダリング・保存前に改変するフック
    fn transform_content_ast(&mut self, mut blocks: Vec<BlockNode>) -> Result<Vec<BlockNode>> {
        log_info!(
            "Executing transform_content_ast hook on {} blocks",
            blocks.len()
        );

        // 1. 各ブロックの走査・加工例: 見出しブロックにカスタム属性を付加
        for block in blocks.iter_mut() {
            if block.name == "core/heading" {
                block.set_attribute(
                    "data-enhanced-by",
                    serde_json::json!("aegis-template-plugin"),
                );
            }
        }

        // 2. プラグインによる新規ブロックの末尾自動追加 (例: 署名/フッター)
        // このフックは保存済み（変換済み）のブロックにも再適用される（再保存・エディタのプレビュー）ため、
        // 追加済みなら追加しない（冪等にする）
        let has_footer = blocks
            .iter()
            .any(|b| b.name == "core/paragraph" && b.text.as_deref() == Some(FOOTER_TEXT));
        if !has_footer {
            blocks.push(BlockNode::paragraph(FOOTER_TEXT));
        }

        Ok(blocks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_plugin_lifecycle() {
        let mut plugin = TemplatePlugin::default();

        // 1. on_before_save のテスト
        let post = PostContext::new("  My Untrimmed Title  ", "test-slug");
        let result = plugin.on_before_save(post).expect("Hook should succeed");
        assert_eq!(result.title, "My Untrimmed Title");

        // 2. transform_content_ast のテスト
        let blocks = vec![BlockNode::heading(2, "Section 1")];
        let transformed = plugin
            .transform_content_ast(blocks)
            .expect("AST hook should succeed");
        assert_eq!(transformed.len(), 2);
        assert_eq!(
            transformed[0].attribute("data-enhanced-by"),
            Some(&serde_json::json!("aegis-template-plugin"))
        );
        assert_eq!(transformed[1].name, "core/paragraph");
    }

    #[test]
    fn test_transform_content_ast_is_idempotent() {
        let mut plugin = TemplatePlugin::default();
        let once = plugin
            .transform_content_ast(vec![BlockNode::heading(2, "Section 1")])
            .expect("AST hook should succeed");
        // Saved blocks are transformed again on re-save and on every editor preview
        let twice = plugin
            .transform_content_ast(once.clone())
            .expect("AST hook should succeed");
        assert_eq!(twice, once);
    }
}
