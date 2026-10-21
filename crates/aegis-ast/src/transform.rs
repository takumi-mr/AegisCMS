use crate::node::BlockNode;

pub trait AstTransformer: Send + Sync {
    fn transform(&self, nodes: Vec<BlockNode>) -> Vec<BlockNode>;
}

/// 見出し（core/heading）に slug/id を自動付加するトランスフォーマー
#[derive(Default)]
pub struct HeadingIdTransformer;

impl AstTransformer for HeadingIdTransformer {
    fn transform(&self, mut nodes: Vec<BlockNode>) -> Vec<BlockNode> {
        let mut counter = 0;
        for node in &mut nodes {
            if node.name == "core/heading" {
                let current_id = node.attributes.get("id").and_then(|v| v.as_str());
                if current_id.is_none() {
                    counter += 1;
                    let generated_id = if let Some(text) = &node.text {
                        let slug = text
                            .to_lowercase()
                            .replace(|c: char| !c.is_alphanumeric() && c != '-', "-")
                            .trim_matches('-')
                            .to_string();
                        if slug.is_empty() {
                            format!("heading-{}", counter)
                        } else {
                            slug
                        }
                    } else {
                        format!("heading-{}", counter)
                    };

                    if let Some(obj) = node.attributes.as_object_mut() {
                        obj.insert("id".to_string(), serde_json::Value::String(generated_id));
                    }
                }
            }
        }
        nodes
    }
}

/// 複数のASTトランスフォーマーをパイプラインとして連結実行する
#[derive(Default)]
pub struct AstPipeline {
    transformers: Vec<Box<dyn AstTransformer>>,
}

impl AstPipeline {
    pub fn new() -> Self {
        Self {
            transformers: Vec::new(),
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn add<T: AstTransformer + 'static>(mut self, transformer: T) -> Self {
        self.transformers.push(Box::new(transformer));
        self
    }

    pub fn execute(&self, mut nodes: Vec<BlockNode>) -> Vec<BlockNode> {
        for t in &self.transformers {
            nodes = t.transform(nodes);
        }
        nodes
    }
}
