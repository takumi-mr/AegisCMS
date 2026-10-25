# AegisCMS 開発進捗 & TODOリスト (TODO.md)

> [!NOTE]
> 作業を再開する際は、まず本ドキュメントの「現在の状況」と「未着手タスク」を確認してください。
> **ルール**: 各修正・実装ステップの終了後には、必ず本ドキュメントを更新してください。
> **完了したタスクはここに残さず**、仕様はリファレンス（`docs/*.md`・`admin/README.md`）、設計判断・注意点は [docs/architecture.md](docs/architecture.md)、開発手順は [docs/development.md](docs/development.md) に書き込んでから消します。経緯は git の履歴（PR）で追えます。

---

## 1. 現在の状況

- **直近の完了**: Admin Studio E2E テストの全体的拡充（全 17 テスト合格）
  - **認証・セッション (`auth.spec.ts`)**: 正常系ログイン・サインアウト（セッション無効化後のリダイレクト）、誤資格情報のエラーメッセージ、レートリミット連動によるアカウント一時ロック（3回連続失敗で 429 ロックアウトメッセージ表示）の E2E 検証。
  - **コンテンツ管理 (`content.spec.ts`)**: 記事の新規作成、Draft 保存・サイドバー反映、Published 公開、リロード後の永続化確認、Markdown モードと Blocks モードの相互変換検証。
  - **テーマ管理 (`themes.spec.ts`)**: インストール済みテーマ一覧、ワンクリックでのアクティブテーマ切り替え（Aegis Horizon ↔ Nordic Minimal Editorial）、トースト表示および永続化検証。
  - **ユーザー管理 (`users.spec.ts`)**: 初期管理者アカウント一覧確認、新規ユーザー作成（エディターロール）、メール招待モーダルによる招待リンク発行（`notifications` log チャンネル連動）の検証。
  - **EC 管理 (`commerce.spec.ts`)**: `[commerce].enabled = true` 環境における商品作成（価格・在庫）とカタログ一覧表示、注文一覧・ステータスフィルター画面の表示・更新検証。
  - **モバイルレスポンシブ (`responsive.spec.ts`)**: 375x667 ビューポートでのハンバーガーメニュー開閉・ビュー遷移、エディタ画面の 3 ペイン（記事一覧 / 編集 / プレビュー）タブ切り替えの動作検証。
  - **品質検証**: `task test:e2e` 全 17 テスト合格、`task check`（fmt・Prettier・Clippy / oxlint ゼロ警告・全 117 Rust テスト・Admin Studio ビルド）パス。
  - **ドキュメント反映**: [docs/development.md 8.1](docs/development.md#81-e2e-テスト-playwrightadmine2e) に全スイートの詳細を記録。
- **その前**: コメント機能とコメントの通知（ブランチ `feat/comments`）
- **実装済みの機能**: 一覧は [README](README.md#23-クレート構成と各モジュールの責務-crates) とリファレンス（README 2.3 末尾のドキュメント一覧）を参照。
- **検証**: `task check`（rustfmt・Prettier・Clippy / oxlint ゼロ警告・全テスト・Admin Studio のビルド）がすべて通る状態を維持する。

### 🚀 次に着手する項目

1. **EC 拡張 Phase 4: 顧客アカウント・売上レポート等の設計・検討**
2. **Storybook ストーリーの拡充・`play` の CI 連携 (`@storybook/addon-vitest`)**


---

## 2. 未着手タスク

### 認証・アカウント

- [ ] **SAML の拡張**（要望があれば）: IdP のメタデータ URL からの設定の読み込み（証明書の自動更新）、暗号化したアサーション（SP の秘密鍵が要る）、AuthnRequest の署名、シングルログアウト、ECDSA の署名。今の対応範囲は [authentication.md 8](docs/authentication.md#8-シングルサインオン-saml-20)。

### Admin Studio

- [ ] **Storybook の `play` を CI で動かす**: `@storybook/addon-vitest` などの導入とコンポーネントストーリーの追加。
- [ ] **翻訳の続き**: サーバーのエラーメッセージ（API がエラーコードを返すようにする。アカウント登録が先行例）、公開サイトの組み込みテンプレート・カートのスクリプト（`Add to cart` など）、翻訳サービス（Crowdin など）の設定と 3 言語目。


### プラグイン

- [ ] **ストアからのインストール**: パッケージの検証（テーマの `validate_theme_package` に相当）・署名・`min_aegis_version` の確認、展開先は `plugins/<name>/`。テーマの ZIP 導入も同じ仕組みで。
- [ ] **AssemblyScript / TinyGo SDK の追従**: EC のフック・ホスト API（`calculate-order` / `on-order-status-changed` / `http_fetch` / `kv_get` / `config_get`）と `send-notification`、`on-content-published`、`check-comment` のラッパー。ABI は [plugin_abi_specification.md](docs/plugin_abi_specification.md) に記載済み。
- [ ] **AssemblyScript SDK の JSON 処理の高速化**: `escapeJson` / `findStringField` が入力の大きさの二乗で遅い（[architecture.md 12](docs/architecture.md#12-性能の記録)）。

### 通知

- [ ] **通知の追加**: HTML メール、送信待ちの永続化（再起動で失われないキュー）、Admin Studio からのテスト送信・テンプレート編集。

### EC

- [x] **EC 拡張 Phase 2: 商品のバリエーション**: `_aegis_product_variants` テーブル新設、サイズ・色ごとの在庫・価格、アトミックな在庫引き当て（`StockReservation` 拡張）、カート・注文明細対応、Admin Studio バリエーション管理。
- [x] **EC 拡張 Phase 3: 複数通貨**: `supported_currencies` 設定、為替レート換算および通貨別価格、最小通貨単位（Zero-decimal / Cent）の正規化、多通貨 Stripe Checkout 連携、テーマ通貨セレクター。
- [ ] **フェーズ 4**: 顧客アカウント（会員登録・注文履歴）、売上レポート、在庫切れの通知、On-demand ISR との連動。
- [ ] **商品レビュー**（顧客アカウントと一緒に作る。方針は「技術メモ」の「商品レビューの方針」）。
- [ ] **返金時の在庫の二重戻しの確認**: `orders_integration.rs` の返金のテスト（#79 で期待値を 5 → 7 に変更）では、取り消しで在庫を戻した注文を返金すると同じ商品がさらに戻る（取り消し後 5 → 返金後 7）。取り消し済みの注文の返金では在庫を戻さないのが正しいかを確かめる（`record_order_refund` の `restock`）。

### コメント

- [ ] **コメントの拡張**（要望があれば）: コンテンツごとのコメントの受付の切り替え（スキーマのフィールド）、Admin Studio からの本文の編集、アバター（Gravatar など）、承認時に投稿者へ知らせる通知、公開サイトのコメント欄の翻訳（組み込みテンプレートは英語のみ）、同じ内容の連投の検出。

### 将来のロードマップ

- [ ] **分散ストレージモード**: libSQL (Turso) への対応とクラスタリング環境の最適化。
- [ ] **On-demand ISR / Cache-Tag Purge**: コンテンツ更新時の Cloudflare / CDN エッジキャッシュの無効化・タグパージ。

---

## 3. 技術メモ・申し送り事項 (Memo & Notes)

作業中の判断や、まだ文書にしていない注意点をここに書きます。機能が完了したら、該当する文書（設計判断は [docs/architecture.md](docs/architecture.md)）へ移して消してください。

- **商品レビューの方針**（コメント機能のときに決定。顧客アカウントの追加と一緒に作るまで保留）:
  - レビューはコメントとは別の機能・別の表（`_aegis_reviews`）にする。レビューには評価（★1〜5）の集計（商品ページ・JSON-LD の `AggregateRating`）、購入者だけ・1 注文 1 件（`UNIQUE(product_id, order_id)`）の制約、返信は運営者の 1 段だけ、通知先は運営者、という違いがあるため。
  - 審査の仕組みはコメントと共有する: `aegis_comments::moderation`（承認制の設定は `_aegis_settings` の `reviews.*`、IP ごとの投稿数はキーの名前空間 `reviews`）、状態 `aegis_core::ModerationStatus`、通知の流れ（`review.*`）。Admin Studio もコメントと同じ一覧・一括操作の作りにする。
  - 購入の確認: 顧客アカウントで注文履歴から書く。アカウントが無い購入者には、発送・完了の後に注文確認と同じトークン方式の「レビューを書く」リンクをメールで送る方法もある。
  - 運用の注意: ステマ規制（2023 年 10 月施行）。低評価だけを選んで非承認にしない（表示が実態とずれる）。非承認の基準を残す。
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
- **SEO & Bot 対策機能設計**:
  - `_aegis_settings` に `seo.*` を永続化し、`AppState.seo`（`RwLock`）でサーバー再起動なしに即時反映。
  - テーマのベーステンプレートの `{{ extra_head | safe }}` に `<link rel="canonical">`、`<meta name="description">`、`<meta property="og:...">`、`<meta name="twitter:...">`、および `noindex, nofollow` を自動注入することで、既存テーマ・新規テーマを問わず改修ゼロで完全な SEO / OGP 互換性を担保。
  - `GET /robots.txt` は `Host` ヘッダーから動的に Sitemap の完全 URL（`http(s)://.../sitemap.xml`）を追記出力し、未設定時も安全なデフォルト（`/admin/`, `/api/` を遮断）を即座に提供。
  - Admin Studio の SEO 設定画面では「標準設定」「開発環境（全ブロック）」「生成AIクローラー拒否」のワンクリックプリセットを用意し、手動入力ミスを防止。
- **次のステップ**:
  - SEO & Bot 対策完了後の Pull Request (PR) 作成（ブランチ: `feat/seo-and-bot-controls` -> `main`）。

