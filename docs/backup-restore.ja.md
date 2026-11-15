[← ドキュメント一覧へ戻る](README.ja.md)

[English](backup-restore.md) | **日本語**

# バックアップと復元

## 何がバックアップされるか

`aegis-server dump` は、次を 1 つの ZIP ファイルにまとめます。

- データベース: コンテンツとその版の履歴、設定、アカウント（パスワードのハッシュを含む）
- アップロードしたファイル

設定ファイル、スキーマ、テーマ、プラグインは**含まれません**。これらは Git で管理してください。ZIP にはアカウントが入っているので、秘密情報として保管します。

形式はデータベースに依存しないので、バックアップを使って SQLite から PostgreSQL や MySQL へ移すこともできます。

## バックアップを取る

サーバーを動かしたままで実行できます。

```bash
# バイナリ（展開したフォルダで）
./aegis-server dump --output backup.zip

# Docker: コンテナの中に作り、外へコピーする
docker exec aegiscms aegis-server dump --output /app/data/backup.zip
docker cp aegiscms:/app/data/backup.zip ./backup-$(date +%F).zip
docker exec aegiscms rm /app/data/backup.zip
```

データベースだけなら `--no-storage`、アップロードだけなら `--no-database` を付けます。

**[更新](upgrade.ja.md)の前には必ず**取ります。運用中のサイトでは、cron などで定期的にも取ってください。コピーは別のマシンにも置きます。

`deploy/` の複数サイト構成では、サイトのデータボリュームのバックアップを [deploy/README.ja.md の運用](../deploy/README.ja.md#5-運用) で確認してください。

## バックアップを検証する

```bash
./aegis-server restore --input backup.zip --verify-only
```

何も変更せずに、ファイルのチェックサムを確かめます。検証したことのないバックアップは、バックアップとは言えません。

## 復元する

できれば、新しい空のデータフォルダ（または新しい Docker ボリューム）に復元します。

```bash
# バイナリ: 同じ config・schemas・themes・plugins を置いた空のフォルダで
./aegis-server restore --input backup.zip

# Docker: まだ一度も起動していない新しいボリュームへ
docker run --rm -v aegis-data:/app/data -v "$PWD:/backup" \
  ghcr.io/takumi-mr/aegiscms:latest restore --input /backup/backup.zip
```

そのあとは、いつもどおりサーバーを起動します。

### すでに起動したサイトに復元する

一度でも起動したサーバーは、いくつかの設定をすでに持っています。そのため、そのまま復元すると `already exists (use overwrite option to replace)` のようなエラーで止まります。誤ってデータを壊さないための確認です。今ある内容を本当に置き換えたいときだけ、`--overwrite` を付けます。

```bash
./aegis-server restore --input backup.zip --overwrite
```

### スキーマを変えたとき

カスタムフィールドは、バックアップ時点のスキーマではなく、`schemas` フォルダにある**現在の**スキーマに合わせて復元されます。使いたいスキーマは、復元の**前に**置いてください。スキーマにもうないフィールドは捨てられ、`Warning:` の行でどれかが示されます。標準のフィールド（タイトル、本文、日付）が黙って捨てられることはありません。

## 必要になる前に復元を試す

一度、予備のフォルダかボリュームにバックアップを復元して、開いてみてください。手順が通ることと、かかる時間が分かります。
