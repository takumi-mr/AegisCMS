[← ドキュメント一覧に戻る](README.md)

# ニュースレター

読者がメールアドレスを登録し、新しい記事のお知らせをメールで受け取れるようにします（`[newsletter]`）。申し込むと確認のメールが届き、**そのリンクを開いて初めて**購読が始まります（ダブルオプトイン）。メールにはそれぞれ配信停止のリンクが付きます。管理者は Admin Studio の **Settings › Newsletter** で、公開中の記事を購読者全員に送れます。

機能はクレート `aegis-newsletter` にあります。メールは `[notifications]` の送信待ち（[通知](notifications.md)）で送るので、**SMTP の設定が先に要ります**。

```text
 読者 ── /newsletter のフォーム（または POST /api/v1/newsletter/subscribe）
        │  メールアドレス
        ▼
 ボット除けの見えない欄／CAPTCHA（任意）／IP ごとの申し込み数 ── 断る・成功と同じに見せて捨てる
        │
 新しいアドレス・解除したアドレス ──▶ 確認待ち（pending）で保存 → 確認メール newsletter.confirm
 登録済み・確認待ち（1 分以内）   ──▶ 何も送らない（どの状態かは応答から分からない）
        │  メールのリンク /newsletter/confirm?token=…（開くとボタン。押すと確定）
        ▼
 購読中（active） ◀── 管理者が記事を選んで送る ── 購読者 1 人ずつに newsletter.issue（解除のリンク付き）
        │  メールのリンク /newsletter/unsubscribe?token=…（開くとボタン。押すと停止）
        ▼
 停止（unsubscribed）
```

---

## 1. 有効にする

```toml
[commerce]
public_url = "https://example.com"   # メールのリンクに使う公開サイトの URL（必須）

[notifications]
enabled = true                        # 確認メールと配信をメールで送る（SMTP の設定は通知のドキュメント）
channels = ["email"]

[newsletter]
enabled = true
locale = "ja"                         # 申し込み・確認・停止のページの言語（en / ja）
```

- `[newsletter].enabled = true` で `/newsletter`・API・Admin Studio の画面が有効になります。`[commerce].public_url` が空だと、メールのリンクが作れないので**サーバーが起動しません**。`[notifications]` が無効のときは、起動はしますが、確認メールも配信も送れません（警告をログに出します）。
- メールの文面（件名・本文）は通知のテンプレート `newsletter.confirm` / `newsletter.issue` です。Admin Studio の **Settings › Notifications** で編集・テスト送信できます（[通知 6](notifications.md#6-admin-studio-から管理する)）。
- 申し込みページ `/newsletter`、確認 `/newsletter/confirm`、停止 `/newsletter/unsubscribe` のパスは変えられません。`/newsletter` は `admin_path` にも使えません。

| キー | 型 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `enabled` | bool | `false` | `true` で有効にする |
| `max_per_ip_per_hour` | integer | `10` | 同じ IP からの 1 時間あたりの申し込みの数（0 で無制限。`[cache]` で数える） |
| `confirm_ttl_hours` | integer | `48` | 確認リンクの有効期限（時間。1〜336）。過ぎたら同じアドレスでもう一度申し込む |
| `locale` | string | `"en"` | 申し込み・確認・停止のページの文言の言語。`en` / `ja` |
| `captcha` | table | なし | `[newsletter.captcha]`。書くと申し込みのたびに CAPTCHA を検証する（[2](#2-captcha任意)） |

## 2. CAPTCHA（任意）

申し込みフォームがボットに狙われるときだけ、CAPTCHA を足せます。書き方・fail-closed の既定・CSP の自動追加は問い合わせフォームと同じです（[問い合わせフォーム 1.2](contact.md#12-captcha任意)）。

```toml
[newsletter.captcha]
provider = "turnstile"
site_key = "0x4AAAA..."
secret = "env:TURNSTILE_SECRET"
```

JSON の API にも同じ検査がかかり、トークンの欄（`cf-turnstile-response` など）が無いと `400 captcha_failed` になります。

## 3. 申し込みのページ

`/newsletter` は、テーマの `page.html`（本文に説明とフォームを入れて描きます）で表示されます。見た目は `page.html` を変えるか、CSS で `.aegis-newsletter-form`・`.aegis-newsletter-submit`・`.aegis-newsletter-notice`・`.aegis-newsletter-sent` を指定します（最低限のスタイルは詳細度 0 で付くので、テーマの CSS が優先されます）。メニューやフッターからリンクしてください。

自分でフォームを置くときは、次の形で `POST /newsletter/subscribe` に送ります。**見えない欄 `website` は必ず入れ、人に見えないように隠してください**（ボットが埋めると、成功と同じ応答で捨てます）。

```html
<form method="post" action="/newsletter/subscribe">
  <div style="position:absolute;left:-10000px" aria-hidden="true">
    <input type="text" name="website" tabindex="-1" autocomplete="off">
  </div>
  <label>Email <input type="email" name="email" required></label>
  <button type="submit">Subscribe</button>
</form>
```

受け付けると `/newsletter?sent=1`（「確認のメールを送りました」）へ移ります。`[server].base_path` があるときは、`action` にも付けてください。

### 3.1 確認と停止のリンク

メールのリンクを開くと、**ボタンのあるページ**が出ます。ボタンを押すと（`POST`）実行します。メールを自動で開くセキュリティソフトやプレビューに、購読の確定や停止を勝手に実行させないためです。

- 確認リンクの有効期限は `confirm_ttl_hours`。期限切れ・使用済み・存在しないリンクは、どれも同じ「無効か期限切れ」のページ（404）です。
- 停止すると、同じ購読者の古い確認リンクは使えません。停止した人がもう一度申し込むと、確認からやり直し（新しいリンク）になります。
- 確認待ちのまま 7 日たった申し込みは、1 日に 1 回の掃除で消えます。

## 4. 申し込みの扱い（プライバシー）

- **登録済みかどうかは分かりません**: 申し込みの応答は、新しいアドレス・登録済み・確認待ち・停止済みのどれでも同じです。登録済みのアドレスには何も送りません（他人のアドレスを入力して、その人に迷惑メールを送らせることを難しくします）。
- 確認待ちのアドレスには、前の確認メールから **1 分以上**たってからでないと、確認メールを送り直しません。IP ごとの申し込みの数（`max_per_ip_per_hour`）も効きます。
- 保存するのは、メールアドレス・状態・確認と停止のリンクのトークン・申し込み時の IP アドレス・日時です。IP アドレスは Admin Studio の一覧にだけ出ます。トークンは API にも画面にも出しません。
- 購読者はバックアップ（`database/newsletter_subscribers.json`・`newsletter_issues.json`）に含まれます。個人情報なので、バックアップの扱いに注意してください。

## 5. Admin Studio（Settings › Newsletter、`admin`）

- **状態ごとの人数**（購読中・確認待ち・停止）。
- **コンテンツを送る**: 公開中の記事を選び、要約（空なら記事の `excerpt` フィールド、無ければ最初の段落）を確かめ、**先に自分へテスト送信**して、購読者全員に送ります。送った記録（履歴）が残り、同じ記事をもう一度送ろうとすると、確認してから送り直せます。
  - メールにはタイトル・要約・記事の URL・購読者ごとの停止リンクが入ります。
  - 送るのは**今公開されている版**です（保存しただけで公開していない変更は含まれません）。
  - 送信は通知の送信待ちに入り、サーバーを再起動しても失われません。購読者の数だけメールを送るので、SMTP サーバーの 1 日の送信上限を確かめてください。
- **購読者**: 一覧（登録の新しい順・状態で絞り込み・メールアドレスで検索）、削除（記録ごと消す）、**CSV の書き出し**（配信する人のメールアドレス）。
- **送信履歴**: 送った記事と宛先の数。

## 6. API

| API | 権限 | 説明 |
| --- | --- | --- |
| `POST /api/v1/newsletter/subscribe` | なし | `{"email": "...", "website": ""}`。受け付けたら `202 {"status": "pending"}`（登録済みでも同じ）。誤りは `400 invalid_email`、`429 rate_limited`、CAPTCHA は `400 captcha_failed` / `503 captcha_unavailable` |
| `GET /api/v1/newsletter/subscribers?status=&q=&limit=&offset=` | admin | 購読者（登録の新しい順）と総数、状態ごとの人数 `counts`。トークンは含めない |
| `POST /api/v1/newsletter/subscribers/delete` | admin | `{"ids": [...]}`（1〜500 件）。`{"count": n}` |
| `GET /api/v1/newsletter/subscribers/export` | admin | 配信する人のメールアドレスの CSV（`text/csv`、ダウンロード） |
| `GET /api/v1/newsletter/issues?limit=&offset=` | admin | 送信履歴（新しい順）と総数 |
| `POST /api/v1/newsletter/issues` | admin | `{"content_type": "article", "slug_or_id": "...", "summary": "", "test_to": "", "force": false}`。`test_to` を付けるとそのアドレスにだけ送る（`202`、履歴に残さない）。購読者全員に送ると `201`（履歴の 1 件）。送信済みは `409 already_sent`（`force: true` で送り直す）、購読者がいないと `409 no_subscribers`、公開していないと `400 content_not_published` |

エラーは英語で、機械が読む `code` を付けます。配信と削除は監査ログ（`newsletter.send` / `newsletter.delete`）に残ります。

## 7. 動作の決まり

- **同じ記事を同時に 2 回送りません**（ボタンの二度押し・複数の管理者）。ほかで送っている間は `503 unavailable` です。
- 配信は購読者を 500 人ずつ読み込みながら通知に渡すので、購読者が多くてもメモリを使い切りません。途中でサーバーが止まった場合、通知の送信待ちに入った分は再起動後に送られますが、まだ渡していない購読者には届きません（履歴は最後に残すので、その場合は `force` で送り直せます。届いた人には重複して届きます）。
- 解除の URL はメールの本文だけに出ます。`List-Unsubscribe` ヘッダー（ワンクリック解除）は付けません（メールのチャネルがヘッダーの追加に対応していないため）。
