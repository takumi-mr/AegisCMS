//! テーマのテンプレートの静的な検査（エラーにはしない注意）

use std::path::Path;

/// 記事の URL（`/articles`）をテンプレートに直接書いた箇所
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardcodedArticlePath {
    /// `templates/` からのパス（例: `index.html`、`partials/card.html`）
    pub template: String,
    /// 1 始まりの行番号
    pub line: usize,
}

impl HardcodedArticlePath {
    /// 利用者に見せる注意文
    pub fn message(&self) -> String {
        format!(
            "templates/{}:{} links to /articles directly. Use {{{{ item.fields.url }}}} or {{{{ articles_url }}}} so the link survives a public_path change",
            self.template, self.line
        )
    }
}

/// `templates/` の下の HTML から、`/articles` を直接書いた行を探す。記事の `public_path` を変えると
/// このリンクは 301 を経由する（`{{ item.fields.url }}`・`{{ articles_url }}` を使えば経由しない）。
/// 外部の URL（`https://example.com/articles`）は対象外
pub fn hardcoded_article_paths(theme_dir: &Path) -> Vec<HardcodedArticlePath> {
    let templates_dir = theme_dir.join("templates");
    let mut found = Vec::new();
    collect(&templates_dir, &templates_dir, &mut found);
    found.sort_by(|a, b| (&a.template, a.line).cmp(&(&b.template, b.line)));
    found
}

fn collect(root: &Path, dir: &Path, found: &mut Vec<HardcodedArticlePath>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, found);
        } else if path.extension().is_some_and(|e| e == "html") {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let template = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            for (index, line) in text.lines().enumerate() {
                if line_has_hardcoded_path(line) {
                    found.push(HardcodedArticlePath {
                        template: template.clone(),
                        line: index + 1,
                    });
                }
            }
        }
    }
}

/// `/articles`（直後が `/`・引用符・`?`・`#`・空白・行末など、名前の続きでないもの）が、外部 URL の一部でなく書かれているか
fn line_has_hardcoded_path(line: &str) -> bool {
    const NEEDLE: &str = "/articles";
    let mut from = 0;
    while let Some(offset) = line[from..].find(NEEDLE) {
        let start = from + offset;
        let end = start + NEEDLE.len();
        from = end;
        let continues_name = line[end..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        if continues_name {
            continue;
        }
        // 直前の「空白・引用符・括弧で区切られた 1 語」に `://` があれば外部の URL
        let token_start = line[..start]
            .rfind(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '(' | '=' | '>'))
            .map_or(0, |i| i + 1);
        let token = &line[token_start..start];
        if token.contains("://") || token.starts_with("//") {
            continue;
        }
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detects_direct_article_links() {
        for line in [
            r#"<a href="/articles">All</a>"#,
            r#"<a href="{{ base_path }}/articles/{{ item.slug }}">"#,
            r#"<a href="/articles?page=2">"#,
            r#"<a href='/articles#top'>"#,
            "go to /articles",
            "/articles",
        ] {
            assert!(line_has_hardcoded_path(line), "{line}");
        }
    }

    #[test]
    fn test_ignores_other_paths_and_external_urls() {
        for line in [
            r#"<a href="{{ articles_url }}">"#,
            r#"<a href="{{ item.fields.url }}">"#,
            r#"<a href="/articles-archive">"#,
            r#"<a href="/articlesfoo">"#,
            r#"<a href="https://example.com/articles">"#,
            r#"<a href="//cdn.example.com/articles/1">"#,
            "no link here",
        ] {
            assert!(!line_has_hardcoded_path(line), "{line}");
        }
    }

    #[test]
    fn test_scans_nested_templates_with_line_numbers() {
        let dir = std::env::temp_dir().join(format!("aegis_theme_lint_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("templates/partials")).unwrap();
        std::fs::write(dir.join("templates/base.html"), "<a href=\"/\">Home</a>\n").unwrap();
        std::fs::write(
            dir.join("templates/partials/card.html"),
            "<div>\n  <a href=\"/articles/{{ item.slug }}\">{{ item.title }}</a>\n</div>\n",
        )
        .unwrap();
        std::fs::write(dir.join("templates/readme.txt"), "/articles\n").unwrap();

        let found = hardcoded_article_paths(&dir);
        assert_eq!(
            found,
            vec![HardcodedArticlePath {
                template: "partials/card.html".to_string(),
                line: 2,
            }]
        );
        assert!(found[0]
            .message()
            .contains("templates/partials/card.html:2"));
        assert!(found[0].message().contains("{{ articles_url }}"));
        // テーマの無いディレクトリは空
        assert!(hardcoded_article_paths(&dir.join("missing")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
