# AegisCMS TODO

> [!NOTE]
> 作業の再開時はここを読む。各作業の完了時に更新する。
> 完了したタスクは残さない。仕様は `docs/*.md`・`admin/README.md`、設計判断・注意点は [docs/architecture.ja.md](docs/architecture.ja.md)、開発手順は [docs/development.ja.md](docs/development.ja.md) に書いてから消す。
> **技術メモ・申し送り事項は変更ログ（CHANGELOG）を書く場所ではない**。単純な実装ログや変更履歴は記載せず、未解決の設計課題や既知の注意点のみを記録する。経緯は Git のコミットログや PR で追う。

---

## 直近の着手タスク

（現在なし）

---

## 技術メモ・申し送り事項

- 公式プラグインおよび `sdk/templates/*`・`sdk/examples/*`（`sdk/tinygo/examples/hooks` を含む）の `plugin.wasm` はリポジトリにコミット済みで、バージョンや SDK を変えても自動では再ビルドされない（`version` は manifest にだけあり、Wasm には入らない）。ソースや SDK を変えたときは各自再生成してから出す。
- `admin/package.json` の `overrides`（`glob` → `minimatch ^11.0.5`）は、Dependabot の minimatch の警告（11.0.1 未満が対象）を避けるための一時的なもの。`@storybook/react-vite` が固定する `@joshwooding/vite-plugin-react-docgen-typescript@0.7` が `glob@13`（`minimatch ^10`）に依存するため。react-vite が新しい docgen プラグイン（`glob` に依存しない 0.9 系）に上がったら外す。`npm audit` に残る `glob` の警告（moderate）は、修正が react-vite のメジャーダウングレードになるため対応しない。

---

## 未着手タスク (Backlog)

### 🚚 WordPress からの移行

WXR（「ツール > エクスポート」の XML）を CLI で取り込む。完全なワンボタンは目指さず、「だいたい移せて、移せなかったものが報告で分かる」を目標にする。残りは次の 2 本。

- [ ] **PR 2b: コメントと旧 URL のリダイレクト**（`aegis-server import wordpress` は投稿・固定ページ・メディア・著者・カテゴリ・タグまで実装済み。[docs/wordpress-migration.ja.md](docs/wordpress-migration.ja.md)）。
    - コメント: 公開用の投稿 API では承認状態・日時・返信の親子を指定できない。admin 専用の取り込み用 API（`POST /api/v1/import/comments`、監査ログつき）を足し、`aegis_wordpress::WpComment` を流し込む。取り込み後は、取り込みコマンドの「コメントは未対応」の報告を外す。
    - リダイレクト: 上限が 2,000 件で、投稿の多いサイトでは足りない。旧パーマリンク（`/2024/05/slug/`・`/?p=123`）をパターンで受ける規則を足す。
    - 取り込みは admin のログインで行う（API トークンは `editor` までで、ユーザーの作成と過去の公開日の指定ができないため）。2 段階認証のユーザーは使えない。必要なら、トークンに取り込み用の許可を足すか、CLI で 2FA コードを受ける。
- [ ] **PR 3: スターターテーマ + 対応表 + 移行ガイド**: WordPress の標準のクラス名（`entry-content`・`post`・`widget` など）で出力するスターターテーマと、`theme import-wordpress`（移行元テーマの `style.css` とアセットを取り込む）。対応表は、テンプレート階層（`single.php` → `article.html` など）と関数 → 変数。ガイドは `docs/wordpress-migration.md`（日本語・英語）。
- 割り切り: PHP のテーマ・プラグインは変換できない。ショートコード・ページビルダー・WooCommerce・フォームのプラグインは、報告に出して手で直す。画像の説明（キャプション）は、画像ブロックが持てないため斜体の段落にする。段落内の改行・`<sup>`・入れ子のリストは、`core/html` ブロックに残す。

### 🔌 プラグイン・将来構想

GitHub の範囲を超えるブロックはすべてプラグインで作る（[architecture.md 3.1a](docs/architecture.ja.md#31a-core-ブロックの範囲github-でできることが上限)）。上から順に着手する。

- [ ] **会員の購入・サブスクの実機確認**: 設計は [members.md 16](docs/members.ja.md#16-購入記事の単独購入サブスク)。実装（記事の単独購入・サブスク・管理画面・マイページ）は済で、Stripe はモックでしか確かめていない。Stripe のテストモードで、決済ページ・Webhook（`checkout.session.completed` / `customer.subscription.*` / `charge.refunded`）・Customer Portal・返金を一度通す。新しい API バージョンで契約の期間の終わりが `items` に移っている点（`current_period_end`）は両対応にしてあるが、実物の応答では未確認。
- [ ] **official-clarity の実機確認**: 単体テストと Wasm の実行で、起動コードが同意を待つ形で入り、録画しないページに入らないことは確かめた。実際の Clarity での動作（CSP のホストで読み込み・送信がブロックされないか、`scripts.clarity.ms` が要るか）は、プロジェクト ID を入れたサイトで確かめていない。
- [ ] **リンク埋め込みの定期更新（検討。要望が出てから）**: `official-link-embed` は「再取得」ボタンと URL の書き換えでだけ取り直す（Wasm に時計が無く、KV に有効期限も無いため、期限切れの自動判定はできない）。ホスト側の定期ジョブが要るかは、要望を見て決める。
