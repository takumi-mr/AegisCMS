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

### 🔍 実機での確認

上から順に着手する。

- [ ] **会員の購入・サブスクの実機確認**: 設計は [members.md 16](docs/members.ja.md#16-購入記事の単独購入サブスク)。実装（記事の単独購入・サブスク・管理画面・マイページ）は済で、Stripe はモックでしか確かめていない。Stripe のテストモードで、決済ページ・Webhook（`checkout.session.completed` / `customer.subscription.*` / `charge.refunded`）・Customer Portal・返金を一度通す。新しい API バージョンで契約の期間の終わりが `items` に移っている点（`current_period_end`）は両対応にしてあるが、実物の応答では未確認。
- [ ] **official-clarity の実機確認（残り）**（プラグインは [AegisCMS-Plugin](https://github.com/takumi-mr/AegisCMS-Plugin) にある）: 実際のブラウザー（Chromium・Strict CSP・同意バナーあり）で、同意の前は Clarity に一切通信せず、同意のあとにタグ（`www.clarity.ms/tag/<ID>`）を CSP に止められずに読むことは確かめた。ダミーのプロジェクト ID ではタグが空（204）を返し、本体のスクリプト（`scripts.clarity.ms`）の読み込みと録画の送信（`*.clarity.ms`・`c.bing.com`）が起きないため、そこは未確認。本物のプロジェクト ID で同じ確認をし、Clarity の画面に録画が届くかを見る。
- [ ] **official-embeds の実機確認（残り）**（プラグインは AegisCMS-Plugin にある）: 実際のブラウザー（Chromium・Strict CSP・`[embed].consent` あり）で確かめた。同意の前は 8 つの iframe のどれも読まず、同意のあとに読む。YouTube・Vimeo・OpenStreetMap・Google マップ・Google カレンダー・CodePen（通常の埋め込み）・汎用の埋め込み（`sandbox` つき）は、CSP の違反もポップアップも無く表示され、埋め込みの中からさらに別のホストを iframe で読むものも無かった。残り: (1) 汎用の埋め込みで Google フォーム・Typeform が動くか（公開のフォームが要る）。(2) CodePen の「クリックで実行」は、自動のブラウザーでは CodePen が実行のときに 403 を返す（AegisCMS の無い素のページでも同じ）ため、普段のブラウザーで手で確かめる。
