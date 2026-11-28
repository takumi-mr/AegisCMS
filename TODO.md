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

未着手のタスクはありません。
