[← ドキュメント一覧に戻る](README.md)

# AegisCMS セキュリティ評価レポート

## 評価の概要

| 項目 | 内容 |
| --- | --- |
| ステータス | 動的検査を部分実施。要修正所見あり |
| 評価対象 | AegisCMS ローカル検証環境 |
| 攻撃元 | Ubuntu WSL（Kali Linux は未インストール。nmap 7.94SVN と curl を使用） |
| 想定対象 | Docker 内の AegisCMS。`127.0.0.1:18080` のみ |
| 評価環境 | Windows / Docker Desktop / Ubuntu WSL2 |
| 記録日 | 2026-10-26 |

## 実施状況と制約

Docker Composeで対象をビルド・起動し、Ubuntu WSLから `127.0.0.1:18080` へ検査しました。nmapの限定TCPサービス検出、HTTP/CORS確認、未認証API、viewer/editor権限、ログイン失敗制限、公開HTMLブロックを確認しました。ZAP Baseline Scanと、制約した未認証active scanを内部Dockerネットワークから実施しました。認証済みスキャン、WhatWeb、Kali固有の検査、ブラウザー上でのJavaScript実行確認は未実施です。対象アプリのDBとアップロードはtmpfsで、検査終了後にCompose環境を破棄します。

検査後、`docker compose -f docker-compose.pentest.yml down` を実行し、対象・ingressコンテナとネットワークを削除しました。使い捨てユーザーとコンテンツを含むtmpfsデータも破棄済みです。

## 検査結果と所見

| ID | 項目 | 根拠・影響の可能性 | 状態 |
| --- | --- | --- | --- |
| F-01 | 公開ページの stored XSS | 既定の lol_html サニタイズと sandbox モードでの不透明オリジン iframe 隔離 | 対応完了 |
| F-02 | CORS が全オリジン・全メソッド・全ヘッダーを許可 | `Origin: https://attacker.invalid` の応答で `Access-Control-Allow-Origin: *`、OPTIONSで `Access-Control-Allow-Methods: *` と `Access-Control-Allow-Headers: *` を確認。`Access-Control-Allow-Credentials` は返らない。Bearer tokenを持たない第三者が保護APIを読める状態は確認していない。**深刻度: Low（設定の過剰許可）**。許可するAdmin Studioのオリジン、メソッド、ヘッダーを限定するのが望ましい。 | 再現済み |
| F-03 | ブラウザー向け防御ヘッダー不足 | 公開トップページおよび管理画面、API の応答でデフォルトの防御ヘッダー（`Content-Security-Policy`、`X-Content-Type-Options: nosniff`、`X-Frame-Options: SAMEORIGIN`、`Referrer-Policy: strict-origin-when-cross-origin`、`Permissions-Policy`、`Cross-Origin-Opener-Policy: same-origin`）を付与するミドルウェアを導入。Admin Studio の「設定 › ヘッダー」画面からカスタマイズ可能。 | 対応完了 |
| P-01 | 未認証アクセス境界 | `/api/v1/schemas`、`/api/v1/contents/article`、`/api/v1/storage/objects`、`/api/v1/auth/session` は401。公開healthとOpenAPIは200。viewerはスキーマ・記事一覧・ストレージ一覧が200、ユーザー一覧は403、記事作成は403。 | 対象範囲で期待どおり |
| P-02 | ログイン失敗ロックアウト | viewerの誤パスワード5回は401、正しいパスワードも続けて429、`Retry-After: 900`。 | 対象範囲で期待どおり |
| N-01 | nmapサービス確認 | `127.0.0.1` のTCP/18080のみを検査しHTTP openを確認。最終検出は`nginx`（詳細版表示は検査用ingressで無効化）。 | 完了 |
| N-02 | metrics | `/metrics` は404。検証用設定の監査sinkはdatabaseのみで、Prometheus sinkが有効でないための結果と考えられる。Prometheus sinkを有効にした構成は未検査。 | 構成依存・未検査 |

## ZAP Active Scan

ZAP 2.17.0のAutomation Frameworkで、対象の内部Dockerネットワークからactive scanを実施しました。プランは [docker/pentest/zap-active.yaml](../docker/pentest/zap-active.yaml) にあります。

- 対象: 未認証の公開ページ。API (`/api/v1/*`) とunsubscribe経路を除外
- spider: 最大1分、深さ2、最大10 children、1 thread。フォーム処理・POST送信を無効化
- active scan: Attack Strength Low、1 thread、100ms間隔、ルールごと最大1分、スキャン最大3分
- 実測: spiderは13 URL、GETのみ。active scanは20秒で完了
- 結果: High 0、Medium 4、Low 1、Informational 1。アラートはBaselineで既に確認したCSP、CORS、クリックジャッキング、SRI、`X-Content-Type-Options`、アプリ形式の通知と一致し、active scan固有の新しい攻撃成立アラートはなし
- 制約: 認証必須API、フォームPOST、editor操作、F-01の保存済みXSS経路は対象外。従ってF-01を再検査するスキャンではない

詳細HTML: [zap-active-report.html](security-reports/zap-active-report.html)。ZAPレポートはログエラー12件、警告3件、ネットワーク失敗率16%も示します。ZAPコンテナは破棄済みのため詳細 `zap.log` は保存されておらず、失敗原因は特定できていません。ネットワークを切ったテスト環境で外部リソース参照が失敗した可能性がありますが、未確認です。

## OWASP ZAP Baseline Scan

公式Dockerイメージ `ghcr.io/zaproxy/zaproxy:stable` の `zap-baseline.py -t http://target:8080 -m 1` を、対象と同じ内部Dockerネットワークから実行しました。Baseline Scanは短時間のspiderとpassive scanで、active attackは行いません。対象データは前回の検査後に破棄済みだったため、今回は未認証の初期状態をクロールしています。

| 集計 | 結果 |
| --- | ---: |
| クロールURL | 12 |
| PASS | 55 |
| WARN-NEW | 6 |
| FAIL-NEW | 0 |

警告は次の通りです。これはpassive scanの確認候補であり、単独で脆弱性の成立を意味しません。

- Missing Anti-clickjacking Header (10020): 4 URL
- X-Content-Type-Options Header Missing (10021): 5 URL
- Content Security Policy (CSP) Header Not Set (10038): 4 URL
- Cross-Domain Misconfiguration (10098): 5 URL。手動確認したF-02のwildcard CORSと重なる
- Modern Web Application (10109): `/admin/`。アプリ形式の情報通知
- Sub Resource Integrity Attribute Missing (90003): 4 URL

ZAPはF-03の一部とF-02に関係する設定を検出しました。F-01は認証が必要なeditor操作と公開ページが対象ですが、このBaseline Scanでは認証せず、検査データも存在しなかったため検出対象外です。`FAIL-NEW: 0` は、今回の未認証・passive scan範囲でFail扱いのアラートが無かったことだけを示します。

## 推奨する次の手順

1. CORSを必要なAdmin Studioオリジンに限定し、method/headerを絞る。
2. プラグイン/テーマ署名、下書き・履歴・プレビュー、IDOR、アップロード、Host/forwarded headers、CSRFを追加検査する。
3. 認証必須APIやeditor操作を対象にする場合は、使い捨てユーザーとデータを用意して認証コンテキストを設定し、対象を明示して追加スキャンする。

検査結果はこのローカル使い捨て構成に限ります。未検査項目を「問題なし」と記載せず、「未検査」と明示してください。
