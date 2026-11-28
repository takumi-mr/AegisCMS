# AegisCMS TODO

> [!NOTE]
> 作業の再開時はここを読む。各作業の完了時に更新する。
> 完了したタスクは残さない。仕様は `docs/*.md`・`admin/README.md`、設計判断・注意点は [docs/architecture.ja.md](docs/architecture.ja.md)、開発手順は [docs/development.ja.md](docs/development.ja.md) に書いてから消す。
> **技術メモ・申し送り事項は変更ログ（CHANGELOG）を書く場所ではない**。単純な実装ログや変更履歴は記載せず、未解決の設計課題や既知の注意点のみを記録する。経緯は Git のコミットログや PR で追う。

---

## 直近の着手タスク

### バッチ 1: 認証・2FA・初期セットアップの緊急修正と競合対策 (`fix/auth-2fa-setup-races`)

- [ ] `POST /api/v1/users/me/2fa/disable` のパスワード検証バグ修正（`.is_ok()` → `matches!(..., Ok(true))`、`spawn_blocking` 化）（`crates/aegis-server/src/handlers/users.rs`）
- [ ] `POST /api/v1/users/me/2fa/enable` の再認証・既存 2FA 有効チェック・サーバー生成シークレット紐付け（`crates/aegis-server/src/handlers/users.rs`）
- [ ] ログイン時 2FA 有無確認の DB エラー Fail-Open 修正（`.ok().flatten()` を廃止し `500` で Fail-Closed）（`crates/aegis-server/src/handlers/auth.rs`）
- [ ] 2FA バックアップコードの同時再利用レース防止（不可分ガードまたは SQL 楽観ロック）および DB 更新失敗時の Fail-Open 修正（`crates/aegis-server/src/handlers/auth.rs`、`crates/aegis-auth/src/users.rs`）
- [ ] 初期セットアップ（`POST /api/v1/setup`）の TOCTOU レース対策（不可分な初期化クレーム導入、既存 `state.db` の再利用）（`crates/aegis-server/src/handlers/setup.rs`、`crates/aegis-server/src/setup.rs`）
- [ ] 2FA 待機中チャレンジ・パスワード検証中ログインの無効化・権限降格・セッション失効すり抜け対策（`crates/aegis-server/src/handlers/auth.rs`、`crates/aegis-auth/src/local.rs`）
- [ ] 最後のアクティブ管理者保護の TOCTOU レース対策（排他トランザクション化、Argon2 ハッシュの事前計算）（`crates/aegis-auth/src/users.rs`）

---

## 技術メモ・申し送り事項

- `admin/package.json` の `overrides`（`glob` → `minimatch ^11.0.5`）は、Dependabot の minimatch の警告（11.0.1 未満が対象）を避けるための一時的なもの。`@storybook/react-vite` が固定する `@joshwooding/vite-plugin-react-docgen-typescript@0.7` が `glob@13`（`minimatch ^10`）に依存するため。react-vite が新しい docgen プラグイン（`glob` に依存しない 0.9 系）に上がったら外す。`npm audit` に残る `glob` の警告（moderate）は、修正が react-vite のメジャーダウングレードになるため対応しない。
- セキュリティ監査対応バッチ（バッチ 1〜6、詳細: [docs/security_race_condition_audit.md](docs/security_race_condition_audit.md)）のうち、`crates/aegis-db`（SQL・マイグレーション）や `crates/aegis-cache`（Redis）を変更するバッチ（2・3・4・6）は、`AGENTS.md` 2a に従い `task docker:dbs:up` のうえで実機 PostgreSQL / MySQL / Redis に対して `task test:dbs` を実行して確認すること（マイグレーション追加時は 3 方言すべてで新規連番ファイルを作成し、リリース済みマイグレーションは編集しない）。

---

## 未着手タスク (Backlog)

### バッチ 2: 単一利用トークン・認証レート制限・SSO/OAuth・MySQL ユーザー名正規化 (`fix/auth-tokens-sso-races`)

- [ ] `CacheBackend::take`（不可分な取得＋削除: メモリは単一 WriteLock、Redis は `GETDEL`/Lua）または不可分消費ガードを共通導入し、招待・パスワード再設定・新規登録・会員メール確認/再設定トークンの同時リプレイを解消（`aegis-core`・`aegis-cache`・`aegis-auth`・`aegis-members`）
- [ ] `_aegis_users.email` の一意制約追加（3 DB 方言マイグレーション）および `PasswordResetService::complete` の `user.disabled` チェック追加（`crates/aegis-auth/src/password_reset.rs`、`crates/aegis-db/migrations/`）
- [ ] ログイン・2FA・会員ログイン・コンテンツパスワードのブルートフォース制限の同時バースト回避（TOCTOU）対策（検証前の試行枠先行確保）（`crates/aegis-auth/src/rate_limit.rs`、各ハンドラ）
- [ ] API トークン失効と OAuth リフレッシュトークン更新の TOCTOU レース解消（リフレッシュトークン先削除またはトランザクション化）（`crates/aegis-server/src/handlers/api_tokens.rs`、`crates/aegis-server/src/oauth.rs`）
- [ ] MySQL 大小文字非区別照合でのユーザー名不一致によるセッション失効漏れ・`is_self` ガード回避修正（DB の正規 `username` を使用）（`crates/aegis-server/src/handlers/users.rs`）
- [ ] OIDC / SAML ログインの Login CSRF 対策（`oauth.rs` 同様のブラウザ紐付け Cookie 導入）（`crates/aegis-server/src/handlers/{oidc,saml,sso}.rs`）
- [ ] API トークン上限（20 件）同時超過レースおよび OAuth 初回起動時の署名鍵生成競合の修正（`crates/aegis-auth/src/api_tokens.rs`、`crates/aegis-server/src/oauth.rs`）

### バッチ 3: 会員アカウントの競合・乗っ取り・退会整合性修正 (`fix/members-concurrency-and-takeover`)

- [ ] 事前登録（`unverified`）パスワード上書きによるアカウント乗っ取り対策（確認トークンの世代/ハッシュ紐付け・旧トークン無効化・`WHERE id = ? AND status = 'unverified'`）（`crates/aegis-members/src/member.rs`）
- [ ] `_aegis_members` 全列上書き（Lost Update）解消（プロフィール・パスワード・アバター・ステータス・ランクごとの列限定 `UPDATE` へ分離、会員自己更新に `AND status = 'active'` 条件を付与）（`crates/aegis-db/src/members.rs`、`crates/aegis-members/src/member.rs`）
- [ ] 会員ログイン中（Argon2 実行中）の停止/パスワード変更すり抜け対策、セッション失効の時刻境界強化・DB ステータス検証、および会員系 Argon2id 処理の `spawn_blocking` 化（`crates/aegis-members/src/member.rs`、`crates/aegis-server/src/handlers/members.rs`）
- [ ] 会員退会（`delete_me`）の競合対策（パスワード確認直後に `deleting`/`suspended` 化＋即時セッション失効、`start_from_checkout` 側で `member.status == "active"` 確認）（`crates/aegis-server/src/handlers/{members,subscription}.rs`）

### バッチ 4: コマース・返金・サブスクリプション・レビューの競合とロジック修正 (`fix/commerce-payments-races`)

- [ ] `CartRequest.member_id` のクライアント指定禁止（`#[serde(skip_deserializing)]` 付与＋セッションからのみ設定）（`crates/aegis-commerce/src/lib.rs`、`crates/aegis-server/src/handlers/shop.rs`）
- [ ] 注文返金・在庫戻しの楽観ロック導入（`WHERE id = ? AND refunded_amount = ? AND status = ?`、キャンセル済み注文の在庫戻し禁止）、同額部分返金の Stripe `Idempotency-Key` 衝突修正、`charge.refunded` Webhook の `transaction_id` 照合・累計額差分反映修正（`crates/aegis-db/src/orders.rs`、`crates/aegis-commerce/src/{lib,stripe}.rs`）
- [ ] サブスクリプション Webhook 順序逆転による解約復活防止（Stripe 最新状態再取得または `cancelled` 遷移禁止＋`event.created` 単調性チェック）および同時契約作成重複防止（`crates/aegis-server/src/handlers/subscription.rs`、`crates/aegis-db/src/subscriptions.rs`）
- [ ] 商品レビューの重複投稿レース対策（`product_id` 正規化＋3 DB 方言マイグレーションで `_aegis_reviews (order_id, product_id)` に `UNIQUE` 制約追加、注文アクセストークンの定数時間比較化）（`crates/aegis-commerce/src/review.rs`、`crates/aegis-db/migrations/`）
- [ ] ショップ・購入のその他不具合修正（記事購入重複と取消漏れ、カート行ロック順序ソートによるデッドロック防止、公開期限切れ商品の購入不可化、下書きバリアント非公開化、未設定時 Host ヘッダー依存の戻り URL 対策）（`crates/aegis-server/src/handlers/{shop,purchase}.rs`、`crates/aegis-commerce/src/lib.rs`、`crates/aegis-db/src/orders.rs`）

### バッチ 5: AST/HTML サニタイズ・公開コンテンツ情報漏洩・Analytics 対策 (`fix/ast-xss-and-public-leaks`)

- [ ] `HtmlRenderer::render_node` のフォールバックタグ名厳格検証（ハイフン必須のカスタム要素規則のみ許可）と `render_sandboxed_html` の親ページ Strict CSP ハッシュ登録の見直し（`crates/aegis-ast/src/renderer.rs`）
- [ ] `sanitize_html_fragment_with_frames` の HTML エンティティ回避対策（エンティティデコード＋スキーム/要素許可リスト化）およびプラグイン出力向け `remove_scripts` の強化（`crates/aegis-ast/src/{renderer,rewriter}.rs`）
- [ ] 公開一覧（`summarize_records`）からの `access_password` ハッシュおよび `public: false` フィールド除外、`themed_response` エラー HTML のエスケープ（`crates/aegis-server/src/handlers/public.rs`）
- [ ] `GET /oembed` の制限付きコンテンツ（`is_restricted()`）メタデータ漏洩修正（`crates/aegis-server/src/handlers/oembed.rs`）
- [ ] Analytics CSV フォーミュラインジェクション無害化（先頭 `=`, `+`, `-`, `@`, `\t`, `\r`）、`POST /api/v1/analytics/event` のレート制限および `pending` マップ上限追加（`crates/aegis-analytics/src/lib.rs`、`crates/aegis-server/src/analytics.rs`）

### バッチ 6: フォーラム・コメント・メルマガ・通知キュー・パッケージ導入の競合と認可修正 (`fix/community-queue-package-races`)

- [ ] フォーラムの認可・XSS 修正（`GET /api/v1/forum/threads` の非公開カテゴリ制限、`api_list_posts` / `create_post` の未承認スレッド制限、`report` の可視性・承認状態チェック、HTML フォームエラー応答の `escape_html` 適用、通報フォームの Same-Origin チェック）（`crates/aegis-server/src/handlers/forum.rs`、`crates/aegis-forum/src/lib.rs`）
- [ ] フォーラムのレース対策（`insert_post` の `AND is_locked = 0 AND status = 'approved'`＋自動ロック不可分化、`rate_limit_secs` クールダウンの `cache.increment` 不可分化）（`crates/aegis-db/src/forum.rs`、`crates/aegis-forum/src/lib.rs`）
- [ ] メルマガのレース対策（`send_issue` のロック取得後 `latest_issue` 再確認・オーナートークン付きロック解放/DB 状態管理、購読者 `confirm` / `unsubscribe` / `subscribe` の条件付き `UPDATE` と `confirm` 時トークン再発行）（`crates/aegis-newsletter/src/lib.rs`、`crates/aegis-db/src/newsletter.rs`）
- [ ] 通知キューワーカーの Claim/Lease レース修正（`build_notification_claim` に `AND next_attempt_at <= ?` 追加、リース一致条件＋SQL 側 `attempts = attempts + 1`、管理者削除の期限切れリース判定と条件付き削除）（`crates/aegis-db/src/notifications.rs`、`crates/aegis-notify/src/lib.rs`、`crates/aegis-server/src/handlers/notifications.rs`）
- [ ] コメント・パッケージ導入・版番号採番の競合対策（コメント重複・同時承認通知重複、プラグイン/テーマ導入のロック直列化＋アトミック配置、`insert_content_version` のトランザクション内採番）（`crates/aegis-comments/src/lib.rs`、`crates/aegis-server/src/handlers/{plugin,theme}.rs`、`crates/aegis-db/src/{sqlite,postgres,mysql}.rs`）
