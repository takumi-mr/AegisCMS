# aegis-members

[English](README.md) | **日本語**

AegisCMS の会員アカウントのクレート（`[members]`）。ショップ・掲示板などが共通で使う、訪問者のアカウントです。

CMS の管理アカウント（`_aegis_users`）とは、テーブルもセッションも権限も共有しません。注文やレビュー、掲示板の投稿など、機能ごとのデータは扱いません（それぞれの機能のクレートが、会員の ID で結びつけます）。

## 提供するもの

- **`MemberService`**: 登録、ログイン、セッション（キャッシュ。ログイン中の端末ごと。パスワードの変更・停止で、その会員の全セッションを失効）、プロフィールの更新、パスワードの再設定、メールアドレスの確認、停止・再開（管理者）、退会。
- **`MembersConfig`** / `MemberRegistration`: `[members]` の設定と検証。

## 退会の手順

`MemberService::delete_account` は、会員の行を消してセッションを失効させるだけです。呼び出す側が次の順に行います（途中で失敗しても、同じ手順をやり直せます）。

1. `confirm_password`（本人の確認）
2. 機能ごとの後始末（ショップ: `aegis_commerce::member_data::release_member`）
3. `delete_account`

仕様は [docs/members.ja.md](../../docs/members.ja.md)。
