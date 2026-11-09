[← ドキュメント一覧に戻る](README.md)

# AegisCMS セキュリティ評価レポート

## 評価の概要

| 項目 | 内容 |
| --- | --- |
| ステータス | 動的検査・API能動検査を実施。F-04修正（Critical）の実サーバー（Windows/Linuxコンテナ双環境）検証完了。未認証および**管理者ログイン済み（Authenticated）プロファイル**による Web/REST API エンドポイント（全 225 系統および OpenAPI 仕様）の動的ペネトレーションテスト完了 |
| 評価対象 | AegisCMS ローカル検証環境（Windows ネイティブプロセスおよび Docker Compose 隔離スタック） |
| 攻撃元 | Windows ネイティブ（PowerShell / curl）および Docker 内部ネットワーク上の OWASP ZAP (2.17.0) コンテナ |
| 想定対象 | Docker 内の AegisCMS（`127.0.0.1:18080`、内部名 `target:8080`）およびローカル開発プロセス |
| 評価環境 | Windows 11 / Docker Desktop (WSL2 engine) |
| 記録日 | 2026-10-26 (初版), 2026-11-02 (改訂: Windows ネイティブ検査、未認証 Web/API スキャン、ログイン済み Authenticated スキャン実施), 2026-11-09 (再検査: Strict CSP 導入後の未認証・認証済みスキャンを再実施) |

---

## 2026-11-09 再検査（Strict CSP 導入後・シードと対象機能を見直し）

公開ページの CSP を Strict（`script-src` / `style-src` に `'unsafe-inline'`・`'unsafe-eval'` なし。インラインのスクリプトと `<style>` は本文のハッシュで許し、`style=""` 属性は使わない）にした後（[csp.md](csp.md)）、Docker の使い捨て環境（OWASP ZAP stable、未認証プロファイルと管理者セッション注入の認証済みプロファイル）で再検査しました。

**検査範囲の見直し**: 前回の検査（11-02）は、シード URL の一部が現在のルートと合わず（`/api/v1/roles`・`/backup/list`・`/forum/topics` など）、さらに Docker 用の設定でフォーラム・ショップ・問い合わせ・メルマガ・コメント・会員・解析が無効だったため、それらの画面と API に届いていませんでした。そこで、①シードを現行のルートに合わせて約 60 URL にし、`requestor` ジョブで事前にサイトツリーへ載せ、②ペンテスト専用の設定 [docker/pentest/pentest.toml](../docker/pentest/pentest.toml)（外部通信なしで上記機能と公式プラグイン 6 つを有効化）を `docker-compose.pentest.yml` に重ねました。両プロファイルとも、初期セットアップ済み（サンプル記事あり）の状態で実行しています。

| プロファイル | 走査した URL | High | Medium | Low | Info | 前回（11-02）の Medium |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 未認証（Low 強度） | 77 | 0 | 2 | 0 | 2 | 6 |
| 認証済み（Medium 強度） | 79 | 0 | 2 | 1 | 2 | 6 |

- **解消**: CSP の `unsafe-inline`・`unsafe-eval` に関する警告、SRI 属性の警告（4 件）。公開ページと API の CSP から `'unsafe-inline'` が消え、Admin Studio も `script-src 'self'` / `style-src 'self'` のまま。
- **Medium 2 件の内訳**（CORS の指摘は、CORS の絞り込み（[#312](https://github.com/takumi-mr/AegisCMS/pull/312)）のあとの再実施で消えた）:
  1. **CSP: Wildcard Directive** — `img-src` と `font-src` の `https:`。本文の外部画像・外部フォントを許すための設計。
  2. **Absence of Anti-CSRF Tokens（新規に検出）** — `/contact`（`POST /contact/default`）と `/newsletter` の公開フォーム。検査範囲を広げたため初めて届いた。どちらも**匿名の送信フォーム**で、ログインセッションや Cookie に依存せず、乗っ取れる権限がない（最悪でも迷惑な送信）。迷惑な送信は、ハニーポット・IP 単位の送信数の制限・任意の CAPTCHA で抑えている。**リスクは低く、現状は許容**（トークンの導入は、フォームが権限のある操作をするようになったときに再検討）。
- **Low 1 件（認証済みのみ）**: Private IP Disclosure（監査ログのアクセス元 IP。P-05）。
- **Info 2 件**: Modern Web Application（SPA）、User Controllable HTML Element Attribute（`/search?q=` の入力値が `<input value>` に入る。ZAP は注意喚起のみで、値は HTML エスケープされている）。
- **High・SQL インジェクション・コマンドインジェクション・認証回避・権限昇格: 0 件**。
- **この検査の限界**: 会員制の閲覧制限・プレビューのフレーム・プラグインの出力など、**ZAP が自動では確かめにくい論理・設計面は、このスキャンでは見ていない**（単体・統合・E2E のテストで確認している）。シードのうち数件（`/api/v1/public/*`・`/api/v1/auth/registration`・`/api/v1/members/me`・`/api/v1/orders/report`）は、GET では 200 を返さない（404/405/401/400）。OpenAPI の定義（`docker/pentest/openapi.json`）は 5 パスのみで、全 API は網羅していない。

## 実施状況と最新サマリー（2026-11-02 改訂）

本評価では、従来の公開 HTML ページ限定検査から範囲を大幅に拡大し、**REST API 全般（未認証、viewer、editor、admin）、初期セットアップ API（Web インストーラー）、および OpenAPI 定義を含めたペネトレーションテスト**を Docker 隔離環境で実施しました。さらに、**管理者認証セッション（Bearer トークン）を注入した別プロファイル（`zap-authenticated.yaml`）を作成し、Medium 攻撃強度での認証済みスキャン**を実施しました。

### 主な確認結果
1. **API を含めた OWASP ZAP Active Scan（未認証プロファイル）**:
   - `docker/pentest/openapi.json` の仕様インポートおよび主要 API エンドポイントをターゲットに追加。
   - クロール・走査対象が前回の 13 URLs から **75 URLs** へ大幅に増加。
   - 能動的スキャン結果: **High: 0**, **Medium: 6**（CSP に関する注意、ワイルドカード CORS、SRI 属性未付与）, **Low: 0**, **Informational: 1**。
   - SQL インジェクション、OS コマンドインジェクション、リモートコード実行などの致命的脆弱性は 0 件。
2. **管理者ログイン済み OWASP ZAP Active Scan（認証済みプロファイル・Medium強度）**:
   - `docker/pentest/zap-authenticated.yaml` を新規作成し、ZAP Replacer により `Authorization: Bearer <session_id>` および `X-Session-Id` を全リクエストに注入。
   - 管理画面（Admin Studio）、スキーマ定義、コンテンツ管理、ストレージ一覧、ユーザー管理、プラグイン、テーマ、監査ログ、ヘッダー設定など **86 URLs** を網羅して走査。
   - 攻撃強度を **Medium**（ペイロード種別・エンコードを拡充）に引き上げて能動攻撃を実施。
   - 能動的スキャン結果: **High: 0**, **Medium: 6**, **Low: 1（新規）**, **Informational: 1**。
   - 新規検出の Low: `/api/v1/audit-logs` のレスポンス内にクライアントアクセス元 IP として Docker 内部プライベート IP（`172.20.0.3`）が記録されていることによる **Private IP Disclosure**。
3. **初期セットアップ API（Web インストーラー）の検証**:
   - 初期化前: `/api/v1/setup/status` は `{"initialized":false,"requires_setup":true}` を返却。`/setup` はインストーラー画面を提供。
   - 入力値検証: 空のユーザー名、8 文字未満のパスワードは `400 Bad Request` (`VALIDATION_FAILED`) で適切に拒否。
   - 多重初期化防止ガード: 一度初期化が完了した後は、`POST /api/v1/setup` への再初期化リクエストがすべて `403 Forbidden` (`ALREADY_INITIALIZED`) で確実に遮断されることを実機確認。また、`/setup` は `/admin/` への 303 リダイレクトとなることを確認。
4. **RBAC（ロール別アクセス制御）の境界検証**:
   - 未認証リクエスト: 全保護 API（スキーマ、コンテンツ、ストレージ、ユーザー、ロール、プラグイン、テーマ、監査ログ、バックアップ等）が確実に `401 Unauthorized` を返却。
   - OpenAPI 仕様（`/api/v1/openapi.json`）: 過去レポート（P-01）の「公開」という記載を訂正。実際には `ViewerSession` が要求され、未認証では 401 で保護されていることを確認。
   - `viewer` ロール: 参照系 API のみ成功（200）。コンテンツ作成（403）、ユーザー管理（403）、監査ログ（403）はすべて拒否。
   - `editor` ロール: コンテンツ作成・更新・通常ストレージアップロード・削除は成功。ユーザー管理（403）、監査ログ（403）はすべて拒否。
   - `admin` ロール: ユーザー管理や監査ログを含め、管理系全エンドポイントへの正当なアクセスを確認。
5. **F-04 パストラバーサル脆弱性の Linux コンテナ環境における回帰検証**:
   - Windows ネイティブで発見・修正されたドライブレター/パストラバーサル脆弱性（F-04）について、Docker（Linux）環境でも同様に検証を実施。
   - `POST /api/v1/storage/upload` の `key` フィールドに `/etc/passwd`、`C:\Windows\System32`、`../../../escaped.txt` を指定した攻撃はいずれも `400 Bad Request` (`{"error":"Invalid storage key"}`) で即座に遮断されることを確認。
   - `DELETE /api/v1/storage/objects/..%2F..%2Fescaped.txt` も同様に `400 Bad Request` で拒否。
6. **認証ブルートフォース防御（ログインロックアウト）の再検証**:
   - `POST /api/v1/auth/session` に対し誤ったパスワードを 5 回送信した時点で、6 回目以降は即座に `429 Too Many Requests` (`Retry-After: 900`) が返され、正しいパスワードであってもロックアウト期間が明けるまで拒否されることを確認。
7. **ブラウザー向け防御ヘッダー**:
   - API レスポンスおよび HTML レスポンスの双方で、F-03 対応により導入されたセキュリティヘッダー（`Content-Security-Policy`、`X-Frame-Options`、`X-Content-Type-Options`、`Referrer-Policy`、`Permissions-Policy`、`Cross-Origin-Opener-Policy`）が統一して付与されていることを確認。

---

## 検査結果と所見一覧

| ID | 項目 | 根拠・影響の可能性 | 状態 |
| --- | --- | --- | --- |
| F-01 | 公開ページの stored XSS | 既定の lol_html サニタイズと sandbox モードでの不透明オリジン iframe 隔離 | 対応完了 |
| F-02 | CORS が全オリジン・全メソッド・全ヘッダーを許可（2026-11-09 に対応: 公開の読み取りだけ `*`、認証が要る API・書き込みは `[server].cors_origins` に書いたオリジンだけ。ZAP の Cross-Domain Misconfiguration は解消） | `Origin: https://attacker.invalid` の応答で `Access-Control-Allow-Origin: *`、OPTIONSで `Access-Control-Allow-Methods: *` と `Access-Control-Allow-Headers: *` を確認。`Access-Control-Allow-Credentials` は返らないためクッキー認証の横取りはないが、公開APIの過剰許可設定に該当。**深刻度: Low** | 対応完了（2026-11-09） |
| F-03 | ブラウザー向け防御ヘッダー不足 | 公開トップページおよび管理画面、API の応答でデフォルトの防御ヘッダー（`CSP`、`X-Content-Type-Options: nosniff`、`X-Frame-Options: SAMEORIGIN`、`Referrer-Policy: strict-origin-when-cross-origin`、`Permissions-Policy`、`Cross-Origin-Opener-Policy: same-origin`）を付与するミドルウェアを導入。ZAP Baseline での警告（10020, 10021）は完全に解消。 | 対応完了 |
| F-04 | ストレージ API のパストラバーサル・任意ファイル操作 | `upload_dir` 外への絶対パス・相対パストラバーサル指定を拒否する共通バリデーション `validate_storage_key` を導入。Windows ネイティブおよび Linux Docker コンテナの双方で再検証を実施し、攻撃ペイロードがすべて 400 で遮断されることを確認。 | 対応完了 |
| P-01 | 未認証アクセス境界 | 全保護 API（スキーマ、コンテンツ、ストレージ、ユーザー、ロール、プラグイン、テーマ、監査ログ、バックアップ等）は 401。公開 health およびセットアップ status のみ 200。OpenAPI 仕様（`/api/v1/openapi.json`）も `viewer` セッションが必須であり未認証では 401 であることを確認。 | 期待どおり |
| P-02 | ログイン失敗ロックアウト | 同一アカウントに対する 5 回の誤パスワード試行で即座に 429 ロックアウト（`Retry-After: 900`）が発動することを確認。 | 期待どおり |
| P-03 | Web インストーラー多重初期化防止 | 未初期化時の `/api/v1/setup` による管理者登録成功後、再度の初期化呼び出しは `403 Forbidden` (`ALREADY_INITIALIZED`) で遮断。`/setup` は `/admin/` へリダイレクトされることを確認。 | 期待どおり |
| P-04 | RBAC 権限昇格防止 | `viewer` によるコンテンツ作成・更新、ユーザー管理、監査ログの呼び出しは 403。`editor` によるユーザー管理、監査ログの呼び出しも 403 で遮断。 | 期待どおり |
| P-05 | 監査ログのプライベート IP 露出 | `/api/v1/audit-logs`（admin限定）の監査レコードにログイン元クライアント IP（`172.20.0.3`）が含まれ、ZAP により Private IP Disclosure (Low) として検出。監査証跡としての正規機能であるが、内部コンテナネットワーク情報の露出に該当。 | 仕様・低リスク |
| F-05 | CSP の `unsafe-inline`・`unsafe-eval`（ZAP の Medium 警告） | 公開ページ・API・Admin Studio のいずれの CSP にも `'unsafe-inline'`・`'unsafe-eval'` を含まない（Strict CSP。インラインはハッシュ、`style` 属性は不使用）。2026-11-09 の再検査で当該警告と SRI 警告が消えたことを確認。残るのは `img-src` / `font-src` の `https:` のみ | 対応完了 |
| P-06 | 匿名フォームの CSRF トークン欠如（ZAP 10202） | `/contact`・`/newsletter` は匿名の送信フォームで、セッション・Cookie に依存しない。ハニーポット・送信数の制限・CAPTCHA（任意）で濫用を抑制。 | 低リスク・許容 |
| N-01 | nmap サービス確認 | `127.0.0.1:18080` のみ開放、nginx ingress による `server_tokens off` で詳細バージョン非開示を確認。 | 完了 |
| N-02 | metrics エンドポイント | `/metrics` はデフォルト設定で 404（Prometheus sink 未有効化時）。不必要なメトリクス露出なし。 | 完了 |

---

## OWASP ZAP Active Scan 実行結果

### 1. 未認証プロファイル（Web + API）
プラン: [docker/pentest/zap-active.yaml](../docker/pentest/zap-active.yaml)
- **対象**: 公開ページ、OpenAPI 定義、未認証 API エンドポイント
- **クロール結果**: 75 URLs
- **能動スキャン設定**: Attack Strength: Low、1-2 threads、ディレイ 50ms
- **結果**: High: 0, Medium: 6, Low: 0, Informational: 1
- **詳細レポート**: [zap-active-report.html](security-reports/zap-active-report.html)

### 2. ログイン済み認証プロファイル（Medium 強度）
プラン: [docker/pentest/zap-authenticated.yaml](../docker/pentest/zap-authenticated.yaml)
- **対象**: 管理者セッション（Bearer トークン注入）による Admin Studio、全保護 API、OpenAPI 仕様
- **クロール結果**: 86 URLs（未認証では 401 となっていた全保護エンドポイントへのアクセスを確認）
- **能動スキャン設定**: Attack Strength: **Medium**（ペイロード拡充）、ディレイ 30ms、最大 6 分
- **結果**:

| リスク水準 | アラート件数 | 主な内容 |
| --- | ---: | --- |
| **High** | **0** | 重大なインジェクション・認証回避・権限昇格・コード実行は検出なし |
| **Medium** | **6** | CSP 設定の緩和（`unsafe-inline`, `unsafe-eval`, `img/font https:`）、CORS ワイルドカード設定（F-02）、SRI 属性欠落 |
| **Low** | **1** | **Private IP Disclosure**（`/api/v1/audit-logs` のアクセス元記録に含まれる `172.20.0.3`） |
| **Informational** | **1** | Modern Web Application（SPA 通知） |
| **False Positives** | **0** | - |

- **詳細レポート**: [zap-authenticated-report.html](security-reports/zap-authenticated-report.html)

---

## 推奨事項・今後の継続課題

1. **初期セットアップ（Web インストーラー）の公開前実施の推奨**:
   - 未初期化状態のインスタンスをパブリックネットワーク上に公開した場合、正規管理者が初期化する前に第三者が `/setup` を実行して管理者アカウントを作成してしまうリスクがあります。
   - 本番環境では環境変数（`AEGIS_ADMIN_PASSWORD` 等）または CLI（`aegis-server init`）による事前初期化を行うか、セットアップ完了までリバースプロキシで `/setup` および `/api/v1/setup` へのアクセスを内部ネットワークに限定することを推奨します。
2. **CORS 設定（F-02 関連。2026-11-09 に実施済み）**:
   - 既定は、公開の読み取りだけ全オリジン、認証が要る API・書き込みは同一オリジンのみ。別ドメインの Admin Studio やヘッドレスのクライアントは `[server].cors_origins` で許可します（[configuration.md の 4.3.1](configuration.md)）。
3. **CSP の厳格化（2026-11-09 時点で実施済み）**:
   - `unsafe-eval` の除去、Admin Studio の `script-src` / `style-src` の `'self'` 化、公開ページのハッシュ方式の Strict CSP（`strict_csp = "on"` が既定）、`style=""` 属性の廃止まで完了。残るのは `img-src` / `font-src` の `https:`（外部画像・フォントを許す設計）。外部画像を使わない運用なら、「設定 › ヘッダー」で CSP を絞れます。
4. **内部 IP の監査ログ表示方針（P-05 関連）**:
   - リバースプロキシ配下での運用時、X-Forwarded-For などのヘッダーの信頼性設定と合わせて、内部ネットワークのアドレスが外部へ漏洩しないよう考慮することが推奨されます。
