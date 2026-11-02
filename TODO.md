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

- **`release-please.yml` の `publish-docker`/`publish-assemblyscript-sdk` 自動連携は未検証（手動公開は確認済み）**: GitHub Packages の npm（`@takumi-mr/plugin-sdk`）と GHCR の Docker イメージ（`ghcr.io/takumi-mr/aegiscms`）はいずれも存在し、visibility も `public`（手動変更は不要だった）。ただしどちらも `workflow_dispatch` による手動実行で公開したもので、`release-please.yml` の `needs.release-please.outputs.paths_released` を使った自動呼び出し（`sdk/assemblyscript` 変更時に `release-sdk.yml`、`.`（コア）変更時に `release-docker.yml`）は、release-please が実際にリリースを作成した実行で一度も `paths_released` に対象パスが含まれず、確認した全実行でスキップされたまま。現在 open の release PR（`chore: release main`、v0.3.0 相当）をマージすれば `.` が対象になるため `publish-docker` の自動発火を確認できる。`sdk/assemblyscript` 側は、同パスへの変更を含むリリースが作られるまで検証不可。
- 公式プラグインの `plugin.wasm` はリポジトリにコミット済みで、バージョンを上げても自動では再ビルドされない（`version` は manifest にだけあり、Wasm には入らない）。ソースを変えたリリースでは各自 `task` で再生成してから出す。

---

## 未着手タスク (Backlog)

### 🔌 プラグイン・将来構想
- [ ] **テンプレートからの動的コンテンツ取得関数（検討）**: `latest("article", 5, category="news")` などの MiniJinja グローバル関数または事前取得宣言機構。

### 🚀 性能最適化
- [ ] **大規模コンテンツ（1 万件超）での最適化**:
  - `on-render-page` フックで HTML 全体を Wasm に渡すオーバーヘッドの軽減。
