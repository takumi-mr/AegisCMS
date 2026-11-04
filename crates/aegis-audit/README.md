# aegis-audit

AegisCMS の監査ログ（Audit Log）配送サービスおよび各種出力先シンク実装。

## 概要

`aegis-audit` は、管理操作、認証イベント、コンテンツ変更、プラグイン実行、設定更新などの監査イベントをキャプチャし、安全に永続化・配送するクレートです。

エンタープライズセキュリティ要件に対応し、改ざん防止、多重シンクへのファンアウト配信、保持期間（TTL）に基づく自動クリーンアップ、Prometheus メトリクス連携を提供します。

## 主な責務・提供機能

- **AuditService**:
  - アプリケーション全体からの監査イベント (`AuditEvent`) の非同期受信およびシンクへの一括配送。
  - 送信元コンテキスト (`AuditOrigin`: IP アドレス、User-Agent、リクエスト ID) の紐付け。
  - 保持期間（`retention_days`）を過ぎた過去ログの自動パージ。
- **各種出力先シンク (`AuditSink`)**:
  - `DatabaseAuditSink`: データベース（`_aegis_audit_logs` テーブル）への構造化永続化。Admin Studio からの検索・閲覧に対応。
  - `LogAuditSink`: ホストの `tracing` ログ（または syslog / 標準出力）への JSON 形式出力。SIEM 連携に最適。
  - `PrometheusAuditSink`: 監査イベントの頻度や異常イベント（認証失敗、アクセス拒否等）を Prometheus カウンターメトリクスとして集計・公開。

## モジュール構成

| モジュール | 説明 |
| :--- | :--- |
| `database` | データベース出力シンク (`DatabaseAuditSink`) |
| `log` | 構造化ロギング出力シンク (`LogAuditSink`) |
| `metrics` | Prometheus メトリクス出力シンク (`PrometheusAuditSink`) |
| `origin` | リクエストコンテキスト抽出 (`AuditOrigin`) |
| `config` | 監査設定 (`AuditConfig`, `AuditSinkKind`) |
