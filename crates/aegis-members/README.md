# aegis-members

**English** | [日本語](README.ja.md)

AegisCMS's crate for member accounts (`[members]`). These are visitor accounts shared by features such as the shop and the forum.

It shares no tables, sessions, or permissions with the CMS's admin accounts (`_aegis_users`). It does not handle per-feature data such as orders, reviews, or forum posts (each feature's crate links them using the member ID).

## What It Provides

- **`MemberService`**: sign-up, login, sessions (cache-backed; per logged-in device; changing the password or suspending revokes all of that member's sessions), profile updates, password reset, email address verification, suspension and reinstatement (by administrators), and withdrawal.
- **`MembersConfig`** / `MemberRegistration`: the `[members]` settings and their validation.

## Withdrawal Procedure

`MemberService::delete_account` only deletes the member's row and revokes the sessions. The caller performs the following steps in order (if a step fails partway, the same procedure can be run again).

1. `confirm_password` (confirming the person's identity)
2. Per-feature cleanup (shop: `aegis_commerce::member_data::release_member`)
3. `delete_account`

For the specification, see [docs/members.md](../../docs/members.md).
