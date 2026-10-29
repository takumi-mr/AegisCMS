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
allow_anonymous = true      # ログインしていない人も投稿できる
rate_limit_secs = 5         # 同じ IP・ユーザーの投稿の間隔
auto_lock_post_count = 1000 # この数のレスでスレッドを自動でロック（0 で無制限）
```

設定項目の一覧は [設定リファレンス](configuration.md#216-forum--掲示板) を参照してください。

- 表は `_aegis_forum_categories` / `_aegis_forum_threads` / `_aegis_forum_posts`。サーバーの起動時と `aegis migrate` で作成します（`database_url` を指定した場合はその DB に作ります）。
- **別の DB に分ける場合**: 投稿が集中したときの書き込みのロック（SQLite の `SQLITE_BUSY`）や、PostgreSQL / MySQL の接続数が本体（記事・EC・セッション）に影響しないようにできます。SQLite・PostgreSQL・MySQL のどれでも指定できます。

---

## 2. 公開サイト

| URL | テンプレート | 内容 |
| --- | --- | --- |
| `/forum` | `forum_index.html` | カテゴリの一覧と、最近更新されたスレッド |
| `/forum/{カテゴリの slug}`（`?page=N`） | `forum_category.html` | カテゴリのスレッドの一覧（ピン留め → 最終投稿の新しい順）とスレッドの作成フォーム |
| `/forum/{カテゴリの slug}/{スレッド ID}` | `forum_thread.html` | レスの一覧（レス番号付き）と返信フォーム |

- フォームは `POST /forum/{slug}/threads`・`POST /forum/{slug}/{スレッド ID}/posts` に送ります。成功すると `303` で元のページ（レスは `#post-N`）に戻り、結果のメッセージを表示します。
- ログイン中は表示名をアカウントから取ります。未ログインで名前を空にすると `Anonymous`。
- テーマに同じ名前のテンプレートを置くと組み込みのものの代わりに使います（共通部分は `forum_common.html`）。

---

## 3. Admin Studio

**Forum** タブ（`[forum].enabled = true` のときに表示）:

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
| `POST /api/v1/forum/threads` | 不要（`allow_anonymous = false` ならログイン） | `{ "category_id", "title", "body", "author_name" }`。`{ "thread", "initial_post" }`（`201`） |
| `POST /api/v1/forum/posts` | 同上 | `{ "thread_id", "body", "author_name", "reply_to_number" }`。`{ "post" }`（`201`） |
| `POST /api/v1/forum/threads/{id}/pin` | `editor` | `{ "pinned": true }` |
| `POST /api/v1/forum/threads/{id}/lock` | `editor` | `{ "locked": true }` |
| `DELETE /api/v1/forum/threads/{id}` | `editor` | スレッドとレスの削除 |
| `DELETE /api/v1/forum/posts/{id}` | `editor` | レスの削除（論理削除。レス番号は残る） |

ページ送りはカーソル方式です。次のページは、前の応答の最後のスレッドの `last_post_at` と `id`（レスは `post_number`）を渡して取ります。

| ステータス | 条件 |
| --- | --- |
| `400` | タイトル・本文の長さが範囲外など |
| `403` | ロックされたスレッドへの投稿、匿名投稿が許可されていない |
| `404` | カテゴリ・スレッドが無い、掲示板が無効 |
| `429` | `rate_limit_secs` 以内の連投 |

監査ログには `forum.category.create` / `update` / `delete`、`forum.thread.create` / `pin` / `lock` / `delete`、`forum.post.create` / `delete` を記録します。

---

## 5. セキュリティと制限事項

- 連投の制限は `[cache]` で数えます。リバースプロキシ配下では `[audit].trust_forwarded_for = true` にしないと全員が同じ IP として数えられます。
- 承認制・NG ワード・通知はありません（コメント機能の審査の仕組みとはまだ共有していません）。
