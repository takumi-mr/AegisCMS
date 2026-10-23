# AegisCMS 開発進捗 & TODOリスト (TODO.md)

> [!NOTE]
> 作業を再開する際は、まず本ドキュメントの「現在の状況」と「未着手タスク」を確認してください。
> **ルール**: 各修正・実装ステップの終了後には、必ず本ドキュメントを更新してください。
> **完了したタスクはここに残さず**、仕様はリファレンス（`docs/*.md`・`admin/README.md`）、設計判断・注意点は [docs/architecture.md](docs/architecture.md)、開発手順は [docs/development.md](docs/development.md) に書き込んでから消します。経緯は git の履歴（PR）で追えます。

---

## 1. 現在の状況

- **直近の完了**: プラグイン一覧画面の実装と動的設定機能（Admin Studio での閲覧・検索・権限表示・設定モーダル、API `GET/PUT /api/v1/plugins/{name}/settings`、キャッシュ永続化とホストサービス連携、設計は [architecture.md 10.3](docs/architecture.md#103-ホスト-api-hostservices)）。その前はロール変更・無効化・削除・パスワード変更の発行済みセッションへの即時反映。
- **実装済みの機能**: 一覧は [README](README.md#23-クレート構成と各モジュールの責務-crates) とリファレンス（README 2.3 末尾のドキュメント一覧）を参照。
- **検証**: `task check`（rustfmt・Prettier・Clippy / oxlint ゼロ警告・全テスト・Admin Studio のビルド）がすべて通る状態を維持する。

### 🚀 次に着手する項目

1. **Themed Page プレビューの iframe の権限を見直す**（下記「セキュリティ」。小さく、影響が大きい）。
2. **パスワードの再設定（メール）**: アカウント登録と同じ仕組みで作れる。

---

## 2. 未着手タスク

### セキュリティ

- [ ] **Themed Page プレビューの `sandbox`**: `Preview.tsx` の `iframe` は `srcdoc` に `sandbox="allow-scripts allow-same-origin"` を付けている（`official-code-enhancer` の Mermaid 描画のために緩めた）。`srcdoc` の文書は Admin Studio と同じオリジンになるため、テーマやプラグインが出力したスクリプトから `sessionStorage` のセッショントークンを読める。`allow-same-origin` を外す（スクリプトだけ許す）か、プレビューを別オリジンで配信する。コード内のコメント（"no scripts"）も合わせて直す。

### 認証・アカウント

- [ ] **パスワードの再設定（メール）**: アカウント登録と同じ仕組み（キャッシュ + ハッシュしたトークン + フラグメントのリンク）。ログイン画面の「パスワードを忘れた」。
- [ ] **承認の知らせ**: admin がアカウントを有効にしたときに本人へメールする。今は無効化の解除と承認を区別していないので、承認待ちを表すカラムが要る。
- [ ] **招待リンク**: admin がメールアドレスとロールを指定して招待し、本人がパスワードを決める（`require_approval` の代わりに使える）。
- [ ] **SAML の拡張**（要望があれば）: IdP のメタデータ URL からの設定の読み込み（証明書の自動更新）、暗号化したアサーション（SP の秘密鍵が要る）、AuthnRequest の署名、シングルログアウト、ECDSA の署名。今の対応範囲は [authentication.md 8](docs/authentication.md#8-シングルサインオン-saml-20)。

### コンテンツ・公開

- [ ] **公開予定の一覧・カレンダー**: Admin Studio で予約中のコンテンツを日時順に見る画面（今は一覧のバッジとツールチップのみ）。
- [ ] **予約での公開終了（非公開化）**: `unpublish_at` で期間限定の公開（キャンペーンページなど）。
- [ ] **公開時のフック・通知**: 予約公開した時点でプラグインフック（例: `on-content-published`）を呼ぶ。通知は `[[notifications.audit.rules]]` の `content.publish` ですでに送れる。

### Admin Studio

- [ ] **モバイル対応**: エディタは一覧 300px・プレビュー 480px 固定の 3 列で、スマホでは本文がほぼ見えない。ヘッダーは `nowrap` / `flex-shrink: 0` ではみ出し、`.app` が `height: 100vh; overflow: hidden` のため切れる。狭い画面では一覧・プレビューをドロワーやタブに、ヘッダーをメニューにまとめる。
- [ ] **メディアライブラリでのアバターの扱い**: `avatars/` もメディア一覧に出て、そこから削除するとアバターが外れる。一覧から除外するか、使用中チェックにユーザーを含める。
- [ ] **翻訳の続き**: サーバーのエラーメッセージ（API がエラーコードを返すようにする。アカウント登録が先行例）、公開サイトの組み込みテンプレート・カートのスクリプト（`Add to cart` など）、翻訳サービス（Crowdin など）の設定と 3 言語目。

### プラグイン

- [ ] **実行時の有効・無効の切り替え**: 今は設定ファイル + 再起動。API で切り替えるには、有効化リストの永続化（DB）と `PluginSupervisor` の差し替えが要る。
- [ ] **ストアからのインストール**: パッケージの検証（テーマの `validate_theme_package` に相当）・署名・`min_aegis_version` の確認、展開先は `plugins/<name>/`。テーマの ZIP 導入も同じ仕組みで。
- [ ] **AssemblyScript / TinyGo SDK の追従**: EC のフック・ホスト API（`calculate-order` / `on-order-status-changed` / `http_fetch` / `kv_get` / `config_get`）と `send-notification` のラッパー。ABI は [plugin_abi_specification.md](docs/plugin_abi_specification.md) に記載済み。
- [ ] **AssemblyScript SDK の JSON 処理の高速化**: `escapeJson` / `findStringField` が入力の大きさの二乗で遅い（[architecture.md 12](docs/architecture.md#12-性能の記録)）。

### 通知

- [ ] **通知の追加**: HTML メール、送信待ちの永続化（再起動で失われないキュー）、Admin Studio からのテスト送信・テンプレート編集。
- [ ] **EC 以外の通知**: パスワードの再設定、コメント、公開予約の完了など。

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
