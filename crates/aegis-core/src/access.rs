//! 閲覧の制限（会員レベル・パスワード保護）の、どのクレートからも使う最小の約束。docs/members.md 15。

/// 記事・ページのパスワード（合言葉）保護を表す `access` の予約値
pub const PASSWORD_ACCESS: &str = "password";

/// 記事・ページの `access`（必要な会員レベル・保護方式）が「誰でも」か。未設定・空・`public` は誰でも
pub fn is_public_access(required: Option<&str>) -> bool {
    matches!(required.map(str::trim), None | Some("") | Some("public"))
}

/// 記事・ページの `access` がパスワード（合言葉）保護（`"password"`）か
pub fn is_password_access(required: Option<&str>) -> bool {
    matches!(required.map(str::trim), Some(PASSWORD_ACCESS))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_missing_blank_or_public_value_is_open() {
        for open in [None, Some(""), Some("  "), Some("public")] {
            assert!(is_public_access(open), "{open:?}");
            assert!(!is_password_access(open), "{open:?}");
        }
        for restricted in [
            Some("free"),
            Some("paid"),
            Some("password"),
            Some("Public"),
            Some("x"),
        ] {
            assert!(!is_public_access(restricted), "{restricted:?}");
        }
        assert!(is_password_access(Some("password")));
        assert!(is_password_access(Some(" password ")));
        assert!(!is_password_access(Some("paid")));
    }
}
