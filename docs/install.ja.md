[← ドキュメント一覧へ戻る](README.ja.md)

[English](install.md) | **日本語**

# AegisCMS の導入

起動方法は [Docker](#1-docker)、[Linux バイナリ](#2-linux-バイナリ)、[Windows バイナリ](#3-windows-バイナリ)から選びます。必要な環境は [動作環境](../README.ja.md#動作環境) を参照してください。どれも SQLite で起動するので、ほかに入れるものはありません。

起動したら [管理者を作成](#4-管理者を作成する) します。作るまでは誰もログインできません。

## 1. Docker

```bash
docker run -d --name aegiscms -p 8080:8080 -v aegis-data:/app/data ghcr.io/takumi-mr/aegiscms:latest
```

- サイトは `http://localhost:8080/`、Admin Studio は `http://localhost:8080/admin/` で開きます。
- データベースとアップロードしたファイルは `/app/data` に入ります。コンテナを入れ替えても残るよう、ボリューム（`-v aegis-data:/app/data`）に置いてください。
- ログは `docker logs aegiscms`、停止は `docker stop aegiscms` です。
- `latest` はリリースのたびに動きます。本番のサイトでは、`ghcr.io/takumi-mr/aegiscms:0.9` のようにバージョンを固定してください（[更新](upgrade.ja.md#バージョンを固定するdocker)）。
- イメージは `linux/amd64` と `linux/arm64` に対応しています。Docker が自動で合うほうを選びます。

## 2. Linux バイナリ

1. [Releases](https://github.com/takumi-mr/AegisCMS/releases/latest) から `aegiscms-<バージョン>-x86_64-unknown-linux-gnu.tar.gz`（arm64 は `aarch64-unknown-linux-gnu`）をダウンロードします。隣の `.sha256` で検証してください。
2. 展開し、**展開したフォルダの中で**起動します（設定・スキーマ・テーマ・プラグインを相対パスで探すためです）。

```bash
sha256sum -c aegiscms-*.tar.gz.sha256
tar xzf aegiscms-*.tar.gz
cd aegiscms-*/
./aegis-server run
```

glibc 2.35 以降が必要です（Ubuntu 22.04 以降、Debian 12 以降）。

既定の設定（`config/default.toml`）は `127.0.0.1:8080` だけで待ち受けるので、ほかのマシンからは届きません。前段にリバースプロキシを置くか（[公開](publish.ja.md)）、信頼できるネットワークなら `[server]` の `host = "0.0.0.0"` にします。ポートは `--port` か環境変数 `PORT` で変えます。

データベース（`aegis.db`）とアップロード（`data/`）は、カレントフォルダに作られます。

### サービスとして動かす（systemd）

```ini
# /etc/systemd/system/aegiscms.service
[Unit]
Description=AegisCMS
After=network.target

[Service]
User=aegis
WorkingDirectory=/opt/aegiscms
ExecStart=/opt/aegiscms/aegis-server run
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now aegiscms
```

`/opt/aegiscms` は展開したフォルダで、`aegis` ユーザーが書き込める必要があります。

## 3. Windows バイナリ

1. [Releases](https://github.com/takumi-mr/AegisCMS/releases/latest) から `aegiscms-<バージョン>-x86_64-pc-windows-msvc.zip` をダウンロードします。
2. 展開し、**展開したフォルダで**ターミナルを開いて実行します。

```powershell
.\aegis-server.exe run
```

`http://localhost:8080/` を開きます。Admin Studio は実行ファイルに組み込まれていて、`/admin/` にあります。Linux と同じく、既定では `127.0.0.1` だけで待ち受けます。

## 4. 管理者を作成する

新しいサイトにはアカウントがありません。AegisCMS を動かしているマシンで、最初の管理者を作ります。

```bash
# Docker（パスワードを聞かれます）
docker exec -it aegiscms aegis-server user add admin --role admin

# バイナリ（展開したフォルダで）
./aegis-server user add admin --role admin
```

スクリプトでは、パスワードを環境変数 `AEGIS_USER_PASSWORD` で渡せます。そのあと `/admin/` でログインします。

ほかのユーザー操作（`user list`、`user passwd`、`user disable` など）は [認証](authentication.ja.md) にあります。

## 次のステップ

- [HTTPS でインターネットに公開する](publish.ja.md)
- コンテンツを入れる前に [バックアップ](backup-restore.ja.md) を取る
- すべての設定は [設定リファレンス](configuration.ja.md)
