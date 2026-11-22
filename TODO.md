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

WXR（「ツール > エクスポート」の XML）を CLI で取り込む。完全なワンボタンは目指さず、「だいたい移せて、移せなかったものが報告で分かる」を目標にする。3 本の PR に分ける（読み取りと変換は `crates/aegis-wordpress` に実装済みで、DB や API には触れない。以下は残りの 2 本）。

- [ ] **PR 2: `aegis-server import wordpress <file.xml>`**: 取り込みは、サーバーの REST API（API トークン）経由で行う。保存パイプライン（AST 変換・バージョン・公開用テーブル・プラグインのフック）を複製しないため。`--dry-run` で「入るもの・入らないもの」の報告だけ出す。順番は、著者 → カテゴリ・タグ → メディア（取得して `storage` に保存し、本文の URL を書き換える）→ 投稿・固定ページ（古い順に取り込む）→ コメント → リダイレクト。
    - 型の割り当て: `post` → `article`、`page` → `page`、カテゴリ → `category`、タグ → タグ。独自の投稿タイプは、同名のスキーマがあるときだけ取り込む。下書き・非公開・予約は状態を保つ。パスワード付きは取り込まない（報告に出す）。
    - **一覧の並び順は `created_at`（取り込んだ時刻）で決まる**ため、古い投稿から順に取り込む。元の日時は `published_at` に入る。API からは `created_at` を指定できない。
    - **リダイレクトの上限は 2,000 件**。投稿の多いサイトでは足りないため、旧パーマリンク（`/2024/05/slug/`・`/?p=123`）をパターンで受ける仕組みが要る。
    - コメントの取り込みは、公開用の投稿 API では承認状態・日時・返信の親子を指定できない。取り込み用の経路を設計する必要がある。
    - 著者は、ログインできない状態のユーザーとして作り、パスワードの再設定を案内する。
- [ ] **PR 3: スターターテーマ + 対応表 + 移行ガイド**: WordPress の標準のクラス名（`entry-content`・`post`・`widget` など）で出力するスターターテーマと、`theme import-wordpress`（移行元テーマの `style.css` とアセットを取り込む）。対応表は、テンプレート階層（`single.php` → `article.html` など）と関数 → 変数。ガイドは `docs/wordpress-migration.md`（日本語・英語）。
- 割り切り: PHP のテーマ・プラグインは変換できない。ショートコード・ページビルダー・WooCommerce・フォームのプラグインは、報告に出して手で直す。画像の説明（キャプション）は、画像ブロックが持てないため斜体の段落にする。段落内の改行・`<sup>`・入れ子のリストは、`core/html` ブロックに残す。

### 🔌 プラグイン・将来構想

GitHub の範囲を超えるブロックはすべてプラグインで作る（[architecture.md 3.1a](docs/architecture.ja.md#31a-core-ブロックの範囲github-でできることが上限)）。上から順に着手する。

- [ ] **会員の購入・サブスクの実機確認**: 設計は [members.md 16](docs/members.ja.md#16-購入記事の単独購入サブスク)。実装（記事の単独購入・サブスク・管理画面・マイページ）は済で、Stripe はモックでしか確かめていない。Stripe のテストモードで、決済ページ・Webhook（`checkout.session.completed` / `customer.subscription.*` / `charge.refunded`）・Customer Portal・返金を一度通す。新しい API バージョンで契約の期間の終わりが `items` に移っている点（`current_period_end`）は両対応にしてあるが、実物の応答では未確認。
- [ ] **official-clarity の実機確認**: 単体テストと Wasm の実行で、起動コードが同意を待つ形で入り、録画しないページに入らないことは確かめた。実際の Clarity での動作（CSP のホストで読み込み・送信がブロックされないか、`scripts.clarity.ms` が要るか）は、プロジェクト ID を入れたサイトで確かめていない。
- [ ] **リンク埋め込みの定期更新（検討。要望が出てから）**: `official-link-embed` は「再取得」ボタンと URL の書き換えでだけ取り直す（Wasm に時計が無く、KV に有効期限も無いため、期限切れの自動判定はできない）。ホスト側の定期ジョブが要るかは、要望を見て決める。
