//! `on-render-page` プラグインが公開ページに足すスクリプト（構造化した注入）。docs/csp.md の 4。
//!
//! プラグインは HTML の文字列に `<script>` を書かず、この型で返す。ホスト（`aegis-runtime`）が、
//! マニフェストの宣言と照合して検査し、タグを作る。属性はホストが決める（`onload` などを書けない）。

use serde::{Deserialize, Serialize};

/// インラインのスクリプトの大きさの上限（バイト）
pub const MAX_INLINE_SCRIPT_BYTES: usize = 64 * 1024;

/// `<head>` に足すスクリプト 1 つ
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InjectedScript {
    #[serde(flatten)]
    pub source: ScriptSource,
    /// 同意の区分（`analytics` など。`[a-z0-9_-]` の 1〜32 文字）。あれば、ホストが
    /// `type="text/plain" data-aegis-consent="区分"` の形で出し、同意のあとにバナーのスクリプトが実行する
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consent: Option<String>,
}

/// スクリプトの中身
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScriptSource {
    /// `<script src="https://..." async defer>`。URL のホストは、マニフェストの
    /// `capabilities.csp.script_domains` にあるものだけ
    External {
        url: String,
        #[serde(default)]
        async_load: bool,
        #[serde(default)]
        defer: bool,
        /// `crossorigin="anonymous"` を付ける（広告のスクリプトなど、提供元が求めるものだけ）
        #[serde(default)]
        crossorigin: bool,
    },
    /// `<script>コード</script>`。マニフェストに `capabilities.inline_scripts: true` の宣言が要る
    Inline { code: String },
}

impl InjectedScript {
    /// インラインのスクリプト
    pub fn inline(code: impl Into<String>) -> Self {
        Self {
            source: ScriptSource::Inline { code: code.into() },
            consent: None,
        }
    }

    /// 外部のスクリプト（読み込みは遅らせない）
    pub fn external(url: impl Into<String>) -> Self {
        Self {
            source: ScriptSource::External {
                url: url.into(),
                async_load: false,
                defer: false,
                crossorigin: false,
            },
            consent: None,
        }
    }

    /// 同意の区分を付ける（同意があるまで実行しない）
    pub fn with_consent(mut self, category: impl Into<String>) -> Self {
        self.consent = Some(category.into());
        self
    }

    /// `async` にする（外部のスクリプトだけ。インラインでは何もしない）
    pub fn async_load(mut self) -> Self {
        if let ScriptSource::External { async_load, .. } = &mut self.source {
            *async_load = true;
        }
        self
    }

    /// `crossorigin="anonymous"` を付ける（外部のスクリプトだけ。CORS を許すサーバーのものに限る）
    pub fn crossorigin(mut self) -> Self {
        if let ScriptSource::External { crossorigin, .. } = &mut self.source {
            *crossorigin = true;
        }
        self
    }

    /// `defer` にする（外部のスクリプトだけ）
    pub fn deferred(mut self) -> Self {
        if let ScriptSource::External { defer, .. } = &mut self.source {
            *defer = true;
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_round_trip_as_tagged_json() {
        let scripts = vec![
            InjectedScript::external("https://cdn.example.com/a.js")
                .async_load()
                .with_consent("analytics"),
            InjectedScript::inline("window.x=1;"),
        ];
        let json = serde_json::to_string(&scripts).unwrap();
        assert!(json.contains(r#""kind":"external""#), "{json}");
        assert!(json.contains(r#""kind":"inline""#), "{json}");
        assert!(!json.contains("consent\":null"), "{json}");
        let back: Vec<InjectedScript> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, scripts);
        // 省略できる項目
        let min: InjectedScript =
            serde_json::from_str(r#"{"kind":"external","url":"https://a.example/x.js"}"#).unwrap();
        assert_eq!(min, InjectedScript::external("https://a.example/x.js"));
    }
}
