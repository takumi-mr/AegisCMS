[← ドキュメント一覧へ戻る](README.ja.md)

[English](publish.md) | **日本語**

# サイトをインターネットに公開する

AegisCMS は素の HTTP で応答します。公開するには、前段に HTTPS を受けるもの（リバースプロキシ）を置きます。方法は 2 つあります。

| やりたいこと                                                                                | 使うもの                                            |
| ------------------------------------------------------------------------------------------- | --------------------------------------------------- |
| Docker のある新しい Linux サーバーで、証明書の面倒も任せて HTTPS にしたい（複数サイトも可） | [Caddy 構成（`deploy/`）](#1-caddy-構成deploy)      |
| すでにある nginx などを使う、または `/cms/` のようなサブパスで公開したい                    | [自前のリバースプロキシ](#2-自前のリバースプロキシ) |

## 1. Caddy 構成（`deploy/`）

Docker と Docker Compose v2 のある Linux サーバー、開放した 80 / 443 ポート、サーバーを指すドメインごとの DNS レコードが必要です。

```bash
git clone https://github.com/takumi-mr/aegiscms
cd aegiscms/deploy
cp sites.example.toml sites.toml
vi sites.toml          # ドメイン、サイト名、テーマを書く
./deploy.sh up         # イメージの取得、設定の生成、起動
./deploy.sh user main  # サイト main の管理者を作る
```

`https://<ドメイン>/` がサイト、`https://<ドメイン>/admin/` が Admin Studio です。証明書は Caddy が取得・更新します。1 サイトが 1 コンテナで動くので、`[[sites]]` を足して `./deploy.sh up` をもう一度実行すれば、サイトを増やせます。

最初は `acme_staging = true` で試すと、Let's Encrypt のレート制限にかかりません。そのほか（秘密情報、自作のテーマ、更新、サイトの削除）は [deploy/README.ja.md](../deploy/README.ja.md) にあります。

## 2. 自前のリバースプロキシ

プロキシから AegisCMS（`127.0.0.1:8080`）へ中継し、次を守ってください。

- `Host`、`X-Forwarded-For`（追記ではなく上書き）、`X-Forwarded-Proto` を渡す。
- ログとログイン制限が訪問者の IP を見るよう、`[audit].trust_forwarded_for = true` にする。プロキシ経由でしか AegisCMS に届かない場合に**限ります**。
- `[storage].max_upload_mb` 以上のアップロードを通す（nginx は `client_max_body_size`）。
- `/metrics`、`/health`、`/api/v1/health` をインターネットに出さない。

nginx の完全な例（サブドメインとサブパス）と、シングルサインオンの URL や正規 URL などほかに確認する設定は、[設定リファレンスの 4](configuration.ja.md#4-リバースプロキシ配下での運用) にあります。

## 公開したら

- フィードとサイトマップが `https://` になるよう、Admin Studio の **設定 › SEO** で正規 URL を設定する。
- [バックアップを設定する](backup-restore.ja.md)。
