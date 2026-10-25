# コメント

公開サイトの記事・固定ページ・商品ページに、読者がコメントを書けるようにします（`[comments]`）。投稿は WordPress と同じ考え方で審査し（承認制・承認済みの投稿者の自動承認・NG ワード・リンク数）、Admin Studio の **Comments** タブで承認・スパム判定・返信を行います。コメントが付いたこと・承認待ちがあること・返信があったことはメールで知らせます（[通知](notifications.md)）。

```text
 読者（公開ページのフォーム）
        │  POST /api/v1/contents/{type}/{slug}/comments
        ▼
 検査: 必須項目・長さ → ハニーポット（ボットは保存しない）→ IP ごとの投稿数
        │
 最初の状態: NG ワード・リンク数 → 承認制（承認済みの投稿者は自動承認）→ check-comment プラグイン
        │
        ├── approved ──▶ 公開ページに表示。著者に comment.posted、返信先の投稿者に comment.replied
        ├── pending ───▶ 著者と運営者に comment.pending → Admin Studio で承認 → comment.replied
        └── spam ──────▶ 誰にも知らせない（投稿者には「承認待ち」と見せる）
```

> [!NOTE]
> 商品レビュー（★の評価・購入者だけが書けるレビュー）はコメントとは別の機能として、顧客アカウントと一緒に作る予定です。審査の仕組み（承認制・スパム対策・通知）はコメントと共有します（[architecture.md 13](architecture.md#13-コメント-aegis-comments)）。

---

## 1. 有効にする

```toml
[comments]
enabled = true
content_types = ["article"]   # コメントを受け付けるコンテンツの種類（商品なら "product" も）
max_depth = 3                 # 返信の深さ（0 で返信なし）
close_after_days = 0          # 公開からこの日数でコメントを閉じる（0 で閉じない）

[comments.moderation]
require_approval = true       # 承認してから公開する（Admin Studio から切り替えられる）
auto_approve_known = true     # 承認済みのコメントがある人は承認を待たずに公開する（同上）
max_links = 2                 # リンクがこれより多いと承認待ち
hold_words = []               # 含むと承認待ちにする語
spam_words = []               # 含むとスパムにする語
max_per_ip_per_hour = 10      # 同じ IP からの 1 時間あたりの投稿数
```

- 設定項目の一覧は [設定リファレンス](configuration.md#215-comments--コメント) を参照してください。
- `content_types` のスキーマが無いとサーバーは起動しません。
- 通知を送るには `[notifications]` も有効にします。返信通知の停止リンクと本文中の URL は `[commerce].public_url` から作ります（無ければリンクを入れません）。

---

## 2. 承認とスパム対策

### 2.1 最初の状態の決め方

投稿は次の順で状態が決まり、後の段は前の段より厳しくだけできます。

| 順 | 条件 | 状態 |
| --- | --- | --- |
| 1 | 名前・メールアドレス・本文に `spam_words` の語がある | `spam` |
| 2 | `hold_words` の語がある、またはリンク（`http://` / `https://` / `www.`）が `max_links` より多い | `pending` |
| 3 | 承認制（`require_approval`）で、同じ名前とメールアドレスの承認済みコメントが無い（または `auto_approve_known` がオフ） | `pending` |
| 4 | それ以外 | `approved` |
| 5 | `check-comment` プラグインが `hold` / `spam` を返した（[6](#6-プラグインで審査する)）。プラグインが失敗したら `pending` | `pending` / `spam` |

- 語の判定は大文字・小文字を区別しない部分一致です。
- スパムとハニーポットに掛かった投稿にも、投稿者には「承認待ち」と同じ応答を返します（判定を悟らせないため）。
- 状態は `pending`（承認待ち）/ `approved`（公開）/ `spam` / `trash`（ゴミ箱）の 4 つです。公開ページに出るのは `approved` だけです。

### 2.2 承認制の切り替え

**Comments** タブ上部の「承認の設定」で、承認制（`require_approval`）と承認済みの投稿者の自動承認（`auto_approve_known`）を切り替えられます（`admin` のみ。`editor` は見るだけ）。値は DB（`_aegis_settings` の `comments.require_approval` / `comments.auto_approve_known`）に保存され、再起動せずに次の投稿から効きます。投稿のたびに DB から読むので、複数のサーバーで動かしていてもすべてに効きます。「設定ファイルの値に戻す」で設定ファイルの値に戻ります。

### 2.3 ボット・連投への対策

- **ハニーポット**: フォームには人には見えない欄（`website`）があり、値が入っていたらボットとして保存も通知もしません。
- **IP ごとの投稿数**: `max_per_ip_per_hour` を超えると `429` です（`[cache]` で数える）。リバースプロキシの配下では `[audit].trust_forwarded_for = true` にしないと全員が同じ IP として数えられます。
- **プラグイン**: 外部のスパム判定サービス（Akismet など）は `check-comment` フックで使えます。

---

## 3. 公開サイト

`content_types` の種類の公開ページ（`article.html` / `page.html` / `product.html`）には、テンプレート変数 `comments` が渡ります。組み込みテンプレートと同梱のテーマは、本文の後に組み込みのコメント欄 `comments.html` を読み込みます。

```jinja
{% if comments %}{% include "comments.html" %}{% endif %}
```

| 変数 | 内容 |
| --- | --- |
| `comments.count` | 承認済みのコメント数 |
| `comments["items"]` | 承認済みのコメント（スレッドの順。返信は親の直後） |
| `comments.open` | 新しいコメントを受け付けているか（`close_after_days`） |
| `comments.post_url` | 投稿先（`POST`、ベースパス込み） |
| `comments.content_id` | コンテンツの ID |
| `comments.max_depth` / `comments.max_length` | 返信できる深さ / 本文の最大文字数 |

`items` の各要素は `id` / `parent_id` / `author_name` / `body_html`（エスケープ済みの本文。段落と改行だけ）/ `created_at` / `depth`（0 から。`max_depth` まで）/ `staff`（Admin Studio からの返信）です。メールアドレス・IP アドレスはテンプレートに渡りません。`items` は Jinja の辞書のメソッド名と重なるため `comments["items"]` で参照します。

- 組み込みのコメント欄は、フォームの送信をスクリプトで行います（`Content-Type: application/json`）。承認済みになったらそのコメントの位置（`#comment-<id>`）で再読み込みし、承認待ちなら案内を表示します。名前とメールアドレスはブラウザ（`localStorage`）に覚えます。
- テーマで見た目を変える場合は、テーマの `templates/comments.html` に同名のテンプレートを置きます（[テーマの作り方](theme_authoring_guide.md)）。独自のフォームから送る場合は [5.1](#51-公開ログイン不要) の API を使います。
- 返信通知のメールの停止リンクは `/comments/unsubscribe?id=...&token=...` で、テーマの `page.html` で「通知を停止しました」と表示します。

---

## 4. Admin Studio

**Comments** タブ（`editor` 以上。`[comments].enabled = false` なら出ません）で次の操作ができます。通知メールのリンク（Admin Studio の URL + `#comments`）からもこのタブが開きます。

- 状態ごとの一覧（承認待ち・承認済み・スパム・ゴミ箱・すべて。件数付き）。投稿者の名前・メールアドレス・IP アドレス、本文、コメントが付いたコンテンツ（公開ページへのリンク）。
- 承認・承認の取り消し（承認待ちに戻す）・スパム・ゴミ箱・完全に削除。チェックボックスでまとめて操作できます（最大 200 件）。
- 返信: Admin Studio のユーザーの表示名で、承認済みとしてすぐに公開されます（公開ページに「Staff」の印）。承認待ちのコメントに返信すると、そのコメントも承認されます。
- 承認の設定（[2.2](#22-承認制の切り替え)）。

コンテンツを削除すると、そのコメントも削除されます。

---

## 5. API

### 5.1 公開（ログイン不要）

| メソッド・パス | 内容 |
| --- | --- |
| `GET /api/v1/contents/{type}/{slug または id}/comments` | 承認済みのコメント（テンプレートの `comments` と同じ形） |
| `POST /api/v1/contents/{type}/{slug または id}/comments` | 投稿。本文は `name` / `email` / `body`（必須）、`parent_id`（返信先）、`notify_replies`（返信をメールで知らせる）、`website`（ハニーポット。空にする） |
| `GET /comments/unsubscribe?id=...&token=...` | 返信通知を止める（通知メールのリンク。HTML） |

投稿の応答は、公開されたら `201` と `{"status": "approved", "comment": {...}}`、承認待ち（スパム・ハニーポットを含む）なら `202` と `{"status": "pending"}` です。エラーは `400`（入力の不備・返信先が無い・深すぎる）、`403`（コメントを閉じた）、`404`（公開中でない・コメントを受け付けない種類）、`422`（プラグインが断った。`error` は投稿者に見せる文）、`429`（投稿が多すぎる）です。

### 5.2 管理

| メソッド・パス | ロール | 内容 |
| --- | --- | --- |
| `GET /api/v1/comments` | `editor` | 一覧（新しい順）。`status` / `content_type` / `content_id` / `limit`（既定 50、最大 200）/ `offset`。応答は `comments`・`total`・状態ごとの件数 `counts` |
| `PATCH /api/v1/comments/{id}` | `editor` | 状態を変える（`{"status": "approved"}` など） |
| `DELETE /api/v1/comments/{id}` | `editor` | 完全に削除する |
| `POST /api/v1/comments/bulk` | `editor` | まとめて操作する（`{"ids": [...], "action": "approved" \| "pending" \| "spam" \| "trash" \| "delete"}`）。応答は `{"count": 変えた件数}` |
| `POST /api/v1/comments/{id}/replies` | `editor` | 返信する（`{"body": "..."}`） |
| `GET /api/v1/comments/settings` | `editor` | 承認の設定（`policy`: 実際の値、`overrides`: Admin Studio で変えた値、`defaults`: 設定ファイルの値） |
| `PUT /api/v1/comments/settings` | `admin` | 承認の設定を変える（`{"require_approval": false, "auto_approve_known": null}`。`null` は設定ファイルの値に戻す） |

監査ログには `comment.create`（Admin Studio からの返信）/ `comment.moderate` / `comment.delete` / `comment.settings_update` が残ります（[監査ログ](audit.md)）。

---

## 6. プラグインで審査する

`check-comment` フックを宣言したプラグインは、公開サイトからの投稿を保存する前に受け取り、`allow`（そのまま）/ `hold`（承認待ち）/ `spam` を返せます。エラー（Rust SDK の `PluginError::Rejected`）を返すと投稿を断り、メッセージを投稿者に見せます。詳細は [フック一覧](plugin_hooks.md#19-check-comment) を参照してください。

```rust
fn check_comment(&mut self, comment: CommentSubmission) -> Result<CommentVerdict> {
    if comment.body.contains("casino") {
        return Ok(CommentVerdict::Spam { reason: Some("casino".into()) });
    }
    Ok(CommentVerdict::Allow)
}
```

---

## 7. セキュリティと制限事項

- **本文はプレーンテキスト**: HTML・Markdown は解釈せず、すべてエスケープして段落と改行だけにします。リンクも自動ではリンクにしません（スパムの動機を減らすため）。
- **個人情報**: メールアドレス・IP アドレス・ブラウザ（User-Agent）は Admin Studio（`editor` 以上）と、著者・運営者への承認待ちの通知にだけ出ます。返信通知（読者宛て）には返信した人のメールアドレスを入れません。
- **返信通知の停止リンク**: コメントごとの推測できないトークンで、停止ページは `Referrer-Policy: no-referrer` です。
- 公開ページに出すのは 1 つのコンテンツにつき古い順に 1,000 件までです。
- Admin Studio からコメントの本文は編集できません。アバター画像（Gravatar など）はありません。
