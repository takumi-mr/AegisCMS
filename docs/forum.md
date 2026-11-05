[← ドキュメント一覧に戻る](README.md)

# 掲示板（フォーラム）

カテゴリ・スレッド・レスからなる掲示板です（`[forum]`、クレート `aegis-forum`）。公開サイトに `/forum` 以下のページが加わり、Admin Studio の **Forum** タブでカテゴリの管理とモデレーション（ピン留め・ロック・削除）を行います。記事やコメントとは別の表に保存し、必要なら別のデータベースに分けられます。

> 動くサンプルとして、山歩きのコミュニティ掲示板 [`examples/hiking-forum/`](../examples/hiking-forum)（`task demo:forum` で起動）があります。

---

## 1. 有効にする

```toml
[forum]
enabled = true
# database_url = "sqlite:data/forum.db?mode=rwc"   # 掲示板だけ別の DB に置く（省略時は [database].url を共有）
posting = "anyone"          # "anyone": ログインしていない人も投稿できる / "members": 会員とスタッフだけ（要 [members]）
rate_limit_secs = 5         # 同じ IP・ユーザーの投稿の間隔
auto_lock_post_count = 1000 # この数のレスでスレッドを自動でロック（0 で無制限）
```

設定項目の一覧は [設定リファレンス](configuration.md#216-forum--掲示板) を参照してください。

- 表は `_aegis_forum_categories` / `_aegis_forum_threads` / `_aegis_forum_posts`。サーバーの起動時と `aegis-server migrate` で作成します（`database_url` を指定した場合はその DB に作ります）。
- **別の DB に分ける場合**: 投稿が集中したときの書き込みのロック（SQLite の `SQLITE_BUSY`）や、PostgreSQL / MySQL の接続数が本体（記事・EC・セッション）に影響しないようにできます。SQLite・PostgreSQL・MySQL のどれでも指定できます。

---

## 2. 公開サイト

| URL | テンプレート | 内容 |
| --- | --- | --- |
| `/forum` | `forum_index.html` | カテゴリの一覧と、最近更新されたスレッド |
| `/forum/{カテゴリの slug}`（`?page=N`） | `forum_category.html` | カテゴリのスレッドの一覧（ピン留め → 最終投稿の新しい順）とスレッドの作成フォーム |
| `/forum/{カテゴリの slug}/{スレッド ID}` | `forum_thread.html` | レスの一覧（レス番号付き）と返信フォーム |

![サンプル掲示板「やまびこ」のトップページ](images/forum-public.png)

![サンプル掲示板のスレッド（レス番号・返信先・スタッフの印）](images/forum-thread.png)

- フォームは `POST /forum/{slug}/threads`・`POST /forum/{slug}/{スレッド ID}/posts` に送ります。成功すると `303` で元のページ（レスは `#post-N`）に戻り、結果のメッセージを表示します。
- ログイン中は表示名をアカウントから取ります。未ログインで名前を空にすると `Anonymous`。
- テーマに同じ名前のテンプレートを置くと組み込みのものの代わりに使います（共通部分は `forum_common.html`）。

---

## 3. Admin Studio

**Forum** タブ（`[forum].enabled = true` のときに表示）:

![Admin Studio のスレッド・モデレーション](images/forum-admin.png)

- **スレッド**: 一覧（カテゴリで絞り込み）、ピン留め・ロックの切り替え、削除、レスの一覧と個別の削除、公開ページへのリンク。
- **カテゴリ**: 作成（名前・slug・説明・並び順）・編集・削除。

> [!NOTE]
> タブは `editor` 以上に表示されます。スレッドとレスのモデレーション（ピン留め・ロック・削除）は `editor` 以上、カテゴリの作成・編集・削除は `admin` だけが行えます（コメントの審査は `editor`・設定は `admin` と同じ分け方）。`editor` にはカテゴリの一覧だけが表示されます。

---

## 4. API

| API | 権限 | 内容 |
| --- | --- | --- |
| `GET /api/v1/forum/categories` | 不要 | カテゴリの一覧 |
| `GET /api/v1/forum/categories/{slug}` | 不要 | カテゴリ |
| `POST /api/v1/forum/categories` | `admin` | カテゴリの作成（`201`） |
| `PUT` / `DELETE /api/v1/forum/categories/{id}` | `admin` | カテゴリの更新・削除 |
| `GET /api/v1/forum/threads?category_slug=&limit=&cursor_last_post_at=&cursor_id=` | 不要 | スレッドの一覧（`limit` は 1〜100、既定 `default_page_size`） |
| `GET /api/v1/forum/categories/{slug}/threads` | 不要 | カテゴリのスレッドの一覧（同じクエリ） |
| `GET /api/v1/forum/threads/{id}` | 不要 | スレッド |
| `GET /api/v1/forum/threads/{id}/posts?cursor_post_number=&limit=` | 不要 | レスの一覧（`cursor_post_number` より後） |
| `POST /api/v1/forum/threads` | 不要（`posting = "members"` なら会員かスタッフのログイン） | `{ "category_id", "title", "body", "author_name" }`。`{ "thread", "initial_post" }`（`201`） |
| `POST /api/v1/forum/posts` | 同上 | `{ "thread_id", "body", "author_name", "reply_to_number" }`。`{ "post" }`（`201`） |
| `POST /api/v1/forum/threads/{id}/pin` | `editor` | `{ "pinned": true }` |
| `POST /api/v1/forum/threads/{id}/lock` | `editor` | `{ "locked": true }` |
| `DELETE /api/v1/forum/threads/{id}` | `editor` | スレッドとレスの削除 |
| `DELETE /api/v1/forum/posts/{id}` | `editor` | レスの削除（論理削除。レス番号は残る） |

ページ送りはカーソル方式です。次のページは、前の応答の最後のスレッドの `last_post_at` と `id`（レスは `post_number`）を渡して取ります。

| ステータス | 条件 |
| --- | --- |
| `400` | タイトル・本文の長さが範囲外など |
| `403` | ロックされたスレッドへの投稿、ログインが要る（`posting = "members"`）、そのカテゴリに書ける立場ではない |
| `404` | カテゴリ・スレッドが無い（見られない公開範囲のカテゴリも「無い」）、掲示板が無効 |
| `429` | `rate_limit_secs` 以内の連投 |

### 4.0 会員とスタッフの投稿、公開範囲

投稿は、次のどれかの立場で行います。立場は、リクエストに付けたセッションから**サーバーが決めます**（本文の値は読みません）。

| 立場 | 認証 | 名前 | 記録される種類（`author_kind`） |
| --- | --- | --- | --- |
| スタッフ | CMS のセッション（`Authorization: Bearer` / `X-Session-Id`） | `author_name` を指定できる（なければ表示名） | `staff` |
| 会員 | 会員のセッション（`Authorization: Bearer mses_...`、またはブラウザーの Cookie） | **公開名**（`nickname`）。指定は無視する | `member` |
| 匿名 | なし | `author_name`（なければ `Anonymous`） | なし |

- 会員に**公開名が無いと投稿できません**（`400` `{"code": "nickname_required"}`。本名は出さない）。マイページで決めます（[会員アカウント 13](members.md#13-公開名-nickname実装済み)）。
- 承認制の「承認済みの投稿がある人は自動承認」は、`(author_kind, author_id)` の組で見ます。同じ ID のスタッフと会員は別の人です。
- 連投の制限（`rate_limit_secs`）も、種類と ID の組で数えます。

**`posting`**: `"anyone"`（既定）はだれでも、`"members"` は会員とスタッフだけが投稿できます。`"members"` は `[members].enabled = true` が要り、無ければ起動しません。前の `allow_anonymous` は廃止しました（残っていると、起動時にエラーです）。

**カテゴリの公開範囲 `visibility`**（カテゴリの作成・更新で指定。管理画面の「カテゴリ」でも選べる）:

| 値 | 読める人 | 書ける人 |
| --- | --- | --- |
| `public`（既定） | だれでも | `posting` に従う |
| `members` | 会員とスタッフ | 会員とスタッフ |
| `staff` | スタッフ（CMS にログインしている人） | スタッフ |

- 見られない人には、そのカテゴリもスレッドも**「無い」ものとして扱います**（`404`。あることも教えない）。一覧にも出ません。
- 会員限定・スタッフ限定のカテゴリのページは、見る人ごとに変わるので、`Cache-Control: private, no-store` で返し、共有のキャッシュには載せません。会員やスタッフが見るトップ（`/forum`）も、そのカテゴリが入るときは同じです。**公開のカテゴリのページは、今までどおりキャッシュされます**（ログインしているかどうかで変わりません）。
- 前の `is_private`（真偽値）は、この `visibility` に置き換わりました。古いバックアップを復元すると、非公開だったカテゴリは `staff` になります。
- 公開の API は、レスの投稿者の IP と User-Agent を返しません（運営者だけが見られます）。

**フォームの投稿（`POST /forum/...`）と CSRF**: 会員は Cookie で認証するので、他のサイトに仕込まれたフォームから送られないよう、**`Origin`（なければ `Referer`）が、このサイトのホストと同じことを確かめます**（`Host`、プロキシ越しなら `X-Forwarded-Host`、`[members].public_url` のホスト）。違う・どちらも無いときは `403` です。掲示板のページは共有のキャッシュに載るため、フォームにトークンを埋める方式は使えません。JSON の API は `Content-Type: application/json` だけを受けるので、他のサイトからは送れません。

**組み込みのテンプレート**は、フォームに「会員の方は、ログインすると公開名で投稿できます」とログインのリンクを出します（`theme_vars.forum_members` / `forum_members_only` / `forum_login_url`）。自前のテンプレートでは、必要なら同じ変数でリンクを足してください。

**退会**: 会員が退会すると、掲示板の投稿は**残して匿名にします**（会員との紐づき・名前・IP・User-Agent を消す）。画面には「退会したユーザー」（`forum.former_user`）と出ます。

### 4.1 CAPTCHA（任意）

共通の `[captcha]` の `targets` に `forum` を入れると、スレッド作成とレスの投稿にかかります（[設定リファレンス 2.24](configuration.md#224-captcha--共通の-captcha)）。通報にはかけません。

- **ログインしていない投稿者だけ**が対象です。`Authorization: Bearer` などでセッションを付けた API の呼び出しは検証しません。ブラウザーの投稿フォーム（`POST /forum/...`）はセッションを送らないため、常に対象です。
- 組み込みのテンプレートは自動で対応します。自前のテンプレートでは、新規スレッドとレスのフォームの送信ボタンの前に `{{ fields.captcha_html | safe }}` を置いてください（掲示板にかけていなければ空です）。ページはキャッシュされるので、ウィジェットはログイン中の人にも出ます。
- JSON の API は、本文に `captcha_token` を足します。通らなければ `400 {"code": "captcha_failed"}`、検証サービスに届かなければ `503 {"code": "captcha_unavailable"}` です。

監査ログには `forum.category.create` / `update` / `delete`、`forum.thread.create` / `pin` / `lock` / `delete`、`forum.post.create` / `delete` を記録します。

---

## 5. セキュリティと制限事項

- 連投の制限は `[cache]` で数えます。リバースプロキシ配下では `[audit].trust_forwarded_for = true` にしないと全員が同じ IP として数えられます。
- 承認制・NG ワード・通知はありません（コメント機能の審査の仕組みとはまだ共有していません）。
