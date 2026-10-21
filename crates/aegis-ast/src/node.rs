use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockNode {
    pub name: String,
    #[serde(default)]
    pub attributes: serde_json::Value,
    #[serde(default)]
    pub children: Vec<BlockNode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

impl BlockNode {
    pub fn new<S: Into<String>>(name: S) -> Self {
        Self {
            name: name.into(),
            attributes: serde_json::json!({}),
            children: Vec::new(),
            text: None,
        }
    }

    pub fn paragraph<S: Into<String>>(text: S) -> Self {
        Self {
            name: "core/paragraph".to_string(),
            attributes: serde_json::json!({}),
            children: Vec::new(),
            text: Some(text.into()),
        }
    }

    pub fn heading<S: Into<String>>(level: u8, text: S) -> Self {
        Self {
            name: "core/heading".to_string(),
            attributes: serde_json::json!({ "level": level }),
            children: Vec::new(),
            text: Some(text.into()),
        }
    }

    pub fn code<S: Into<String>>(language: &str, code: S) -> Self {
        Self {
            name: "core/code".to_string(),
            attributes: serde_json::json!({ "language": language }),
            children: Vec::new(),
            text: Some(code.into()),
        }
    }

    pub fn with_attr<V: Into<serde_json::Value>>(mut self, key: &str, value: V) -> Self {
        if let Some(obj) = self.attributes.as_object_mut() {
            obj.insert(key.to_string(), value.into());
        }
        self
    }

    pub fn with_child(mut self, child: BlockNode) -> Self {
        self.children.push(child);
        self
    }
}
