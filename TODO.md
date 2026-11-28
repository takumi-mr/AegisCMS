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
- 公式のカタログの URL と公開鍵は `store.rs` の既定値（`OFFICIAL_CATALOG_*`）。カタログの鍵を替えると、その値を持つ版のサーバーだけが読める（新旧の鍵を並べる仕組みは store.md 8 の未決事項）。

---

## 未着手タスク (Backlog)

### 🔍 実機での確認

上から順に着手する。

- [ ] **会員の購入・サブスクの実機確認**: 設計は [members.md 16](docs/members.ja.md#16-購入記事の単独購入サブスク)。実装（記事の単独購入・サブスク・管理画面・マイページ）は済で、Stripe はモックでしか確かめていない。Stripe のテストモードで、決済ページ・Webhook（`checkout.session.completed` / `customer.subscription.*` / `charge.refunded`）・Customer Portal・返金を一度通す。新しい API バージョンで契約の期間の終わりが `items` に移っている点（`current_period_end`）は両対応にしてあるが、実物の応答では未確認。
- [ ] **official-clarity の実機確認**（プラグインは [AegisCMS-Plugin](https://github.com/takumi-mr/AegisCMS-Plugin) にある）: 単体テストと Wasm の実行で、起動コードが同意を待つ形で入り、録画しないページに入らないことは確かめた。実際の Clarity での動作（CSP のホストで読み込み・送信がブロックされないか、`scripts.clarity.ms` が要るか）は、プロジェクト ID を入れたサイトで確かめていない。
- [ ] **official-embeds の実機確認**（プラグインは AegisCMS-Plugin にある）: URL の読み取り・`src` の組み立て・サニタイズ後に iframe が残ることは単体テストと Wasm の実行で確かめた。実際のブラウザーで、各サービスの埋め込みが CSP（足したホストだけの `frame-src`）の下で表示されるか（埋め込みの中からさらに別のホストを iframe で読む・ポップアップを開くものが無いか。とくに Google マップ・カレンダー・フォーム、CodePen の「クリックで実行」）は確かめていない。同意を待つ埋め込み（`[embed].consent`）で、許可したあとに iframe が読み込まれて案内が隠れること、汎用の埋め込み（`official-embeds/iframe`）の `sandbox` で、よく使うサービス（Google フォーム・Typeform など）が動くことも、実際のブラウザーでは確かめていない。

### 🏪 パッケージストア（第 1 段階）

設計は [store.md](docs/store.ja.md)。当面は公式のプラグイン・テーマだけを配る。上から順に着手する。

- [ ] **Admin Studio のストア画面**: API（`GET /api/v1/store/packages`・`POST /api/v1/store/install`）は済。一覧、互換性のないものの無効化、インストール前の `capabilities` の確認、更新のお知らせ、インストール後に有効化へ進む導線。
- [ ] **テスト用のプラグイン（移行の手順 5）**: 計画は [store.md 7](docs/store.ja.md#7-公式パッケージの別リポジトリへの移行計画)。本体のテスト（`handlers/plugin.rs` の切り替え・`shop_tests.rs`・`notify_plugins.rs`・`aegis-runtime/tests/sdk_plugins.rs` の blocks・code-enhancer）、E2E（`plugins.spec.ts`・`csp.spec.ts`）、診断（`docker/pentest/pentest.toml`）が使う公式プラグインを、テスト用のプラグイン（Rust の SDK で作る。同梱しない）に置き換える。
- [ ] **前後の記事を本体の設定に（手順 6）**: `official-post-navigation` の `mode`・`same_category`・`fallback_to_all`・`order_by` を本体の設定にし、Admin Studio で変えられるようにする。新しい設定が無ければ古いプラグインの設定も読み、警告を出す。
- [ ] **既定で何も有効にしない・おすすめの表示（手順 7）**: `config/default.toml` とベンチマークの設定で code-enhancer・mermaid・plantuml を有効にするのをやめる。カタログでおすすめに印を付け（AegisCMS-Store）、ストアのタブに並べて 1 クリックで入れられるようにする。
- [ ] **残りのプラグインの移行と同梱の廃止（手順 8・9）**: blocks・code-enhancer・mermaid・plantuml・page-toc・post-navigation・shop-rules・order-notifier を AegisCMS-Plugin へテストごと移し、本体から消す（`feat!:`、移行の手順つき）。Dockerfile・リリースのアーカイブ・同梱の設定の `bundled_plugins_dir` も見直す。
- [ ] **テーマの移行（手順 10）**: 翻訳（`aegis-theme/src/messages.rs`）をテーマ側に持てるようにしてから。
