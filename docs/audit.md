# AegisCMS 監査ログリファレンス (Audit Log)

AegisCMS は、コンテンツ作成・セッション操作・ファイルアップロード・プラグインのエラーを監査イベントとして記録します。
出力先は `AuditSink` トレイトで抽象化されており、設定で複数の出力先を同時に有効にできます。

関連ソース:

- イベント型 / `AuditSink` トレイト: [`crates/aegis-core/src/audit.rs`](../crates/aegis-core/src/audit.rs)
- 出力先の実装と配送: [`crates/aegis-server/src/audit/`](../crates/aegis-server/src/audit)
- 参照 API / メトリクス: [`crates/aegis-server/src/handlers/audit.rs`](../crates/aegis-server/src/handlers/audit.rs)

---

## 1. 設定

```toml
[audit]
enabled = true
sinks = ["database", "log", "prometheus"]
trust_forwarded_for = false
```

各項目の説明は [設定リファレンス](./configuration.md#28-audit--監査ログ) を参照してください。

| 出力先 (`sinks`) | 内容 | 主な連携先 |
| --- | --- | --- |
| `database` | `_aegis_audit_logs` テーブルへ永続化し、`GET /api/v1/audit-logs` で検索可能にする（既定） | SQLite / PostgreSQL / MySQL |
| `log` | `aegis::audit` ターゲットへ 1 行 1 JSON の構造化ログを出力 | CloudWatch Logs, Loki, Datadog など（標準出力の転送経由） |
| `prometheus` | `aegis_audit_events_total` カウンタを `GET /metrics` で公開 | Prometheus, CloudWatch Agent (Prometheus 収集) |

- 出力先への書き込みに失敗しても**リクエストは失敗しません**（`ERROR` ログを出して処理を続行）。
- `enabled = false` または `sinks = []` の場合は記録を行いません。

---

## 2. 記録されるイベント

| `action` | 記録タイミング | `outcome` | `resource_type` / `resource_id` | 主な `details` |
| --- | --- | --- | --- | --- |
| `content.create` | `POST /api/v1/contents/{content_type}` | `success` / `denied`（`on-before-save` フックが拒否）/ `failure`（DB エラー） | `content` / コンテンツ ID | `content_type`, `slug`, `title`, `status`, `reason` / `error` |
| `session.create` | `POST /api/v1/auth/session` | `success` / `failure` | `session` / セッション指紋 | `expires_at`, `error` |
| `session.delete` | `DELETE /api/v1/auth/session` | `success` / `failure` | `session` / セッション指紋 | `error` |
| `storage.upload` | `POST /api/v1/storage/upload` | `success` / `failure`（ストレージエラー） | `asset` / ストレージキー | `original_name`, `content_type`, `size`, `url`, `error` |
| `plugin.error` | Wasm プラグインフックの実行失敗（トラップ、タイムアウト、Fuel 枯渇など） | `failure` | `content` / コンテンツ ID | `hook`, `content_type`, `error` |

- 入力不備（必須フィールド欠落、存在しないスキーマなど）で処理に入る前に弾かれたリクエストは記録しません。
- `on-before-save` フックが失敗した場合は、`plugin.error` と `content.create`（`denied`）の 2 件が記録されます。

### 2.1 操作主体 (`actor`)

リクエストの `Authorization: Bearer <token>` / `X-Session-Id` ヘッダーからセッションを解決し、`user_id` と `role` を記録します。セッションが無い・無効な場合は `null` です。
`session.create` では、作成されたセッションのユーザーが `actor` になります。

### 2.2 セッション指紋

セッショントークンは認証情報そのものであるため、監査ログには**記録しません**。
代わりに SHA-256 ハッシュの先頭 16 桁（セッション指紋）を `resource_id` に記録し、`session.create` と `session.delete` を突き合わせられるようにしています。

### 2.3 クライアント情報

| フィールド | 内容 |
| --- | --- |
| `ip` | 接続元 IP。`trust_forwarded_for = true` の場合は `X-Forwarded-For` の先頭アドレスを優先 |
| `user_agent` | `User-Agent` ヘッダー（最大 512 文字） |

> [!WARNING]
> `X-Forwarded-For` はクライアントが自由に設定できるため、`trust_forwarded_for` は ALB / nginx などの信頼できるリバースプロキシ配下でのみ有効にしてください。

---

## 3. イベントの形式

```json
{
  "id": "c2ec7420-f797-4fca-8168-2f0466988926",
  "occurred_at": "2026-10-21T18:23:23.508321Z",
  "action": "content.create",
  "outcome": "success",
  "actor": { "user_id": "alice", "role": "editor" },
  "resource_type": "content",
  "resource_id": "9ec53581-4616-441d-9473-8014acc52674",
  "ip": "127.0.0.1",
  "user_agent": "curl/8.5.0",
  "details": { "content_type": "article", "slug": "hello", "title": "Hello", "status": "draft" }
}
```

`occurred_at` は常に UTC・マイクロ秒精度・`Z` 終端の RFC 3339 形式です（文字列の辞書順と時系列順が一致します）。

---

## 4. 参照 API: `GET /api/v1/audit-logs`

`database` 出力先が有効な場合に利用できます。**`admin` ロールのセッションが必要です。**

| クエリパラメータ | 説明 |
| --- | --- |
| `action` | `content.create` などのアクション名 |
| `outcome` | `success` / `failure` / `denied` |
| `actor_id` | 操作主体のユーザー ID |
| `resource_type`, `resource_id` | 対象リソース |
| `since` | この時刻以降（含む）。RFC 3339 (例: `2026-10-21T00:00:00+09:00`) |
| `until` | この時刻より前（含まない）。RFC 3339 |
| `limit` | 取得件数（既定 50、最大 500） |
| `offset` | 読み飛ばす件数（既定 0） |

```bash
curl -H "Authorization: Bearer $ADMIN_TOKEN" \
  "http://localhost:8080/api/v1/audit-logs?action=content.create&since=2026-10-21T00:00:00Z&limit=20"
```

```json
{ "items": [ { "id": "...", "action": "content.create", "...": "..." } ], "limit": 20, "offset": 0 }
```

結果は `occurred_at` の新しい順です。

| ステータス | 条件 |
| --- | --- |
| `401` | セッションヘッダーが無い、またはセッションが無効 |
| `403` | セッションのロールが `admin` ではない |
| `400` | 不正な `action` / `outcome` / 日時 |
| `404` | `database` 出力先が無効 |

> [!IMPORTANT]
> 現在の `POST /api/v1/auth/session` はリクエストボディで任意のロールを指定できるため、`admin` ロールのチェックは本格的な認可にはなりません。認証プロバイダ（`AuthProvider`）の実装時に、セッション発行側で権限を検証する必要があります。

---

## 5. 構造化ログ (`log`)

`aegis::audit` ターゲットへ `INFO` レベルで、イベント JSON をメッセージとして 1 行ずつ出力します。

```text
2026-10-21T18:22:53.520299Z  INFO aegis::audit: {"id":"ade1ec8b-...","occurred_at":"2026-10-21T18:22:53.513401Z","action":"session.create",...}
```

- `RUST_LOG` で出力を制御できます（例: `RUST_LOG=warn,aegis::audit=info` で監査ログ以外を抑制）。
- **CloudWatch Logs**: コンテナの標準出力を `awslogs` ログドライバ（ECS）や CloudWatch Agent / Fluent Bit で転送し、`aegis::audit:` 以降を JSON としてパースします。
- **Loki など**: Promtail / Fluent Bit の regex / json パーサーで同様に取り込めます。

---

## 6. メトリクス (`prometheus`)

`GET /metrics` で Prometheus テキスト形式のカウンタを公開します。全 `action` × `outcome` の組み合わせを 0 から公開するため、イベント発生前からアラートルールを書けます。

```text
# HELP aegis_audit_events_total Total number of audit events recorded since startup.
# TYPE aegis_audit_events_total counter
aegis_audit_events_total{action="content.create",outcome="success"} 12
aegis_audit_events_total{action="plugin.error",outcome="failure"} 0
...
```

- カウンタはプロセス内メモリに保持され、再起動で 0 に戻ります（Prometheus の `rate()` / `increase()` はこれを考慮します）。
- 複数インスタンス構成では、各インスタンスをスクレイプして集約してください。
- `/metrics` は認証なしで公開されます。必要に応じてネットワーク / リバースプロキシで公開範囲を制限してください。
- CloudWatch では CloudWatch Agent の Prometheus メトリクス収集で取り込めます。

アラート例:

```yaml
- alert: AegisPluginErrors
  expr: increase(aegis_audit_events_total{action="plugin.error"}[5m]) > 0
```

---

## 7. 出力先の追加

`aegis_core::AuditSink` を実装し、`AuditService::from_config`（`crates/aegis-server/src/audit/mod.rs`）に登録します。

```rust
#[async_trait]
impl AuditSink for MySink {
    fn name(&self) -> &str {
        "my-sink"
    }

    async fn record(&self, event: &AuditEvent) -> Result<()> {
        // event を外部へ送信する
        Ok(())
    }
}
```

`record` はリクエスト処理の中で順番に `await` されます。外部サービスへの送信など時間のかかる処理は、内部でキューイングするなどしてリクエストの遅延を避けてください。
