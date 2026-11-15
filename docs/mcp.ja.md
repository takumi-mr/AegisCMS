[← ドキュメント一覧へ戻る](README.ja.md)

[English](mcp.md) | **日本語**

# AI エージェントから使う (MCP)

AegisCMS には [MCP](https://modelcontextprotocol.io/)（Model Context Protocol）サーバーが組み込まれています。Claude Code や Cursor などの AI アシスタントが、スクリプトを挟まずにサイトの内容を読み、下書きを書けます。

- **アドレス**: サイトの `POST /mcp`（例: `https://example.com/mcp`）。トランスポートは Streamable HTTP で、サーバー側の追加設定は要りません。
- **認証**: [API トークン](content-api.ja.md#13-スクリプトciエージェント向けの-api-トークン)を `Authorization: Bearer aegis_pat_...` で送ります。ほかの方法は受け付けず、付けないと `401` です。
- **権限**: REST API と同じです。ツールはトークンのロールで動くので、同じ検査（ロール、レビューの流れ、入力の検証）を通り、ほかの変更と同じく[監査ログ](audit.ja.md)に残ります。

> [!IMPORTANT]
> この版でできるのは**読み取りと下書きの保存**です。公開、非公開、状態の変更、削除はできません。MCP からの変更は、人が Admin Studio で公開するまで訪問者には見えません。

## 1. つなぐ

1. API トークンを発行します（[Admin Studio › プロフィール › API トークン](admin-studio.ja.md#4-プロフィール)）。足りる範囲で一番低いロールを選びます。読むだけなら `viewer`、下書きを書くなら `contributor` か `editor` です。
2. クライアントにサーバーを追加します。Claude Code の例:

    ```bash
    claude mcp add --transport http aegiscms https://example.com/mcp \
      --header "Authorization: Bearer aegis_pat_..."
    ```

    URL とヘッダーを受け付けるクライアントなら、同じ 2 つの値を設定します。サイトをサブパスで公開している場合（[`[server].base_path`](configuration.ja.md)）は、`/mcp` の前に付けます。

3. アシスタントに頼みます。たとえば「下書きの記事を一覧にして」「新しいリリースについての記事を下書きで書いて」。

> [!NOTE]
> OAuth でしかサインインできないクライアント（たとえば Web 版 Claude.ai のカスタムコネクタ）は、まだつなげません。OAuth は後続の対応として予定しています。

## 2. ツール

| ツール               | できること                                                                                                                                                       |
| -------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `list_content_types` | サイトのコンテンツタイプ（記事、固定ページなど）。                                                                                                               |
| `get_content_schema` | 1 つのコンテンツタイプのフィールド。                                                                                                                             |
| `list_content`       | タイプごとの一覧（新しい順）。本文のない要約です。`q`・`status`・`tag`・`category` で絞り、`limit`（既定 20、最大 100）と `offset` でページを送ります。          |
| `get_content`        | スラッグか ID で 1 件。本文のブロックとカスタムフィールドを含みます。                                                                                            |
| `list_tags`          | サイトのタグ。                                                                                                                                                   |
| `create_draft`       | 新しい 1 件を、必ず**下書き**で保存します。`content_type`・`title`・`slug` が必須で、`blocks` と `fields` は任意です。                                           |
| `update_content`     | 既存の 1 件を変更します。変えたい項目だけ送ります。`title`、`slug`、`blocks`（本文を丸ごと置き換え）、`fields`（今のカスタムフィールドにマージ）が指定できます。 |

読み取りで見えるものは、REST API と同じくトークンのロールで決まります。`viewer` のトークンには、Admin Studio で閲覧者に見える範囲が見えます。

### 書き込みの動き

- **`create_draft` は必ず下書きで保存します。** 一緒に送った `status` や `publish` は無視されます。スラッグはタイプの中で重複できません。
- **`update_content` は状態を変えません。** 公開中の 1 件なら、変更は**作業用のコピー**として保存されます。人が Admin Studio で**公開版を更新**を押すまで、訪問者には公開版が見え続けます（[コンテンツを書いて公開する](admin-editing.ja.md#4-状態と公開)）。
- 書き込めないロールや、他人の 1 件を編集できないロールのトークンは、REST API と同じ理由で断られます（`403`）。
- 本文はブロックのリストで、REST API と同じ形式です（[本文ブロックの仕様](content-api.ja.md#3-本文ブロックの仕様-blocks)）。形が分からないときは、似た 1 件を `get_content` で読ませてから書かせると確実です。

ツールが失敗したときは、REST API の HTTP ステータスとエラーメッセージがアシスタントに返ります（たとえば、スラッグがすでに使われていれば `409`、存在しない 1 件なら `404`）。

## 3. この版の制限

- ツールは上の 7 つだけです。公開、予約、削除、メディアのアップロード、タグの付与は、まだ MCP からはできません。Admin Studio か [REST API](content-api.ja.md) を使ってください。
- エンドポイントを止めるスイッチはありません。アシスタントを止めたいときは、そのトークンを失効させます。
- 受け付けるのは `POST /mcp` だけです（サーバーからのストリームはありません）。`GET /mcp` は `405` を返します。

## 関連ガイド

- [投稿 API](content-api.ja.md)
- [認証・API トークン](authentication.ja.md#13-api-トークン)
- [コンテンツを書いて公開する](admin-editing.ja.md)
