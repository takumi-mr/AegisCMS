[← ドキュメント一覧に戻る](README.md)

# AegisCMS プラグイン開発サーバー (`aegis plugin dev`)

`aegis plugin dev`（または `task dev:plugin -- <dir>`）は、AegisCMS の Wasm プラグインをデータベースやログイン不要で、単体で即時テスト・デバッグするための軽量開発サーバーです。

`theme dev`（Theme Builder）と同様に、プラグインの変更を監視して自動リロードし、Wasm サンドボックス内でのフック実行結果、CPU 命令数（Fuel）、ミリ秒単位の実行時間、プラグインログ（`log_info!` 等）、KVS 変更、送信 HTTP リクエストをリアルタイムに可視化する REST API を提供します。

---

## 1. クイックスタート

```bash
# 既存の公式プラグインで開発サーバーを起動 (既定: http://127.0.0.1:8091)
task dev:plugin -- plugins/official-code-enhancer

# または直接 cargo で起動
cargo run -p aegis-server -- plugin dev plugins/official-code-enhancer --port 8091
```

ブラウザで `http://127.0.0.1:8091` を開くと、プラグインのロード状況と利用可能なエンドポイントが表示されます。

---

## 2. API 仕様 (`/__dev/*`)

開発サーバーは以下のエンドポイントを提供します。

### 2.1 プラグイン情報の取得 (`GET /__dev/api/plugin`)

マニフェスト（`plugin.manifest.json`）、エントリポイント、Wasm ファイルサイズ、モジュールがエクスポートしている関数名一覧を返します。

```bash
curl http://127.0.0.1:8091/__dev/api/plugin
```

**レスポンス例**:
```json
{
  "manifest": {
    "name": "official-code-enhancer",
    "version": "0.1.0",
    "entrypoint": "plugin.wasm",
    "capabilities": {
      "hooks": ["transform-content-ast", "on-render-page"]
    },
    "resource_limits": {
      "max_memory_mb": 16,
      "max_fuel_units": 100000000,
      "execution_timeout_ms": 250
    }
  },
  "entrypoint": "plugin.wasm",
  "wasm_exists": true,
  "wasm_size_bytes": 1450280,
  "exports": ["memory", "allocate", "transform_content_ast", "on_render_page"],
  "updated_at": 1730424567890
}
```

### 2.2 標準モックフィクスチャ一覧 (`GET /__dev/api/fixtures`)

全フック（`on-before-save`, `transform-content-ast`, `render-blocks`, `on-render-page`, `calculate-order` など）の標準テスト用ダミーデータを返します。

```bash
curl http://127.0.0.1:8091/__dev/api/fixtures
```

### 2.3 フックの即時テスト実行 (`POST /__dev/api/invoke`)

フック名・入力データ・初期 KV・モック HTTP ルール・動的設定値を指定して、Wasmtime 上で安全に隔離実行します。
※書き込み系エンドポイントのため、ヘッダー `x-aegis-dev: 1` が必要です。

```bash
curl -X POST http://127.0.0.1:8091/__dev/api/invoke \
  -H "x-aegis-dev: 1" \
  -H "Content-Type: application/json" \
  -d '{
    "hook": "transform-content-ast",
    "input": {
      "blocks": [
        {
          "type": "code",
          "language": "rust",
          "code": "fn main() { println!(\"Hello\"); }"
        }
      ]
    },
    "config": {
      "highlight_theme": "github-dark"
    }
  }'
```

**レスポンス例**:
```json
{
  "success": true,
  "output": [
    {
      "type": "raw_html",
      "html": "<pre class=\"highlight\"><code class=\"language-rust\">...</code></pre>"
    }
  ],
  "raw_output": "[{\"type\":\"raw_html\",...}]",
  "error": null,
  "fuel_consumed": 15420,
  "duration_ms": 1.45,
  "logs": [
    {
      "level": "info",
      "message": "Highlighted 1 code block using github-dark theme",
      "timestamp": "2026-11-01T01:50:00Z"
    }
  ],
  "kv_mutations": {},
  "http_requests": []
}
```

### 2.4 KVS / HTTP 通信のモックとキャプチャ

外部 API 連携（Slack 通知など）や状態保存を行うプラグインも、実環境に影響を与えずに検証できます。

```json
{
  "hook": "on-order-status-changed",
  "input": {
    "order_id": "ord-123",
    "new_status": "paid"
  },
  "kv": {
    "notify_count": "5"
  },
  "mock_http": [
    {
      "url_pattern": "https://hooks.slack.com/*",
      "status": 200,
      "body": "{\"ok\":true}"
    }
  ]
}
```

実行後、`kv_mutations` に書き換えられたキーと値、`http_requests` に送信された HTTP メソッド・URL・本文が記録されて返却されます。

### 2.5 ホットリロード用ロングポーリング (`GET /__dev/version?since=<version>`)

プラグインフォルダー内のファイル更新（`plugin.wasm` の再コンパイルなど）を検知すると、即座に新しいタイムスタンプを返します。フロントエンド（Plugin Builder）の自動再実行に使用されます。

---

## 3. セキュリティ設計

開発サーバーはローカル開発用途に特化していますが、ブラウザの別タブ・悪意ある外部スクリプトからの不正操作を防ぐため、以下の防御機構を備えています。

1. **DNS リバインディング防止**: ループバック接続時、`Host` ヘッダーが `localhost` / `127.0.0.1` / `::1` であることを検証。
2. **CSRF / 不正 Fetch 防止**: `GET` / `HEAD` 以外のすべてのミューテーションリクエストで `x-aegis-dev` カスタムヘッダーを必須化。
3. **パストラバーサル防止**: ファイル API はプラグインディレクトリ配下から脱出するパス（`..` やシンボリックリンク）を遮断。

