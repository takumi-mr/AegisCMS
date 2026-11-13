[← ドキュメント一覧に戻る](README.ja.md)

[English](penetration-testing.ja.md) | **日本語**

# ローカル・ペネトレーションテスト環境

この手順は AegisCMS を対象とする、所有者が管理するローカル環境専用です。外部ホストや本番環境へスキャンを向けないでください。

## 構成

- 防御側: `docker-compose.pentest.yml` で起動する AegisCMS コンテナ
- 攻撃側: Kali Linux などの WSL ディストリビューション
- 接続先: `http://127.0.0.1:18080`
- 設定: `docker/pentest/pentest.toml`（`config/docker.toml` に重ねる）。会員・ショップ・フォーラム・問い合わせ・メルマガ・コメント・解析と公式プラグイン 6 つを有効にし、ZAP がそれらに届くようにしてある（外部通信なし）
- ZAP のプラン（`zap-active.yaml` / `zap-authenticated.yaml`）は、`requestor` ジョブで、公開ページと API のシード URL へ先に GET を送ってからクロールする。ルートを足したら、シードも足すこと（404 のままだと検査されない）。認証済みのプランの `Authorization` は、実行のたびに取り直したセッション ID に置き換える（リポジトリの値は無効）
- DB とアップロード領域: 512 MiB の tmpfs。停止時に消去
- Docker ネットワーク: AegisCMS は内部ネットワークのみ。loopback ポートを公開する nginx ingress が内部ネットワークと標準 bridge の両方に接続し、対象自体には外向き経路を与えない
- コンテナ: 非 root、read-only root filesystem、全 Linux capabilities を削除、CPU/メモリ/PID 上限あり

## 起動

Windows の PowerShell でプロジェクトルートから実行:

```powershell
docker compose -f docker-compose.pentest.yml up --build -d --wait
curl.exe -i http://127.0.0.1:18080/api/v1/health
```

WSL の Kali から疎通を確認:

```bash
curl -i http://127.0.0.1:18080/api/v1/health
```

WSL から `127.0.0.1` に接続できない環境では、WSL の localhost forwarding を有効にしてください。ポート公開範囲を LAN に広げないでください。

## 初期確認

Kali 側で対象を上記 URL に固定して実施します。まず軽い列挙を行い、出力を `artifacts/` などに保存します。

```bash
nmap -Pn -sT -sV -p 18080 --script http-title,http-headers 127.0.0.1
whatweb http://127.0.0.1:18080
```

続けて OWASP ZAP の passive scan、手動の認証・認可テスト、入力検証テストを行います。能動スキャンや負荷試験はこの使い捨て環境だけに限定し、開始前に同時実行数とリクエスト速度を低く設定してください。認証ロックアウト、入力サイズ制限、アクセス制御を記録し、発見ごとに再現手順・期待結果・実際の結果・影響度を記録します。

ZAPのBaseline Scanは、対象を内部ネットワークに置いたまま実行できます。これはspiderとpassive scanのみです。

```powershell
docker compose -f docker-compose.pentest.yml up -d --wait target
docker run --rm --network aegiscms_lab --memory 1g --cpus 2 --pids-limit 256 `
  ghcr.io/zaproxy/zaproxy:stable zap-baseline.py -t http://target:8080 -m 1
```

## Active Scan の実行

Active scan も使い捨て環境に限定して実行できます。未認証スキャンとログイン済み認証スキャンの 2 つのプロファイルが用意されています。

### 1. 未認証スキャン（Web + API）
リポジトリ内のプラン（`docker/pentest/zap-active.yaml`）は公開 Web ページおよび主要 API エンドポイント（OpenAPI 定義 `docker/pentest/openapi.json` 含む）を対象とし、Low 強度でスキャンを実施します。結果 HTML は `docs/security-reports/zap-active-report.html` に保存されます。

```powershell
docker compose -f docker-compose.pentest.yml up -d --wait target
New-Item -ItemType Directory -Force -Path docs/security-reports | Out-Null
docker run --rm --network aegiscms_lab --memory 1g --cpus 1 --pids-limit 256 `
  -v "${PWD}/docker/pentest:/zap/wrk:ro" `
  -v "${PWD}/docs/security-reports:/zap/out:rw" `
  ghcr.io/zaproxy/zaproxy:stable zap.sh -cmd -autorun /zap/wrk/zap-active.yaml
docker compose -f docker-compose.pentest.yml down
```

### 2. ログイン済み認証スキャン（Medium 強度）
別プロファイル（`docker/pentest/zap-authenticated.yaml`）は、管理者セッション（Bearer トークン / X-Session-Id）を ZAP Replacer で全リクエストに注入し、保護 API や Admin Studio に対して Medium 強度での能動的検査を実施します。結果は `docs/security-reports/zap-authenticated-report.html` に保存されます。

```powershell
# 1. ターゲットと ingress を起動
docker compose -f docker-compose.pentest.yml up -d --wait target
docker compose -f docker-compose.pentest.yml up -d ingress

# 2. 初期セットアップを行い管理者セッションを取得
Invoke-RestMethod -Uri "http://127.0.0.1:18080/api/v1/setup" -Method Post -ContentType "application/json" -Body '{"admin_username":"admin","admin_password":"AdminPassword123!"}'
$login = Invoke-RestMethod -Uri "http://127.0.0.1:18080/api/v1/auth/session" -Method Post -ContentType "application/json" -Body '{"username":"admin","password":"AdminPassword123!"}'

# 3. 認証済みスキャンを実行
New-Item -ItemType Directory -Force -Path docs/security-reports | Out-Null
docker run --rm --network aegiscms_lab --memory 1g --cpus 1 --pids-limit 256 `
  -v "${PWD}/docker/pentest:/zap/wrk:ro" `
  -v "${PWD}/docs/security-reports:/zap/out:rw" `
  ghcr.io/zaproxy/zaproxy:stable zap.sh -cmd -autorun /zap/wrk/zap-authenticated.yaml

# 4. 破棄
docker compose -f docker-compose.pentest.yml down
```

重点確認項目:

- 未認証 API と公開ページに意図しない管理データが含まれない
- `viewer` / `editor` / `admin` 間の権限境界、IDOR、下書き・履歴・プレビューへのアクセス
- ログイン失敗時の IP/ユーザー単位レート制限とセッション無効化
- HTML ブロック、アップロード、ファイル名、リダイレクト、Host / forwarded headers の入力処理
- CSRF/CORS、セキュリティヘッダー、エラー応答・OpenAPI/metrics の情報露出
- プラグイン/テーマ署名設定と管理者限定操作

## 停止・破棄

```powershell
docker compose -f docker-compose.pentest.yml down
```

データは tmpfs なのでコンテナ削除時に破棄されます。別途ボリュームを追加しないでください。
