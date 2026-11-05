# AegisCMS TODO

> [!NOTE]
> 作業の再開時はここを読む。各作業の完了時に更新する。
> 完了したタスクは残さない。仕様は `docs/*.md`・`admin/README.md`、設計判断・注意点は [docs/architecture.md](docs/architecture.md)、開発手順は [docs/development.md](docs/development.md) に書いてから消す。
> **技術メモ・申し送り事項は変更ログ（CHANGELOG）を書く場所ではない**。単純な実装ログや変更履歴は記載せず、未解決の設計課題や既知の注意点のみを記録する。経緯は Git のコミットログや PR で追う。

---

## 直近の着手タスク

- [ ] **会員アカウント（掲示板への組み込み）**（設計は [docs/members.md](docs/members.md) の 14 の順）: ①設計書・②`[members]` への切り出しと改名・③`aegis-members` クレートへの移動・④公開名（`nickname`）は完了 → ⑤掲示板を会員に対応（`author_kind`・`posting`・`visibility`・CSRF・退会時の匿名化）→ ⑥管理画面（会員の詳細に投稿、掲示板に会員の印）。

---

## 技術メモ・申し送り事項

- 公式プラグインの `plugin.wasm` はリポジトリにコミット済みで、バージョンを上げても自動では再ビルドされない（`version` は manifest にだけあり、Wasm には入らない）。ソースを変えたリリースでは各自 `task` で再生成してから出す。

---

## 未着手タスク (Backlog)

### 🔌 プラグイン・将来構想
- [ ] **リンク埋め込みの更新（検討。要望が出てから）**: `official-link-embed` は保存時に 1 回取得してブロックに保存し、更新しない（Wasm に時計が無く、KV に有効期限も無いため、期限切れの判定ができない）。最新にしたいときは URL を書き直す。管理画面の「再取得」ボタン（またはホスト側の定期ジョブ）が要るかは、要望を見て決める。

### 🚀 性能最適化
- [ ] **`on-render-page` の結果キャッシュの利用状況を見えるようにする**: `WasmEngine::render_cache_stats()`（ヒット・ミス・設定変更で使えなかった回数・保存数）はあるが、サーバーの外からは見えない。ヒット率が低いサイト（記事ごとに 1 回しか見られない長い裾野）ではサーバー内のキャッシュが効かず、CDN に任せるほうが合うので、判断の材料として `/metrics` などに出す（[architecture.md 10.8](docs/architecture.md#108-on-render-page-の結果のキャッシュ-aegis-runtimesrcwasmresult_cachers)）。
