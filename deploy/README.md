# AegisCMS + Caddy で HTTPS 公開する（複数サイト対応）

`sites.toml` にドメインとサイトの設定を書き、`./deploy.sh up` 1 つで、AegisCMS と Caddy（HTTPS の証明書を Let's Encrypt から自動で取得・更新）を起動します。サイトを増やすときは `sites.toml` に `[[sites]]` を足して、もう一度 `./deploy.sh up` を実行するだけです。

```text
                    ┌──────────── サーバー（Docker） ────────────┐
 example.com ──┐    │  Caddy ──┬─ main  (AegisCMS) ── main-data   │
 blog.example ─┼─▶ │  :80/443 └─ blog  (AegisCMS) ── blog-data   │
 www.example ──┘    │   （www は example.com へリダイレクト）       │
                    └─────────────────────────────────────────────┘
```

- **1 サイト = 1 コンテナ**です。データベース（SQLite）・アップロード・管理者・プラグインの有効化はサイトごとに分かれ、1 つのサイトの不具合や負荷が他のサイトに影響しにくい構成です。
- サイトのコンテナどうしは別のネットワークに置き、外から届くのは Caddy の 80 / 443 番だけです。
- Caddy は HTTP を HTTPS にリダイレクトし、各サイトに HSTS（`max-age=31536000`）を付けます。
- 各サイトは `[audit].trust_forwarded_for = true`（監査ログ・ログイン試行の制限に閲覧者の IP を使う）で動きます。Caddy は信頼していない接続元から届いた `X-Forwarded-For` を使わずに付け直し、サイトのコンテナには外から直接届かないので、IP は偽装できません（[設定リファレンス 4.4](../docs/configuration.md#44-あわせて確認する設定)）。

## 必要なもの

- Docker と Docker Compose v2（`docker compose`）が入った Linux サーバー
- サーバーの 80 / 443 番ポートが開いていること（UDP 443 は HTTP/3 用。任意）
- 各ドメインの DNS（A / AAAA レコード）がサーバーを向いていること

## 1. 最初の起動

```bash
git clone https://github.com/takumi-mr/aegiscms
cd aegiscms/deploy
cp sites.example.toml sites.toml
vi sites.toml          # ドメイン・サイト名・テーマなどを書く
./deploy.sh up         # イメージをビルドし、設定を生成して起動
./deploy.sh user main  # サイト main の管理者 admin を作る（パスワードを聞かれる）
```

`https://<ドメイン>/` がサイト、`https://<ドメイン>/admin/` が Admin Studio です。

初回の `./deploy.sh up` は AegisCMS のイメージをリポジトリのソースからビルドするので、数分〜十数分かかります。レジストリに置いたイメージを使う場合は `AEGIS_IMAGE=ghcr.io/... ./deploy.sh up` のように指定します（`docker compose` を直接使うときも同じ環境変数を使います）。

> [!TIP]
> 初めて試すときは `acme_staging = true` にすると、Let's Encrypt の発行回数の制限を気にせず試せます（証明書はブラウザーに信頼されません）。うまく動いたら `false` に戻して `./deploy.sh up` します。手元で試すなら、`domains = ["main.localhost"]` のように `.localhost` のドメインにすると、Caddy が自分の認証局で証明書を作ります。

## 2. `sites.toml`

```toml
[deploy]
acme_email = "admin@example.com"   # Let's Encrypt の連絡先
acme_staging = false

[[sites]]
name = "main"                      # サービス名・データのボリューム名
domains = ["example.com", "www.example.com"]   # 先頭が正規のドメイン。残りは先頭へリダイレクト
site_title = "Example"
site_description = "..."
theme = "minimal-editorial"
plugins = ["official-blocks"]
memory = "512m"                    # コンテナのメモリの上限（任意）

[sites.config.commerce]            # このサイトの aegis.toml に重ねる設定（任意）
enabled = true
```

| キー | 内容 |
| --- | --- |
| `[deploy].acme_email` | Let's Encrypt に登録する連絡先。証明書の更新に失敗したときなどに通知が届く |
| `[deploy].acme_staging` | `true` で Let's Encrypt のステージング（試し用） |
| `[deploy].project` | Docker Compose のプロジェクト名（既定 `aegiscms`）。コンテナ・ボリュームの名前の接頭辞。同じサーバーで別の構成を動かすときに変える |
| `[deploy].caddy_image` | Caddy のイメージ（既定 `caddy:2.10-alpine`） |
| `name` | 英小文字で始まり、英小文字・数字・`-`（40 文字以内）。データはボリューム `<project>_<name>-data` |
| `domains` | 小文字のホスト名。`https://`・ポート・パス・ワイルドカード・IP アドレスは書けない。同じドメインを複数のサイトに書くことはできない |
| `site_title` / `site_description` / `theme` / `plugins` | そのサイトの `[theme]` と `[plugins].enabled` |
| `memory` | コンテナのメモリの上限（`512m`・`1g` など） |
| `public_health` | `true` でヘルスチェック（`/health`・`/api/v1/health`）をインターネットに公開する（既定 `false`。[5. 運用](#5-運用)） |
| `[sites.config.*]` | そのサイトの `aegis.toml` に重ねる任意の設定（[設定リファレンス](../docs/configuration.md)）。`[sites.config.xxx]` は直前の `[[sites]]` に付く |

書き間違い（知らないキー・不正なドメイン・重複・無いテーマやプラグイン・設定の型の誤り）は、生成の時点でエラーになり、起動中のサイトはそのまま動き続けます。

### 生成されるファイル

`./deploy.sh up`（または `./deploy.sh generate`）は、AegisCMS のイメージの中の `aegis-server deploy generate` で次のファイルを作ります。手で編集せず、`sites.toml` を直して作り直してください（どれも Git の管理外）。

| ファイル | 内容 |
| --- | --- |
| `compose.yaml` | Caddy と各サイトのコンテナ・ネットワーク・ボリューム |
| `Caddyfile` | ドメインごとの振り分け・リダイレクト・HSTS・圧縮・内部向けのパスの遮断 |
| `sites/<name>/aegis.toml` | サイトの設定（[`config/docker.toml`](../config/docker.toml) に `sites.toml` の値を重ねたもの。`[audit].trust_forwarded_for = true`） |

設定を変えて `./deploy.sh up` すると、設定が変わったサイトのコンテナだけを作り直し、Caddy は止めずに設定を読み直します。

## 3. 秘密の値（SMTP のパスワード・決済のキーなど）

`sites.toml` には書かず、`secrets/<サイト名>.env` に環境変数で書きます。ファイルがあればそのサイトのコンテナに渡されます（Git の管理外）。

```bash
# secrets/shop.env
AEGIS_SMTP_PASSWORD=...
AEGIS_STRIPE_SECRET_KEY=sk_live_...
AEGIS_STRIPE_WEBHOOK_SECRET=whsec_...
```

## 4. 自作のテーマ・プラグイン

`themes/<テーマID>/`・`plugins/<プラグイン名>/` に置くと、すべてのサイトのコンテナに読み取り専用でマウントされます。`theme` / `plugins` に名前を書くか、Admin Studio から切り替えて使います。

## 5. 運用

| やりたいこと | コマンド |
| --- | --- |
| 状態を見る | `./deploy.sh ps` |
| ログを見る | `./deploy.sh logs`（サイトを絞るなら `./deploy.sh logs main`） |
| サイトを足す・設定を変える | `sites.toml` を直して `./deploy.sh up` |
| AegisCMS を更新する | `git pull && ./deploy.sh build && ./deploy.sh up` |
| 再起動 | `./deploy.sh restart main` |
| 全部止める | `./deploy.sh down`（データのボリュームは残る） |

- **サイトを外す**: `sites.toml` から消して `./deploy.sh up` すると、コンテナは止まって消えますが、データのボリューム（`<project>_<name>-data`）は残ります。完全に消すときは `docker volume rm aegiscms_<name>-data`。
- **バックアップ**: サイトのデータはボリュームにまとまっています。例: `docker run --rm -v aegiscms_main-data:/data -v "$PWD:/backup" alpine tar czf /backup/main-$(date +%F).tar.gz -C /data .`。書き込み中のデータまで確実に取るには、先に `docker compose -f compose.yaml stop main` でそのサイトを止め、取ったあと `docker compose -f compose.yaml start main` で戻します。
- **フィード・サイトマップの URL**: Caddy が `X-Forwarded-Proto: https` を付けるので、`https://<ドメイン>/...` になります。Admin Studio の **Settings › SEO** で canonical URL を設定しておくと確実です。
- **内部向けのパス（`/metrics`・`/health`）**: Caddy が `/metrics`・`/health`・`/api/v1/health`（`[server].base_path` があればその配下も）に 404 を返し、インターネットからは見えません（使っている CMS を特定されにくくするため。[設定リファレンス 4.6](../docs/configuration.md#46-使っている-cms-を特定されにくくする)）。
  - Prometheus は Caddy を通さず、Docker のネットワークの中から `http://<name>:8080/metrics` を読みます（Prometheus のコンテナをサイトのネットワーク `<project>_site-<name>` に参加させる。`/metrics` は `[sites.config.audit]` で `prometheus` の出力先を有効にしたときだけ応答します）。
  - コンテナのヘルスチェックはコンテナの中から `127.0.0.1:8080` を読むので影響しません。外部の死活監視サービスでヘルスチェックを使う場合は、そのサイトに `public_health = true` を書きます（製品名とバージョンは `admin` のセッションにしか返しません）。
- **アップロードの上限**: Caddy 側には上限を設けていません。AegisCMS の `[storage].max_upload_mb`（既定 10MB）で決まります（`[sites.config.storage]` で変更）。
