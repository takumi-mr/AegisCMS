//! 閲覧の制限（会員レベル）の、どのクレートからも使う最小の約束。docs/members.md 15。

/// 記事・ページの `access`（必要な会員レベル）が「誰でも」か。未設定・空・`public` は誰でも
pub fn is_public_access(required: Option<&str>) -> bool {
    matches!(required.map(str::trim), None | Some("") | Some("public"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_missing_blank_or_public_value_is_open() {
        for open in [None, Some(""), Some("  "), Some("public")] {
            assert!(is_public_access(open), "{open:?}");
        }
        for restricted in [Some("free"), Some("paid"), Some("Public"), Some("x")] {
            assert!(!is_public_access(restricted), "{restricted:?}");
        }
    }
}
