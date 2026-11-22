//! 取り込みの報告（何を作り、何を飛ばし、何を手で直す必要があるか）。

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// 1 つの種類（`author`・`media`・`article` など）の結果の数
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Counter {
    pub created: usize,
    /// すでにあった（再実行で飛ばした）
    pub existing: usize,
    pub skipped: usize,
    pub failed: usize,
}

/// 手直しや確認が必要な項目
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub kind: &'static str,
    /// 元の項目（タイトルや URL）
    pub subject: String,
    pub message: String,
}

#[derive(Debug, Default)]
pub struct Report {
    counts: BTreeMap<&'static str, Counter>,
    entries: Vec<Entry>,
}

impl Report {
    fn counter(&mut self, kind: &'static str) -> &mut Counter {
        self.counts.entry(kind).or_default()
    }

    pub fn created(&mut self, kind: &'static str) {
        self.counter(kind).created += 1;
    }

    pub fn existing(&mut self, kind: &'static str) {
        self.counter(kind).existing += 1;
    }

    /// 取り込まなかった（理由を報告に残す）
    pub fn skipped(&mut self, kind: &'static str, subject: &str, message: &str) {
        self.counter(kind).skipped += 1;
        self.note(kind, subject, message);
    }

    /// 取り込もうとして失敗した
    pub fn failed(&mut self, kind: &'static str, subject: &str, message: &str) {
        self.counter(kind).failed += 1;
        self.note(kind, subject, message);
    }

    /// 取り込めたが、手直しが要る
    pub fn note(&mut self, kind: &'static str, subject: &str, message: &str) {
        self.entries.push(Entry {
            kind,
            subject: subject.to_string(),
            message: message.to_string(),
        });
    }

    pub fn failures(&self) -> usize {
        self.counts.values().map(|c| c.failed).sum()
    }

    #[cfg(test)]
    pub fn counts(&self) -> &BTreeMap<&'static str, Counter> {
        &self.counts
    }

    /// Markdown の報告。`dry_run` のときは「作る予定」の数として書く
    pub fn render(&self, dry_run: bool) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "# WordPress import report{}\n",
            if dry_run { " (dry run)" } else { "" }
        );
        let _ = writeln!(
            out,
            "| Kind | {} | Already there | Skipped | Failed |\n|---|---:|---:|---:|---:|",
            if dry_run { "Would create" } else { "Created" }
        );
        for (kind, c) in &self.counts {
            let _ = writeln!(
                out,
                "| {kind} | {} | {} | {} | {} |",
                c.created, c.existing, c.skipped, c.failed
            );
        }
        if self.entries.is_empty() {
            let _ = writeln!(out, "\nNothing needs attention.");
            return out;
        }
        // 種類ごと、同じ内容はまとめる（数百件の同じ注意で埋まらないように）
        let mut grouped: BTreeMap<(&str, &str), Vec<&str>> = BTreeMap::new();
        for e in &self.entries {
            grouped
                .entry((e.kind, e.message.as_str()))
                .or_default()
                .push(e.subject.as_str());
        }
        let _ = writeln!(out, "\n## Needs attention\n");
        for ((kind, message), subjects) in grouped {
            let _ = writeln!(out, "### {kind}: {message} ({})\n", subjects.len());
            for subject in subjects.iter().take(50) {
                let _ = writeln!(out, "- {subject}");
            }
            if subjects.len() > 50 {
                let _ = writeln!(out, "- … and {} more", subjects.len() - 50);
            }
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_and_groups_entries() {
        let mut report = Report::default();
        report.created("article");
        report.created("article");
        report.existing("category");
        report.skipped("product", "Mug", "unsupported post type");
        report.failed("media", "a.jpg", "404");
        report.note("article", "Post A", "contains a shortcode");
        report.note("article", "Post B", "contains a shortcode");

        assert_eq!(report.failures(), 1);
        assert_eq!(report.counts()["article"].created, 2);

        let text = report.render(false);
        assert!(text.contains("| article | 2 | 0 | 0 | 0 |"), "{text}");
        assert!(text.contains("### article: contains a shortcode (2)"));
        assert!(text.contains("- Post A") && text.contains("- Post B"));
        assert!(report.render(true).contains("Would create"));
    }

    #[test]
    fn a_long_list_is_cut_off() {
        let mut report = Report::default();
        for i in 0..60 {
            report.note("article", &format!("Post {i}"), "same");
        }
        let text = report.render(false);
        assert!(text.contains("… and 10 more"));
        assert!(!text.contains("Post 59"));
    }
}
