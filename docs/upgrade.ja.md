[← ドキュメント一覧へ戻る](README.ja.md)

[English](upgrade.md) | **日本語**

# AegisCMS の更新

## 更新の前に

1. [Releases](https://github.com/takumi-mr/AegisCMS/releases) のリリースノートを読みます。**BREAKING** と書かれた変更は、こちらの対応が必要なことがあります。
2. [バックアップを取り](backup-restore.ja.md)、検証します。
3. 今のバージョンを確認します: `aegis-server --version`（Docker は `docker exec aegiscms aegis-server --version`）。

## 更新する

データベースの変更は、新しいバージョンを初めて起動したときに自動で適用されます。移行の手順を自分で実行する必要はなく、古いバージョンから最新へ一気に更新して構いません。

### Docker

```bash
docker pull ghcr.io/takumi-mr/aegiscms:latest
docker stop aegiscms && docker rm aegiscms
docker run -d --name aegiscms -p 8080:8080 -v aegis-data:/app/data ghcr.io/takumi-mr/aegiscms:latest
```

以前と同じボリューム（`-v`）とオプションを使ってください。データはボリュームに残ります。`deploy/` では `./deploy.sh pull && ./deploy.sh up` を実行します。

### バイナリ

1. サーバーを止めます。
2. 新しいアーカイブをダウンロードし、**新しいフォルダ**に展開します。
3. 古いフォルダからデータと設定をコピーします: `aegis.db*`、`data/`、編集した `config/` と `schemas/`。新しい設定が増えていることがあるので、新しい `config/default.toml` と `schemas/` を自分のものと見比べてください。
4. 新しい `plugins/` と `themes/`（リリースに合わせて更新されます）と、自作のものを置きます。
5. 新しいバイナリを起動します。すべて動くと確認できるまで、古いフォルダは残しておきます。

サービスとして動かしている場合は、`WorkingDirectory` と `ExecStart` を新しいフォルダに向けます。

## 動作を確認する

- ログにエラーがなく、`/` と `/admin/` が開く。
- Admin Studio にログインして、いくつかのページを開く。

## 元に戻す

ダウングレードの手順はありません。新しいバージョンが更新したデータベースは、古いバージョンが `The database has migration N applied, but this version of AegisCMS does not know it` のようなメッセージで拒否します。戻すには、古いバージョンを入れ、更新の前に取った[バックアップを復元](backup-restore.ja.md)します。バックアップを最初に取るのは、このためです。
