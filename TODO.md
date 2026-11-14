# AegisCMS TODO

> [!NOTE]
> 作業の再開時はここを読む。各作業の完了時に更新する。
> 完了したタスクは残さない。仕様は `docs/*.md`・`admin/README.md`、設計判断・注意点は [docs/architecture.ja.md](docs/architecture.ja.md)、開発手順は [docs/development.ja.md](docs/development.ja.md) に書いてから消す。
> **技術メモ・申し送り事項は変更ログ（CHANGELOG）を書く場所ではない**。単純な実装ログや変更履歴は記載せず、未解決の設計課題や既知の注意点のみを記録する。経緯は Git のコミットログや PR で追う。

---

## 直近の着手タスク

- [ ] **宣言的クエリの残り**（本体、全言語プラグイン SDK の `PageRenderContext.queries`、`theme dev` / Theme Builder のサンプル（`samples/queries.json`）は実装済み。仕様は [queries.md](docs/queries.ja.md)、設計判断は [architecture.md 25](docs/architecture.ja.md#25-宣言的クエリ-aegis-schemasrcqueryrs-aegis-dbsrcpublished_columnsrs-aegis-serversrcqueriesrs)）: ① Admin Studio でクエリの一覧・結果のプレビューを出すか（要望を見て決める）。型をまたぐ結合は対応しない方針（[queries.md 10](docs/queries.ja.md#10-型をまたぐ結合をしない方針と代わりの方法)）。

---

## 技術メモ・申し送り事項

- `cargo audit`（`task audit`・CI の `security-audit.yml`）の例外が [.cargo/audit.toml](.cargo/audit.toml) に 2 件ある（保留中。解消したら例外から消す）: ① **RUSTSEC-2026-0459**（SQLite 本体の CVE。`sqlx` 0.9 の `libsqlite3-sys` 0.37 → 0.39 以上待ち。**同梱の SQLite なので優先度が高い**）② RUSTSEC-2023-0071（`rsa` 0.9.10 の Marvin 攻撃。`aegis-auth` が SAML 用に直接使う。`jsonwebtoken` 12 は 0.10 に更新済みなので、残りはこの直接の依存だけ）。
- 公式プラグインおよび `sdk/templates/*`・`sdk/examples/*`（`sdk/tinygo/examples/hooks` を含む）の `plugin.wasm` はリポジトリにコミット済みで、バージョンや SDK を変えても自動では再ビルドされない（`version` は manifest にだけあり、Wasm には入らない）。ソースや SDK を変えたときは各自再生成してから出す。
- `admin/package.json` の `overrides`（`glob` → `minimatch ^11.0.5`）は、Dependabot の minimatch の警告（11.0.1 未満が対象）を避けるための一時的なもの。`@storybook/react-vite` が固定する `@joshwooding/vite-plugin-react-docgen-typescript@0.7` が `glob@13`（`minimatch ^10`）に依存するため。react-vite が新しい docgen プラグイン（`glob` に依存しない 0.9 系）に上がったら外す。`npm audit` に残る `glob` の警告（moderate）は、修正が react-vite のメジャーダウングレードになるため対応しない。

---

## 未着手タスク (Backlog)

### 🔌 プラグイン・将来構想

GitHub の範囲を超えるブロックはすべてプラグインで作る（[architecture.md 3.1a](docs/architecture.ja.md#31a-core-ブロックの範囲github-でできることが上限)）。上から順に着手する。

- [ ] **会員の購入・サブスクの実機確認**: 設計は [members.md 16](docs/members.ja.md#16-購入記事の単独購入サブスク)。実装（記事の単独購入・サブスク・管理画面・マイページ）は済で、Stripe はモックでしか確かめていない。Stripe のテストモードで、決済ページ・Webhook（`checkout.session.completed` / `customer.subscription.*` / `charge.refunded`）・Customer Portal・返金を一度通す。新しい API バージョンで契約の期間の終わりが `items` に移っている点（`current_period_end`）は両対応にしてあるが、実物の応答では未確認。
- [ ] **official-clarity の実機確認**: 単体テストと Wasm の実行で、起動コードが同意を待つ形で入り、録画しないページに入らないことは確かめた。実際の Clarity での動作（CSP のホストで読み込み・送信がブロックされないか、`scripts.clarity.ms` が要るか）は、プロジェクト ID を入れたサイトで確かめていない。
- [ ] **リンク埋め込みの定期更新（検討。要望が出てから）**: `official-link-embed` は「再取得」ボタンと URL の書き換えでだけ取り直す（Wasm に時計が無く、KV に有効期限も無いため、期限切れの自動判定はできない）。ホスト側の定期ジョブが要るかは、要望を見て決める。
