# AegisCMS 開発進捗 & TODOリスト (TODO.md)

> [!NOTE]
> 作業を再開する際は、まず本ドキュメントの「現在の状況」と「未着手タスク」を確認してください。
> **ルール**: 各修正・実装ステップの終了後には、必ず本ドキュメントを更新してください。
> **完了したタスクはここに残さず**、仕様はリファレンス（`docs/*.md`・`admin/README.md`）、設計判断・注意点は [docs/architecture.md](docs/architecture.md)、開発手順は [docs/development.md](docs/development.md) に書き込んでから消します。経緯は git の履歴（PR）で追えます。

---

## 1. 現在の状況

- **直近の完了**: EC 拡張 Phase 1: 外部決済抽象化層（`PaymentProvider`）と返金 API（Stripe Refund / 手動返金）
  - **決済プロバイダ抽象化**: `PaymentProvider` トレイトに `supports_refund` / `refund` メソッドを追加し、外部決済ゲートウェイ通信を完全に抽象化。Stripe 以外の将来的な決済サービス（PayPal, Square 等）もコアロジック（`CommerceService`）や DB 構造を変更せずに追加できる設計を実現。
  - **取引識別子の分離**: チェックアウト開始時の識別子（Stripe Checkout Session ID `cs_...`）を `payment_ref`、確定取引 ID（Stripe Payment Intent ID `pi_...`）を `transaction_id` として分離管理し、確実な返金照合を保証。
  - **返金 API & ACID トランザクション**: `POST /api/v1/orders/{id}/refund` を新設。部分返金・全額返金、注文商品の在庫復帰（オプション）、累計返金額の加算、ステータス自動遷移（全額返金時に `refunded` へ）、監査ログ（`order.refund`）の記録を SQLite / PostgreSQL / MySQL のすべてで同一トランザクション内でアトミックに実行。
  - **Stripe Refund & Webhook 同期**: 冪等性キー（`Idempotency-Key`）を付与した Stripe API `POST /v1/refunds` 呼び出し、および Stripe 側で直接返金された際の Webhook イベント（`charge.refunded`）の自動解釈・状態同期を実装。
  - **Admin Studio (Orders)**: `OrderStatus::Refunded` バッジ、返金フィルター、注文詳細での返金額・返金履歴・トランザクションID表示、返金操作モーダル（残額バリデーション、リアルタイム通貨プレビュー、在庫復帰トグル、手動/オンライン別案内バナー）を実装。
  - **品質検証**: `task check`（fmt・Prettier・Clippy / oxlint ゼロ警告・全テスト・Admin Studio ビルド）パス。
  - **ドキュメント反映**: 仕様・API は [docs/commerce.md](docs/commerce.md)、アーキテクチャ・設計判断は [docs/architecture.md 9](docs/architecture.md#9-ec-aegis-commerce)。
- **その前**: 予約公開・予約非公開の完了を著者にメールで通知（`content.published` / `content.unpublished`、組み込みテンプレート en / ja、公開ページの URL `content_url`）。仕様は [notifications.md 2](docs/notifications.md#2-送る通知)。あわせて Windows で依存を足したときのロックファイルの注意（CI の npm 10 で `npm ci` を確かめる）を [development.md 8.3](docs/development.md#83-依存パッケージを追加したときpackage-lockjson) に記載。その前は Admin Studio に Playwright の E2E テストと Storybook を導入（[development.md 8](docs/development.md#8-admin-studio-の-e2e-テストと-storybook)）。その前は Admin Studio のモバイル対応。
- **実装済みの機能**: 一覧は [README](README.md#23-クレート構成と各モジュールの責務-crates) とリファレンス（README 2.3 末尾のドキュメント一覧）を参照。
- **検証**: `task check`（rustfmt・Prettier・Clippy / oxlint ゼロ警告・全テスト・Admin Studio のビルド）がすべて通る状態を維持する。

### 🚀 次に着手する項目

1. **EC 拡張 Phase 2: 商品のバリエーション（サイズ・色ごとの在庫・価格）**:
   - `_aegis_product_variants` テーブルの新設（`product_id`, `sku`, `title`, `price_override`, `stock`, `attributes_json`, `position`）と SQLite / PostgreSQL / MySQL マイグレーション。
   - アトミック在庫引き当て（`StockReservation`）を商品単位からバリエーション（SKU）単位へ拡張。
   - カート・注文明細（`OrderLine`）におけるバリエーション ID・選択属性（サイズ、カラーなど）の保持。
   - Admin Studio（Products 画面）へのバリエーション一覧・追加・在庫/価格編集 UI の実装。

---

## 2. 未着手タスク

### 認証・アカウント

- [ ] **SAML の拡張**（要望があれば）: IdP のメタデータ URL からの設定の読み込み（証明書の自動更新）、暗号化したアサーション（SP の秘密鍵が要る）、AuthnRequest の署名、シングルログアウト、ECDSA の署名。今の対応範囲は [authentication.md 8](docs/authentication.md#8-シングルサインオン-saml-20)。

### Admin Studio

- [ ] **E2E テスト・ストーリーの拡充**: 今はプラグイン画面だけ。ログイン（失敗・ロック）、記事の作成・保存・公開、テーマの切り替え、ユーザー管理・招待、EC（商品・注文）、狭い画面（モバイル対応の確認）。Storybook の `play` を CI で動かす（`@storybook/addon-vitest` など）。
- [ ] **翻訳の続き**: サーバーのエラーメッセージ（API がエラーコードを返すようにする。アカウント登録が先行例）、公開サイトの組み込みテンプレート・カートのスクリプト（`Add to cart` など）、翻訳サービス（Crowdin など）の設定と 3 言語目。

### プラグイン

- [ ] **実行順の並べ替え**: Admin Studio で有効にしたプラグインはフックの実行順の最後に加わるだけ。順序を変えるには設定ファイルを直して DB の `plugins.enabled` を消す必要がある。並べ替え UI と、設定ファイルの値に戻す操作。
- [ ] **複数台での切り替えの伝播**: 有効・無効の切り替えは操作を受けたサーバーだけに反映される（他は再起動で揃う）。キャッシュ（Redis）の Pub/Sub などで他のサーバーにも差し替えを伝える。
- [ ] **ストアからのインストール**: パッケージの検証（テーマの `validate_theme_package` に相当）・署名・`min_aegis_version` の確認、展開先は `plugins/<name>/`。テーマの ZIP 導入も同じ仕組みで。
- [ ] **AssemblyScript / TinyGo SDK の追従**: EC のフック・ホスト API（`calculate-order` / `on-order-status-changed` / `http_fetch` / `kv_get` / `config_get`）と `send-notification`、`on-content-published` のラッパー。ABI は [plugin_abi_specification.md](docs/plugin_abi_specification.md) に記載済み。
- [ ] **AssemblyScript SDK の JSON 処理の高速化**: `escapeJson` / `findStringField` が入力の大きさの二乗で遅い（[architecture.md 12](docs/architecture.md#12-性能の記録)）。

### 通知

- [ ] **通知の追加**: HTML メール、送信待ちの永続化（再起動で失われないキュー）、Admin Studio からのテスト送信・テンプレート編集。
- [ ] **コメントの通知**: コメント機能そのものがまだ無い（コメントの投稿・承認・スパム対策を作るときに、著者・運営者への通知 `comment.*` も一緒に）。

### EC

- [x] **EC 拡張 Phase 2: 商品のバリエーション**: `_aegis_product_variants` テーブル新設、サイズ・色ごとの在庫・価格、アトミックな在庫引き当て（`StockReservation` 拡張）、カート・注文明細対応、Admin Studio バリエーション管理。
- [x] **EC 拡張 Phase 3: 複数通貨**: `supported_currencies` 設定、為替レート換算および通貨別価格、最小通貨単位（Zero-decimal / Cent）の正規化、多通貨 Stripe Checkout 連携、テーマ通貨セレクター。
- [ ] **フェーズ 4**: 顧客アカウント（会員登録・注文履歴）、売上レポート、在庫切れの通知、On-demand ISR との連動。

### 将来のロードマップ

- [ ] **分散ストレージモード**: libSQL (Turso) への対応とクラスタリング環境の最適化。
- [ ] **On-demand ISR / Cache-Tag Purge**: コンテンツ更新時の Cloudflare / CDN エッジキャッシュの無効化・タグパージ。

---

## 3. 技術メモ・申し送り事項 (Memo & Notes)

作業中の判断や、まだ文書にしていない注意点をここに書きます。機能が完了したら、該当する文書（設計判断は [docs/architecture.md](docs/architecture.md)）へ移して消してください。

- **Phase 2 バリエーション設計**:
  - `_aegis_product_variants` テーブル（SQLite / PostgreSQL / MySQL）を追加し、`StockReservation` に `variant_id` を伝搬。親商品テーブルかバリエーションテーブルかの在庫引き当てを同一トランザクション内で自動分岐。
  - カート・明細は `(product_id, Option<variant_id>)` のペアで識別し、価格上書き・SKU・在庫復帰（キャンセル/返金時）まで一貫対応。
  - Admin Studio の商品編集モーダルでバリエーション CRUD をシームレスに操作可能。
- **Phase 3 複数通貨設計**:
  - `CommerceConfig` に `supported_currencies` および `currency_rates` を新設し、小文字 ISO 4217 コードと正の実数レートを厳格にバリデーション。
  - `aegis-core::commerce` に `currency_decimals`（0/2/3 桁）、`convert_currency`（丸めとオーバーフロー安全処理）を実装。
  - 単価解決 `resolve_item_price`: バリエーションの `attributes.price_{currency}` または親商品の `price_{currency}` による特定通貨固定価格を最優先とし、未設定時は基軸価格から為替レートで自動換算。
  - 見積もり (`/quote`)・注文 (`/checkout`) 共にリクエストの `currency`（未指定時は基軸通貨）で計算され、Stripe Checkout Session も注文通貨の最小単位でそのまま連携。
  - 公開ストアフロントの `theme_vars`（`supported_currencies`, `currency_rates`）および `cart.js`（通貨セレクターバインディング、`localStorage` 永続化、自動再見積もり）を完備。
- **次のステップ**:
  - Phase 3 完了後の Pull Request (PR) 作成（ブランチ: `feat/commerce-expansion` -> `main`）。
