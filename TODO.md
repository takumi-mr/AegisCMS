# AegisCMS TODO

> [!NOTE]
> 作業の再開時はここを読む。各作業の完了時に更新する。
> 完了したタスクは残さない。仕様は `docs/*.md`・`admin/README.md`、設計判断・注意点は [docs/architecture.md](docs/architecture.md)、開発手順は [docs/development.md](docs/development.md) に書いてから消す。
> **技術メモ・申し送り事項は変更ログ（CHANGELOG）を書く場所ではない**。単純な実装ログや変更履歴は記載せず、未解決の設計課題や既知の注意点のみを記録する。経緯は Git のコミットログや PR で追う。

---

## 直近の着手タスク

1. **プラグイン例の Plugin Builder 上での動作確認**: `sdk/examples/assemblyscript-hooks` と `sdk/tinygo/examples/hooks` は実ホストの統合テスト（`sdk_hook_examples.rs`）で検証済み。Plugin Builder の画面上でも `task dev:plugin` で動かして確認する。

---

## 技術メモ・申し送り事項

- **release-please は未実行（初回の動作確認が必要）**: 設定（`release-please-config.json`）と `.github/workflows/release-please.yml` は手元で動かせないため未検証。`main` にマージ後、最初に出るリリース PR で、(1) 各 `extra-files`（`Cargo.toml` の TOML・`package-lock.json` の `packages['']`）が更新される、(2) 変更のない公式プラグインが対象に入らない、(3) `Cargo.lock` の同期コミットが付く、を確かめる。`GITHUB_TOKEN` の PR では CI が動かないので、必要なら `RELEASE_PLEASE_TOKEN`（PAT）を登録する（[docs/versioning.md](docs/versioning.md)）。
- 公式プラグインの `plugin.wasm` はリポジトリにコミット済みで、バージョンを上げても自動では再ビルドされない（`version` は manifest にだけあり、Wasm には入らない）。ソースを変えたリリースでは各自 `task` で再生成してから出す。

---

## 未着手タスク (Backlog)

### 🔌 プラグイン・将来構想
- [ ] **読者向けの小さな公式プラグイン**（`official-page-toc` / `official-back-to-top` と同じ作りで足す。それぞれ別プラグイン）:
  - 読了時間・文字数の表示（`on-render-page`）
  - 前後の記事ナビ（同じカテゴリ内の「前の記事 / 次の記事」）
- [x] **広告配信プラグイン**: `on-render-page` によるヘッダー注入とカスタムブロック表示、動的 CSP 設定との連携、Cookie 同意バナー対応: 公式プラグイン [official-ads](plugins/official-ads)
- [ ] **テンプレートからの動的コンテンツ取得関数（検討）**: `latest("article", 5, category="news")` などの MiniJinja グローバル関数または事前取得宣言機構。

### 🚀 性能最適化
- [ ] **大規模コンテンツ（1 万件超）での最適化**:
  - `on-render-page` フックで HTML 全体を Wasm に渡すオーバーヘッドの軽減。

### 🌐 サイト機能拡張
- [ ] **コメント・掲示板への CAPTCHA 適用**: `aegis-captcha`（問い合わせフォームで導入済み。[contact.md 1.2](docs/contact.md#12-captcha任意)）を `[comments]` / `[forum]` にも使えるようにする。
