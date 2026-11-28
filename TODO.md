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

- `admin/package.json` の `overrides`（`glob` → `minimatch ^11.0.5`）は、Dependabot の minimatch の警告（11.0.1 未満が対象）を避けるための一時的なもの。`@storybook/react-vite` が固定する `@joshwooding/vite-plugin-react-docgen-typescript@0.7` が `glob@13`（`minimatch ^10`）に依存するため。react-vite が新しい docgen プラグイン（`glob` に依存しない 0.9 系）に上がったら外す。`npm audit` に残る `glob` の警告（moderate）は、修正が react-vite のメジャーダウングレードになるため対応しない。

---

## 未着手タスク (Backlog)

### ⏸️ 保留中の検討

- [ ] **X・Threads のリッチ埋め込み**（保留中。プラグインは AegisCMS-Plugin にある）: 今は `official-link-embed` が X を oEmbed（`publish.twitter.com/oembed`）からリンクカードにするだけで、X の投稿の表示にはならない。Threads は既定の許可ドメインに無い。案: `official-embeds` に X・Threads のブロックを足す。API キーは要らない見込みだが未確認。第三者のスクリプトを置かずに済むよう、`sandbox` つきの iframe（X は `platform.twitter.com/embed/Tweet.html?id=<ID>`、Threads は投稿の URL の末尾に `/embed`）にし、`[embed].consent` の同意のあとに読む。着手するときは、まずキーなし・Strict CSP で表示できるかを実際のブラウザーで確かめる。X の iframe の URL は公式の文書に無い使い方なので、使えなくなるおそれがある。Threads をカードで出すだけなら、threads.net が OGP を返すかを確かめ、許可ドメインに足すだけで済む。
