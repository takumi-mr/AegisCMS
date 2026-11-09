# AegisCMS TODO

> [!NOTE]
> 作業の再開時はここを読む。各作業の完了時に更新する。
> 完了したタスクは残さない。仕様は `docs/*.md`・`admin/README.md`、設計判断・注意点は [docs/architecture.md](docs/architecture.md)、開発手順は [docs/development.md](docs/development.md) に書いてから消す。
> **技術メモ・申し送り事項は変更ログ（CHANGELOG）を書く場所ではない**。単純な実装ログや変更履歴は記載せず、未解決の設計課題や既知の注意点のみを記録する。経緯は Git のコミットログや PR で追う。

---

## 直近の着手タスク

（なし）

---

## 技術メモ・申し送り事項

- 公式プラグインの `plugin.wasm` はリポジトリにコミット済みで、バージョンを上げても自動では再ビルドされない（`version` は manifest にだけあり、Wasm には入らない）。ソースを変えたリリースでは各自 `task` で再生成してから出す。
- `admin/package.json` の `overrides`（`glob` → `minimatch ^11.0.5`）は、Dependabot の minimatch の警告（11.0.1 未満が対象）を避けるための一時的なもの。`@storybook/react-vite` が固定する `@joshwooding/vite-plugin-react-docgen-typescript@0.7` が `glob@13`（`minimatch ^10`）に依存するため。react-vite が新しい docgen プラグイン（`glob` に依存しない 0.9 系）に上がったら外す。`npm audit` に残る `glob` の警告（moderate）は、修正が react-vite のメジャーダウングレードになるため対応しない。

---

## 未着手タスク (Backlog)

### 🔌 プラグイン・将来構想
GitHub の範囲を超えるブロックはすべてプラグインで作る（[architecture.md 3.1a](docs/architecture.md#31a-core-ブロックの範囲github-でできることが上限)）。上から順に着手する。
- [ ] **会員レベルによる閲覧の制限（有料メンバー限定・「ここから先は有料」）**: 設計は [members.md 15](docs/members.md#15-閲覧の制限会員レベル)。実装の分け方（15.7）の残り（①〜⑤は済）: ⑥ 付与の入口（商品購入・外部決済との連動。要望を見て）。
- [ ] **official-clarity の実機確認**: 単体テストと Wasm の実行で、起動コードが同意を待つ形で入り、録画しないページに入らないことは確かめた。実際の Clarity での動作（CSP のホストで読み込み・送信がブロックされないか、`scripts.clarity.ms` が要るか）は、プロジェクト ID を入れたサイトで確かめていない。
- [ ] **CSP の見直し（残り）**: ① 組み込みの default テーマが `fonts.googleapis.com` のスタイルシートを読み込むが、既定の CSP（`style-src 'self' 'unsafe-inline'`）が止めている（`admin/e2e/csp.spec.ts` で `style-src-elem` の違反を確認。フォントは代替の書体になる）。フォントを同梱する・`style-src` に足す・読み込みをやめる、のどれにするか決める。② Admin Studio（`/admin/`）は `script-src 'self'`（`unsafe-inline` なし）でも動くことを E2E で確認した。管理画面だけ、公開ページより厳しい CSP を返す（`style-src` も `unsafe-inline` なしで動くかは未確認）。③ 公開ページの `script-src 'unsafe-inline'` を外すには、組み込みのコメント欄・`onclick`・公式プラグインが出すインラインのスクリプトに nonce を付ける変更が要る（テーマとプラグインに影響する破壊的変更。要望が出てから）。
- [ ] **リンク埋め込みの定期更新（検討。要望が出てから）**: `official-link-embed` は「再取得」ボタンと URL の書き換えでだけ取り直す（Wasm に時計が無く、KV に有効期限も無いため、期限切れの自動判定はできない）。ホスト側の定期ジョブが要るかは、要望を見て決める。
