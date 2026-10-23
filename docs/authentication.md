# 認証 & ユーザー管理 (Authentication)

AegisCMS のログインは、ユーザー名とパスワードを `AuthProvider` が検証したときだけセッション（Bearer トークン）を発行します。ロールはリクエストではなく、サーバーに登録されたユーザーアカウントで決まります。

プロバイダはローカル認証（`LocalAuthProvider`、`_aegis_users` テーブル）と、外部 IdP の OpenID Connect（[6. シングルサインオン](#6-シングルサインオン-openid-connect)、`[auth.oidc]`）です。SAML は未対応です。

---

## 1. ユーザーアカウント

アカウントは DB のシステムテーブル `_aegis_users` に保存します（SQLite / PostgreSQL / MySQL 共通。サーバー起動時・CLI 実行時に自動作成）。

| カラム | 内容 |
| --- | --- |
| `id` | UUID |
| `username` | ログイン名（一意。英数字と `.` `_` `-` `@`、1〜64 文字）。セッションの `user_id`・監査ログの actor になる |
| `email` / `display_name` | 任意。セッションの `metadata` に入る |
| `role` | `admin` / `editor` / `viewer` |
| `password_hash` | Argon2id（既定パラメータ）の PHC 文字列。平文は保存しない |
| `disabled` | `1` ならログイン不可 |
| `avatar_key` | アバター画像のストレージキー（`avatars/...`）。未設定なら NULL。既存の DB には起動時に列を追加 |
| `created_at` / `updated_at` | RFC 3339 (UTC) |

### ロールと API の権限

ロールは `admin` ⊃ `editor` ⊃ `viewer` の包含関係です（上位ロールは下位ロールの操作をすべて行える）。

| API | 必要なロール |
| --- | --- |
| `GET /api/v1/health`, `GET /api/v1/openapi.json`, `/metrics` | 不要（公開） |
| 公開サイト（`/`, `/articles/*`, `/{slug}`, `/theme/assets/*`, `/uploads/*`）, Admin Studio の静的ファイル | 不要（公開） |
| `POST /api/v1/auth/session`（ログイン）, `GET` / `DELETE /api/v1/auth/session` | 不要（資格情報・セッションで判定） |
| `GET /api/v1/auth/providers`, `GET /api/v1/auth/oidc/login`, `GET /api/v1/auth/oidc/callback`（[シングルサインオン](#6-シングルサインオン-openid-connect)） | 不要 |
| ショップ（`/products`, `/cart`, `/orders/{id}`, `/shop/cart.js`, `/api/v1/shop/*`。[EC](commerce.md#61-公開ログイン不要)） | 不要（注文確認はトークン、Webhook は署名で確認） |
| `GET /api/v1/schemas`, `GET /api/v1/schemas/{type}` | `viewer` |
| `GET /api/v1/contents/{type}`, `GET /api/v1/contents/{type}/{slug}`（下書きを含む） | `viewer` |
| `GET /api/v1/storage/objects` | `viewer` |
| `POST /api/v1/preview`（エディタのプレビュー。保存しない） | `viewer` |
| `POST /api/v1/contents/{type}`, `PUT` / `DELETE /api/v1/contents/{type}/{slug または id}` | `editor` |
| `GET /api/v1/orders`, `GET /api/v1/orders/{id}`, `POST /api/v1/orders/{id}/status`（[EC](commerce.md#62-管理editor-以上)） | `editor` |
| `POST /api/v1/storage/upload`, `DELETE /api/v1/storage/objects/{key}`（[メディア](media.md)） | `editor` |
| `GET /api/v1/users/{username}`, `PATCH /api/v1/users/{username}`, `PUT` / `DELETE /api/v1/users/{username}/avatar`（[ユーザー管理 API](#3-ユーザー管理-api)） | 本人（`viewer` 以上）か `admin` |
| `GET /api/v1/users`, `POST /api/v1/users`, `DELETE /api/v1/users/{username}` | `admin` |
| `GET /api/v1/audit-logs` | `admin` |

- セッションが無い・無効・期限切れなら `401`、ロールが足りなければ `403` を返し、監査ログに `access.denied`（理由・必要なロール・エンドポイント）を記録します（[監査ログ](audit.md#2-記録されるイベント)）。
- コンテンツ作成時の `author_id` は省略するとログインユーザー（`user_id`）になります。他のユーザーを `author_id` に指定できるのは `admin` だけです（それ以外は `403`）。
- `/metrics`（Prometheus）は監視系からの取得を想定して公開のままです。外部に公開する場合はリバースプロキシなどでアクセス元を制限してください。

### プラグインによるロールマッピング・ログイン制限

資格情報の確認後・セッション発行前に `on-user-authenticated` フック（[プラグインフック](plugin_hooks.md#14-on-user-authenticated)）が呼ばれます。プラグインはセッションのロール・表示名・`metadata` を変えたり、ログインを拒否（`403`）したりできます。ロールを変えた場合は監査ログ `session.create` の `details.role_mapped` に記録されます。

---

## 2. CLI でのユーザー管理 (`aegis user`)

```bash
# 最初の管理者を作成（パスワードは対話入力。端末では非表示・確認つき）
aegis-server user add admin --role admin --email admin@example.com --display-name "Site Admin"

# 非対話（CI / Docker）: 環境変数か標準入力の 1 行からパスワードを読む
AEGIS_USER_PASSWORD='change-me-please' aegis-server user add editor1 --role editor
echo 'change-me-please' | aegis-server user add viewer1 --role viewer

aegis-server user passwd editor1        # パスワード変更（既存セッションは即時失効）
aegis-server user role editor1 admin    # ロール変更（既存セッションは即時失効）
aegis-server user disable editor1       # ログイン不可にする（既存セッションは即時失効）
aegis-server user enable editor1
aegis-server user list
aegis-server user link-oidc alice 248289761001   # 既存ユーザーを IdP のアカウント（sub）に紐付ける
```

- 開発時は `cargo run -p aegis-server -- user add ...`、Docker では `docker exec -it <container> aegis-server user add ...` で実行します。設定ファイル（`-c` / `AEGIS_CONFIG`）の `[database]` に接続します。
- パスワードは 8 文字以上です。
- ユーザーが 1 人もいない状態でサーバーを起動すると、作成方法を示す警告ログを出します。

> [!NOTE]
> ロール変更・無効化・パスワード変更は、発行済みのセッションを即座に失効（キャッシュ上の失効マーカーによるブラックリスト判定）させます。対象ユーザーの既存セッションは次回アクセス時に `401 Unauthorized` となり、再ログインが必要になります（無効化されたユーザーは再ログインも不可）。キャッシュ（Redis）利用時は CLI からの変更も即座に共有されます。

---

## 3. ユーザー管理 API

Admin Studio の「Users」画面（`admin`）と、ヘッダーのアバターから開く「My profile」（全ロール）はこの API を使います。応答のユーザーは次の形で、パスワードハッシュは返しません。

```json
{
  "id": "8a9e4ef1-4039-4b0f-b1fb-ea684ae66697",
  "username": "carol",
  "email": "carol@example.com",
  "display_name": "Carol Writer",
  "role": "editor",
  "disabled": false,
  "avatar_url": "/uploads/avatars/8a9e4ef1-4039-4b0f-b1fb-ea684ae66697-6665ae18.png",
  "created_at": "2026-10-22T23:23:26Z",
  "updated_at": "2026-10-22T23:23:30Z"
}
```

| API | 内容 |
| --- | --- |
| `GET /api/v1/users` | 全ユーザー（`{"users": [...]}`、username 順） |
| `POST /api/v1/users` | 作成。本文は `username` / `password` / `role`（必須）、`email` / `display_name`（任意）。`201` |
| `GET /api/v1/users/{username}` | 1 人分（本人か `admin`） |
| `PATCH /api/v1/users/{username}` | 変更。省略した項目はそのまま、`email` / `display_name` は空文字で削除 |
| `DELETE /api/v1/users/{username}` | 削除（ID 連携・アバター画像も削除。コンテンツの `author_id` は残る）。`204` |
| `PUT /api/v1/users/{username}/avatar` | アバター画像を設定（multipart の `file`） |
| `DELETE /api/v1/users/{username}/avatar` | アバター画像を外す |

`PATCH` の項目と権限:

| 項目 | 本人 | `admin`（他のユーザー） |
| --- | --- | --- |
| `email`, `display_name` | ✓ | ✓ |
| `password` | ✓（`current_password` が必要。違えば `403`） | ✓（現在のパスワード不要＝リセット） |
| `role`, `disabled` | ✗（`admin` でも自分自身は `400`、それ以外は `403`） | ✓ |

- 本人以外のユーザーへの `viewer` / `editor` のアクセスは `403` で拒否し、`access.denied`（`reason: not_own_account`、`resource_type: user`）を記録します。
- 最後の有効な `admin` は降格・無効化・削除できません（`400`。全員が締め出されるのを防ぐ）。自分自身も削除できません。CLI（`aegis user role` など）は復旧用のため確かめません。
- **アバター画像**: PNG / JPEG / GIF / WebP、2 MB まで。形式はファイルの先頭バイトで判定し（クライアントの Content-Type は使わない）、スクリプトを含められる SVG は受け付けません（`415`）。2 MB を超えると `413`。`[storage]` のバックエンドに `avatars/{ユーザー ID}-{ランダム 8 桁}.{拡張子}` として保存し、差し替えるたびにキーを変えて（キャッシュ対策）以前の画像を削除します。
- 作成・変更・削除は監査ログの `user.create` / `user.update` / `user.delete` に記録します（[監査ログ](audit.md#2-記録されるイベント)）。パスワードは記録しません。
- シングルサインオンで作成したユーザーはパスワードを知らないため、Admin Studio の「My profile」ではパスワード変更を表示しません。

> [!NOTE]
> ロール変更・無効化・削除・パスワード変更は、発行済みのセッションに即時反映されます（キャッシュ上のユーザー単位の失効マーカーにより既存セッションは即座に無効化され `401` となります）。降格や無効化されたユーザーが直前のセッションで不正に操作を継続することはできません。

---

## 4. セッション API

### `POST /api/v1/auth/session` — ログイン

```bash
curl -s -X POST http://127.0.0.1:8080/api/v1/auth/session \
  -H 'Content-Type: application/json' \
  -d '{"username":"admin","password":"change-me-please"}'
```

| フィールド | 必須 | 内容 |
| --- | --- | --- |
| `username` | ✓ | ログイン名（前後の空白は無視） |
| `password` | ✓ | パスワード |
| `ttl_secs` | | 有効期限（秒）。`[cache].default_ttl_secs`（既定 3600）を超える値は既定値に切り詰める |

成功時は `201` で次を返します。以降のリクエストでは `Authorization: Bearer <id>`（または `X-Session-Id: <id>`）を付けます。

```json
{
  "id": "ses_0ce22abac5cc409298d05447e7a002b1",
  "user_id": "admin",
  "role": "admin",
  "created_at": "2026-10-22T04:44:14.066870871+00:00",
  "expires_at": "2026-10-22T05:44:14.066870871+00:00",
  "metadata": { "provider": "local", "display_name": "Site Admin", "email": "admin@example.com" }
}
```

| ステータス | 条件 |
| --- | --- |
| `401` | ユーザーが存在しない・パスワード不一致・アカウント無効（応答はすべて `Invalid username or password` で区別しない） |
| `403` | 資格情報は正しいが、`on-user-authenticated` プラグインがログインを拒否した（本文はプラグインのメッセージ） |
| `422` | `username` / `password` が無い（以前の `{"user_id", "role"}` 形式は受け付けない） |
| `429` | ログイン失敗が続いたためロック中（`Retry-After` ヘッダーと本文の `retry_after_secs` に解除までの秒数。正しいパスワードでも拒否） |
| `500` | DB・キャッシュのエラー、`on-user-authenticated` プラグインの失敗 |

### `GET /api/v1/auth/session` / `DELETE /api/v1/auth/session`

現在のセッションの取得とログアウトです（変更なし）。

### Admin Studio

Admin Studio（`/admin`）はログイン画面からこの API でログインし、トークンを `sessionStorage` に保持してすべての API 呼び出しに付与します。API が `401` を返すとログイン画面に戻ります。ロールが足りない操作（`viewer` の保存・アップロードなど）は画面上でも無効化します。

---

## 5. セキュリティ上の挙動

- **ユーザー名の推測対策**: 存在しないユーザーでもダミーハッシュで同じ Argon2 検証を行い、応答時間と応答内容を揃えます。アカウントが無効かどうかは、正しいパスワードを送った場合にしか判定しません。
- **監査ログ**: ログインの成否を `session.create` として記録します。失敗時は `details` に `username`・`provider`・`error`（`unknown_user` / `invalid_password` / `account_disabled` / `unsupported_credentials`）を残します。パスワードは記録しません。
- **CPU 負荷**: Argon2 の検証は `spawn_blocking` で実行し、非同期ランタイムのワーカーを塞ぎません。
- **ログイン試行のレート制限**（`[auth.login_rate_limit]`）: 認証失敗（ユーザー不在・パスワード不一致・アカウント無効）をユーザー名とクライアント IP ごとに数え、`window_secs`（既定 15 分）内に上限（既定: ユーザー名 5 回 / IP 20 回）に達すると、`lockout_secs`（既定 15 分）の間 `429` で拒否します。
  - ロック中は資格情報を検証しません（正しいパスワードでも拒否し、Argon2 の負荷もかからない）。応答ではユーザー名と IP のどちらでロックされたかを区別しません。
  - ユーザー名は大文字・小文字を区別せずに数え、存在しないユーザー名も同じように数えます（ロックの有無からユーザーの存在を推測させないため）。
  - ログインに成功するとそのユーザー名の失敗回数はリセットされます。IP の失敗回数は、有効なアカウントを 1 つ持つ攻撃者が計数を消せないようリセットしません。
  - 失敗回数は `[cache]` のバックエンドに保存します（キー `login:fail:*` / `login:lock:*`）。`memory` ではプロセスごと・再起動で消えるため、複数インスタンスで運用する場合は `redis` を使ってください。
  - リバースプロキシ配下では全リクエストがプロキシの IP になるため、`[audit].trust_forwarded_for = true` にしてください（有効にすると `X-Forwarded-For` を偽装できるため、信頼できるプロキシ配下でのみ）。
  - ユーザー名単位のロックは、第三者がわざと失敗してそのユーザーを一時的にログインできなくすることにも使えます。運用で問題になる場合は `max_failures_per_user` を大きくし、IP 単位の制限を主に使ってください。ロックを手動で解除するにはキャッシュの `login:lock:*` キーを削除します（`memory` ではサーバーの再起動）。

---

## 6. シングルサインオン (OpenID Connect)

`[auth.oidc]`（[設定リファレンス](configuration.md#210-authoidc--シングルサインオン-openid-connect)）を有効にすると、Admin Studio のログイン画面に **Sign in with {display_name}** ボタンが表示され、外部 IdP（Microsoft Entra ID、Google、Okta、Keycloak など OpenID Connect 対応のもの）でログインできます。パスワードによるローカルログインもそのまま使えます。

### 6.1 IdP 側の設定

1. IdP に「Web アプリケーション」（コンフィデンシャルクライアント）を登録し、クライアント ID とシークレットを発行します。
2. リダイレクト URI に `https://<AegisCMS のホスト>/api/v1/auth/oidc/callback` を登録し、同じ値を `redirect_url` に設定します。
3. ロールをグループで分けたい場合は、ID トークンに `groups` などのクレームが入るよう IdP を設定します。

```toml
[auth.oidc]
enabled = true
display_name = "Company SSO"
issuer_url = "https://login.example.com/realms/staff"
client_id = "aegis-cms"
client_secret = "..."
redirect_url = "https://cms.example.com/api/v1/auth/oidc/callback"
role_claim = "groups"
post_login_redirect = "https://cms.example.com/admin/"

[auth.oidc.role_mapping]
cms-admins = "admin"
cms-editors = "editor"
```

### 6.2 ログインの流れ

1. ボタン（`GET /api/v1/auth/oidc/login`）が IdP の認可エンドポイントへリダイレクトします。`state`・`nonce`・PKCE（`S256`）の検証値は 10 分間キャッシュ（`[cache]`）に保存します。
2. IdP でのログイン後、`GET /api/v1/auth/oidc/callback` が認可コードをトークンに交換し、ID トークンを検証します（JWKS による署名、`iss`、`aud`、`exp`、`nonce`、`azp`。署名アルゴリズムは RS / PS / ES 系のみ）。`state` は 1 回限りです。
3. ユーザーを決め（5.3）、`on-user-authenticated` フック（[プラグインフック](plugin_hooks.md#14-on-user-authenticated)）を通してセッションを発行します。
4. `post_login_redirect` に `#session=<トークン>` を付けてリダイレクトし、Admin Studio がトークンを保存してアドレスバーから消します。失敗時は `#login_error=<理由>` で戻り、ログイン画面に表示します。

フラグメント（`#` 以降）はサーバーのログや `Referer` に残らないため、トークンの受け渡しに使っています。

### 6.3 ユーザーとロール

- IdP のアカウント（Issuer + `sub`）と AegisCMS のユーザーの対応は `_aegis_user_identities` テーブルに保存します。
- **初回ログイン**: `auto_create_users = true`（既定）なら、`username_claim`（既定 `preferred_username`、無ければ `email`）をユーザー名にしてユーザーを作成し、紐付けます。パスワードは誰も知らないランダム値なので、そのユーザーはパスワードではログインできません。
- **同名のユーザーが既にいる場合は紐付けません**（`username_taken`）。IdP 側で同じ名前を名乗るだけで既存のアカウント（ローカルの管理者など）に入れてしまうのを防ぐためです。そのアカウントで SSO を使うには、管理者が `aegis-server user link-oidc <username> <sub>` で紐付けます。
- `auto_create_users = false` なら、紐付け済みのユーザーだけがログインできます（それ以外は `not_provisioned`）。
- **ロール**: `role_claim` を設定すると、ログインのたびにそのクレームの値を `role_mapping` で変換し（複数一致なら最も強いロール、一致しなければ `default_role`）、ユーザーアカウントのロールも更新します。`role_claim` が空なら、ロールはユーザーアカウントのもの（`aegis user role` で変更）で、新規ユーザーは `default_role` です。どちらの場合も `on-user-authenticated` プラグインで上書きできます。
- メールアドレス・表示名（`email` / `name` クレーム）はログインのたびにユーザーアカウントへ反映します。
- 無効化したユーザー（`aegis user disable`）は SSO でもログインできません（`account_disabled`）。
- セッションの `metadata` には `provider: "oidc"`、`issuer`、`sub`、`claims`（ID トークンのクレーム）が入ります。
- ユーザーの決め方は SAML（8.3）と共通です。

### 6.4 失敗時の理由（`login_error`・監査ログ `session.create` の `error`）

| 理由 | 内容 |
| --- | --- |
| `idp_error` | IdP がエラーを返した（ユーザーがキャンセルしたなど） |
| `invalid_state` | `state` が無い・期限切れ・使用済み |
| `invalid_token` | トークン交換の失敗、ID トークンの検証失敗（詳細はサーバーの警告ログ） |
| `missing_username` | ユーザー名にするクレームが無い・使えない文字を含む |
| `username_taken` | 同名の（紐付けられていない）ユーザーが既にいる |
| `not_provisioned` | `auto_create_users = false` で、紐付けられたユーザーがいない |
| `account_disabled` | ユーザーが無効化されている |
| `server_error` | IdP に接続できない・DB エラーなど（詳細はサーバーのエラーログ） |

`on-user-authenticated` プラグインが拒否した場合は、プラグインのメッセージが `login_error` になります。

> [!NOTE]
> SSO のログインは IdP 側で保護されるため、`[auth.login_rate_limit]` の対象外です。ディスカバリー文書（`/.well-known/openid-configuration`）は初回ログイン時に取得してプロセス内にキャッシュし、JWKS は未知の `kid` が来たとき（最短 60 秒間隔）に再取得します。IdP の Issuer を変更した場合はサーバーを再起動してください。


## 7. アカウント登録

`[auth.registration]`（[設定リファレンス](configuration.md#211-authregistration--アカウント登録)）を有効にすると、Admin Studio のログイン画面に **アカウントを作成** が表示され、管理者がユーザーを作らなくても自分でアカウントを申し込めます。メールアドレスの確認が必須で、確認メールは [通知](notifications.md) で送ります。

```toml
[auth.registration]
enabled = true
admin_url = "https://cms.example.com/admin/"   # 確認リンクの開き先
default_role = "viewer"
require_approval = true                         # 確認後も admin が有効にするまでログインできない
# allowed_email_domains = ["example.com"]       # 社内のアドレスだけに限る

[notifications]
enabled = true
channels = ["email"]   # 開発中は ["log"] でサーバーのログに出せる
```

通知（`[notifications].enabled`）が無効、`disabled_events` に `account.verify` がある、確認リンクの開き先（`admin_url`、無ければ `[commerce].public_url` + `/admin/`）が決まらない、`default_role` が `admin` または不明なロール、のいずれかの場合はサーバーが起動しません。

### 7.1 登録の流れ

1. ログイン画面の **アカウントを作成** から、ユーザー名・メールアドレス・表示名（任意）・パスワードを送ります（`POST /api/v1/auth/registration`）。
2. サーバーは入力を検証し、申し込みを確認リンクの有効期間（`token_ttl_secs`、既定 24 時間）だけキャッシュ（`[cache]`）に置いて、メールアドレスに確認リンク（`{admin_url}#verify-email=<トークン>`）を送ります（通知 `account.verify`）。**この時点ではユーザーは作られません。**
3. リンクを開くと Admin Studio が `POST /api/v1/auth/registration/verify` を呼び、ユーザーを作ります。運営者（`staff_recipients`）には `account.registered` を送ります。
4. `require_approval = true`（既定）なら、アカウントは **無効** の状態で作られます。admin が **設定 → ユーザー** で有効にすると、ログインできるようになります。`false` なら確認した時点でログインできます。

確認されない申し込みでユーザー名がふさがることはありません。同じユーザー名の申し込みが複数あれば、先に確認したほうが作られます。

### 7.2 API

#### `POST /api/v1/auth/registration` — 申し込み

```json
{ "username": "hanako", "email": "hanako@example.com", "password": "...", "display_name": "花子" }
```

- `202 Accepted`: `{ "status": "pending_verification", "expires_in_secs": 86400 }`
- 失敗時は `{ "error": "<文言>", "code": "<理由>" }` を返します。`code` は `invalid_username` / `invalid_email` / `password_too_short`（`400`）、`username_taken`（`409`）、`email_domain_not_allowed`（`403`）、`rate_limited`（`429`。`Retry-After` 付き）です。
- 登録が無効なら `404` を返します。

#### `POST /api/v1/auth/registration/verify` — 確認

```json
{ "token": "<確認リンクのトークン>" }
```

- `201 Created`: `{ "status": "active" | "pending_approval", "username": "hanako" }`
- 失敗時: `invalid_token`（`400`。無効・期限切れ・使用済み）、`username_taken` / `email_taken`（`409`。申し込みの後に同じユーザー名・メールアドレスのユーザーができていた）。

`GET /api/v1/auth/providers` の `registration`（`true` / `false`）で、ログイン画面に **アカウントを作成** を出すかを決めます。

### 7.3 セキュリティ上の挙動

- **メールアドレスの確認**: アカウントは確認リンクを開いたときに初めて作られるので、他人のメールアドレスでは作れません。
- **登録済みのメールアドレスを伏せる**: 既存のユーザーのメールアドレスで申し込まれても、応答は通常と同じ `202` にし、メールは送りません（監査ログには `email_in_use` として残ります）。パスワードのハッシュ計算も通常と同じように行うので、応答時間でも区別できません。ユーザー名は申し込み画面で重複を知らせる必要があるため、`username_taken` を返します。
- **確認リンク**: トークンは 256 ビットの乱数で、1 回しか使えません。キャッシュにはトークンの SHA-256 をキーにして保存し、トークン自体は残しません。URL のフラグメント（`#`）で渡すので、サーバーのログや `Referer` に残りません。Admin Studio は読み取った後にアドレスバーから消します。
- **リンクの URL はリクエストから作らない**: 確認リンクの開き先は設定（`admin_url`）だけから作ります。`Host` ヘッダーを偽装して、攻撃者のサイトへのリンクを本人に送らせることはできません。
- **申し込みの数の制限**: 同じクライアント IP からの申し込みは 1 時間に `max_per_ip_per_hour` 件（既定 5）までです。超えると `429` を返します。クライアント IP の扱いは `[audit].trust_forwarded_for` に従います。
- **パスワード**: 申し込みの時点で Argon2id でハッシュし、平文は保存しません。8 文字以上が必要です。
- **監査ログ**: 申し込み（`stage: requested`）と確認（`stage: confirmed`）を `user.register` に記録します。メールアドレスとトークンは記録しません（[監査ログ](audit.md)）。
- **ログチャネル**: `channels = ["log"]` では確認メールの本文（確認リンクを含む）がサーバーのログに出ます。本番ではメールを使ってください。プラグインの通知チャネル（`plugin:<名前>`）にも確認メールが渡るので、Slack などに送るプラグインでは `account.verify` を送らないよう設定してください（`official-order-notifier` なら `channel_kinds`）。

---

## 8. シングルサインオン (SAML 2.0)

`[auth.saml]`（[設定リファレンス](configuration.md#214-authsaml--シングルサインオン-saml-20)）を有効にすると、ログイン画面に **Sign in with {display_name}** ボタンが表示され、SAML 2.0 の IdP（Microsoft Entra ID、Okta、AD FS、Keycloak、Google Workspace など）でログインできます。OpenID Connect（6）・パスワードと併用できます。

対応するのは **SP-initiated の Web Browser SSO** です（AegisCMS のボタンから始めるログイン）。AuthnRequest は HTTP-Redirect バインディングで送り、応答は HTTP-POST バインディングで受け取ります。IdP のポータルから始めるログイン（IdP-initiated）、暗号化したアサーション（`EncryptedAssertion`）、シングルログアウトには対応していません。

### 8.1 IdP 側の設定

1. AegisCMS の SP メタデータ `https://<ホスト>/api/v1/auth/saml/metadata`（`enabled = true` で起動した後に取得できます）を IdP に読み込ませるか、次の値を手で登録します。
   - **Entity ID（識別子）**: `entity_id` と同じ値
   - **ACS（応答 URL / Reply URL）**: `https://<ホスト>/api/v1/auth/saml/acs`（`acs_url` と同じ値）。バインディングは HTTP-POST
2. **署名**: アサーション（推奨）または応答全体に署名するよう設定します（どちらか一方で十分です）。署名方式は RSA-SHA256 以上です（SHA-1 は拒否します）。
3. **NameID**: `persistent`（変わらない ID）か `emailAddress` を使います。ログインのたびに変わる `transient` はユーザーを識別できないため拒否します。
4. メールアドレス・表示名・グループを属性として送るよう設定し、その属性名を `email_attribute` などに書きます。
5. IdP のメタデータから **Entity ID**、**SSO の URL**（HTTP-Redirect）、**署名用証明書** を取り出して設定します。

```toml
[auth.saml]
enabled = true
display_name = "Company SSO"
entity_id = "https://cms.example.com/api/v1/auth/saml/metadata"
acs_url = "https://cms.example.com/api/v1/auth/saml/acs"
idp_entity_id = "https://sts.windows.net/<テナント ID>/"
idp_sso_url = "https://login.microsoftonline.com/<テナント ID>/saml2"
idp_certificates = ["""
-----BEGIN CERTIFICATE-----
MIIC8DCCAdigAwIBAgIQ...
-----END CERTIFICATE-----
"""]
email_attribute = "http://schemas.xmlsoap.org/ws/2005/05/identity/claims/emailaddress"
display_name_attribute = "http://schemas.microsoft.com/identity/claims/displayname"
role_attribute = "http://schemas.microsoft.com/ws/2008/06/identity/claims/groups"
post_login_redirect = "https://cms.example.com/admin/"

[auth.saml.role_mapping]
"<管理者グループの ID>" = "admin"
"<編集者グループの ID>" = "editor"
```

IdP の署名鍵を切り替えるときは、新旧の証明書を `idp_certificates` に並べてから IdP 側を切り替え、終わったら古いほうを消します。

### 8.2 ログインの流れ

1. ボタン（`GET /api/v1/auth/saml/login`）が AuthnRequest を付けて IdP の SSO の URL へリダイレクトします。AuthnRequest の `ID` は 10 分間キャッシュ（`[cache]`）に保存します。
2. IdP でのログイン後、ブラウザが `SAMLResponse` を `POST /api/v1/auth/saml/acs` に送ります。AegisCMS は署名と内容（8.4）を検証します。応答が答えている AuthnRequest の `ID` は 1 回限りで、同じ応答をもう一度送っても使えません。
3. ユーザーを決め（8.3）、`on-user-authenticated` フック（[プラグインフック](plugin_hooks.md#14-on-user-authenticated)）を通してセッションを発行します。
4. OpenID Connect と同じく、`post_login_redirect` に `#session=<トークン>`（失敗時は `#login_error=<理由>`）を付けて Admin Studio に戻ります。

### 8.3 ユーザーとロール

OpenID Connect（6.3）と同じ規則です。

- IdP のアカウント（IdP の Entity ID + NameID）と AegisCMS のユーザーの対応は `_aegis_user_identities` テーブルに保存します（プロバイダ名 `saml`）。
- **初回ログイン**: `auto_create_users = true`（既定）なら、`username_attribute`（空なら NameID）をユーザー名にしてユーザーを作成し、紐付けます。2 回目以降は NameID で同じユーザーを見つけるので、ユーザー名の属性が変わってもアカウントは変わりません。
- **同名のユーザーが既にいる場合は紐付けません**（`username_taken`）。既存のアカウントで SAML を使うには、管理者が `aegis-server user link-saml <username> <NameID>` で紐付けます。
- `auto_create_users = false` なら、紐付け済みのユーザーだけがログインできます（`not_provisioned`）。
- **ロール**: `role_attribute` を設定すると、ログインのたびにその属性の値を `role_mapping` で変換し（複数一致なら最も強いロール、一致しなければ `default_role`）、ユーザーアカウントのロールも更新します。空なら、ロールはユーザーアカウントのものです。
- メールアドレス・表示名はログインのたびにユーザーアカウントへ反映します。無効化したユーザーはログインできません（`account_disabled`）。
- セッションの `metadata` には `provider: "saml"`、`issuer`（IdP の Entity ID）、`name_id`、`name_id_format`、`session_index`、`attributes`（属性名 → 値の配列）が入ります。

### 8.4 検証の内容

| 対象 | 検証 |
| --- | --- |
| XML | DTD・処理命令・未宣言の名前空間接頭辞・重複した属性を含む文書と、256 KiB を超える応答は拒否 |
| 署名 | 応答全体かアサーションの少なくとも一方に、`idp_certificates` の鍵による署名が必要。応答の `KeyInfo` の証明書は使わない。Exclusive C14N・enveloped-signature のみ、RSA-SHA256 / 384 / 512。署名が参照する `ID` は文書内で一意 |
| 応答 | `StatusCode` が `Success`（それ以外は `idp_error`）、`Destination` が `acs_url`、`Issuer` が `idp_entity_id`、アサーションがちょうど 1 つ |
| アサーション | `Issuer` が `idp_entity_id`、`Conditions` の有効期間内、`AudienceRestriction` に `entity_id`、`AuthnStatement` がある |
| Subject | NameID がある（`transient` 以外）。bearer の `SubjectConfirmationData` の `Recipient` が `acs_url`、`NotOnOrAfter` が未来、`InResponseTo` が発行済みで未使用の AuthnRequest の `ID` |

時刻の検証には 60 秒の誤差を認めます。

**XML Signature Wrapping への対策**: 署名を検証した後は、受け取った文書からは何も読みません。署名の対象（応答またはアサーション）を、ダイジェストを計算したバイト列そのもの（正規化した XML）から改めて解析し、ユーザー名・属性・条件はすべてそこから読みます。署名済みのアサーションを別の場所に隠して偽のアサーションを置く、同じ `ID` の要素を足す、といった改変は、署名の検証に失敗するか、読む対象に入りません。NameID の途中にコメントを挟まれても、値は署名されたとおりの全体として読みます。

### 8.5 失敗時の理由

OpenID Connect（6.4）と同じ理由コードを使います。`invalid_token` は応答の検証に失敗した場合（署名・条件・形式。詳細はサーバーの警告ログ）、`invalid_state` は応答が答えている AuthnRequest が無い・期限切れ・使用済みの場合、`idp_error` は IdP が失敗の `StatusCode` を返した場合です。

> [!NOTE]
> SAML のログインも `[auth.login_rate_limit]` の対象外です。AuthnRequest には署名しません（メタデータは `AuthnRequestsSigned="false"`）。IdP 側で署名付きの AuthnRequest を必須にしている場合は外してください。
