# AegisCMS 開発進捗 & TODOリスト (TODO.md)

> [!NOTE]
> 作業を再開する際は、まず本ドキュメントの「現在の状況」と「未着手タスク」を確認してください。
> **ルール**: 各修正・実装ステップの終了後には、必ず本ドキュメントを更新してください。
> **完了したタスクはここに残さず**、仕様はリファレンス（`docs/*.md`・`admin/README.md`）、設計判断・注意点は [docs/architecture.md](docs/architecture.md)、開発手順は [docs/development.md](docs/development.md) に書き込んでから消します。経緯は git の履歴（PR）で追えます。

---

## 1. 現在の状況

- **直近の完了**: パスワードの再設定・承認の知らせ・ユーザー招待リンク機能の実装。
  1. **パスワード再設定（メール）**: キャッシュ + SHA-256 ハッシュ + URL フラグメント（`#reset-password=<token>`）。アカウント列挙防止（常に `202 Accepted`、Argon2id 擬似ハッシュ処理）、ログイン画面の「パスワードをお忘れですか？」および再設定フォーム、通知 `account.password_reset`。仕様は [authentication.md 9](docs/authentication.md#9-パスワードの再設定)、設計は [architecture.md 6.7](docs/architecture.md#67-パスワードの再設定-aegis_authpassword_reset-handlerspassword_resetrs)。
  2. **承認の知らせ**: `_aegis_users` に `pending_approval` カラム追加（SQLite / PostgreSQL / MySQL マイグレーション対応）、自己登録時の承認待ちフラグ設定、admin がユーザーを有効化（`disabled: false`）した際の自動クリアおよび承認通知メール `account.approved` 送信、Admin Studio での「承認待ち」バッジと承認ボタン。仕様は [authentication.md 11](docs/authentication.md#11-承認通知-accountapproved)、設計は [architecture.md 6.6](docs/architecture.md#66-アカウント登録-aegis_authregistration-handlersregistrationrs)。
  3. **ユーザー招待リンク**: admin によるメールアドレス・ロール指定での招待発行（`POST /api/v1/auth/invitations`）、キャッシュ + SHA-256 + URL フラグメント（`#invite=<token>`）、通知 `account.invite`、招待受諾フォーム（`GET /api/v1/auth/invitations/info`、`POST /api/v1/auth/invitations/accept`）でのユーザー名・パスワード設定、即時有効アカウント開設。仕様は [authentication.md 10](docs/authentication.md#10-ユーザー招待リンク)、設計は [architecture.md 6.8](docs/architecture.md#68-ユーザー招待リンク-aegis_authinvitation-handlersinvitationrs)。その前は予約公開時のプラグインフック呼び出し（`on-content-published` フック）。
- **実装済みの機能**: 一覧は [README](README.md#23-クレート構成と各モジュールの責務-crates) とリファレンス（README 2.3 末尾のドキュメント一覧）を参照。
- **検証**: `task check`（rustfmt・Prettier・Clippy / oxlint ゼロ警告・全テスト・Admin Studio のビルド）がすべて通る状態を維持する。

### 🚀 次に着手する項目

1. **Admin Studio のモバイル対応**: 狭い画面での一覧・プレビュー・ヘッダーのレスポンシブ化。

---

## 2. 未着手タスク

### 認証・アカウント

- [ ] **SAML の拡張**（要望があれば）: IdP のメタデータ URL からの設定の読み込み（証明書の自動更新）、暗号化したアサーション（SP の秘密鍵が要る）、AuthnRequest の署名、シングルログアウト、ECDSA の署名。今の対応範囲は [authentication.md 8](docs/authentication.md#8-シングルサインオン-saml-20)。

### Admin Studio

- [ ] **モバイル対応**: エディタは一覧 300px・プレビュー 480px 固定の 3 列で、スマホでは本文がほぼ見えない。ヘッダーは `nowrap` / `flex-shrink: 0` ではみ出し、`.app` が `height: 100vh; overflow: hidden` のため切れる。狭い画面では一覧・プレビューをドロワーやタブに、ヘッダーをメニューにまとめる。
- [ ] **メディアライブラリでのアバターの扱い**: `avatars/` もメディア一覧に出て、そこから削除するとアバターが外れる。一覧から除外するか、使用中チェックにユーザーを含める。
- [ ] **翻訳の続き**: サーバーのエラーメッセージ（API がエラーコードを返すようにする。アカウント登録が先行例）、公開サイトの組み込みテンプレート・カートのスクリプト（`Add to cart` など）、翻訳サービス（Crowdin など）の設定と 3 言語目。

### プラグイン

- [ ] **実行時の有効・無効の切り替え**: 今は設定ファイル + 再起動。API で切り替えるには、有効化リストの永続化（DB）と `PluginSupervisor` の差し替えが要る。
- [ ] **ストアからのインストール**: パッケージの検証（テーマの `validate_theme_package` に相当）・署名・`min_aegis_version` の確認、展開先は `plugins/<name>/`。テーマの ZIP 導入も同じ仕組みで。
- [ ] **AssemblyScript / TinyGo SDK の追従**: EC のフック・ホスト API（`calculate-order` / `on-order-status-changed` / `http_fetch` / `kv_get` / `config_get`）と `send-notification`、`on-content-published` のラッパー。ABI は [plugin_abi_specification.md](docs/plugin_abi_specification.md) に記載済み。
- [ ] **AssemblyScript SDK の JSON 処理の高速化**: `escapeJson` / `findStringField` が入力の大きさの二乗で遅い（[architecture.md 12](docs/architecture.md#12-性能の記録)）。

### 通知

- [ ] **通知の追加**: HTML メール、送信待ちの永続化（再起動で失われないキュー）、Admin Studio からのテスト送信・テンプレート編集。
- [ ] **EC 以外の通知**: コメント、公開予約の完了など。

### EC

- [ ] **フェーズ 4**: 顧客アカウント（会員登録・注文履歴）、売上レポート、在庫切れの通知、On-demand ISR との連動。
- [ ] **商品のバリエーション**（サイズ・色ごとの在庫・価格）、返金 API（Stripe Refund）、複数通貨。
- [ ] **PostgreSQL / MySQL での注文・在庫の実機検証**（テストは SQLite のみ）。

### 将来のロードマップ

- [ ] **分散ストレージモード**: libSQL (Turso) への対応とクラスタリング環境の最適化。
- [ ] **On-demand ISR / Cache-Tag Purge**: コンテンツ更新時の Cloudflare / CDN エッジキャッシュの無効化・タグパージ。

---

## 3. 技術メモ・申し送り事項 (Memo & Notes)

作業中の判断や、まだ文書にしていない注意点をここに書きます。機能が完了したら、該当する文書（設計判断は [docs/architecture.md](docs/architecture.md)）へ移して消してください。

- （なし）
