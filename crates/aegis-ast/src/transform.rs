use crate::inline::plain_text;
use crate::node::BlockNode;
use std::collections::HashSet;

pub trait AstTransformer: Send + Sync {
    fn transform(&self, nodes: Vec<BlockNode>) -> Vec<BlockNode>;
}

/// 見出し（core/heading）に slug/id を自動付加するトランスフォーマー。
/// id はインライン書式の記号を除いたテキストから作り、文書の中で重なれば `-2`、`-3` … を付ける
/// （目次（`core/toc`）のリンク先になる）。既に id のある見出しはそのまま
#[derive(Default)]
pub struct HeadingIdTransformer;

impl AstTransformer for HeadingIdTransformer {
    fn transform(&self, mut nodes: Vec<BlockNode>) -> Vec<BlockNode> {
        let mut used: HashSet<String> = nodes
            .iter()
            .filter(|n| n.name == "core/heading")
            .filter_map(|n| n.attributes.get("id").and_then(|v| v.as_str()))
            .map(str::to_string)
            .collect();
        let mut counter = 0;
        for node in &mut nodes {
            if node.name == "core/heading" {
                let current_id = node.attributes.get("id").and_then(|v| v.as_str());
                if current_id.is_none() {
                    counter += 1;
                    let slug = node
                        .text
                        .as_deref()
                        .map(|text| {
                            plain_text(text)
                                .to_lowercase()
                                .replace(|c: char| !c.is_alphanumeric() && c != '-', "-")
                                .trim_matches('-')
                                .to_string()
                        })
                        .unwrap_or_default();
                    let base = if slug.is_empty() {
                        format!("heading-{}", counter)
                    } else {
                        slug
                    };
                    let mut generated_id = base.clone();
                    let mut n = 2;
                    while used.contains(&generated_id) {
                        generated_id = format!("{}-{}", base, n);
                        n += 1;
                    }
                    used.insert(generated_id.clone());

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

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(nodes: &[BlockNode]) -> Vec<&str> {
        nodes
            .iter()
            .map(|n| n.attributes["id"].as_str().unwrap())
            .collect()
    }

    #[test]
    fn test_heading_ids_are_plain_and_unique() {
        let nodes = vec![
            BlockNode::heading(2, "Install **AegisCMS**"),
            BlockNode::heading(2, "概要"),
            BlockNode::heading(2, "概要"),
            BlockNode::heading(2, "Taken").with_attr("id", "taken"),
            BlockNode::heading(3, "taken"),
            BlockNode::heading(3, "!!!"),
        ];
        let out = HeadingIdTransformer.transform(nodes);
        assert_eq!(
            ids(&out),
            [
                "install-aegiscms",
                "概要",
                "概要-2",
                "taken",
                "taken-2",
                "heading-5"
            ]
        );
        // もう一度掛けても変わらない
        assert_eq!(ids(&HeadingIdTransformer.transform(out.clone())), ids(&out));
    }
}
