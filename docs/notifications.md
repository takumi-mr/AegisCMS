# 通知（メール）

注文の受付・入金確認・発送・取り消しを顧客と運営者に、管理者のログインやログインのロックなどの監査イベントを運営者に、メールで知らせます（`[notifications]`）。メールは外部の SMTP サーバー（自社のメールサーバー、Amazon SES、SendGrid、Google Workspace など）から送ります。

配信方法は「チャネル」として差し替えられる作りで、メール（`email`）とサーバーのログ（`log`）が組み込みです。Slack や SMS などは、Wasm プラグインをチャネルとして加えられます（`plugin:<名前>`。[5. 配信方法を加える](#5-配信方法を加える)）。

```text
 EC（注文の作成・状態の変化）          監査ログ（ルールに合うイベント。2.1）
        │  NotificationEvent { 種類: "order.placed" / "audit.session.create", 宛先, 値 }
        ▼
 NotificationService（aegis-notify）
        │  宛先ごとにテンプレートから件名・本文を作る（<種類>.subject / <種類>.txt）
        ├──▶ email チャネル ── SMTP サーバー ──▶ 顧客 / staff_recipients
        ├──▶ log チャネル   ── サーバーのログ（aegis::notify）
        └──▶ plugin:<名前>  ── Wasm プラグインの send-notification フック ──▶ Slack / SMS など
        │
        └── 結果を監査ログに記録（notification.send）。一時的な失敗は再試行
```

---

## 1. 有効にする

```toml
[notifications]
enabled = true
channels = ["email"]
locale = "ja"                    # 組み込みテンプレートの言語（en / ja）

[notifications.email]
smtp_host = "smtp.example.com"
smtp_port = 587                  # 省略（0）なら tls に応じて 587 / 465 / 25
tls = "starttls"                 # starttls / tls / none
username = "apikey"
# password は設定ファイルに書かず、環境変数 AEGIS_SMTP_PASSWORD で渡せる
from = "Aegis Shop <shop@example.com>"
reply_to = "support@example.com"
staff_recipients = ["orders@example.com"]   # 新しい注文・取り消しの知らせ先
```

設定したら、テスト通知で SMTP サーバーへの接続と認証を確かめます。

```bash
aegis-server -c config/production.toml notify test --to you@example.com
# email: sent "【Aegis Shop】テスト通知"
```

失敗した場合は SMTP サーバーの応答（`535 Authentication failed` など）が表示されます。

- 設定項目の一覧は [設定リファレンス](configuration.md#212-notifications--通知メール) を参照してください。
- SMTP サーバーの無い開発環境では `channels = ["log"]` にすると、通知の件名と本文をサーバーのログに出します。
- 本文中の URL（`site_url` / 運営者向けの管理画面の URL）は `[commerce].public_url` から作ります。注文確認ページの URL は注文したときのリクエストのホストから作ります。
- 差出人のドメインの SPF / DKIM / DMARC は SMTP サーバー側で設定してください（設定が無いと迷惑メールに分類されやすくなります）。

---

## 2. 送る通知

| 種類 | いつ | 宛先 | 主な内容 |
| --- | --- | --- | --- |
| `order.placed` | 注文の受付。`manual`（銀行振込など）・0 円の注文は注文したとき、`stripe` は支払いが済んだとき | 顧客・運営者 | 注文内容、お届け先、支払い方法の案内（支払い待ちのとき）、注文確認ページの URL（注文したときのみ） |
| `order.paid` | 管理者が入金を確認した（`manual`） | 顧客 | 入金の確認 |
| `order.shipped` | 発送した | 顧客 | 発送の知らせ。状態を変えるときのメモ（配送業者・追跡番号など）を本文に入れる |
| `order.cancelled` | 取り消した（管理者の操作・支払い期限切れ） | 顧客・運営者 | 取り消しの理由（`reason`）、支払い済みなら返金の案内 |
| `test` | `aegis-server notify test` | 指定したアドレス | テスト |
| `audit.<操作>` | 監査ログのイベントが `[[notifications.audit.rules]]` に合った（[2.1](#21-監査イベントの通知)） | 運営者（またはルールの `recipients`） | 操作・結果・ユーザー・IP アドレス・ブラウザ・詳細 |

- `stripe` で決済ページに進んだまま支払われなかった注文（期限切れ・失敗で取り消し）は、受付を知らせていないので取り消しも知らせません。
- `completed`（完了）は知らせません。
- `[notifications].disabled_events = ["order.shipped"]` のように、種類ごとに止められます。
- 注文確認ページのトークンはサーバーに保存していない（ハッシュのみ）ため、URL を本文に入れられるのは注文したときの `order.placed` だけです。

### 2.1 監査イベントの通知

監査ログ（[docs/audit.md](audit.md)）に記録されるイベントのうち、`[[notifications.audit.rules]]` のどれかに合うものを運営者に知らせます。ルールは上から順に見て、最初に合ったもので送ります。`[audit].enabled = true` が必要です（出力先 `sinks` の設定は問いません）。

```toml
# 管理者のログイン
[[notifications.audit.rules]]
title = "管理者がログインしました"   # 件名・本文の見出し（省略すると操作と結果から作る）
action = "session.create"
outcomes = ["success"]
roles = ["admin"]

# ログインのロック（失敗が [auth.login_rate_limit] の上限に達した）
[[notifications.audit.rules]]
action = "session.create"
details = { locked = "*" }

# 権限の無い操作。宛先を変え、1 時間に 5 通まで
[[notifications.audit.rules]]
action = "access.denied"
recipients = ["security@example.com"]
max_per_hour = 5
```

| キー | 説明 |
| --- | --- |
| `action` | 監査ログの操作（`session.create`、`access.denied`、`user.update`、`theme.switch` など。必須） |
| `outcomes` | 結果（`success` / `failure` / `denied`）。省略するとすべて |
| `roles` | 操作者のロール（`admin` など）。操作者の分からないイベント（失敗したログインなど）は、指定すると合わない |
| `details` | 監査ログの `details` の値。`"*"` はキーがあれば何でもよい |
| `title` | 件名・本文の見出し |
| `recipients` | 宛先のメールアドレス。省略すると `[notifications.email].staff_recipients` |
| `max_per_hour` | 1 時間あたりの上限（既定 20、0 で無制限）。総当たり攻撃などで通知が殺到しないように、超えた分は送らずログに警告を出す |

よく使う条件:

| 知りたいこと | 条件 |
| --- | --- |
| 管理者のログイン | `action = "session.create"`, `outcomes = ["success"]`, `roles = ["admin"]` |
| ログインのロック（ロックがかかった時点の 1 回） | `action = "session.create"`, `details = { locked = "*" }` |
| ロック中のログイン試行（試行のたび） | `action = "session.create"`, `outcomes = ["denied"]`, `details = { error = "rate_limited" }` |
| パスワードの誤り | `action = "session.create"`, `outcomes = ["failure"]`, `details = { error = "invalid_password" }` |
| 権限の無い操作（401 / 403） | `action = "access.denied"` |
| ユーザーの作成・変更・削除 | `action = "user.create"` / `"user.update"` / `"user.delete"` |
| 公開サイトのテーマの切り替え | `action = "theme.switch"` |
| プラグインのエラー | `action = "plugin.error"` |

- 通知の種類は `audit.<操作>`（`audit.session.create` など）です。`disabled_events` で止められ、テンプレートは `audit.session.create.subject` → `audit.session.subject` → `audit.subject` の順に探します（組み込みは `audit` のみ）。テンプレートには監査イベント（`audit.action` / `audit.outcome` / `audit.actor` / `audit.ip` / `audit.user_agent` / `audit.details` など）と `title` を渡します。
- 通知の配信結果（`notification.send`）は通知しません（配信の失敗が通知を呼び続けないように）。
- 上限の数はサーバーのメモリ上で数えます（再起動でリセット、サーバーごと）。

---

## 3. テンプレート

件名と本文は [MiniJinja](https://docs.rs/minijinja)（Jinja2 互換）のプレーンテキストのテンプレートです。組み込みのものは `crates/aegis-notify/src/templates/<言語>/` にあります。

`[notifications].templates_dir` を指定すると、そのディレクトリにあるファイルを優先して使います（無いファイルは組み込み）。

```text
notifications/
├── order.shipped.subject         # 件名（1 行。改行は空白にまとめる）
├── order.shipped.txt             # 本文
├── order.placed.staff.subject    # 運営者向け（無ければ顧客向けと同じものを使う）
├── order.placed.staff.txt
└── _summary.txt                  # 注文内容の共通部分（{% include "_summary.txt" %}）
```

```jinja
{# notifications/order.shipped.txt #}
{{ order.customer_name }} 様

ご注文 {{ order.number }} を発送しました。
{% if order.note %}
{{ order.note }}
{% endif %}

{% include "_summary.txt" %}
{% include "_footer.txt" %}
```

テンプレートで使える値:

| 値 | 内容 |
| --- | --- |
| `site_name` | `[theme].site_title` |
| `site_url` / `admin_url` | `[commerce].public_url` と、その `/admin/`（未設定なら空） |
| `kind` | 通知の種類 |
| `order.number` / `order.status` / `order.created_at` / `order.paid_at` / `order.note` | 注文番号・状態・日時・管理者のメモ |
| `order.lines[]` | `title`, `sku`, `unit_price`, `quantity`, `line_total` |
| `order.adjustments[]` | 送料・割引・税（`label`, `amount`, `kind`, `code`） |
| `order.subtotal` / `order.total` / `order.currency` | 金額（通貨の最小単位の整数） |
| `order.customer_name` / `order.customer_email` / `order.shipping_address` | 顧客とお届け先（`postal_code`, `region`, `city`, `line1`, `line2`, `phone` など） |
| `order_url` | 注文確認ページ（`order.placed` を注文したときに送る場合のみ） |
| `payment_instructions` / `payment_due` | 支払い方法の案内（`[commerce.manual].instructions`）と期限（支払い待ちのとき） |
| `reason` / `was_paid` | 取り消しのきっかけ（`admin` / `expired` など）と、支払い済みだったか（`order.cancelled`） |
| `{{ amount \| money(order.currency) }}` | 金額の表示（`¥1,200`、`$12.50`） |

- テンプレートが無ければ種類の最後の `.` 以降を外した名前を順に探します（`audit.session.create` → `audit.session` → `audit`）。
- 本文はエスケープしません（プレーンテキスト）。
- `{% if %}` などのタグだけの行は出力に空行を残しません（`trim_blocks`）。3 行以上続く空行は 1 行にまとめます。

---

## 4. 配信と信頼性

- 通知は注文の処理を待たせないよう、バックグラウンドで送ります。SMTP サーバーが落ちていても注文は成功します。
- 一時的な失敗（接続できない、`4xx` の応答）は `max_attempts` 回まで、2 秒・8 秒・32 秒…と間隔を空けて再試行します。宛先の不備や認証の拒否（`5xx`）は再試行しません。
- 結果はチャネルごとに監査ログ `notification.send` に残します（`success` / `failure`、試行回数、エラー）。宛先のアドレスは残しません（顧客か運営者か）。Admin Studio の **Settings › Audit Logs** で `notification.send` に絞り込めます。
- 送信待ちの通知はメモリ上にあるため、配信中にサーバーを再起動すると送られずに失われます。確実に届ける必要がある場合は、SMTP サーバー（リレー）側のキューを使ってください。

---

## 5. 配信方法を加える

### 5.1 Wasm プラグインで加える

`send-notification` フックを持つプラグインは、そのまま通知のチャネルになります。Rust の本体を変えずに、Slack・Teams・Discord・LINE・SMS 業者の API などへの配信を加えられます。

```toml
[notifications]
enabled = true
channels = ["email", "plugin:official-order-notifier"]

[plugins]
enabled = ["official-order-notifier"]

[plugins.settings.official-order-notifier]
webhook_url = "env:SLACK_WEBHOOK_URL"
notify_on = "none"                # 注文の旧来の通知（on-order-status-changed）は止め、二重に届かないように
channel_recipients = "staff"      # 運営者向けの通知だけ（all で顧客向けも）
channel_kinds = "audit.,order.placed,order.cancelled"   # 種類の前方一致（空ならすべて）
```

- ホストはテンプレートから件名・本文を作って `send-notification` に渡します。テンプレート・再試行（`max_attempts`）・監査ログ `notification.send`（`channel: plugin:<名前>`）・監査イベントの上限（`max_per_hour`）はメールと同じです。
- プラグインは扱わない通知（宛先・種類）に `Skipped` を返します。`Skipped` は失敗ではなく、監査ログにも残りません。
- `channels` に書いたプラグインが `[plugins].enabled` に無い、またはマニフェストで `send-notification` を宣言していない場合、サーバーは起動しません。
- 動作確認は `aegis-server notify test --staff`（運営者向け。`--to` で顧客向けも）。
- プラグインの作り方は [プラグインフック 1.7](plugin_hooks.md#17-send-notification)、ABI は [ABI 仕様 4.7](plugin_abi_specification.md#47-send_notification) を参照してください。

### 5.2 Rust で加える

本体に組み込む場合は `aegis_notify::NotificationChannel` トレイトを実装します。

```rust
use aegis_notify::{Delivered, NotificationChannel, NotificationMessage, Recipient, SendError};
use async_trait::async_trait;

pub struct SmsChannel { /* API クライアントなど */ }

#[async_trait]
impl NotificationChannel for SmsChannel {
    fn name(&self) -> &str {
        "sms"
    }

    /// 送れる宛先か（送れないものは呼ばれない）
    fn accepts(&self, recipient: &Recipient) -> bool {
        matches!(recipient, Recipient::Staff)
    }

    async fn send(&self, message: &NotificationMessage) -> Result<Delivered, SendError> {
        // message.subject / message.body / message.data を使って送る
        // 一時的な失敗は SendError::temporary、再試行しない失敗は SendError::permanent
        Ok(Delivered::Sent)
    }
}
```

- 設定から選べるようにするには、`aegis_notify::ChannelKind` に種類を、`NotificationService::from_config_with_plugins` に作り方を加えます（設定項目は `NotificationsConfig` に）。
- コードからは `NotificationService::with_channels` にチャネルを渡しても使えます。
- テンプレート・再試行・監査ログはチャネルに関係なく `NotificationService` が行います。チャネルは届けることだけを担当します。

通知を出す側を増やす（会員登録の確認など）ときは、`aegis_core::Notifier` に `NotificationEvent`（種類・宛先・テンプレートに渡す値）を渡し、組み込みテンプレート（`crates/aegis-notify/src/templates/`）を加えます。テンプレートの無い種類は送りません。

### Wasm プラグインとの関係

公式プラグイン [`official-order-notifier`](commerce.md#official-order-notifierslack--webhook-通知) は 2 つの使い方ができます。

- `on-order-status-changed`: 注文の状態の変化を、ホストのテンプレートを通さずに Slack / Webhook に送る（従来の使い方）
- `send-notification`: 通知のチャネル（5.1）。監査イベントも含むすべての通知を、ホストのテンプレートの件名・本文で送る

チャネルとして使う場合は、注文が二重に届かないよう `notify_on = "none"` にしてください。
