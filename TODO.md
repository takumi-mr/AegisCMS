# AegisCMS TODO

> [!NOTE]
> 作業の再開時はここを読む。各作業の完了時に更新する。
> 完了したタスクは残さない。仕様は `docs/*.md`・`admin/README.md`、設計判断・注意点は [docs/architecture.md](docs/architecture.md)、開発手順は [docs/development.md](docs/development.md) に書いてから消す。
> **技術メモ・申し送り事項は変更ログ（CHANGELOG）を書く場所ではない**。単純な実装ログや変更履歴は記載せず、未解決の設計課題や既知の注意点のみを記録する。経緯は Git のコミットログや PR で追う。

---

## 現在の状況

- コアエンジン（Rust）、Admin Studio（React/TypeScript）、Wasm プラグインランタイム、テーマエンジン、二段階認証（TOTP + バックアップコード 6 枚）、動的運用設定（`system.*`）、一括バックアップ＆リストア機能が安定稼働。全検証（`task check`）を通過。

---

## 直近の着手タスク

1. **AssemblyScript / TinyGo SDK の機能追従**:
   - Rust SDK に追加済みのフック（`calculate-order`, `on-order-status-changed`, `send-notification`, `on-content-published`, `check-comment`, `render-blocks`）およびホスト API（`http_fetch`, `kv_get`, `config_get`）のラッパーを整備（ABI は [plugin_abi_specification.md](docs/plugin_abi_specification.md) 参照）。

---

## 技術メモ・申し送り事項

- 現在、未解決の申し送り事項はありません。

---

## 未着手タスク (Backlog)

### 🚀 性能最適化
- [ ] **監査ログ `database` 出力先の非同期化（保留）**: 有限 `mpsc` キュー + バッチ INSERT。WAL モード導入により現状十分なスループットが出ているため、終了時フラッシュ仕様決定後に再検討。
- [ ] **大規模コンテンツ（1 万件超）での最適化**:
  - `on-render-page` フックで HTML 全体を Wasm に渡すオーバーヘッドの軽減。
  - コンテンツ一覧のキーセットページネーション対応（掲示板は対応済み）。

### 🌐 サイト機能拡張
- [ ] **タグの検索・一覧への対応**: タグの基盤（[docs/tags.md](docs/tags.md)）と公開サイト（`/tags`・`/tags/{slug}`・フィード・サイトマップ・テーマのテンプレート変数）は対応済み。残りは、サイト内検索のタグでの絞り込み（索引に `tag` を持たせ、`?tag=` を足す）と、Admin Studio の一覧のタグでの絞り込み（`GET /api/v1/contents/{type}?tag=`）。
- [ ] **問い合わせフォームの拡張**: ファイル添付、Admin Studio からの直接返信、フォーム編集 UI、Cloudflare Turnstile / CAPTCHA 連携、受信箱の全文検索。
- [ ] **サイト内検索の拡張**: 検索キーワードのアクセス解析集計。
- [ ] **アクセス解析の拡張**: 前期間との比較表示、国・ブラウザ別集計、UTM パラメータ解析、カスタムイベントトラッキング、CSV エクスポート。

### 🎨 テーマ & Theme Builder
- [ ] **同梱テーマの固定文言の多言語化**: 同梱テーマ 5 種のナビゲーションなどデザイン固有の文言（`<html lang>` と組み込みテンプレートは `[theme].locale` に対応済み。方針は [architecture.md 19](docs/architecture.md#19-多言語対応の方針)）。テーマ開発サーバー（`aegis-server theme dev`）にも言語の指定を足す。
- [ ] **Theme Builder の機能拡張**: ファイルの改名・削除、`theme.json` のフォーム編集 UI、ZIP パッケージ書き出しと署名、ブロック単位のプレビュー、プレビュー要素からテンプレート該当行へのジャンプ。
- [ ] **テーマ開発サーバーの機能拡張**: 実サイトの公開 API から記事を取得して描画する `--content-from <URL>` オプション。

### 🔐 認証・アカウント
- [ ] **SAML 2.0 の拡張**: IdP メタデータ URL からの自動読み込み、暗号化アサーション対応、AuthnRequest への署名付与、シングルログアウト (SLO)、ECDSA 対応（[authentication.md 8](docs/authentication.md#8-シングルサインオン-saml-20) 参照）。

### 🔌 プラグイン・将来構想
- [ ] **広告配信プラグイン**: `on-render-page` によるヘッダー注入とカスタムブロック表示、動的 CSP 設定との連携、Cookie 同意バナー対応。
- [ ] **テンプレートからの動的コンテンツ取得関数（検討）**: `latest("article", 5, category="news")` などの MiniJinja グローバル関数または事前取得宣言機構。
- [ ] **分散ストレージ・クラスタ対応**: libSQL (Turso) への対応およびマルチノードクラスタ環境の最適化。
