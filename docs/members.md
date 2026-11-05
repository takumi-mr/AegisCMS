[← ドキュメント一覧に戻る](README.md)

# 会員アカウント（設計）

サイトの訪問者が作るアカウントです。**CMS の管理アカウント（`_aegis_users`）とは完全に別**で、テーブルも、セッションも、権限も共有しません。

今は「顧客アカウント」として `[commerce]` の中にあり、ショップの購入者だけが使えます（動きは [customers.md](customers.md) に書いたとおり）。これを **`[commerce]` から切り出して「会員（member）」にし、ショップ・掲示板（[forum](forum.md)）・（将来の）コメントが同じアカウントを使う**ようにします。

> [!NOTE]
> **設計**。まだ実装していません。実装の順は §9。後方互換は残しません（破壊的変更。§8）。実装のたびに、この文書を現状に合わせて直し、終わったら [customers.md](customers.md) をこの文書に統合します。

---

## 1. 目的と範囲

- 同じ人が、ショップでも掲示板でも**同じアカウント**でログインできる。
- 掲示板だけのサイト（`[commerce]` なし）でも、訪問者が登録して投稿できる。
- 機能（登録・ログイン・Cookie・パスワードの再設定・メールの確認・停止・退会）は、今の顧客アカウントをそのまま流用する。**作り直さない**。
- ショップ固有のもの（注文履歴・配送先・レビュー）は、ショップが有効なときだけ使える。

範囲外: ソーシャルログイン、会員の公開プロフィール、フォロー、会員どうしのメッセージ、会員の権限の段階（ロール）。

---

## 2. 呼び方

内部は **member に統一**し、訪問者に見える文言だけを機能ごとに変えます。

| 層 | 呼び方 |
| --- | --- |
| 設定・API・DB・コード・監査ログ・通知 | `member`（`[members]`、`/api/v1/members/*`、`_aegis_members`、`MemberService`、`member.*`） |
| 管理画面 | タブは「会員」。詳細で、ショップが有効なら注文、掲示板が有効なら投稿を見せる |
| 訪問者に見える文言 | 画面の種類で変える。ショップの画面（注文履歴・レビュー）は「お客様」、掲示板の画面は「ユーザー」。メッセージのキー（`js.*`）で出し分け、`[theme].locale` の言語で出す |

ページのパスは変えません（`/account`、`/account/login` …）。会員はショップのものではなくなりますが、`/account` は特定の機能の名前ではないので、そのまま使えます。

---

## 3. 設定

```toml
[members]
enabled = true                      # 会員アカウントを使う（既定 false。ショップを使うなら true に）
registration = "open"               # "open" | "closed"
session_days = 30                   # ログインの有効期間（1〜365）
password_reset = true               # パスワードの再設定を使う（要 [notifications] のメールと public_url）
require_email_verification = false  # true はメールの確認リンクを開くまでログインできない（同上）
public_url = ""                     # メールのリンクの元（例 "https://example.com"）。空だと、メールを使う機能は動かない
```

- **既定を `false` にする**: 今の `[commerce.customers].enabled` は既定 `true` でした。会員が `[commerce]` の外に出るので、使うサイトだけが `[members].enabled = true` にします。ショップを使っていて会員を使わない（ゲスト購入だけ）サイトは、何も書かなければ今までどおり動きます。
- `[members].public_url` は**会員のメールのリンクだけ**に使います。注文確認のリンクは、今までどおり `[commerce].public_url`（ふたつを別に持つので、ショップとフォーラムを別のホストで出すサイトでも合う）。
- 起動時の確認は今と同じです。`require_email_verification = true` で `[notifications]` が無効、または `public_url` が空なら、サーバーは起動しません。
- `[members].enabled = false` なら、会員の API・画面は 404 です。

### 機能ごとの設定

会員を**どう使うか**は、機能の側に置きます。

```toml
[forum]
posting = "anyone"      # "anyone"（今の allow_anonymous = true）| "members"（会員とスタッフだけが投稿できる）
# [commerce] は変更なし: レビューは今までどおり購入者（ゲストは注文のトークン）が書く
```

- `[forum].allow_anonymous` は廃止し、`posting` に置き換えます（§5）。`posting = "members"` は `[members].enabled = true` が要り、無ければ**起動しない**。
- 会員を要求するかどうかは、機能ごとに決めます（ショップはゲスト購入を残す）。

---

## 4. データ

### 4.1 テーブルと列

| 今 | これから |
| --- | --- |
| `_aegis_customers` | `_aegis_members` |
| `_aegis_orders.customer_id` | `member_id` |
| `_aegis_reviews.customer_id` | `member_id` |

- 会員の列は今の顧客の列を引き継ぎ、**`nickname`（公開名）を足す**。
  - 今の `name` は配送先にも使う本名で、そのまま公開名にすると本名が出てしまう。掲示板では**公開名（`nickname`）が必須**にする（未設定なら、最初の投稿の前に決めてもらう）。
  - レビューは今までどおり `name` を出す（変えない）。会員が `nickname` を持っていれば、新しいレビューは `nickname` を使う。
- `phone`・`shipping_address_json` は、ショップを使わない会員には空のまま。
- 状態は今と同じ（`active` / `suspended` / `unverified`）。

### 4.2 ID・セッション・Cookie

| 今 | これから |
| --- | --- |
| セッションのトークン `cses_...` | `mses_...` |
| キャッシュの `customer_session:` / `customer_revoked:` / `customer_verify_*` など | `member_session:` / `member_revoked:` / `member_verify_*` … |
| Cookie `{名前空間}_customer`（既定 `cms_customer`） | `{名前空間}_member`（既定 `cms_member`） |

- 切り替えのとき、ログイン中の人はいったんログアウトになります（Cookie の名前が変わるため。破壊的変更）。
- 会員のトークンは管理 API で通らず、管理者のトークンは会員の API で通らない（今のまま）。

---

## 5. 掲示板への組み込み

### 5.1 投稿者の持ち方

今の掲示板は `author_id` に **CMS ユーザー（スタッフ）の ID**を入れ、匿名は `author_id` が空で名前が自由入力です。

- スレッドとレスに **`author_kind`（`staff` / `member`。匿名は空）** を足す。`author_id` の接頭辞では区別しない（ユーザー名が会員の ID と同じになりうるため）。
- `author_name` は投稿時の表示名のコピーのまま（会員は `nickname`）。投稿者が後から名前を変えても、過去の投稿は変わらない。
- 掲示板の表は別の DB に置ける（`[forum].database_url`）ので、`author_id` は会員の表への外部キーにしない（今のスタッフも同じ）。
- 承認の判定（承認済みの投稿がある人は自動承認）は、`(author_kind, author_id)` で見る。匿名は今までどおり名前で見る。

### 5.2 投稿できる人

| `posting` | 誰が投稿できるか |
| --- | --- |
| `"anyone"` | だれでも（会員は会員として、匿名は名前を入れて）。スタッフはスタッフとして |
| `"members"` | 会員とスタッフだけ。未ログインはログインの画面へ |

- 非公開のカテゴリ（`is_private`）は、今は「ログインしていれば投稿できる」ですが、スタッフしかログインできなかったので実質スタッフ専用でした。会員が入ると意味が変わるので、**カテゴリごとに公開範囲（`visibility`: `public` / `members` / `staff`）を持たせる**。`is_private = true` は `staff` に読み替える。
- 会員が自分の投稿を**編集・削除**できるかは、まず**できない**とする（モデレーションの履歴を保つ。スタッフは今までどおり）。要望が出たら別に設計する。

### 5.3 画面

- フォームの投稿は Cookie で認証する。フォームは JSON ではないので、会員の API（JSON だけを受ける）にある CSRF の防ぎ方が使えない。**投稿は `Origin` / `Referer` を確かめ、フォームにも CSRF トークンを入れる**（`SameSite=Lax` だけに頼らない）。
- 投稿者の表示: 会員の名前にバッジは付けない。スタッフには今までの印を出す。管理画面の掲示板では、会員の投稿に「会員」と出し、会員の詳細へのリンクを付ける。
- 掲示板のページに、ログイン・登録・ログアウトのリンクを出す（`posting = "members"` のときは、フォームの代わりにログインを促す）。

### 5.4 退会

会員が退会しても、掲示板の投稿は**残して匿名にする**（スレッドが途切れないように）。`author_id` を空にして、`author_kind` も空にし、`author_name` は「退会したユーザー」に置き換える。レビューや注文と同じ方針で、途中で失敗しても同じ手順をやり直せる。

---

## 6. ショップとの関係

- 注文の `member_id`、注文履歴（`GET /api/v1/members/orders`・`/{id}`）、会員のレビュー（`GET /api/v1/members/reviews`）、配送先の保存は、**ショップが有効なときだけ**使える（無効なら 404）。
- ゲスト購入は残す。会員は任意。
- 会員の登録・ログイン・再設定・確認・停止・退会の仕組みは、`aegis-commerce` から新しいクレート **`aegis-members`** に移す。ショップのクレートは会員のクレートに依存せず（注文は `member_id` の文字列を持つだけ）、注文と会員をつなぐ処理（注文履歴・退会時の紐づき外し）はサーバー側に置く。
- 退会は、注文の紐づきを外す・レビューを匿名にする・掲示板の投稿を匿名にする、のうち**有効な機能の分**だけを行う。

---

## 7. 管理画面

- 「顧客」タブを「会員」にする（`[members].enabled` のとき）。一覧に公開名を足し、詳細は有効な機能の分だけ見せる（ショップ: 注文と支払い済みの合計 / 掲示板: 投稿の件数と直近のスレッド）。
- 「レビュー」タブは今のまま（ショップが有効なとき）。
- 管理 API: `GET /api/v1/admin/members`・`/{id}`、`PATCH`。

---

## 8. 破壊的変更（互換は残さない）

| 変わるもの | 移行 |
| --- | --- |
| 設定 `[commerce.customers]` | `[members]` に移す。古いキーが残っていたら、**起動時に「`[members]` に移してください」と案内してエラー**にする |
| `/api/v1/customers/*`・`/api/v1/admin/customers/*` | `/api/v1/members/*`・`/api/v1/admin/members/*`（古いパスは 404） |
| `_aegis_customers`、注文・レビューの `customer_id` | 表と列の名前を変える。起動時の列の確認で古い DB を拒否し、手順を案内する（下記） |
| 掲示板の `author_kind`、`visibility` | 同上 |
| 監査ログの `customer.*`、通知テンプレート `customer.*` | `member.*`（過去の監査ログの行はそのまま読める） |
| セッション・Cookie | 全員ログアウト |
| `[forum].allow_anonymous` | `posting` に置き換え |
| バックアップ（`task dump` / `restore`） | ファイル名を `database/members.json` にする。古い dump は、顧客のファイルを読んで取り込む |

古い DB の移行は、**列と表の名前を直す SQL を文書に載せる**（自動では直さない。システム表の途中までの更新は、今までも持たない方針）。

```sql
ALTER TABLE _aegis_customers RENAME TO _aegis_members;
ALTER TABLE _aegis_members ADD COLUMN nickname TEXT;           -- 型は DB に合わせる
ALTER TABLE _aegis_orders RENAME COLUMN customer_id TO member_id;
ALTER TABLE _aegis_reviews RENAME COLUMN customer_id TO member_id;
ALTER TABLE _aegis_forum_threads ADD COLUMN author_kind TEXT;  -- 既存の author_id がある行は 'staff'
ALTER TABLE _aegis_forum_posts ADD COLUMN author_kind TEXT;
```

---

## 9. 実装の分け方

1. **この設計書。**
2. **切り出しと改名**（振る舞いは変えない）: `aegis-members` クレートに移す、`[members]`、`/api/v1/members/*`、`_aegis_members`、`member_id`、Cookie、通知、監査ログ、管理画面の「会員」、dump / restore、テスト、docs（`customers.md` を統合）。
3. **公開名（`nickname`）**: 列、マイページでの編集、レビューでの使い分け。
4. **掲示板を会員に対応**: `author_kind`、`posting`、`visibility`、会員の投稿、CSRF、画面のログイン導線、退会時の匿名化、テスト。
5. **管理画面**: 会員の詳細に掲示板の投稿を出す、掲示板の管理画面に会員の印を出す。

2 が大きいので、機械的な改名に徹し、機能の変更は 3 以降に分けます。

---

## 10. テスト方針

- 2: 今の顧客のテスト（Rust・Storybook・E2E）を、名前を変えて**すべて通す**。古い設定キー・古い DB を拒否すること。
- 4: 会員の投稿と匿名の投稿の区別、`posting = "members"` で未ログインを弾くこと、`visibility` ごとの読み書き、他のサイトからのフォーム送信（`Origin` が違う）を弾くこと、退会で投稿が匿名になること、掲示板の DB を分けた構成。
- 管理アカウントと会員のトークンが互いに通らないこと（今のまま）。
