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
- [ ] `official-blocks/figure`（画像＋キャプション）
- [ ] `official-blocks/timeline`
- [ ] `official-blocks/chat`（吹き出し。アバターの画像参照の扱いを先に決める）
- [ ] `official-blocks/tabs`（`<details name>` 方式を検討。キーボード操作・印刷時の表示を確認）
- [ ] `official-blocks/stats`、`official-blocks/definition-list`
- [ ] （保留）脚注 `[^1]`: GitHub 対応の記法なので core の対象だが、文書全体の状態管理が要り重い。要望が出てから
- [ ] （保留）`rating`: 構造化データ（JSON-LD の `Review`）の設計とセットで検討
- [ ] **リンク埋め込みの更新（検討。要望が出てから）**: `official-link-embed` は保存時に 1 回取得してブロックに保存し、更新しない（Wasm に時計が無く、KV に有効期限も無いため、期限切れの判定ができない）。最新にしたいときは URL を書き直す。管理画面の「再取得」ボタン（またはホスト側の定期ジョブ）が要るかは、要望を見て決める。

