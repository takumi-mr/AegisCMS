[← ドキュメント一覧に戻る](README.md)

# 顧客アカウント（EC）

ショップ（[EC](commerce.md)）の購入者が作るアカウントです。**CMS の管理アカウント（`_aegis_users`）とは完全に別**で、テーブルも、セッションも、権限も共有しません。購入者は、注文履歴の確認、商品レビューの投稿、配送先の保存に使います。

> [!NOTE]
> **設計と現状**。API、設定（§3）、購入者向けの画面（ログイン・会員登録・マイページ。§4）は実装済みです。商品ページのレビュー欄、メールアドレスの確認、退会、管理画面の顧客・レビュー管理は未実装で、この文書が実装の前に範囲と形を決めます。実装のたびに、この文書を実装に合わせて直します。

---

## 1. 現状（実装済み）

`[commerce].enabled = true` のとき、顧客アカウントとレビューが自動で有効になります（専用の設定はありません）。

| API | 内容 |
|---|---|
| `POST /api/v1/customers/register` | 会員登録（メール・パスワード・名前。電話・配送先は任意） |
| `POST /api/v1/customers/login` | ログイン。`{ "token": "cses_...", "expires_at": ..., "customer": {...} }` を返す |
| `POST /api/v1/customers/logout` | ログアウト |
| `GET` / `PUT /api/v1/customers/me` | 自分の情報の取得・更新（パスワード変更は現在のパスワードが要る） |
| `GET /api/v1/customers/orders`・`/orders/{id}` | 自分の注文履歴（ログイン中に注文すると、注文が顧客に紐づく） |
| `GET /api/v1/customers/reviews` | 自分のレビュー |
| `GET` / `POST /api/v1/products/{product_id}/reviews` | 商品のレビュー（公開済みの一覧・投稿） |
| `GET /api/v1/admin/customers`・`/{id}`、`PATCH` | 顧客の一覧・詳細・状態（`active` / `suspended`）の変更（`editor` 以上） |
| `/api/v1/admin/reviews`・`/settings`・`/bulk`・`/{id}`・`/{id}/reply` | レビューの管理（承認・一括・返信・設定） |

- 認証は `Authorization: Bearer cses_...`（API のクライアント向け）。セッションはキャッシュに置き、既定で 30 日。
- パスワードは Argon2id。登録済みのメールでの登録は `409`。
- レビューは「購入者（Verified Purchase）」だけが、1 注文につき 1 件、★1〜5 で投稿できる。**ゲスト購入の人も、注文のアクセストークンでレビューできる**（アカウントは要らない）。審査（承認待ち・スパム判定）は `[comments.moderation]` の仕組みを共有する。

### 足りないもの

1. 購入者が使う**画面**（ログイン・会員登録・マイページ・商品ページのレビュー欄）がない。API を直接呼ぶしかない。
2. 管理画面に、顧客とレビューの**画面**がない。
3. パスワードを忘れたときの**再設定**と、登録したメールアドレスの**確認**がない。
4. **退会**（自分のアカウントの削除）がない。
5. 新規登録を止める、メール確認を必須にする、などの**設定**がない。

---

## 2. 範囲

- ゲスト購入は残します（アカウントなしで注文できる。注文の確認は今までどおり URL のトークン）。アカウントは任意です。
- 顧客のアカウントで、CMS の管理画面には入れません（逆も同じ）。
- ソーシャルログイン、ポイント・クーポンの顧客別管理、複数の配送先、ウィッシュリストは範囲外です。

---

## 3. 設定

```toml
[commerce.customers]
enabled = true                      # 顧客アカウントを使う（既定 true。今までどおり）
registration = "open"               # "open" | "closed"（closed は新規登録だけを止める。既存の顧客はログインできる）
session_days = 30                   # ログインの有効期間（1〜365）
# require_email_verification = false  # 実装予定（§5）: true は、メールの確認リンクを開くまでログインできない（要 [notifications] のメール）
```

`enabled` / `registration` / `session_days` は実装済みです（[設定リファレンス](configuration.md#commercecustomers)）。`registration = "closed"` の新規登録は `403` です。

- `enabled = false` なら、顧客の API と画面は 404 です（ゲスト購入とレビューは使えます）。
- `require_email_verification = true` で `[notifications]` のメールが使えないときは、確認メールを送れず誰もログインできなくなるので、**起動時にエラー**にします。

---

## 4. 購入者向けの画面

### 4.1 ルート

| パス | 内容 |
|---|---|
| `/account` | マイページ（ログイン中: 注文履歴・自分のレビュー・登録情報。未ログイン: ログイン画面へ） |
| `/account/login` | ログイン |
| `/account/register` | 会員登録（`registration = "closed"` なら 404） |
| `/account/forgot` | パスワードを忘れたとき（再設定メールの申し込み） |
| `/account/reset?token=...` | 新しいパスワードの設定 |
| `/account/verify?token=...` | メールアドレスの確認 |

### 4.2 ログインの持ち方

画面（ブラウザ）では、ログインすると **HttpOnly の Cookie** にセッションを持たせます。JavaScript はトークンに触れません（XSS で盗まれにくい）。API のクライアントは、今までどおり `Authorization: Bearer` を使います。顧客の API は、どちらの形でも受け付けます（`Authorization` があればそちらを使う）。ログインの応答の本文には、従来どおりトークンも入ります。

- Cookie: 名前は `{名前空間}_customer`（既定は `cms_customer`。`[server]` の名前空間から作るので、管理画面のものと混ざらない）。`HttpOnly`、`SameSite=Lax`、`Path=/`、`Max-Age` はログインの有効期間（`session_days`）、HTTPS のとき（`X-Forwarded-Proto: https` または `[commerce].public_url` が `https://`）は `Secure`。ログアウトで `Max-Age=0` にして消す。
- **CSRF**: 状態を変える API は JSON（`Content-Type: application/json`）だけを受け付けます（今のまま）。`SameSite=Lax` で他のサイトからの POST には Cookie が付かず、CORS は認証情報つきの要求を許さない（今のまま）ので、他のサイトから操作できません。
- ログイン中に注文すると、注文が顧客に紐づきます（Cookie でも Bearer でも同じ）。
- 顧客のトークン（`cses_...`）は管理画面の API では通らず、管理者のトークンは顧客の API で通らない。

マイページは、ほかのショップのページ（カート・注文確認）と同じ作りです。サーバーはログインの状態だけを見て（未ログインなら `/account/login` へ、ログイン済みなら `/account` へ転送）、**骨組みのページ**を返します。中身は、スクリプト（`/shop/account.js`）が API を呼んで作ります（注文履歴は最初の応答には入らない）。どの応答も `Cache-Control: private, no-store` で、共有のキャッシュに載りません。

### 4.3 テーマ

- 組み込みのテンプレート `account.html` を用意しています（テーマに `account.html` がなければ、これを使う）。同梱のテーマは、何も変えなくても動きます。
- テーマ制作者は `account.html` を上書きできます。`slug` が表示（`home` / `login` / `register`）、`theme_vars` に次が入ります: `account_enabled`（`"true"` / `"false"`）、`account_url`、`account_login_url`、`account_register_url`（登録を止めているときは空）、`account_script`、`account_api`、`account_min_password`。
- 組み込みの `account.html` は、`<div id="aegis-account" data-view="{{ slug }}">` をスクリプトが埋める作りです。上書きするときは、この要素と、`account_*` を `data-*` に渡すスクリプトの読み込みを残してください（スクリプトの文言は `[theme].locale` の言語）。見た目は、`.aegis-account-*` と `.aegis-field`・`.aegis-button` などのクラスで変えられます。
- ショップのナビゲーション（`shop_common.html`）に、顧客アカウントを使うときだけ「アカウント」のリンクが出ます。

### 4.4 商品ページのレビュー

- `product.html` に `reviews` を渡す: 公開済みの一覧（★・本文・投稿者名・Verified Purchase・運営の返信・日付）、平均と件数。サーバー側で描画する（検索エンジンにも見える）。平均は JSON-LD の `AggregateRating` にも使う。
- 投稿欄は、ログイン中の購入者か、注文のトークン付きのリンク（`/orders/{id}?token=...`）から開いたゲスト購入者に出す。それ以外には「購入した方がレビューできます」と出す。
- 審査待ちになる場合は、投稿後にそう伝える（`[comments.moderation]` に従う）。

---

## 5. パスワードの再設定とメールの確認

### 5.1 パスワードの再設定（実装済み）

管理アカウントの再設定（`/api/v1/auth/password-reset`）と同じ作りです（トークンはキャッシュにハッシュで保存し、1 回だけ使える）。

| API | 内容 |
| --- | --- |
| `POST /api/v1/customers/password-reset` | `{ "email" }`。登録の有無によらず、メールアドレスの形が正しければ**常に `202`**。登録があり停止中でなければ、再設定のリンクをメールで送る。有効 1 時間 |
| `POST /api/v1/customers/password-reset/complete` | `{ "token", "password" }`。パスワードを変え、その顧客の**すべてのセッションを失効**させる。リンクは 1 回だけ。無効・期限切れ・使用済みは `400` |

- **画面**: `/account/forgot`（メールアドレスを入れる）、`/account/reset?token=...`（新しいパスワード。読んだ直後に、アドレスバーからトークンを消す）。ログイン画面に「パスワードをお忘れの方」のリンクが出る。
- **使える条件**: `[notifications]` が有効（メールを送れる）で、`[commerce].public_url` が設定されていること（`[commerce.customers].password_reset = false` で止められる）。メールのリンクは、**リクエストの `Host` ではなく `public_url` から作る**（`Host` は偽装できるので、攻撃者が被害者に偽のリンクを届けられてしまう）。使えない設定では、API もページも 404 で、ログイン画面はリンクを出さない。
- **制限**: 同じ IP からの申し込みは 1 時間に 5 回まで（登録の有無によらず数える。超えると `429`）。同じ宛先へのメールは 1 時間に 3 通まで（超えた分は、応答は同じで、送らない）。
- **通知**: `customer.password_reset`（日本語・英語。文面は `[notifications]` の仕組みで編集できる）。監査ログ: `customer.password_reset`（完了したとき）。

### 5.2 セッションの失効（実装済み）

顧客のセッションは、次のときにすべて終わります。セッションごとには消さず、顧客ごとに「この時刻より前に作ったセッションは無効」の印をキャッシュに置きます（Redis でも全サーバーで効く）。

- 管理者が顧客を**停止**したとき（以前は、停止してもログイン中のセッションは使えたままだった）。再開しても、停止前のセッションは戻らない。
- **パスワードを変えた**とき（マイページ・再設定）。マイページで変えた本人のブラウザには、新しい Cookie が渡るので、そのまま使い続けられる。

### 5.3 メールアドレスの確認（未実装）

- 確認が必要な顧客の状態は、新しい `unverified`（`active` / `suspended` に加わる）。ログインは `403`（確認を促すメッセージ）。確認すると `active` になる。**テーブルの列は増やさない**（古い DB を拒否する対象を増やさないため。状態は文字列の列にそのまま入る）。
- `POST /api/v1/customers/verify`（`{ "token" }`。有効 48 時間）、`POST /api/v1/customers/verify/resend`（`{ "email" }`。常に `202`）。通知 `customer.verify`。
- 設定 `[commerce.customers].require_email_verification`。`true` で `[notifications]` のメールが使えないときは、確認メールを送れず誰もログインできなくなるので、**起動時にエラー**にする。

---|---|
| `POST /api/v1/customers/password-reset` | `{ "email" }`。**常に `202`**（登録の有無を応答で教えない）。登録があれば再設定のリンクをメールで送る。有効 1 時間 |
| `POST /api/v1/customers/password-reset/complete` | `{ "token", "password" }`。パスワードを変え、**すべてのセッションを失効**させる |
| `POST /api/v1/customers/verify` | `{ "token" }`。メールアドレスを確認する。有効 48 時間 |
| `POST /api/v1/customers/verify/resend` | `{ "email" }`。**常に `202`**。確認メールを送り直す |

- 確認が必要な顧客の状態は、新しい `unverified`（`active` / `suspended` に加わる）。ログインは `403`（確認を促すメッセージ）。確認すると `active` になる。**テーブルの列は増やさない**（古い DB を拒否する対象を増やさないため。状態は文字列の列にそのまま入る）。
- 通知のテンプレートを足す: `customer.verify`・`customer.password_reset`（日本語・英語）。メールの文面は `[notifications]` の仕組みで編集できる。
- 申し込みは IP と宛先のメールごとに回数を制限する。

---

## 6. 退会

`DELETE /api/v1/customers/me`（パスワードが要る）で、自分のアカウントを削除できます。

- 顧客の行とセッションを削除する。
- **注文は残す**（会計の記録）。注文と顧客の紐づき（`customer_id`）だけを外す。注文に書かれたメール・氏名・配送先は、注文の記録として残る。
- レビューは残し、投稿者名を「退会した顧客」に置き換えて、顧客との紐づきを外す。
- 監査ログに記録する（メールアドレスは記録しない）。

---

## 7. 管理画面

Admin Studio の「ショップ」に 2 つの画面を足します（API は実装済み）。

- **顧客**（`editor` 以上）: 一覧（メール・名前・状態・登録日・最終ログイン。検索）、詳細（登録情報・注文の件数と合計・レビュー）、状態の変更（`active` / `suspended`。停止するとセッションを失効させる）。
- **レビュー**（`editor` 以上）: 状態での絞り込み（承認待ち・承認・非承認・スパム）、一括の承認・非承認、本文の確認、運営の返信、設定。

---

## 8. 安全性

- **登録の有無を教えない**: パスワードの再設定と確認メールの再送は、常に `202`。ログインの失敗も、メールが無い場合とパスワードが違う場合を区別しない（今のまま）。
- **登録時の重複**: 登録済みのメールでの登録は `409` のまま。`require_email_verification = true` のときは、登録の応答を一律にして（確認メールを送ったとだけ伝える）、登録済みかどうかを漏らさない。
- **レート制限**: 登録・ログイン・再設定・再送を、IP ごとに制限する（既存のキャッシュのカウンター）。
- **セッション**: パスワードを変えたら、ほかのセッションをすべて失効させる。顧客を停止したときも失効させる。
- **管理アカウントと混ざらない**: 顧客のトークン（`cses_...`）は、管理画面の API では通らない。逆も同じ。
- **個人情報**: 顧客の API の応答にパスワードのハッシュは含めない。退会で削除できる。バックアップ（`task dump`）には顧客も入る。

---

## 9. テスト方針

- 顧客の登録・ログイン・Cookie・ログアウト・停止の流れ（Cookie と Bearer の両方）。
- 他人の注文・レビューが見えないこと。管理アカウントのトークンと顧客のトークンが互いに通らないこと。
- 再設定・確認のトークン: 期限切れ・再利用・改ざんの拒否、申し込みが常に `202` であること、レート制限。
- 退会後に、注文が残って顧客との紐づきだけが外れること。レビューの匿名化。
- 画面: 組み込みの `account.html` が各 `view` で描画されること。ゲスト購入が今までどおり動くこと。
- 管理画面: Storybook と Playwright の E2E。

---

## 10. 実装の分け方（案）

1. 設定 `[commerce.customers]`（`enabled`・`registration`・`session_days`）と、このドキュメント、`commerce.md` の誤りの修正。**済**
2. Cookie のセッションと、購入者向けの画面（`/account`、`account.js`、`account.html`）。**済**。商品ページのレビュー欄（§4.4）は、続けて行う。
3. パスワードの再設定、セッションの失効。**済**。メールアドレスの確認（`require_email_verification`、`customer.verify`）は、続けて行う。
4. 退会。
5. 管理画面の顧客・レビュー。
