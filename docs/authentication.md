# 認証 & ユーザー管理 (Authentication)

AegisCMS のログインは、ユーザー名とパスワードを `AuthProvider` が検証したときだけセッション（Bearer トークン）を発行します。ロールはリクエストではなく、サーバーに登録されたユーザーアカウントで決まります。

現在のプロバイダはローカル認証（`LocalAuthProvider`、`_aegis_users` テーブル）のみです。OIDC / SAML は今後 `AuthProvider` の実装として追加します。

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
| `created_at` / `updated_at` | RFC 3339 (UTC) |

### ロールと API の権限

ロールは `admin` ⊃ `editor` ⊃ `viewer` の包含関係です（上位ロールは下位ロールの操作をすべて行える）。

| API | 必要なロール |
| --- | --- |
| `GET /api/v1/health`, `GET /api/v1/openapi.json`, `/metrics` | 不要（公開） |
| 公開サイト（`/`, `/articles/*`, `/{slug}`, `/theme/assets/*`, `/uploads/*`）, Admin Studio の静的ファイル | 不要（公開） |
| `POST /api/v1/auth/session`（ログイン）, `GET` / `DELETE /api/v1/auth/session` | 不要（資格情報・セッションで判定） |
| `GET /api/v1/schemas`, `GET /api/v1/schemas/{type}` | `viewer` |
| `GET /api/v1/contents/{type}`, `GET /api/v1/contents/{type}/{slug}`（下書きを含む） | `viewer` |
| `GET /api/v1/storage/objects` | `viewer` |
| `POST /api/v1/preview`（エディタのプレビュー。保存しない） | `viewer` |
| `POST /api/v1/contents/{type}` | `editor` |
| `POST /api/v1/storage/upload` | `editor` |
| `GET /api/v1/audit-logs` | `admin` |

- セッションが無い・無効・期限切れなら `401`、ロールが足りなければ `403` を返します。
- コンテンツ作成時の `author_id` は省略するとログインユーザー（`user_id`）になります。他のユーザーを `author_id` に指定できるのは `admin` だけです（それ以外は `403`）。
- `/metrics`（Prometheus）は監視系からの取得を想定して公開のままです。外部に公開する場合はリバースプロキシなどでアクセス元を制限してください。

---

## 2. CLI でのユーザー管理 (`aegis user`)

```bash
# 最初の管理者を作成（パスワードは対話入力。端末では非表示・確認つき）
aegis-server user add admin --role admin --email admin@example.com --display-name "Site Admin"

# 非対話（CI / Docker）: 環境変数か標準入力の 1 行からパスワードを読む
AEGIS_USER_PASSWORD='change-me-please' aegis-server user add editor1 --role editor
echo 'change-me-please' | aegis-server user add viewer1 --role viewer

aegis-server user passwd editor1        # パスワード変更
aegis-server user role editor1 admin    # ロール変更（次回ログインから有効）
aegis-server user disable editor1       # ログイン不可にする
aegis-server user enable editor1
aegis-server user list
```

- 開発時は `cargo run -p aegis-server -- user add ...`、Docker では `docker exec -it <container> aegis-server user add ...` で実行します。設定ファイル（`-c` / `AEGIS_CONFIG`）の `[database]` に接続します。
- パスワードは 8 文字以上です。
- ユーザーが 1 人もいない状態でサーバーを起動すると、作成方法を示す警告ログを出します。

> [!NOTE]
> ロール変更・無効化・パスワード変更は、発行済みのセッションには反映されません（セッションは有効期限まで有効）。すぐに締め出す必要がある場合は、キャッシュ（Redis など）から該当セッションを削除してください。

---

## 3. セッション API

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
| `422` | `username` / `password` が無い（以前の `{"user_id", "role"}` 形式は受け付けない） |
| `500` | DB・キャッシュのエラー |

### `GET /api/v1/auth/session` / `DELETE /api/v1/auth/session`

現在のセッションの取得とログアウトです（変更なし）。

### Admin Studio

Admin Studio（`/admin`）はログイン画面からこの API でログインし、トークンを `sessionStorage` に保持してすべての API 呼び出しに付与します。API が `401` を返すとログイン画面に戻ります。ロールが足りない操作（`viewer` の保存・アップロードなど）は画面上でも無効化します。

---

## 4. セキュリティ上の挙動

- **ユーザー名の推測対策**: 存在しないユーザーでもダミーハッシュで同じ Argon2 検証を行い、応答時間と応答内容を揃えます。アカウントが無効かどうかは、正しいパスワードを送った場合にしか判定しません。
- **監査ログ**: ログインの成否を `session.create` として記録します。失敗時は `details` に `username`・`provider`・`error`（`unknown_user` / `invalid_password` / `account_disabled` / `unsupported_credentials`）を残します。パスワードは記録しません。
- **CPU 負荷**: Argon2 の検証は `spawn_blocking` で実行し、非同期ランタイムのワーカーを塞ぎません。

> [!CAUTION]
> ログイン試行回数の制限（レート制限・アカウントロック）はまだありません。インターネットに公開する場合は、リバースプロキシ側で `/api/v1/auth/session` にレート制限をかけてください。
