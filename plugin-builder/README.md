# Plugin Builder

AegisCMS の Wasm プラグインを、エディターと即時実行サンドボックスで開発するための開発ツール（React 19 + Vite + CodeMirror 6）。
DB もログインも不要で、`aegis-server plugin dev` の JSON API に接続します。

```bash
task dev:plugin -- plugins/official-code-enhancer   # 開発サーバー（http://127.0.0.1:8091）
npm --prefix plugin-builder ci                      # 初回のみ
task dev:plugin-builder                             # http://localhost:5175/
```

| コマンド                                | 内容                                                |
| --------------------------------------- | --------------------------------------------------- |
| `npm run dev`                           | 開発サーバー（`PLUGIN_DEV_URL` で接続先を変更可能） |
| `npm run test`                          | ユニットテスト（Vitest）                            |
| `npm run lint` / `npm run format:check` | oxlint（ゼロ警告）/ Prettier                        |
| `npm run build`                         | 型チェックと本番ビルド（`dist/`）                   |

## 機能

- **フック即時実行**: マニフェストに定義されたフックを選択し、ワンクリック（または `Ctrl+Enter`）で Wasm サンドボックス内で実行
- **標準フィクスチャ**: 各フックに対応する標準入力データをワンクリックでロード
- **サンドボックス・インスペクション**:
  - Wasm 内の `log_info!`, `log_warn!`, `log_error!` をキャプチャしたリアルタイムログ
  - CPU 命令数（Fuel）およびミリ秒単位の実行時間の可視化
  - プラグインが書き換えた KVS の差分表示
  - プラグインが送信した外部 HTTP リクエストの記録
- **モック設定**:
  - プラグイン設定値（`config_get`）の注入
  - KVS（`kv_get` / `kv_set`）の初期データ設定
  - 外部 API（Slack や Webhook 等）のモック応答ルールの定義
- **ホットリロード**: Wasm バイナリやマニフェストが再コンパイルされると自動でメタデータを同期
