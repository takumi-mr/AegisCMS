[← ドキュメント一覧へ戻る](README.ja.md)

[English](upgrade.md) | **日本語**

# AegisCMS の更新

## 更新の前に

1. [Releases](https://github.com/takumi-mr/AegisCMS/releases) のリリースノートを読みます。**BREAKING** と書かれた変更は、こちらの対応が必要なことがあります。
2. [バックアップを取り](backup-restore.ja.md)、検証します。
3. 今のバージョンを確認します: `aegis-server --version`（Docker は `docker exec aegiscms aegis-server --version`）。

## バージョンごとの手順

### v0.13.0 への更新: 一部の公式プラグインはストアから入れる

v0.13.0 から、次の公式プラグインは AegisCMS に同梱されなくなり、[パッケージストア](store.ja.md)から入れるものになりました: `official-ads`・`official-back-to-top`・`official-charts`・`official-clarity`・`official-code-lines`・`official-embeds`・`official-external-links`・`official-lightbox`・`official-link-embed`・`official-reading-time`・`official-share-buttons`。

どれも有効にしていなければ、することはありません。どれかを有効にしていると、入れ直すまで**新しいバージョンが起動しません**（ログに `Plugin '…' is enabled in [plugins].enabled but not installed` と出ます）。設定は残ります。更新の前に、次のようにします。

1. 有効にしているものを控えてから、無効にします（Admin Studio › Plugins、または `[plugins].enabled` から外す）。
2. 下の手順で更新します。`[paths]` は新しい同梱の設定を使います（または自分で `plugins_dir = "data/plugins"`・`bundled_plugins_dir = "plugins"` にします。[設定の 2.5](configuration.ja.md#25-paths--ディレクトリ)）。入れたものが、次からの更新でも残るようにするためです。
3. Admin Studio › Plugins の**ストア**のタブから入れ直します。設定で `[store]` の `enabled = false` にしていなければ、ストアは使えます（[設定の 2.28](configuration.ja.md#228-store--パッケージストア)）。ほかに、`POST /api/v1/store/install`（`{"catalog": "official", "kind": "plugin", "id": "official-charts"}`。[設定の 2.28](configuration.ja.md#228-store--パッケージストア)）を使うか、[ストアの Releases](https://github.com/takumi-mr/AegisCMS-Store/releases) から ZIP をダウンロードし、同じタブでアップロードしても入れられます。
4. もう一度有効にします。

同梱の `config/default.toml` も、**プラグインを何も有効にしなくなりました**（以前は `official-code-enhancer`・`official-mermaid`・`official-plantuml` を有効にしていました）。同梱のファイルのまま動かしているサイトや、その `[plugins]` を写したサイトでは、コードのハイライトと図が出なくなります。Admin Studio › Plugins で有効にするか（このリリースではまだ同梱しています）、`[plugins].enabled` に戻してください。ストアのタブでは、おすすめとして並びます。

## 更新する

データベースの変更は、新しいバージョンを初めて起動したときに自動で適用されます。移行の手順を自分で実行する必要はなく、古いバージョンから最新へ一気に更新して構いません。

### Docker

```bash
docker pull ghcr.io/takumi-mr/aegiscms:latest
docker stop aegiscms && docker rm aegiscms
docker run -d --name aegiscms -p 8080:8080 -v aegis-data:/app/data ghcr.io/takumi-mr/aegiscms:latest
```

以前と同じボリューム（`-v`）とオプションを使ってください。データはボリュームに残ります。Admin Studio やストアから入れたプラグイン・テーマもボリューム（`/app/data/plugins`・`/app/data/themes`）に残り、同梱のものはイメージと一緒に更新されます。`deploy/` では `./deploy.sh pull && ./deploy.sh up` を実行します。

#### バージョンを固定する（Docker）

`latest` は常に最新のリリースを指すので、`docker pull` で破壊的な変更をまたいでしまうことがあります。本番のサイトでは、使いたいバージョンを指定してください。

| タグ                                | 追従するもの                                   |
| ----------------------------------- | ---------------------------------------------- |
| `ghcr.io/takumi-mr/aegiscms:0.9.0`  | 何にも追従しない。このリリースだけ             |
| `ghcr.io/takumi-mr/aegiscms:0.9`    | 修正（`0.9.1`、`0.9.2`…）。`0.10` には動かない |
| `ghcr.io/takumi-mr/aegiscms:latest` | 新しいリリースすべて                           |

1.0 になるまでは、マイナーバージョンが上がる（`0.9` から `0.10`）と破壊的な変更が入ることがあるので、`0.9` がおすすめです。もう一度 `docker pull` するだけで修正を受け取り、次のマイナーバージョンへは、リリースノートを読んだうえで自分のタイミングで移れます。バージョンタグは `0.9.0` 以降にあります。

### バイナリ

1. サーバーを止めます。
2. 新しいアーカイブをダウンロードし、**新しいフォルダ**に展開します。
3. 古いフォルダからデータと設定をコピーします: `aegis.db*`、`data/`、編集した `config/` と `schemas/`。新しい設定が増えていることがあるので、新しい `config/default.toml` と `schemas/` を自分のものと見比べてください。
4. 同梱のプラグイン・テーマ（`plugins/`・`themes/`）は、新しいフォルダのものを使います（リリースに合わせて更新されます）。Admin Studio やストアから入れたものは、同梱の設定（`[paths].plugins_dir = "data/plugins"`・`themes_dir = "data/themes"`）なら `data/` の下にあるので、手順 3 で一緒に移ります。古い設定（`plugins_dir = "plugins"`）のままなら、自分で入れたものを新しい `plugins/`・`themes/` にコピーします（[設定の 2.5](configuration.ja.md#25-paths--ディレクトリ)）。
5. 新しいバイナリを起動します。すべて動くと確認できるまで、古いフォルダは残しておきます。

サービスとして動かしている場合は、`WorkingDirectory` と `ExecStart` を新しいフォルダに向けます。

## 動作を確認する

- ログにエラーがなく、`/` と `/admin/` が開く。
- Admin Studio にログインして、いくつかのページを開く。

## 元に戻す

ダウングレードの手順はありません。新しいバージョンが更新したデータベースは、古いバージョンが `The database has migration N applied, but this version of AegisCMS does not know it` のようなメッセージで拒否します。戻すには、古いバージョンを入れ、更新の前に取った[バックアップを復元](backup-restore.ja.md)します。バックアップを最初に取るのは、このためです。
