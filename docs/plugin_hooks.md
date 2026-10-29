[← ドキュメント一覧に戻る](README.md)

# AegisCMS プラグインフック & ホスト API リファレンス

Wasm プラグインから利用できるフック、マニフェスト項目、ホスト API の一覧です。
プラグイン開発の手順そのものは [Wasm プラグイン開発公式ガイド](./plugin_sdk_guide.md) を参照してください。

関連ソース:

- フックのディスパッチ: [`crates/aegis-runtime/src/plugin.rs`](../crates/aegis-runtime/src/plugin.rs) (`PluginSupervisor`)
- Wasm 呼び出し: [`crates/aegis-runtime/src/wasm/instance.rs`](../crates/aegis-runtime/src/wasm/instance.rs)
- ホスト API: [`crates/aegis-runtime/src/wasm/engine.rs`](../crates/aegis-runtime/src/wasm/engine.rs)
- 権限チェック: [`crates/aegis-runtime/src/guard.rs`](../crates/aegis-runtime/src/guard.rs)
- 共通 ABI 仕様書: [`docs/plugin_abi_specification.md`](./plugin_abi_specification.md)
- Rust SDK: [`crates/aegis-plugin-sdk`](../crates/aegis-plugin-sdk)
- AssemblyScript テンプレート: [`sdk/templates/assemblyscript`](../sdk/templates/assemblyscript)
- TinyGo テンプレート: [`sdk/templates/tinygo`](../sdk/templates/tinygo)

---

## 1. フック一覧

| マニフェストのフック名 | Wasm エクスポート名 | SDK トレイトメソッド | 入力 | 出力 | 状態 |
| --- | --- | --- | --- | --- | --- |
| `transform-content-ast` | `transform_content_ast` | `Plugin::transform_content_ast` | `BlockNode[]` | `BlockNode[]` | ✅ 実装済み |
| `on-before-save` | `on_before_save` | `Plugin::on_before_save` | `PostContext` | `PostContext` | ✅ 実装済み（`title` / `slug` / 本文の変更、保存の拒否） |
| `on-user-authenticated` | `on_user_authenticated` | `Plugin::on_user_authenticated` | `UserAuthContext` | `UserAuthContext` | ✅ 実装済み（ロールマッピング、属性の補完、ログインの拒否） |
| `calculate-order` | `calculate_order` | `Plugin::calculate_order` | `OrderContext` | `OrderContext` | ✅ 実装済み（送料・税・割引の追加、注文の拒否。[EC](commerce.md)） |
| `on-order-status-changed` | `on_order_status_changed` | `Plugin::on_order_status_changed` | `OrderStatusChange` | なし | ✅ 実装済み（注文の作成・状態の変化の通知） |
| `send-notification` | `send_notification` | `Plugin::send_notification` | `NotificationMessage` | `Delivery` | ✅ 実装済み（通知の配信チャネル。Slack・SMS など。[通知](notifications.md#5-配信方法を加える)） |
| `on-content-published` | `on_content_published` | `Plugin::on_content_published` | `PublishedContent` | なし | ✅ 実装済み（予約公開などコンテンツ公開時の通知） |
| `check-comment` | `check_comment` | `Plugin::check_comment` | `CommentSubmission` | `CommentVerdict` | ✅ 実装済み（コメントの審査。スパム判定・承認待ち・投稿の拒否。[コメント](comments.md)） |
| `render-blocks` | `render_blocks` | `Plugin::render_blocks` | `BlockNode[]`（このプラグインのブロック） | `Option<String>[]`（HTML） | ✅ 実装済み（本文のカスタムブロックの描画。[1.10](#110-render-blocks)） |

- フックを有効にするには、**マニフェストの `capabilities.hooks` にフック名（ケバブケース）を列挙**する必要があります。列挙されていないフックは、Wasm 側に関数がエクスポートされていても呼び出されません。
- 逆に、マニフェストに列挙されていても該当の関数がエクスポートされていない場合は、入力がそのまま次へ渡されます（エラーにはなりません）。
- SDK の `#[aegis_plugin]` マクロ（または `register_plugin!`）はすべてのフックのエクスポートを常に生成します。実装しなかったメソッドはトレイトの既定実装（入力をそのまま返す）になります。

### 1.1 実行タイミングと順序

`transform-content-ast` / `on-before-save` が呼ばれるのは **コンテンツ作成 API `POST /api/v1/contents/{content_type}`** と、`transform-content-ast` のみ **エディタのプレビュー API `POST /api/v1/preview`** です（一覧・取得 API や公開ページの表示では呼ばれず、保存済みのブロックを手順 4・5 だけでレンダリングします）。処理順は以下のとおりです。

```
リクエストの blocks
  │
  ├─ 1. 組み込み AST パイプライン (HeadingIdTransformer: 見出しに id を付与)
  ├─ 2. transform-content-ast  … 全プラグインを順に連鎖適用
  ├─ 3. on-before-save         … 全プラグインを順に連鎖適用（title / slug / 本文の変更、保存の拒否）
  ├─ 4. HTML レンダリング (HtmlRenderer)
  ├─ 5. ストリーミング HTML リライター ([security] 設定)
  └─ 6. データベースへ保存
```

- プレビュー API は手順 1・2・4・5 を実行し（保存・`on-before-save` はなし）、保存して公開ページで表示した場合と同じ HTML を返します。テーマを適用したページ全体（`page_html`）のため、`on-render-page` フック（[テーマガイド](theme_and_publishing_guide.md)）もプレビューのたびに呼ばれます。
- 保存済みの記事を開いて再保存・プレビューすると、`transform-content-ast` は保存済み（変換済み）のブロックに再度適用されます。ブロックを追加するプラグインは、すでに追加済みなら追加しない（冪等な）実装にしてください。
- 複数プラグインがある場合、前のプラグインの出力が次のプラグインの入力になります。
- プラグインの実行順は有効化リストの順（設定ファイルの `[plugins].enabled` に書いた順。Admin Studio で有効にしたものは最後）です（[設定リファレンス](configuration.md#plugins--プラグインの有効化)、[7.1](#71-admin-studio-からの有効無効の切り替え)）。

### 1.2 `transform-content-ast`

本文のブロック AST を変換します。見出しへのアンカー付与、ショートコード展開、ブロックの追加・削除などに使います。

```rust
fn transform_content_ast(&mut self, blocks: Vec<BlockNode>) -> Result<Vec<BlockNode>>;
```

- 入力: 組み込みパイプライン適用後のブロック配列（トップレベルのみの配列。子ブロックは `children` に入る）
- 出力: 変換後のブロック配列。**これがそのまま保存・HTML レンダリングされます。**
- エラー時の挙動: いずれかのプラグインでホスト側エラー（トラップ、タイムアウト、Fuel 枯渇、不正な JSON の返却など）が発生すると、**このフック全体の結果が破棄**され、手順 1 の結果のまま保存処理が続行されます（リクエストは失敗しません）。プレビューでは同じ場合に `warnings` として返し、エディタに表示します（監査ログには記録しません）。
- **副作用を持たせないでください**: エディタのプレビューは編集のたび（入力が 300ms 止まるごと）にこのフックを呼びます。KVS への書き込みや外部通知など、保存時に一度だけ行いたい処理は `on-before-save` に置いてください。

### 1.3 `on-before-save`

保存直前の記事情報を受け取ります。バリデーション（保存の拒否）、保存値の補正、外部通知などに使います。プレビューでは呼ばれないため、保存時に一度だけ行いたい副作用はここに置きます。

```rust
fn on_before_save(&mut self, post: PostContext) -> Result<PostContext>;
```

- 入力: `PostContext`（`blocks_json` は `transform-content-ast` 適用後の内容）
- 出力: `PostContext`。次のプラグインへの入力になり、最後のプラグインの出力が保存されます。
  - **`title` / `slug` / `blocks_json` の変更は保存に反映されます**（API の応答も変更後の値。Admin Studio は保存後に保存された内容をエディタに表示します）。
  - `id` と `author_id` の変更は無視されます（警告ログのみ）。プラグインが著者を差し替えることはできません。
  - `blocks_json` を変更した場合、組み込み AST パイプライン（見出しの id 付与）を再度適用してから保存します。HTML のレンダリングとセキュリティリライターは通常どおり適用されます。`transform-content-ast` は再実行されません。
  - 空の `slug`、`BlockNode[]` としてパースできない `blocks_json` を返すとプラグインエラーとして保存を中止します（`blocks_json` を空文字列にした場合は空の本文）。
- **保存の拒否**: SDK でトレイトメソッドから `Err(...)` を返すと保存は中止され、API は `422 Unprocessable Entity` と `{"error": "Rejected by plugin: <プラグイン名>: <メッセージ>"}` を返します（Admin Studio ではエラーのトーストに表示）。利用者に見せるメッセージには `PluginError::Rejected` を使ってください（他のバリアントは `Execution error: ...` のように種別が前に付きます）。監査ログには `content.create`（`denied`）だけを記録します。

  ```rust
  fn on_before_save(&mut self, post: PostContext) -> Result<PostContext> {
      if post.title.trim().is_empty() {
          return Err(PluginError::Rejected("title is required".to_string()));
      }
      Ok(post)
  }
  ```

- **プラグインの失敗**: パニック（Wasm トラップ）、タイムアウト、Fuel 枯渇、メモリ上限超過、不正な JSON の返却などでも保存は中止され、API は `500 Internal Server Error` と `{"error": "Plugin hook failed: ..."}` を返します。監査ログには `plugin.error` と `content.create`（`denied`）を記録します。

### 1.4 `on-user-authenticated`

ログイン（`POST /api/v1/auth/session`）で認証プロバイダが資格情報を確認した**後**、セッションを発行する**前**に呼ばれます。IdP のグループからのロールマッピング、表示名・属性の補完、ドメインや時間帯によるログインの制限、ログイン通知などに使います。資格情報の検証そのもの（パスワード・IdP の署名の確認）はプラグインには開放されません。

```rust
fn on_user_authenticated(&mut self, ctx: UserAuthContext) -> Result<UserAuthContext>;
```

- 入力: `UserAuthContext`（[2.3](#23-userauthcontext)）。`role` はユーザーアカウント（ローカル認証）またはプロバイダが決めたロールです。
- 出力: `UserAuthContext`。次のプラグインへの入力になり、最後のプラグインの出力でセッションを発行します。
  - `role` / `display_name` / `email` / `metadata` の変更はセッションに反映されます。`metadata` はセッションの `metadata` に入ります（`provider` / `display_name` / `email` のキーはサーバーが設定するため上書きできません）。
  - `provider` と `user_id` の変更は無視されます（警告ログのみ）。
  - `role` は `admin` / `editor` / `viewer` のいずれかでなければならず、それ以外を返すとプラグインエラーとしてログインは失敗します。
  - ロールを変更した場合は、監査ログ `session.create`（`success`）の `details.role_mapped` に `{ "from": 元のロール, "to": 新しいロール }` を記録します。
- **ログインの拒否**: `Err(PluginError::Rejected("..."))` を返すと、API は `403 Forbidden` と `{"error": "Rejected by plugin: <プラグイン名>: <メッセージ>"}` を返し、セッションは発行されません。監査ログは `session.create`（`denied`、`details.error = "plugin_rejected"`）です。
- **プラグインの失敗**: トラップ・タイムアウト・不正な JSON・不明なロールなどでもログインは失敗します（安全側に倒す）。API は `500` を返し、監査ログに `plugin.error` と `session.create`（`failure`、`details.error = "plugin_failed"`）を記録します。
- 拒否・失敗はどちらもログイン試行の失敗回数（[レート制限](authentication.md#5-セキュリティ上の挙動)）には数えません（資格情報は正しいため）。
- ログインのたびに呼ばれます。外部への通知など時間のかかる処理は `execution_timeout_ms` に収まるようにしてください。

```rust
fn on_user_authenticated(&mut self, mut ctx: UserAuthContext) -> Result<UserAuthContext> {
    if !ctx.email.ends_with("@example.com") {
        return Err(PluginError::Rejected("only example.com accounts may sign in".to_string()));
    }
    let groups = ctx.metadata.get("groups").and_then(|g| g.as_array());
    if groups.is_some_and(|g| g.iter().any(|v| v == "cms-admins")) {
        ctx.role = "admin".to_string();
    }
    Ok(ctx)
}
```

### 1.5 `calculate-order`

EC（`[commerce]`、[EC ドキュメント](commerce.md)）のカートの見積もり（`POST /api/v1/shop/quote`）と注文の確定（`POST /api/v1/shop/checkout`）のたびに呼ばれます。送料・税・割引を `adjustments` に加えます。

```rust
fn calculate_order(&mut self, ctx: OrderContext) -> Result<OrderContext>;
```

- 入力: `OrderContext`（[2.5](#25-ordercontext--orderstatuschange)）。`lines` の単価・小計はサーバーが商品から決めた値で、`adjustments` には前のプラグインが加えたものが入っています。
- 出力: `OrderContext`。**変更が反映されるのは `adjustments` と `metadata` だけ**です。`lines`・`subtotal`・`currency`・顧客・クーポンコードを変えても無視され、`total` はホストが `subtotal + adjustments の合計` で計算し直します（プラグインが価格を書き換えることはできません）。
- プラグインが新しく加えた `adjustments` の `source` にはホストがプラグイン名を入れます。割引は負の金額です。合計が負になる注文は拒否します。
- **注文の拒否**: `Err(PluginError::Rejected("..."))` を返すと見積もり・注文は `422` になり、メッセージ（プラグイン名は除く）を顧客に表示します。無効なクーポン、配送できない地域などに使います。失敗（トラップ・タイムアウト）は `500` です。
- 見積もりのたびに呼ばれるため、`http::fetch` で外部の送料 API などを呼ぶ場合は `execution_timeout_ms` を大きくしてください。

### 1.6 `on-order-status-changed`

注文の作成（`from_status` が `None`）と、状態の変化（支払い済み・発送・完了・取り消し・期限切れ）のたびに呼ばれます。Slack・メール配信サービス・会計ソフトなどへの連携に使います。

```rust
fn on_order_status_changed(&mut self, change: OrderStatusChange) -> Result<()>;
```

- 状態の変更を保存した**後**に、応答を待たずに（別タスクで）呼ばれます。結果は注文に影響しません。
- `Err` を返す・失敗した場合は警告ログと監査ログ `plugin.error`（`resource_type: order`）に残ります。ほかのプラグインの呼び出しは続きます。

### 1.7 `send-notification`

プラグインを通知の配信チャネルにします（[通知](notifications.md)）。サーバーの設定で `[notifications].channels` に `"plugin:<プラグイン名>"` と書くと、通知（注文の受付・発送、管理者のログイン・ログインのロックなどの監査イベント）のたびに呼ばれます。件名・本文はホストがテンプレートから作ってあり、プラグインは `http::fetch` で Slack・Teams・Discord・SMS 業者の API などへ送るだけです。

```rust
fn send_notification(&mut self, message: NotificationMessage) -> Result<Delivery>;

impl Plugin for SlackPlugin {
    fn send_notification(&mut self, message: NotificationMessage) -> Result<Delivery> {
        // 運営者向けの通知だけを扱う（顧客向けはメールに任せる）
        if message.recipient != Recipient::Staff {
            return Ok(Delivery::Skipped);
        }
        let url = config::get("webhook_url")
            .ok_or_else(|| PluginError::Rejected("webhook_url is not configured".into()))?;
        let text = format!("*{}*\n{}", message.subject, message.body);
        let response = http::fetch(&http::Request::post_json(url, &serde_json::json!({ "text": text }))?)?;
        if !response.is_success() {
            return Err(PluginError::Execution(format!("Slack answered {}", response.status)));
        }
        Ok(Delivery::Sent)
    }
}
```

| 返り値 | ホストの扱い |
| --- | --- |
| `Ok(Delivery::Sent)` | 送信済み。監査ログ `notification.send`（`success`、`channel: plugin:<名前>`） |
| `Ok(Delivery::Skipped)` | このプラグインが扱わない通知（種類・宛先）。失敗ではなく、監査ログにも残さない |
| `Err(PluginError::Execution / Host)` | 一時的な失敗。`[notifications].max_attempts` まで間隔を空けて再試行する（タイムアウト・トラップも同じ） |
| `Err(PluginError::Rejected / Serialization)` | 再試行しても変わらない失敗（設定の不備、相手が 4xx で断ったなど）。再試行しない |

- マニフェストの `capabilities.hooks` に `send-notification` が無い・`[plugins].enabled` に無いプラグインを `channels` に書くと、サーバーは起動しません。
- 再試行・送信数の上限（監査イベントの `max_per_hour`）・監査ログ・テンプレートはホストが行います。宛先の選び分け（運営者向けだけ、`audit.` だけなど）はプラグインが `Skipped` で行います。
- 送り先のドメインはマニフェストの `capabilities.network.allowed_domains` に含めてください。フックの `execution_timeout_ms` に通信の時間も含まれます。
- `aegis-server notify test --staff` で、運営者向けのテスト通知をプラグインに送れます。

### 1.8 `on-content-published`

予約公開などでコンテンツが公開状態になった直後に呼び出されます。外部 Webhook の呼び出し、SNS への自動投稿、静的サイトジェネレーターの再ビルドトリガーなどに使います。

```rust
fn on_content_published(&mut self, content: PublishedContent) -> Result<()>;
```

- 予約日時にスケジューラ（`publish_due_contents`）がコンテンツを `published` に更新し、監査ログ `content.publish` を記録した**後**に呼び出されます。
- 結果はコンテンツの公開状態に影響しません。
- プラグインが `Err` を返した（またはトラップ・タイムアウトが発生した）場合は、警告ログおよび監査ログ `plugin.error`（`details.hook = "on-content-published"`）が記録されます。他のプラグインの呼び出しや公開処理はそのまま継続します。
- 運営者への通知（メール等）は `[[notifications.audit.rules]]` で `action = "content.publish"` を設定すれば自動的に送信できます。

### 1.9 `check-comment`

公開サイトからコメントが投稿され、保存する前に呼び出されます（[コメント](comments.md#6-プラグインで審査する)）。外部のスパム判定サービス（Akismet など）への問い合わせや独自の NG ワードで、承認待ち・スパムに振り分けます。

```rust
fn check_comment(&mut self, comment: CommentSubmission) -> Result<CommentVerdict>;
```

- `CommentVerdict::Allow`（ホストの判定のまま）/ `Hold { reason }`（承認待ち）/ `Spam { reason }`（スパム）を返します。ホストの判定（`CommentSubmission.status`）より厳しくだけでき、承認待ちを公開に変えることはできません。
- `Err(PluginError::Rejected(..))` で投稿を断ります。メッセージは投稿者にそのまま表示されます（`422`）。
- 複数のプラグインがあれば順に呼び、いちばん厳しい判定を使います（`Spam` が出たらそこで止めます）。
- トラップ・タイムアウトなどの失敗はコメントを承認待ちにし、警告ログと監査ログ `plugin.error`（`details.hook = "check-comment"`）を残します。
- NG ワード・リンク数で既にスパムと判定した投稿・ハニーポットに掛かった投稿では呼ばれません。
- ABI の出力は `{"verdict": "allow" | "hold" | "spam", "reason": "..."}`（何も返さない・`verdict` が無ければ `allow`）、断るときは `{"error": "..."}` です。

### 1.10 `render-blocks`

本文に置けるカスタムブロックを提供します。マニフェストの `blocks`（[3.1](#31-カスタムブロック-blocks)）で宣言したブロックは Admin Studio のツールバーに並び、宣言した入力欄のフォームで編集できます。保存される名前は `<プラグイン名>/<ブロック名>`、入力値は `attributes` です。

```rust
fn render_blocks(&mut self, blocks: Vec<BlockNode>) -> Result<Vec<Option<String>>>;
```

- **呼ばれるとき**: 公開ページ（記事・固定ページ・商品）・フィード・コンテンツ API の `rendered_html`・エディタのプレビューで、**表示のたび**に呼ばれます。1 回の描画につきプラグインごとに 1 回、そのプラグインのブロックが文書の順にまとめて渡されます（トップレベルのブロックのみ）。
- **出力**: 入力と同じ数・同じ順の HTML。`None`（や足りない分）のブロックは何も表示しません。入力が足りない・おかしいブロックは `None` にしてください。
- **HTML はサニタイズされます**: `<script>`・`<iframe>` などと `on*` 属性・`javascript:` などの URL を取り除いたうえで（`<iframe>` は `capabilities.embed.frame_domains` で宣言したホストのものだけ残せます。[3.2](#32-外部のページの埋め込み-capabilitiesembed)）、`<div class="aegis-plugin-block" data-block="<名前>">` に包んで出力します（カスタム HTML ブロックの標準モードと同じ）。`<style>` と `style` 属性は残ります。スクリプトが必要な場合は `on-render-page` で `extra_head` に足してください（`official-code-enhancer` と同じ方法）。
- **見た目**: まとめて渡されるので、`<style>` は最初のブロックに 1 回だけ付けられます。`:where()` で詳細度を 0 にするとテーマの CSS で上書きしやすくなりますが、リンクの色・下線はテーマの本文のリンクの指定（`.prose a` など）に負けるので、クラス 2 つ程度の詳細度にしてください（`official-blocks` を参照）。
- **失敗**: トラップ・タイムアウト・不正な JSON などのとき、そのプラグインのブロックは何も表示せず、警告ログを残します（ページの表示は続けます）。プレビューでは `warnings` に表示します。
- **プラグインが無効なとき**: ブロックはそのまま保存され、Admin Studio には「プラグインが無効」と表示されます。公開ページには表示されません（中身の無いカスタム要素になります）。
- **性能**: 表示のたびに Wasm を呼ぶため、`http::fetch` で外部の API を呼ぶとその分だけ表示が遅くなります。CDN のキャッシュ（[`[cdn]`](configuration.md#217-cdn--on-demand-cache-tag-purge-cdn-エッジキャッシュ連携)）と組み合わせるか、外部のデータは別の手段で用意してください。
- ABI の入力は `{"blocks": [BlockNode, ...]}`、出力は `{"html": [string | null, ...]}`、失敗は `{"error": "..."}` です。

```rust
impl Plugin for MyBlocks {
    fn render_blocks(&mut self, blocks: Vec<BlockNode>) -> Result<Vec<Option<String>>> {
        Ok(blocks
            .iter()
            .map(|b| match b.name.as_str() {
                "my-blocks/note" => b
                    .attributes
                    .get("text")
                    .and_then(|v| v.as_str())
                    .map(|t| format!("<p class=\"my-note\">{}</p>", escape(t))),
                _ => None,
            })
            .collect())
    }
}
```

属性の値は利用者が入力したものです。HTML に入れるときは必ずエスケープしてください（サニタイズはスクリプトを取り除くだけで、レイアウトを壊すタグは残ります）。

---

## 2. フックに渡されるデータ型

すべて JSON としてゲストのリニアメモリ経由で受け渡されます。

### 2.1 `PostContext`

| フィールド | 型 | 説明 |
| --- | --- | --- |
| `id` | string | 新規発行されたコンテンツ ID (UUID v4) |
| `title` | string | タイトル |
| `slug` | string | スラッグ |
| `author_id` | string | 著者 ID（未指定時は空文字列） |
| `blocks_json` | string | ブロック配列を JSON 文字列化したもの |

SDK ヘルパー:

| メソッド | 説明 |
| --- | --- |
| `PostContext::new(title, slug)` | テスト用コンストラクタ |
| `post.blocks()` | `blocks_json` を `Vec<BlockNode>` にパース |
| `post.set_blocks(&blocks)` | `Vec<BlockNode>` を `blocks_json` に書き戻す |

### 2.2 `BlockNode`

```json
{ "name": "core/heading", "attributes": { "level": 2 }, "text": "見出し", "children": [] }
```

| フィールド | 型 | 説明 |
| --- | --- | --- |
| `name` | string | ブロック種別（`core/heading`, `core/paragraph`, `core/code`, `core/image`, `core/table` など。一覧と属性は [本文のブロック](blocks.md)） |
| `attributes` | object \| null | 任意の属性。属性を省略して保存されたブロックは `null` で渡される（SDK は空のマップとして読む） |
| `text` | string \| null | テキスト内容 |
| `children` | `BlockNode[]` | 子ブロック |

SDK ヘルパー:

| メソッド | 説明 |
| --- | --- |
| `BlockNode::new(name)` | 空のブロックを作成 |
| `BlockNode::paragraph(text)` | `core/paragraph` |
| `BlockNode::heading(level, text)` | `core/heading`（`level` 属性付き） |
| `BlockNode::code(language, code)` | `core/code`（`language` 属性付き） |
| `block.attribute(key)` | 属性を取得 |
| `block.set_attribute(key, value)` | 属性を設定 |

### 2.3 `UserAuthContext`

| フィールド | 型 | 説明 |
| --- | --- | --- |
| `provider` | string | 認証したプロバイダ（`local` / `oidc` / `saml`）。変更は無視される |
| `user_id` | string | セッションの `user_id`（ローカル認証ではユーザー名）。変更は無視される |
| `email` | string | メールアドレス（未登録なら空文字列） |
| `display_name` | string \| null | 表示名 |
| `role` | string | `admin` / `editor` / `viewer` |
| `metadata` | object | プロバイダ固有の属性。OIDC では `issuer`、`sub`、`claims`（ID トークンのクレーム。`groups` などのマッピングに使える）、SAML では `issuer`、`name_id`、`name_id_format`、`session_index`、`attributes`（属性名 → 値の配列）、ローカル認証では空 |

### 2.4 `OrderContext` / `OrderStatusChange`

金額はすべて通貨の最小単位の整数です（JPY なら円、USD ならセント）。

```json
{
  "order_id": "8f0c…（見積もり中は null）",
  "order_number": "261023-U6D3F6",
  "currency": "jpy",
  "lines": [
    { "product_id": "…", "slug": "ceramic-mug", "title": "Ceramic Mug", "sku": null,
      "unit_price": 1200, "quantity": 2, "line_total": 2400 }
  ],
  "adjustments": [
    { "kind": "discount", "label": "Coupon WELCOME10", "amount": -540, "code": "WELCOME10", "source": "official-shop-rules" },
    { "kind": "shipping", "label": "Shipping", "amount": 600, "code": null, "source": "official-shop-rules" }
  ],
  "subtotal": 5400,
  "total": 5460,
  "customer_email": "hanako@example.com",
  "customer_name": "Hanako Yamada",
  "shipping_address": { "name": "…", "postal_code": "100-0001", "region": "Tokyo", "city": "Chiyoda",
                        "line1": "1-1 Marunouchi", "line2": "", "phone": "", "country": "JP" },
  "coupon_code": "WELCOME10",
  "status": "pending_payment",
  "metadata": {}
}
```

`OrderStatusChange` は `{ "order": OrderContext, "from_status": "pending_payment" | null, "to_status": "paid", "occurred_at": "RFC 3339" }` です。状態は `pending_payment` / `paid` / `shipped` / `completed` / `cancelled`。

### 2.5 `NotificationMessage` / `Recipient` / `Delivery`（`send-notification`）

| フィールド | 型 | 説明 |
| --- | --- | --- |
| `kind` | string | 通知の種類（`order.placed` / `order.paid` / `order.shipped` / `order.cancelled` / `audit.<操作>` / `test`） |
| `recipient` | object | `{"type": "staff"}`（運営者）または `{"type": "person", "email": "...", "name": "..."}` |
| `subject` | string | 件名（テンプレートから作ったもの） |
| `body` | string | 本文（プレーンテキスト） |
| `html` | string（省略可） | HTML の本文（`[notifications].html = true` のとき。メール配信 API へ送るチャネル向け。[通知 3.1](notifications.md#31-html-メール)） |
| `data` | object | テンプレートに渡した値（`order` / `audit` / `site_name` / `site_url` / `admin_url` / `kind` など。[通知 3](notifications.md#3-テンプレート)） |

`Delivery` は `Sent`（送った）/ `Skipped`（扱わない）。

### 2.6 `PluginError`（SDK）

| バリアント | 用途 |
| --- | --- |
| `PluginError::Rejected(String)` | 入力を受け付けない（`on_before_save` の保存拒否、`on_user_authenticated` のログイン拒否）。メッセージがそのまま API の応答に出る |
| `PluginError::Execution(String)` | プラグインロジックのエラー |
| `PluginError::Serialization(String)` | JSON 変換エラー（`serde_json::Error` から自動変換） |
| `PluginError::Host(String)` | ホスト API 呼び出しのエラー |

### 2.7 `PublishedContent`（`on-content-published`）

| フィールド | 型 | 説明 |
| --- | --- | --- |
| `id` | string | 公開されたコンテンツ ID (UUID v4) |
| `content_type` | string | スキーマ名（`articles`, `news` など） |
| `title` | string | タイトル |
| `slug` | string | スラッグ |
| `published_at` | string | 公開日時 (RFC 3339) |
| `author_id` | string \| null | 著者 ID（未指定時は null） |
| `fields` | object | カスタムフィールド値のマップ |

### 2.8 `CommentSubmission` / `CommentVerdict`（`check-comment`）

| フィールド | 型 | 説明 |
| --- | --- | --- |
| `content_type` / `content_id` | string | コメントが付くコンテンツの種類と ID |
| `content_title` / `content_slug` | string | コンテンツのタイトルとスラッグ |
| `parent_id` | string \| null | 返信先のコメント ID |
| `author_name` / `author_email` | string | 投稿者の名前とメールアドレス（小文字） |
| `body` | string | 本文（プレーンテキスト） |
| `ip` / `user_agent` | string \| null | 投稿者の IP アドレスとブラウザ |
| `status` | string | ホストの判定（`approved` / `pending`） |

`CommentVerdict` は `{"verdict": "allow"}`、`{"verdict": "hold", "reason": "..."}`、`{"verdict": "spam", "reason": "..."}` のいずれかです（`reason` はログに残す理由。省略可）。

---

## 3. マニフェスト (`plugin.manifest.json`)

各プラグインは `plugins_dir` 直下の `<name>/`（`<name>` はマニフェストの `name`）に配置し、`plugin.manifest.json` を置きます。配置しただけでは読み込まれず、`[plugins].enabled` に名前を追加すると有効になります（[5. インストールと有効化](#5-インストールと有効化)）。

```json
{
  "name": "my-plugin",
  "version": "0.1.0",
  "entrypoint": "plugin.wasm",
  "capabilities": {
    "network": { "allowed_domains": ["hooks.slack.com"] },
    "storage": { "kv_namespace": "my_plugin_state" },
    "hooks": ["on-before-save", "transform-content-ast"]
  },
  "resource_limits": {
    "max_memory_mb": 16,
    "max_fuel_units": 100000000,
    "execution_timeout_ms": 250
  }
}
```

| キー | 型 | 必須 | 説明 |
| --- | --- | --- | --- |
| `name` | string | ✅ | プラグインの識別子。英小文字・数字で始まり、英小文字・数字・`-`・`_` のみ（64 文字以内）。インストール先のディレクトリ名・`[plugins].enabled`・ログのタグ (`[Wasm:<name>]`) に使用 |
| `version` | string | ✅ | バージョン |
| `entrypoint` | string | ✅ | Wasm バイナリのパス（マニフェストからの相対） |
| `display_name` / `description` / `author` / `license` / `tags` / `homepage` / `repository` / `min_aegis_version` | | | ストアや一覧表示用のメタデータ（テーマの `theme.json` と同じ項目。`author` は文字列または `{"name", "email", "url"}`）。動作には影響しない |
| `capabilities.network.allowed_domains` | string[] | | `net::is_allowed` で許可するドメイン。完全一致またはサブドメイン一致 |
| `capabilities.storage.kv_namespace` | string | | KVS のネームスペース。未宣言の場合 `kv::set` は権限エラー |
| `capabilities.hooks` | string[] | | 有効にするフック名（[1. フック一覧](#1-フック一覧)） |
| `capabilities.embed.frame_domains` | string[] | | カスタムブロックに埋め込める `<iframe>` の配信元のホスト（[3.2](#32-外部のページの埋め込み-capabilitiesembed)） |
| `resource_limits` | object | | 省略した項目（または全体）はサーバー設定 `[runtime].default_*` で補われる（下記） |

有効にしたプラグインのマニフェストが読めない、`name` とディレクトリ名が違う、`entrypoint` のファイルが無い、ホストに無い関数をインポートしている（[ABI 仕様](plugin_abi_specification.md#3-ホスト-api-インポート仕様-host-モジュール)）場合は、**サーバーが起動しません**。アクセス制限などを実装したプラグインが黙って外れたまま動き続けることを防ぐためです。有効にしていないプラグインの不備は警告ログと `aegis plugin list` に表示されます。

### 3.1 カスタムブロック (`blocks`)

`render-blocks` フック（[1.10](#110-render-blocks)）で描画するブロックを宣言します。`blocks` があるのに `capabilities.hooks` に `render-blocks` が無いマニフェストは読み込めません。

```json
"capabilities": { "hooks": ["render-blocks"] },
"blocks": [
  {
    "name": "button",
    "label": { "en": "Button", "ja": "ボタン" },
    "description": { "en": "A link styled as a button", "ja": "ボタンの形のリンク" },
    "fields": [
      { "name": "label", "type": "string", "label": { "en": "Label", "ja": "ボタンの文字" }, "required": true },
      { "name": "url", "type": "url", "label": "URL", "required": true, "placeholder": "https://" },
      { "name": "style", "type": "select", "label": "Style", "default": "primary",
        "options": [{ "value": "primary", "label": "Filled" }, { "value": "secondary", "label": "Outline" }] }
    ]
  }
]
```

| キー | 説明 |
| --- | --- |
| `name` | ブロック名（プラグイン名と同じ形式）。本文には `<プラグイン名>/<name>` で保存する |
| `label` / `description` | Admin Studio に出す名前・説明。文字列か、言語ごとの `{"en": ..., "ja": ...}`（Admin Studio の言語 → 英語 → 最初のものの順に使う） |
| `fields[].name` | 属性名（英字で始まり、英数字と `_`）。値は `attributes.<name>` |
| `fields[].type` | `string`（1 行）/ `text`（複数行）/ `url` / `number` / `boolean` / `select`（`options` が必要）/ `image`（URL。アップロード・メディアライブラリから選べる） |
| `fields[].label` / `placeholder` / `help` | 入力欄の名前・プレースホルダー・説明（`label` と同じく言語ごとにできる） |
| `fields[].required` | 空のとき、エディタに注意を出す（保存は止めない） |
| `fields[].default` | 新しく置いたブロックの初期値 |

宣言の不備（名前の形式・重複、選択肢の無い `select`、知らない `type`）があると、そのプラグインを有効にしたサーバーは起動しません。

### 3.2 外部のページの埋め込み (`capabilities.embed`)

動画のプレーヤーなど、外部のページを `<iframe>` でカスタムブロックに埋め込むには、配信元のホストを宣言します。

```json
"capabilities": {
  "hooks": ["render-blocks"],
  "embed": { "frame_domains": ["www.youtube-nocookie.com", "player.vimeo.com"] }
}
```

- **サニタイズ**: そのプラグインのブロックの HTML に限り、`src` が `https://<宣言したホスト>/...` の `<iframe>` を残します。ホストは完全一致です（サブドメイン・ユーザー情報 `user@`・ポートの付いた URL は不可）。残す属性は `src`・`title`・`width`・`height`・`allow`・`allowfullscreen`・`loading`・`referrerpolicy`・`sandbox`・`class`・`style`・`frameborder` だけです（`srcdoc`・`on*` などは取り除きます）。宣言していないホストの `<iframe>`、ほかのプラグインのブロックの `<iframe>`、`<script>` は今までどおり取り除きます。
- **CSP**: 有効なプラグインが宣言したホストを、すべてのレスポンスの `Content-Security-Policy` の `frame-src` に `https://<ホスト>` として足します。CSP に `frame-src` があればそこに、無ければ iframe に効いている `child-src`（無ければ `default-src`）の値に足した `frame-src` を加えます。管理者が `frame-src 'none'`・`*` と書いた CSP と、iframe を制限していない CSP はそのままです。Admin Studio の **Settings › Headers** で CSP を変えても、この追加は続きます。
- **書き方**: ホスト名だけ（`www.youtube-nocookie.com`）。英小文字・数字・`-` のラベルを `.` でつなぎ、2 つ以上。スキーム・パス・ポート・ワイルドカードは書けません。`blocks` の無いプラグインには宣言できません。
- 利用者が書いた HTML をそのまま埋め込むのではなく、入力された URL から ID を取り出してプレーヤーの URL を組み立ててください（`official-blocks` の動画ブロックを参照）。
- 宣言したホストは Admin Studio のプラグインの一覧の「機能・権限」に表示されます。

## 4. リソース制限 (`resource_limits`)

| キー | 省略時の値 | 説明 |
| --- | --- | --- |
| `max_fuel_units` | `[runtime].default_max_fuel_units`（既定 `100000000`） | フック 1 回あたりの Wasm 命令実行量の上限。超過するとトラップ |
| `execution_timeout_ms` | `[runtime].default_timeout_ms`（既定 `250`） | フック 1 回あたりの実行時間の上限（最小 50ms に切り上げ） |
| `max_memory_mb` | `[runtime].default_max_memory_mb`（既定 `16`） | 線形メモリの上限（MiB、最小 1）。`memory.grow` で超えようとするとトラップし、初期メモリが上限を超えるモジュールはインスタンス化に失敗する。Rust の `wasm32-unknown-unknown` ビルドは初期メモリだけで約 1.1 MiB を使うため、2 以上を指定する。インスタンスのプールを使う場合（既定）は `[runtime].pool_max_memory_mb`（既定 `64`）以下にする（超えると読み込み時にエラー） |

- 項目ごとに補われるため、`{"max_memory_mb": 8}` のように一部だけ指定できます。
- 上限超過（トラップ）はプラグインのエラーとして扱われます（各フックのエラー時の挙動に従い、`plugin.error` を監査ログに記録）。

> [!NOTE]
> フックは呼び出しごとに新しい Wasm インスタンスで実行されます。プラグイン構造体も毎回 `Default::default()` で生成されるため、**フック呼び出し間でメモリ上の状態は保持されません**。

---

## 5. ホスト API

ホストは `host` モジュールとして以下の関数をインポート提供します。

| SDK API | Wasm インポート | シグネチャ | 必要な権限 |
| --- | --- | --- | --- |
| `log_debug!` / `log_info!` / `log_warn!` / `log_error!` | `host.log` | `(level: i32, ptr: i32, len: i32)` | なし |
| `net::is_allowed(url) -> bool` | `host.http_check_allowed` | `(ptr: i32, len: i32) -> i32` | `capabilities.network` |
| `kv::set(key, value) -> Result<()>` | `host.kv_set` | `(k_ptr, k_len, v_ptr, v_len) -> i32` | `capabilities.storage` |
| `kv::set_json(key, &value) -> Result<()>` | `host.kv_set` | 同上（値を JSON 文字列化） | `capabilities.storage` |
| `kv::get(key) -> Result<Option<String>>` / `kv::get_json` | `host.kv_get` | `(k_ptr, k_len) -> i64` | `capabilities.storage` |
| `http::fetch(&Request) -> Result<Response>` | `host.http_fetch` | `(req_ptr, req_len) -> i64` | `capabilities.network` |
| `config::get(key) -> Option<String>` / `config::get_or(key, default)` | `host.config_get` | `(k_ptr, k_len) -> i64` | なし |

`i64` を返す関数は、ホストがゲストの `allocate` で確保した領域に結果を書き込み、`(ptr << 32) | len` を返します（SDK が読み取って解放します）。

### 5.1 ログ (`host.log`)

| `level` | SDK マクロ | ホスト側 `tracing` レベル |
| --- | --- | --- |
| `0` | `log_debug!` | `debug` |
| `1` | `log_info!` | `info` |
| `2` | `log_warn!` | `warn` |
| `3` 以上 | `log_error!` | `error` |

### 5.2 ネットワーク許可チェック (`host.http_check_allowed`)

- `http://` または `https://` の URL のホスト名が `allowed_domains` と完全一致、またはそのサブドメインであれば `1`、それ以外は `0` を返します。ホスト名は HTTP クライアントと同じ URL パーサーで取り出し（大文字・小文字は区別しない）、ユーザー情報（`user@host`）を含む URL は拒否します。
- `capabilities.network` が未宣言の場合は常に `0` です。
- 実際に通信するには `host.http_fetch`（[5.5](#55-http-リクエスト-hosthttp_fetch)）を使います。

### 5.3 KVS 書き込み (`host.kv_set`)

| 戻り値 | 意味 | SDK の結果 |
| --- | --- | --- |
| `0` | 成功 | `Ok(())` |
| `1` | 権限なし（`capabilities.storage` 未宣言） | `Err(PluginError::Host(...))` |
| `2` | メモリ読み取り失敗などのエラー | `Err(PluginError::Host(...))` |

- キーは `{kv_namespace}:{key}` の形でホスト側に保存され、他プラグインのネームスペースとは分離されます。
- 値はサーバーの `[cache]` のバックエンドにキー `plugin-kv:{kv_namespace}:{key}` で保存します（有効期限なし）。`redis` なら再起動後も残り、全サーバーで共有されます。`memory` ではプロセス内にだけあり、再起動で消えます。
- 読み取りは `host.kv_get`（SDK の `kv::get` / `kv::get_json`）。戻り値は値の `(ptr << 32) | len`、未設定なら `0`、権限なしは `-1`、エラーは `-2` です。
- 1 回の読み書きは独立しています（読んで +1 して書くような更新は同時実行で数え漏れることがあります）。

### 5.4 HTTP リクエスト (`host.http_fetch`)

```rust
let request = http::Request::post_json("https://hooks.slack.com/services/…", &json!({ "text": "New order" }))?;
let response = http::fetch(&request)?;
if !response.is_success() { /* response.status, response.body */ }
```

- 宛先はマニフェストの `capabilities.network.allowed_domains`（とそのサブドメイン）だけです。許可されていない URL は送らずにエラーになります。
- メソッドは `GET` / `POST` / `PUT` / `PATCH` / `DELETE`。`Host` / `Content-Length` などのヘッダーは指定できません。
- **リダイレクトは追いません**（3xx をそのまま返します。リダイレクト先は許可の確認を受けていないため）。
- 本文は送信・受信とも 1 MiB まで。1 回の通信は最大 10 秒、1 回のフック呼び出しで 8 回まで。フックの `execution_timeout_ms` には通信の待ち時間も含まれるので、通信するプラグインは大きめ（例: 5000〜10000）にしてください。
- 失敗（許可されていない宛先・接続できない・上限超過）は `Err(PluginError::Host(...))` です。ホストは `{"error": "..."}` の JSON を返します。

### 5.5 設定値 (`host.config_get`)

サーバーの設定ファイルの `[plugins.settings.<プラグイン名>]` の値を読みます（他のプラグインの設定は読めません）。文字列はそのまま、数値・真偽値・配列は JSON の文字列で返ります。

また、Admin Studio のプラグイン管理画面（`PUT /api/v1/plugins/{name}/settings`）から設定された動的設定値も即座に反映され、設定ファイルの記述を上書きします（キャッシュに永続化され、再起動後も保持されます）。

```toml
[plugins.settings.official-order-notifier]
webhook_url = "env:SLACK_WEBHOOK_URL"   # env: で始まる値は環境変数から（API キーを設定ファイルに書かない）
notify_on = "pending_payment,paid,cancelled"
```

```rust
let url = config::get("webhook_url");             // Option<String>
let fee: i64 = config::get_or("shipping_fee", 0); // 読めなければ既定値
```

### 5.6 ネイティブテスト時のモック

`cargo test` などネイティブターゲット (`not(target_arch = "wasm32")`) でビルドした場合、SDK のホスト API はモックに置き換わります。

| API | モックの挙動 |
| --- | --- |
| ログ | 標準出力に `[WasmMock:LEVEL] message` を出力 |
| `net::is_allowed` | 常に `true`（マニフェストは参照しない） |
| `kv::set` / `kv::get` | スレッドローカルのメモリに保存・読み出し |
| `config::get` | `host::mock::set_config(key, value)` で設定した値 |
| `http::fetch` | 送らずに記録して `200` を返す（`host::mock::take_http_requests()` で取り出す） |

`host::mock::reset()` でモックを空にできます。

権限まわりの挙動は、Wasm にビルドしてランタイム上で確認してください。

---

## 6. 低水準 ABI（SDK を使わない場合）

| エクスポート | シグネチャ | 必須 | 説明 |
| --- | --- | --- | --- |
| `memory` | memory | ✅ | リニアメモリ |
| `allocate` | `(len: i32) -> i32` | ✅ | ホストが入力 JSON を書き込む領域を確保し、ポインタを返す |
| `transform_content_ast` | `(ptr: i32, len: i32) -> i64` | | 入力 `BlockNode[]` JSON、戻り値 `(ptr << 32) \| len` |
| `on_before_save` | `(ptr: i32, len: i32) -> i64` | | 入力 `PostContext` JSON、戻り値 `(ptr << 32) \| len` |
| `on_user_authenticated` | `(ptr: i32, len: i32) -> i64` | | 入力 `UserAuthContext` JSON、戻り値 `(ptr << 32) \| len` |
| `calculate_order` | `(ptr: i32, len: i32) -> i64` | | 入力 `OrderContext` JSON、戻り値 `(ptr << 32) \| len`（`{"error"}` で拒否） |
| `on_order_status_changed` | `(ptr: i32, len: i32) -> i64` | | 入力 `OrderStatusChange` JSON。戻り値は `0`（成功）か `{"error"}` |

- 戻り値の `len` が `0` の場合、ホストは入力をそのまま採用します。
- `on_before_save` で保存を、`on_user_authenticated` でログインを拒否するには、入力の型の代わりに `{"error": "<メッセージ>"}` を返します（SDK の `abi::write_error_to_host`）。`error` キーを持つオブジェクトはすべて拒否として扱います。
- 返却された JSON が期待する型としてパースできない場合はホスト側エラー（[1.2](#12-transform-content-ast) / [1.3](#13-on-before-save) のエラー時の挙動を参照）になります。

## 7. インストールと有効化

テーマ（`themes/<id>/` + `[theme].active`）と同じく、プラグインも「インストール」と「有効化」を分けて管理します。将来のストアからのインストールもこの形（`plugins/<name>/` への展開）を前提にしています。

1. **インストール**: `plugin.manifest.json` と `plugin.wasm` を `plugins/<name>/` に置く（`<name>` はマニフェストの `name`）。
2. **有効化**: Admin Studio のプラグイン画面で「有効にする」を押す（再起動不要）か、設定ファイルの `[plugins].enabled` に `<name>` を追加してサーバーを再起動する。フックは有効化リストの順に実行されます。

```toml
[plugins]
enabled = ["seo-tools", "slack-notifier"]
```

- インストール済みのプラグインと状態は `aegis plugin list`（CLI）または `GET /api/v1/plugins`（admin 権限）で確認できます。

### 7.1 Admin Studio からの有効・無効の切り替え

Admin Studio のプラグイン画面（`admin` ロールのみ）の「有効にする」「無効にする」で、サーバーを再起動せずに切り替えられます。

- API: `PUT /api/v1/plugins/{name}/enabled` に `{"enabled": true}` / `{"enabled": false}` を送ります。応答は `GET /api/v1/plugins` と同じ一覧です。監査ログは `plugin.enable` / `plugin.disable`。
- 有効にしたプラグインは、フックの実行順の**最後**に加わります。順序はプラグイン画面の「フックの実行順」で変えられます（下記）。
- 切り替えた一覧は DB（`_aegis_settings` の `plugins.enabled`）に保存され、**以後は設定ファイルの `[plugins].enabled` より優先されます**（起動時のログに `Using the plugin list saved from Admin Studio` と出ます）。設定ファイルの値に戻すには「設定ファイルの値に戻す」を使います（下記）。
- 読み込めないプラグイン（未インストール、マニフェストの不備、ホストに無い関数のインポートなど）は有効にできず `400` を返します。そのときは何も変わりません。
- `[notifications].channels` に `plugin:<名前>` で使っているプラグインは無効にできません（`409`。先に `channels` から外してください）。
- `[runtime].enable_wasm_plugins = false` のサーバーでは切り替えられません（`409`）。
- 複数台で動かしている場合（`[cache].backend = "redis"`）、切り替え・並べ替え・設定値の変更はほかのサーバーにもすぐ反映されます（[7.3](#73-複数台での反映)）。
- プラグインの設定値（`PUT /api/v1/plugins/{name}/settings`）は有効・無効にかかわらず保持され、有効にした時点から読まれます。

### 7.2 フックの実行順の並べ替えと、設定ファイルの値に戻す

プラグイン画面の上部の「フックの実行順」に、有効なプラグインが実行順に並びます（`admin` ロールのみ操作できます）。上下のボタンで並べ替えると、再起動せずにすぐ反映され、DB に保存されます。

- API: `GET /api/v1/plugins/order` → `{"enabled": [...], "configured": [...], "customized": true}`。`enabled` は今の実行順、`configured` は設定ファイルの `[plugins].enabled`、`customized` は Admin Studio で変えた一覧を DB に保存しているか。
- 並べ替え: `PUT /api/v1/plugins/order` に `{"enabled": ["b", "a"]}`。今有効なプラグインをちょうど 1 回ずつ並べます（足りない・余計・重複は `400`。有効・無効は `PUT /api/v1/plugins/{name}/enabled` で切り替えます）。監査ログは `plugin.reorder`。
- 設定ファイルの値に戻す: `DELETE /api/v1/plugins/order`（画面の「設定ファイルの値に戻す」）。DB に保存した一覧を消し、`[plugins].enabled` のプラグインをその順に読み込み直します（Admin Studio だけで有効にしたものは無効になります）。監査ログは `plugin.reset`。`[plugins].enabled` のプラグインが読み込めなければ `400` で、何も変わりません。
- 応答はどれも `GET /api/v1/plugins/order` と同じ形です。`[runtime].enable_wasm_plugins = false` なら `409`、通知チャネル（`[notifications].channels` の `plugin:<名前>`）が外れる場合も `409` です。

### 7.3 複数台での反映

Admin Studio での変更は、キャッシュと同じバックエンドのイベントバスで全サーバーに知らせます。

- `[cache].backend = "redis"`: Redis の Pub/Sub（チャネル `aegis:events`）で知らせ、受け取ったサーバーが DB の一覧（設定値はキャッシュ）を読み直して差し替えます。通常は 1 秒もかからずに揃います。Redis との接続が切れていた間の知らせは、繋ぎ直したときにまとめて読み直して取り戻します。
- `[cache].backend = "memory"`: 1 台構成専用です（セッションもプロセスの中にしか無いため）。同じ DB で 2 台目を起動すると、起動時に止まります（[設定リファレンス 2.4](./configuration.md#24-cache--キャッシュ--セッション)）。
- 対象: 有効・無効の切り替え、並べ替え、設定ファイルの値に戻す、プラグインの設定値（`PUT /api/v1/plugins/{name}/settings`）。
- 新しく有効になったプラグインがそのサーバーで読み込めない（そのサーバーにだけインストールされていない、など）場合は、エラーログを出して今のプラグインのまま動きます。プラグインはすべてのサーバーに同じものを置いてください。
- `[runtime].enable_wasm_plugins = false` にすると、`enabled` に関係なくすべてのプラグインを読み込みません。
- テンプレートとサンプルは `sdk/templates/*`・`sdk/examples/*` にあり、`plugins/` には含まれません（既定ではプラグインは 1 つも有効になっていません）。

### 7.4 ZIP パッケージからのインストールと署名

Admin Studio のプラグイン画面（ドラッグ＆ドロップ）または `POST /api/v1/plugins/install`（`admin`、multipart の `file` フィールド）で、ZIP をアップロードしてインストールできます。テーマも同じ仕組みで `POST /api/v1/themes/install`（[テーマのガイド](./theme_and_publishing_guide.md#zip-からのインストール)）。

1. ZIP を一時ディレクトリに展開します。パスの `..`・絶対パス（ZipSlip）は拒否し、1 ファイル 50MB・合計 100MB・1,000 エントリを超えるものも拒否します。
2. `plugin.manifest.json` を ZIP の直下か、1 段下のディレクトリから探します。
3. マニフェストと Wasm を検証し、`min_aegis_version`（semver）が今のバージョン以下かを確かめます。
4. 署名を確かめます（下記）。
5. `plugins/<name>/` に配置します。**同じ名前のプラグインがあれば置き換えます**。有効にはしません（[5.1](#51-admin-studio-からの有効無効の切り替え) で有効化）。監査ログは `plugin.install`。

検証に失敗すると `400` で、何も配置しません。

**署名（`signature.json`）**: パッケージの直下に置きます。

```json
{ "algorithm": "ed25519", "public_key": "<32 バイトの hex>", "signature": "<64 バイトの hex>", "target": "package" }
```

- `target` は `"package"`（既定。省略可）のみで、`signature.json` 以外の全ファイルのパスと SHA-256 を並べたダイジェストに対する署名です（`"manifest"` などの他の値は受け付けません）。Wasm やテンプレートを含め、パッケージ内のどのファイルの改ざんも検出します。
- `[plugins].require_signatures = true` なら署名の無いパッケージを拒否します。このとき `[plugins].trusted_public_keys` に少なくとも 1 つの公開鍵を設定する必要があります（空だとサーバーが起動せず、パッケージ検証も拒否されます）。`false` でも、`signature.json` があれば確かめます（不正なら拒否）。
- `[plugins].trusted_public_keys` には信頼する Ed25519 公開鍵（32 バイトの hex）を列挙します（`require_signatures = false` で空のときは、署名が正しければどの鍵でも受け入れます）。テーマは `[theme]` の同名の設定です。
