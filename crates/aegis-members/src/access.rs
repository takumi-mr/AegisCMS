//! 閲覧の制限（会員レベル）の判定。docs/members.md 15。
//!
//! 閲覧者の種類（`Viewer`）と、記事・ページが求めるレベル（`access`）から、本文を見せてよいかを決める。
//! どの面（公開ページ・フィード・検索・API）も、この 1 つの判定を通す。

/// 閲覧者の種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Viewer<'a> {
    /// 未ログイン
    Guest,
    /// ログイン中の会員（権限レベルつき）
    Member { level: &'a str },
    /// 管理者・編集者など。レベルによらず全文を見られる（プレビュー・確認のため）
    Staff,
}

/// 必要なレベルの指定。`public`・空・未設定は「誰でも」
pub fn is_public(required: Option<&str>) -> bool {
    aegis_core::is_public_access(required)
}

/// `required`（記事・ページの `access`）を満たす閲覧者か。
/// `levels` は `[members].levels`（低 → 高の順）。高いレベルは低いレベルの内容も見られる。
///
/// - 公開なら誰でも見られる。
/// - ゲストは、レベルの指定があれば見られない（最も低いレベルの指定は「会員だけ」の意味）。
/// - 設定に無いレベルの指定、設定に無いレベルの会員は見られない（設定の誤り・変更で、制限が外れないように）。
pub fn may_view(levels: &[String], required: Option<&str>, viewer: Viewer<'_>) -> bool {
    if is_public(required) {
        return true;
    }
    let required = required.map(str::trim).unwrap_or_default();
    match viewer {
        Viewer::Staff => true,
        Viewer::Guest => false,
        Viewer::Member { level } => {
            let rank = |name: &str| levels.iter().position(|l| l == name);
            matches!((rank(level), rank(required)), (Some(have), Some(need)) if have >= need)
        }
    }
}

/// 会員の有効なレベル: 手動で付けたレベル（`base`）と、契約中のプランのレベル（`granted`）の高いほう
/// （docs/members.md 16.3）。`levels` に無いレベルは無視する（設定から消したプランで、制限が外れないように）。
/// `base` が設定に無いときは、そのまま返す（`may_view` が見せない側に倒す）
pub fn effective_level<'a>(levels: &[String], base: &'a str, granted: &[&'a str]) -> &'a str {
    let rank = |name: &str| levels.iter().position(|l| l == name);
    let mut best = base;
    let mut best_rank = rank(base);
    for &level in granted {
        if let Some(r) = rank(level) {
            if best_rank.is_none_or(|b| r > b) {
                best = level;
                best_rank = Some(r);
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    #[test]
    fn effective_level_is_the_higher_of_manual_and_granted() {
        let l = levels();
        assert_eq!(effective_level(&l, "free", &[]), "free");
        assert_eq!(effective_level(&l, "free", &["paid"]), "paid");
        assert_eq!(effective_level(&l, "vip", &["paid"]), "vip");
        assert_eq!(effective_level(&l, "free", &["paid", "vip"]), "vip");
        // 設定にないレベルは無視する
        assert_eq!(effective_level(&l, "free", &["gold"]), "free");
        assert_eq!(effective_level(&l, "gold", &["paid"]), "paid");
        assert_eq!(effective_level(&l, "gold", &[]), "gold");
    }

    use super::*;

    fn levels() -> Vec<String> {
        ["free", "paid", "vip"].map(String::from).to_vec()
    }

    #[test]
    fn public_content_is_open_to_everyone() {
        for required in [None, Some(""), Some("  "), Some("public")] {
            assert!(may_view(&levels(), required, Viewer::Guest), "{required:?}");
        }
    }

    #[test]
    fn guests_cannot_see_any_restricted_content() {
        for required in ["free", "paid", "vip", "unknown"] {
            assert!(!may_view(&levels(), Some(required), Viewer::Guest));
        }
    }

    #[test]
    fn higher_levels_see_lower_levels_content() {
        let l = levels();
        assert!(may_view(&l, Some("free"), Viewer::Member { level: "free" }));
        assert!(!may_view(
            &l,
            Some("paid"),
            Viewer::Member { level: "free" }
        ));
        assert!(may_view(&l, Some("paid"), Viewer::Member { level: "paid" }));
        assert!(may_view(&l, Some("paid"), Viewer::Member { level: "vip" }));
        assert!(!may_view(&l, Some("vip"), Viewer::Member { level: "paid" }));
    }

    #[test]
    fn unknown_levels_fail_closed() {
        let l = levels();
        // 設定に無いレベルを求める記事は、会員にも見せない
        assert!(!may_view(&l, Some("gold"), Viewer::Member { level: "vip" }));
        // レベルを設定から消した会員も、制限された内容は見られない
        assert!(!may_view(
            &l,
            Some("free"),
            Viewer::Member { level: "gold" }
        ));
    }

    #[test]
    fn staff_see_everything() {
        assert!(may_view(&levels(), Some("vip"), Viewer::Staff));
        assert!(may_view(&levels(), Some("unknown"), Viewer::Staff));
    }
}
