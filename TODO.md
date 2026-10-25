# AegisCMS 開発進捗 & TODOリスト (TODO.md)

> [!NOTE]
> 作業を再開する際は、まず本ドキュメントの「現在の状況」と「未着手タスク」を確認してください。
> **ルール**: 各修正・実装ステップの終了後には、必ず本ドキュメントを更新してください。
> **完了したタスクはここに残さず**、仕様はリファレンス（`docs/*.md`・`admin/README.md`）、設計判断・注意点は [docs/architecture.md](docs/architecture.md)、開発手順は [docs/development.md](docs/development.md) に書き込んでから消します。経緯は git の履歴（PR）で追えます。

---

## 1. 現在の状況

- **直近の完了**: SEO & Bot 対策機能（robots.txt / sitemap.xml / メタタグ / OGP / Admin Studio 設定画面）
  - **SEO & Bot 設定機構**: `_aegis_settings` テーブルに `seo.*`（`robots_txt`, `meta_description`, `meta_keywords`, `canonical_base_url`, `og_image`, `noindex_all`, `twitter_card`）を永続化し、`AppState.seo`（`Arc<RwLock<SeoOverrides>>`）で管理。サーバー再起動不要で即時反映。
  - **HTML `<head>` 自動注入**: テーマの規約である `{{ extra_head | safe }}` を通じて、SEO メタタグ・OGP・Twitter Card・Canonical タグ・noindex 指示を自動挿入（`render::build_extra_head`）。個別記事の `excerpt` や商品サムネイルを優先し、サイト全体設定にフォールバック。既存テーマの改修不要。
  - **`GET /robots.txt` & `GET /sitemap.xml`**: カスタム robots.txt の配信（未設定時は安全な既定値）および動的 Sitemap 参照の自動注入。公開コンテンツ（記事・固定ページ・商品）を DB から動的集約した Sitemap 0.9 準拠 XML 配信。
  - **管理 API**: `GET /api/v1/seo`, `PUT /api/v1/seo` を実装し、変更時は監査ログ `seo.update` を記録。
  - **Admin Studio (Settings > SEO & Bot)**: インデックス制御（noindex 全体適用トグル＆注意喚起バナー）、Canonical Base URL、Meta Description/Keywords、OGP 画像設定（メディアライブラリ・アップローダー連携）、Twitter Card 種別、robots.txt プリセット（標準、開発環境全ブロック、生成AIクローラー拒否）挿入ツールバー付きエディタ、ライブ URL リンクを完備。
  - **品質検証**: `task check`（fmt・Prettier・Clippy / oxlint ゼロ警告・全テスト・Admin Studio ビルド）パス。
  - **ドキュメント反映**: 仕様・設定は [docs/configuration.md 5](docs/configuration.md#5-管理画面から設定する動的サイトseo設定)、アーキテクチャ・設計判断は [docs/architecture.md 3.5](docs/architecture.md#35-seo--bot-制御-aegis-serversrcseors-handlersseors)。
- **その前**: EC 拡張 Phase 1: 外部決済抽象化層（`PaymentProvider`）と返金 API（Stripe Refund / 手動返金）
- **実装済みの機能**: 一覧は [README](README.md#23-クレート構成と各モジュールの責務-crates) とリファレンス（README 2.3 末尾のドキュメント一覧）を参照。
- **検証**: `task check`（rustfmt・Prettier・Clippy / oxlint ゼロ警告・全テスト・Admin Studio のビルド）がすべて通る状態を維持する。

### 🚀 次に着手する項目

1. **EC 拡張 Phase 4: 顧客アカウント・売上レポート等の設計・検討**
2. **Admin Studio E2E テスト・Storybook ストーリーの拡充**


---

## 2. 未着手タスク

### 認証・アカウント

- [ ] **SAML の拡張**（要望があれば）: IdP のメタデータ URL からの設定の読み込み（証明書の自動更新）、暗号化したアサーション（SP の秘密鍵が要る）、AuthnRequest の署名、シングルログアウト、ECDSA の署名。今の対応範囲は [authentication.md 8](docs/authentication.md#8-シングルサインオン-saml-20)。

### Admin Studio

- [ ] **E2E テスト・ストーリーの拡充**: 今はプラグイン画面だけ。ログイン（失敗・ロック）、記事の作成・保存・公開、テーマの切り替え、ユーザー管理・招待、EC（商品・注文）、狭い画面（モバイル対応の確認）。Storybook の `play` を CI で動かす（`@storybook/addon-vitest` など）。
- [ ] **翻訳の続き**: サーバーのエラーメッセージ（API がエラーコードを返すようにする。アカウント登録が先行例）、公開サイトの組み込みテンプレート・カートのスクリプト（`Add to cart` など）、翻訳サービス（Crowdin など）の設定と 3 言語目。

### プラグイン

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
- **SEO & Bot 対策機能設計**:
  - `_aegis_settings` に `seo.*` を永続化し、`AppState.seo`（`RwLock`）でサーバー再起動なしに即時反映。
  - テーマのベーステンプレートの `{{ extra_head | safe }}` に `<link rel="canonical">`、`<meta name="description">`、`<meta property="og:...">`、`<meta name="twitter:...">`、および `noindex, nofollow` を自動注入することで、既存テーマ・新規テーマを問わず改修ゼロで完全な SEO / OGP 互換性を担保。
  - `GET /robots.txt` は `Host` ヘッダーから動的に Sitemap の完全 URL（`http(s)://.../sitemap.xml`）を追記出力し、未設定時も安全なデフォルト（`/admin/`, `/api/` を遮断）を即座に提供。
  - Admin Studio の SEO 設定画面では「標準設定」「開発環境（全ブロック）」「生成AIクローラー拒否」のワンクリックプリセットを用意し、手動入力ミスを防止。
- **次のステップ**:
  - SEO & Bot 対策完了後の Pull Request (PR) 作成（ブランチ: `feat/seo-and-bot-controls` -> `main`）。

