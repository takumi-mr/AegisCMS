# aegis-auth

AegisCMS のユーザー認証・セッション管理・SSO (OpenID Connect / SAML 2.0)・セキュリティ機能クレート。

## 概要

`aegis-auth` は、Admin Studio の管理者ユーザーおよび公開サイトの会員ユーザーに対する認証・認可機能を提供します。

パスワードの安全なハッシュ化（Argon2id）、TOTP 二要素認証（2FA）、セッション管理、ブルートフォース攻撃対策のレートリミット、招待制アカウント登録、パスワードリセット、そしてエンタープライズ向けの外部 IdP 連携（OpenID Connect, SAML 2.0）を網羅しています。

## 主な責務・提供機能

- **ユーザー・アカウント管理 (`users`, `registration`, `invitation`)**:
  - 管理者および会員ユーザーの CRUD。
  - ロールベースの認可（Admin, Editor, Author, Contributor, Subscriber 等）。
  - メール確認付きサインアップおよび管理者による招待トークン発行。
- **パスワードセキュリティ (`password`, `password_reset`)**:
  - Argon2id アルゴリズムによる安全なソルト・ハッシュ化。
  - タイミング攻撃耐性を持つ一定時間比較。
  - 有効期限付きワンタイムトークンによるパスワード再設定フロー。
- **セッションと二要素認証 (`session`, `totp`)**:
  - 暗号論的に安全な乱数によるセッション ID 発行と失効管理。
  - RFC 6238 準拠の TOTP 二要素認証（Google Authenticator, 1Password 等に対応）。
- **ブルートフォース対策 (`rate_limit`)**:
  - IP アドレスおよびユーザー識別子単位の試行回数制限と一時的アカウントロックアウト。
- **エンタープライズ SSO (`oidc`, `saml`)**:
  - **OpenID Connect (OIDC)**: Google, GitHub, Okta, Azure AD 等との OAuth 2.0 / OIDC 連携。
  - **SAML 2.0**: 企業向け IdP (Okta, Keycloak, Azure AD) との SP-Initiated / IdP-Initiated シングルサインオン。

## モジュール構成

| モジュール | 説明 |
| :--- | :--- |
| `users` | ユーザーデータモデルおよびサービスクラス |
| `password` | Argon2id によるパスワード検証・ハッシュ化 |
| `session` | セッションライフサイクル管理 |
| `totp` | RFC 6238 TOTP 二要素認証 |
| `rate_limit` | ログイン試行制限・レートリミッター |
| `registration` | 公開ユーザー登録フロー |
| `invitation` | 招待トークン生成および検証 |
| `password_reset` | パスワードリセットトークン管理 |
| `oidc` | OpenID Connect 認証プロバイダ (`feature = "oidc"`) |
| `saml` | SAML 2.0 認証プロバイダ (`feature = "saml"`) |
| `config` | 認証全般の設定 (`AuthConfig`) |
