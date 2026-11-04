# AegisCMS TODO

> [!NOTE]
> 作業の再開時はここを読む。各作業の完了時に更新する。
> 完了したタスクは残さない。仕様は `docs/*.md`・`admin/README.md`、設計判断・注意点は [docs/architecture.md](docs/architecture.md)、開発手順は [docs/development.md](docs/development.md) に書いてから消す。
> **技術メモ・申し送り事項は変更ログ（CHANGELOG）を書く場所ではない**。単純な実装ログや変更履歴は記載せず、未解決の設計課題や既知の注意点のみを記録する。経緯は Git のコミットログや PR で追う。

---

## 直近の着手タスク

（現在対応中のタスクはありません。未着手タスクから選択してください）

---

## 技術メモ・申し送り事項

- **`release-please.yml` の `publish-docker`/`publish-assemblyscript-sdk` 自動連携は未検証（手動公開は確認済み）**: GitHub Packages の npm（`@takumi-mr/plugin-sdk`）と GHCR の Docker イメージ（`ghcr.io/takumi-mr/aegiscms`）はいずれも存在し、visibility も `public`（手動変更は不要だった）。ただしどちらも `workflow_dispatch` による手動実行で公開したもので、`release-please.yml` の `needs.release-please.outputs.paths_released` を使った自動呼び出し（`sdk/assemblyscript` 変更時に `release-sdk.yml`、`.`（コア）変更時に `release-docker.yml`）は、release-please が実際にリリースを作成した実行で一度も `paths_released` に対象パスが含まれず、確認した全実行でスキップされたまま。現在 open の release PR（`chore: release main`、v0.3.0 相当）をマージすれば `.` が対象になるため `publish-docker` の自動発火を確認できる。`sdk/assemblyscript` 側は、同パスへの変更を含むリリースが作られるまで検証不可。
- 公式プラグインの `plugin.wasm` はリポジトリにコミット済みで、バージョンを上げても自動では再ビルドされない（`version` は manifest にだけあり、Wasm には入らない）。ソースを変えたリリースでは各自 `task` で再生成してから出す。

---

## 未着手タスク (Backlog)

### 🔌 プラグイン・将来構想
- [ ] **公開中のコンテンツの変更のレビュー（検討。要望が出てから）**: レビュー（[architecture.md 3.4a](docs/architecture.md#34a-コンテンツのレビュー-aegis-serversrcreviewrs)）は公開前の 1 段だけで、寄稿者は公開後の自分の記事を直せない（編集者に頼む）。公開版のポインター（`publish: false` の保存）を使えば、公開版を残したまま変更をレビューに出せる余地がある。段階を自由に組めるワークフローは作らない方針。
- [ ] **テンプレートからの動的コンテンツ取得関数（検討）**: `latest("article", 5, category="news")` などの MiniJinja グローバル関数または事前取得宣言機構。

### 🚀 性能最適化
- [ ] **`on-render-page` の結果キャッシュの利用状況を見えるようにする**: `WasmEngine::render_cache_stats()`（ヒット・ミス・設定変更で使えなかった回数・保存数）はあるが、サーバーの外からは見えない。ヒット率が低いサイト（記事ごとに 1 回しか見られない長い裾野）ではサーバー内のキャッシュが効かず、CDN に任せるほうが合うので、判断の材料として `/metrics` などに出す（[architecture.md 10.8](docs/architecture.md#108-on-render-page-の結果のキャッシュ-aegis-runtimesrcwasmresult_cachers)）。
- [ ] **SDK が変更したキーだけ返す仕組み（検討）**: 本文を読むプラグインは、SDK の `PageRenderContext` が本文を含めて全体を返すため、返送側の JSON の書き出しが本文に比例して残る（キャッシュに当たらない最初の 1 回）。ホストは返したキーだけ上書きする（[5.3.1](docs/plugin_abi_specification.md#531-オブジェクトを返すフックの部分更新)）ので、SDK 側で変えたキーだけ返せば足りる。`content_html` を空で返したときの扱い（従来は「本文を消す」）の互換が論点。
