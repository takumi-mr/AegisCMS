# AegisCMS 開発進捗 & TODOリスト (TODO.md)

> [!NOTE]
> 作業を再開する際は、まず本ドキュメントの「現在の状況」と「未着手タスク」を確認してください。
> **ルール**: 各修正・実装ステップの終了後には、必ず本ドキュメントを更新してください。
> **完了したタスクはここに残さず**、仕様はリファレンス（`docs/*.md`・`admin/README.md`）、設計判断・注意点は [docs/architecture.md](docs/architecture.md)、開発手順は [docs/development.md](docs/development.md) に書き込んでから消します。経緯は git の履歴（PR）で追えます。

---

## 1. 現在の状況

- **直近の完了**: Admin Studio のモバイル・レスポンシブ対応（狭小端末での UI 崩れ解消・ペイン切り替え化）。
  - **ヘッダー**: 画面幅 900px 以下でコンパクト化。右上のハンバーガーメニューに各画面タブ、接続状態、言語切替、アカウント情報・プロフィール・ログアウトを統合。エディタ時の保存ボタンはヘッダー右側に維持しつつラベルを短縮。
  - **エディタ**: デスクトップの 3 列固定から、モバイルではサブタブによるペイン切り替え（記事一覧 / 編集 / プレビュー）に動的変更。一覧から記事を選択（または新規作成）した際は自動的に編集ペインへ遷移する直感的な導線を実装。
  - **各画面・コンポーネント**: ビューポート高さを動的アドレスバーに対応する `height: 100dvh` に改善。入力行（`FieldRow`）の折り返し、カレンダー（`ScheduleCalendar`）やテーブルの横スクロール保護、設定・商品・ユーザー・監査ログ・モーダル・ログイン画面の狭小画面向けレスポンシブ調整を実施。
  - **多言語 & 品質検証**: 日英ロケールキー追加、`task check`（rustfmt・Prettier・Clippy / oxlint ゼロ警告・全テスト・Admin Studio ビルド）パス。
  - **ドキュメント反映**: 仕様・設計は [admin/README.md](../admin/README.md#-設計メモ) および [architecture.md 11](docs/architecture.md#11-admin-studio-の配信)。
- **その前**: プラグインの有効・無効を Admin Studio から再起動なしで切り替え（`PUT /api/v1/plugins/{name}/enabled`、一覧は DB の `_aegis_settings` に保存して `[plugins].enabled` より優先、監査ログ `plugin.enable` / `plugin.disable`。[plugin_hooks.md 5.1](docs/plugin_hooks.md#51-admin-studio-からの有効無効の切り替え)、[architecture.md 10.2](docs/architecture.md#102-インストールと有効化-wasmpluginloader)）。その前は PostgreSQL / MySQL での注文・在庫の実機検証。
- **実装済みの機能**: 一覧は [README](README.md#23-クレート構成と各モジュールの責務-crates) とリファレンス（README 2.3 末尾のドキュメント一覧）を参照。
- **検証**: `task check`（rustfmt・Prettier・Clippy / oxlint ゼロ警告・全テスト・Admin Studio のビルド）がすべて通る状態を維持する。

### 🚀 次に着手する項目

1. **翻訳の続き**: サーバーのエラーメッセージ（API がエラーコードを返すようにする。アカウント登録が先行例）、公開サイトの組み込みテンプレート・カートのスクリプト（`Add to cart` など）、翻訳サービス（Crowdin など）の設定と 3 言語目。

---

## 2. 未着手タスク

### 認証・アカウント

- [ ] **SAML の拡張**（要望があれば）: IdP のメタデータ URL からの設定の読み込み（証明書の自動更新）、暗号化したアサーション（SP の秘密鍵が要る）、AuthnRequest の署名、シングルログアウト、ECDSA の署名。今の対応範囲は [authentication.md 8](docs/authentication.md#8-シングルサインオン-saml-20)。

### Admin Studio

- [ ] **翻訳の続き**: サーバーのエラーメッセージ（API がエラーコードを返すようにする。アカウント登録が先行例）、公開サイトの組み込みテンプレート・カートのスクリプト（`Add to cart` など）、翻訳サービス（Crowdin など）の設定と 3 言語目。

### プラグイン

- [ ] **実行順の並べ替え**: Admin Studio で有効にしたプラグインはフックの実行順の最後に加わるだけ。順序を変えるには設定ファイルを直して DB の `plugins.enabled` を消す必要がある。並べ替え UI と、設定ファイルの値に戻す操作。
- [ ] **複数台での切り替えの伝播**: 有効・無効の切り替えは操作を受けたサーバーだけに反映される（他は再起動で揃う）。キャッシュ（Redis）の Pub/Sub などで他のサーバーにも差し替えを伝える。
- [ ] **ストアからのインストール**: パッケージの検証（テーマの `validate_theme_package` に相当）・署名・`min_aegis_version` の確認、展開先は `plugins/<name>/`。テーマの ZIP 導入も同じ仕組みで。
- [ ] **AssemblyScript / TinyGo SDK の追従**: EC のフック・ホスト API（`calculate-order` / `on-order-status-changed` / `http_fetch` / `kv_get` / `config_get`）と `send-notification`、`on-content-published` のラッパー。ABI は [plugin_abi_specification.md](docs/plugin_abi_specification.md) に記載済み。
- [ ] **AssemblyScript SDK の JSON 処理の高速化**: `escapeJson` / `findStringField` が入力の大きさの二乗で遅い（[architecture.md 12](docs/architecture.md#12-性能の記録)）。

### 通知

- [ ] **通知の追加**: HTML メール、送信待ちの永続化（再起動で失われないキュー）、Admin Studio からのテスト送信・テンプレート編集。
- [ ] **EC 以外の通知**: コメント、公開予約の完了など。

### EC

- [ ] **フェーズ 4**: 顧客アカウント（会員登録・注文履歴）、売上レポート、在庫切れの通知、On-demand ISR との連動。
- [ ] **商品のバリエーション**（サイズ・色ごとの在庫・価格）、返金 API（Stripe Refund）、複数通貨。

### 将来のロードマップ

- [ ] **分散ストレージモード**: libSQL (Turso) への対応とクラスタリング環境の最適化。
- [ ] **On-demand ISR / Cache-Tag Purge**: コンテンツ更新時の Cloudflare / CDN エッジキャッシュの無効化・タグパージ。

---

## 3. 技術メモ・申し送り事項 (Memo & Notes)

作業中の判断や、まだ文書にしていない注意点をここに書きます。機能が完了したら、該当する文書（設計判断は [docs/architecture.md](docs/architecture.md)）へ移して消してください。

- （なし）
