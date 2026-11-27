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

### 🔍 実機での確認

上から順に着手する。

- [ ] **会員の購入・サブスクの実機確認**: 設計は [members.md 16](docs/members.ja.md#16-購入記事の単独購入サブスク)。実装（記事の単独購入・サブスク・管理画面・マイページ）は済で、Stripe はモックでしか確かめていない。Stripe のテストモードで、決済ページ・Webhook（`checkout.session.completed` / `customer.subscription.*` / `charge.refunded`）・Customer Portal・返金を一度通す。新しい API バージョンで契約の期間の終わりが `items` に移っている点（`current_period_end`）は両対応にしてあるが、実物の応答では未確認。
- [ ] **official-clarity の実機確認**: 単体テストと Wasm の実行で、起動コードが同意を待つ形で入り、録画しないページに入らないことは確かめた。実際の Clarity での動作（CSP のホストで読み込み・送信がブロックされないか、`scripts.clarity.ms` が要るか）は、プロジェクト ID を入れたサイトで確かめていない。
- [ ] **official-embeds の実機確認**: URL の読み取り・`src` の組み立て・サニタイズ後に iframe が残ることは単体テストと Wasm の実行で確かめた。実際のブラウザーで、各サービスの埋め込みが CSP（足したホストだけの `frame-src`）の下で表示されるか（埋め込みの中からさらに別のホストを iframe で読む・ポップアップを開くものが無いか。とくに Google マップ・カレンダー・フォーム、CodePen の「クリックで実行」）は確かめていない。同意を待つ埋め込み（`[embed].consent`）で、許可したあとに iframe が読み込まれて案内が隠れること、汎用の埋め込み（`official-embeds/iframe`）の `sandbox` で、よく使うサービス（Google フォーム・Typeform など）が動くことも、実際のブラウザーでは確かめていない。

### 🏪 パッケージストア（第 1 段階）

設計は [store.md](docs/store.ja.md)。当面は公式のプラグイン・テーマだけを配る。上から順に着手する。

- [ ] **配布用リポジトリの初回公開**: [AegisCMS-Store](https://github.com/takumi-mr/AegisCMS-Store) にスクリプトとワークフロー（`Publish`）は用意済み。鍵を作って secret（`CATALOG_SIGNING_KEY`・`PACKAGE_SIGNING_KEY`）に登録し、Pages の Source を GitHub Actions にして初回の公開を行う。そのあと、実際の Pages のカタログを `[store]` に設定し、`GET /api/v1/store/packages` で取得できること（`objects.githubusercontent.com` へのリダイレクトを含む）を確かめる。
- [ ] **ストアからのインストール・更新 API**: カタログの読み込みと一覧（`crates/aegis-server/src/store.rs`・`GET /api/v1/store/packages`）は済。ZIP の取得（`store::build_client` を使う） → `size`・`sha256` の照合 → エントリの `public_key` と `signature.json` の鍵の一致 → 既存の ZIP インストール。`capabilities` がマニフェストと食い違えば拒否。監査ログに取得元を残す。
- [ ] **Admin Studio のストア画面**: 一覧、互換性のないものの無効化、インストール前の `capabilities` の確認、更新のお知らせ。
