//! 会員アカウント（`[members]`）。ショップ・掲示板などが共通で使う、訪問者のアカウント。
//!
//! CMS の管理アカウント（`_aegis_users`）とは、テーブルもセッションも権限も共有しない。
//! 注文やレビューなど、機能ごとのデータは扱わない（それぞれの機能のクレートが、会員の ID で結びつける）。
//! 仕様は `docs/members.md`。

pub mod access;
pub mod config;
pub mod member;

pub use access::{is_public, may_view, Viewer};
pub use config::{MemberRegistration, MembersConfig, PurchaseConfig, PurchaseStripeConfig};
pub use member::{
    MemberLoginRequest, MemberRegisterRequest, MemberResponse, MemberService, MemberSessionData,
    MemberUpdateProfileRequest, MemberVerification, EMAIL_NOT_VERIFIED_MESSAGE,
    EMAIL_VERIFICATION_TTL_SECS, MEMBER_SESSION_PREFIX, MEMBER_TOKEN_PREFIX, NICKNAME_MAX_CHARS,
    PASSWORD_RESET_TTL_SECS,
};

#[cfg(test)]
mod member_reset_tests;
#[cfg(test)]
mod member_tests;
#[cfg(test)]
mod member_verify_tests;
