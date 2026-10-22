# メディア（アップロードファイル）API

画像などのアップロードファイルは `[storage]` のバックエンド（ローカルディスク / S3 互換。[設定リファレンス](configuration.md#23-storage--アセットストレージ)）に保存し、公開 URL で配信します。Admin Studio では画像ブロックのアップロードと **Media Library**（一覧・選択・削除）から使います。

| API | 必要なロール | 内容 |
| --- | --- | --- |
| `POST /api/v1/storage/upload` | `editor` | マルチパートの `file` を保存し、`201` と `{ key, url, size, content_type }` を返す。キーは `YYYY/MM/<8 桁>-<ファイル名>`（`key` / `path` フィールドで指定も可）。上限は `[storage].max_upload_mb`（超過は `413`） |
| `GET /api/v1/storage/objects` | `viewer` | 新しい順の一覧。`prefix`（キーの前方一致）, `content_type`（mime type の前方一致。例 `image/`）, `q`（キーの部分一致）, `limit`（既定 50、最大 500）, `offset` |
| `DELETE /api/v1/storage/objects/{key}` | `editor` | ファイルを削除する（下記） |

すべての操作は監査ログに記録されます（`storage.upload` / `storage.delete`。[監査ログ](audit.md#2-記録されるイベント)）。

---

## ファイルの削除 — `DELETE /api/v1/storage/objects/{key}`

`{key}` は一覧・アップロードの応答の `key`（例: `2026/10/1a2b3c4d-photo.png`）です。`/` を含めたままパスに続けます（各セグメントは URL エンコード可）。

```bash
curl -i -X DELETE http://127.0.0.1:8080/api/v1/storage/objects/2026/10/1a2b3c4d-photo.png \
  -H "Authorization: Bearer $TOKEN"
```

| ステータス | 条件 |
| --- | --- |
| `204` | 削除した |
| `400` | キーが空、または `..` を含む |
| `401` / `403` | セッションが無い・無効 / `editor` 未満 |
| `404` | そのキーのファイルが無い |
| `409` | 保存済みコンテンツから参照されている（`force=true` が無い場合）。何も削除しない |
| `500` | ストレージ・DB のエラー |

### 使用中のファイル（`409`）

削除の前に、全コンテンツタイプの保存済みコンテンツの本文（ブロック）とカスタムフィールドに、そのファイルの公開 URL（`url`）が含まれていないかを調べます。見つかった場合は削除せず、参照元を返します。

```json
{
  "error": "File is used by 1 content item(s); pass force=true to delete it anyway",
  "references": [
    { "content_type": "article", "id": "2dc94a91-...", "slug": "hello-world", "title": "Hello World" }
  ]
}
```

それでも削除する場合は `?force=true` を付けます（参照しているページの画像はリンク切れになります）。Admin Studio では、確認ダイアログで参照元を表示し、**Delete anyway** で強制削除します。

- 判定は公開 URL の文字列一致です。URL を別の形（CDN の別ドメイン、クエリ付きなど）で書いた参照や、テーマ・外部サイトからの参照は検出しません。
- 参照の確認は削除のたびに全コンテンツを読み込むため、コンテンツ数に比例して時間がかかります。
- 監査ログの `details` には `url`、`force`、`references`（削除時点の参照元の件数）を記録します。
