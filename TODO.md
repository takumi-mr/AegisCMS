# AegisCMS TODO

> [!NOTE]
> 作業の再開時はここを読む。各作業の完了時に更新する。
> 完了したタスクは残さない。仕様は `docs/*.md`・`admin/README.md`、設計判断・注意点は [docs/architecture.md](docs/architecture.md)、開発手順は [docs/development.md](docs/development.md) に書いてから消す。
> **技術メモ・申し送り事項は変更ログ（CHANGELOG）を書く場所ではない**。単純な実装ログや変更履歴は記載せず、未解決の設計課題や既知の注意点のみを記録する。経緯は Git のコミットログや PR で追う。

---

## 直近の着手タスク

- [ ] **管理画面で、スキーマの追加フィールドを編集できるようにする**: `getSchemas`（`GET /api/v1/schemas`、いまは未使用）の定義から、専用 UI が扱わないフィールドだけを型ごとの入力欄（string / text / integer / boolean / timestamp / relation / jsonb）で出し、`fields` に載せて保存する。`excerpt`・`price`・`layout` など専用 UI があるフィールドは重複させない。追加フィールドはコアのロジックには使わない前提。検索・絞り込みへの反映は保留。

---

## 技術メモ・申し送り事項

- 公式プラグインの `plugin.wasm` はリポジトリにコミット済みで、バージョンを上げても自動では再ビルドされない（`version` は manifest にだけあり、Wasm には入らない）。ソースを変えたリリースでは各自 `task` で再生成してから出す。

---

## 未着手タスク (Backlog)

### 🔌 プラグイン・将来構想
- [ ] **テンプレートからの動的コンテンツ取得関数（検討）**: `latest("article", 5, category="news")` などの MiniJinja グローバル関数または事前取得宣言機構。
- [ ] **リンク埋め込みの更新（検討。要望が出てから）**: `official-link-embed` は保存時に 1 回取得してブロックに保存し、更新しない（Wasm に時計が無く、KV に有効期限も無いため、期限切れの判定ができない）。最新にしたいときは URL を書き直す。管理画面の「再取得」ボタン（またはホスト側の定期ジョブ）が要るかは、要望を見て決める。

### 🚀 性能最適化
- [ ] **`on-render-page` の結果キャッシュの利用状況を見えるようにする**: `WasmEngine::render_cache_stats()`（ヒット・ミス・設定変更で使えなかった回数・保存数）はあるが、サーバーの外からは見えない。ヒット率が低いサイト（記事ごとに 1 回しか見られない長い裾野）ではサーバー内のキャッシュが効かず、CDN に任せるほうが合うので、判断の材料として `/metrics` などに出す（[architecture.md 10.8](docs/architecture.md#108-on-render-page-の結果のキャッシュ-aegis-runtimesrcwasmresult_cachers)）。
- [ ] **SDK が変更したキーだけ返す仕組み（検討）**: 本文を読むプラグインは、SDK の `PageRenderContext` が本文を含めて全体を返すため、返送側の JSON の書き出しが本文に比例して残る（キャッシュに当たらない最初の 1 回）。ホストは返したキーだけ上書きする（[5.3.1](docs/plugin_abi_specification.md#531-オブジェクトを返すフックの部分更新)）ので、SDK 側で変えたキーだけ返せば足りる。`content_html` を空で返したときの扱い（従来は「本文を消す」）の互換が論点。
