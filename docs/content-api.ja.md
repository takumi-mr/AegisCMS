[← ドキュメント一覧に戻る](README.ja.md)

[English](content-api.ja.md) | **日本語**

# 記事・コンテンツ投稿 API (Content Management API)

外部ツール、CI/CD パイプライン、静的サイト連携、CLI スクリプトなどから AegisCMS に記事や固定ページなどのコンテンツを自動投稿・編集・公開するための REST API ガイドです。

認証なしで公開データのみを取得する[ヘッドレス配信 API](headless.ja.md)（読み取り専用）とは異なり、認証トークン（Bearer トークン）を用いてコンテンツの作成・下書き保存・更新・即時公開・予約公開・タグ付け・削除を行います。

---

## 1. 認証と権限 (Authentication & Roles)

コンテンツの作成・更新 API には、すべて `Authorization: Bearer <token>` ヘッダーが必要です。トークンは 2 種類あります。

- **API トークン**（`aegis_pat_...`）: 長く使え、あとから失効できます。スクリプト・CI・AI エージェントには、こちらを使います（[1.3](#13-スクリプトciエージェント向けの-api-トークン)）。
- **セッショントークン**（`ses_...`）: ユーザー名とパスワードでログインして得ます。約 1 時間で期限が切れます（[1.1](#11-ログインセッショントークン)）。

### 1.1 ログイン（セッショントークン）

登録済みのアカウントでセッションを発行します。

```bash
curl -X POST "http://localhost:8080/api/v1/auth/session" \
  -H "Content-Type: application/json" \
  -d '{
    "username": "editor_user",
    "password": "your-secure-password"
  }'
```

**成功時のレスポンス (201 Created):**

```json
{
  "id": "ses_0ce22abac5cc409298d05447e7a002b1",
  "user_id": "editor_user",
  "role": "editor",
  "created_at": "2026-11-15T01:52:00+00:00",
  "expires_at": "2026-11-15T02:52:00+00:00",
  "metadata": { "provider": "local" }
}
```

トークンは **`id`** です（`token` というフィールドはありません）。以降のリクエストのヘッダーに指定します。

```http
Authorization: Bearer ses_0ce22abac5cc409298d05447e7a002b1
```

（`X-Session-Id: <id>` でも使えます。）セッションの有効期間は、既定で 1 時間です（`[cache].default_ttl_secs`）。パスワードが違うと `401` で、失敗が続くとしばらくロックされます（`429`）。詳しくは[認証](authentication.ja.md)を参照してください。

### 1.2 ロールと実行可能操作

操作に必要なロールは以下のとおりです（詳細は [認証 & ユーザー管理](authentication.ja.md) を参照）。

| 操作                                                     | 必要なロール       | 備考                                                                                          |
| -------------------------------------------------------- | ------------------ | --------------------------------------------------------------------------------------------- |
| 下書き (`draft`) / レビュー待ち (`pending`) の作成       | `contributor`      | 自身が著者となるコンテンツのみ（※`reviewer` は新規作成不可）                                  |
| 公開 (`published`) / 予約公開 (`scheduled`) での新規作成 | `editor` 以上      | 即時公開または日時指定公開                                                                    |
| レビュー待ち (`pending`) 記事の校正・承認・差し戻し      | `reviewer` 以上    | `reviewer` は `pending` の手直し、承認 (`published`/`scheduled`)、差し戻し (`draft`) のみ可能 |
| 既存コンテンツの自由な変更・直接公開                     | `editor` 以上      | `contributor` は自身の `draft` / `pending` のみ編集可能                                       |
| 他のユーザーを著者 (`author_id`) に指定                  | `admin` のみ       | 省略時はログインユーザーが著者になる                                                          |
| コンテンツの削除                                         | `editor` 以上      | `reviewer` は削除不可。`contributor` は自分の `draft` / `pending` のみ削除可能                |
| タグの紐付け (`PUT .../tags`)                            | `contributor` 以上 | `reviewer` は `pending` 記事のタグ付けを調整可能                                              |

> [!TIP]
> **LLMマルチエージェント自動投稿のベストプラクティス**
>
> - **執筆エージェント**: `contributor` ロールのアカウントを使用し、`status: "pending"` で記事を提出します（勝手に公開されない安全なサンドボックス）。
> - **校閲・監査エージェント**: `reviewer` ロールのアカウントを使用し、提出された `pending` 記事の内容を検査・校正し、問題なければ `status: "published"` で承認、修正が必要なら `status: "draft"` に `review_note` を添えて差し戻します。`reviewer` には新規作成権限や削除権限がないため、万が一の誤動作やプロンプトインジェクションでも既存サイトが破壊されません。

### 1.3 スクリプト・CI・エージェント向けの API トークン

人が操作しないもの（スクリプト・CI・AI エージェントなど）には、ユーザー名とパスワードの代わりに API トークンを使います。パスワードをどこにも保管する必要がなく、漏れたときはトークンを失効させるだけで済みます。詳しくは[認証の API トークン](authentication.ja.md#13-api-トークン)にあります。

**トークンを発行する**（トークンは**一度しか表示されない**ので、すぐに控えてください）。便利な方法を選びます。

- **Admin Studio**: ヘッダーから**プロフィール**を開き、**API トークン**で発行します（[Admin Studio](admin-studio.ja.md#4-プロフィール)）。
- **サーバー上で**:

    ```bash
    aegis-server user token-create editor_user --name ci-poster --role editor --days 90
    ```

- **セッションを使って**（トークンでトークンは作れません）:

    ```bash
    curl -X POST "http://localhost:8080/api/v1/users/me/tokens" \
      -H "Authorization: Bearer <セッションの id>" \
      -H "Content-Type: application/json" \
      -d '{"name": "ci-poster", "role": "editor", "expires_in_days": 90}'
    ```

    レスポンス（`201`）の `token` フィールドに、`aegis_pat_` で始まるトークンが入っています。

**使い方**は、セッショントークンとまったく同じです。

```bash
export AEGIS_TOKEN='aegis_pat_...'

curl -X POST "http://localhost:8080/api/v1/contents/article" \
  -H "Authorization: Bearer $AEGIS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"title": "トークンで投稿", "slug": "posted-with-a-token", "status": "published"}'
```

トークンは、CI のシークレット機能や環境変数に置き、リポジトリには入れないでください。

**トークンでできること:**

| 項目              | ルール                                                                                                                                                  |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `role`            | `viewer`（既定）、`contributor`、`reviewer`、`editor`。自分のロールより高くはできず、`admin` は選べません。動く範囲でいちばん低いロールにしてください。 |
| `expires_in_days` | 1 から 3650。既定は 90。`null` は無期限です。期限が切れると `401` になります。                                                                          |
| 制限              | 1 ユーザーにつき 20 個まで。トークンでは、トークンの発行・失効、パスワードの変更、二段階認証の管理はできません（`403`）。                               |
| 持ち主が…         | 降格されると、トークンも降格されます。無効化・削除されると、トークンは使えなくなります。                                                                |
| 失効              | **プロフィール**で失効させます（またはセッションで `DELETE /api/v1/users/me/tokens/{id}`）。すぐに使えなくなります。                                    |
| バックアップ      | トークンは[バックアップ](backup-restore.ja.md)に**含まれません**。復元したあとに、発行し直してください。                                                |

> [!TIP]
> ロールの働きはセッションと同じです（[1.2](#12-ロールと実行可能操作)）。執筆用のスクリプトには `contributor` のトークンを使い、`"status": "pending"` で投稿します。公開できるのは、編集者のトークン（または人）だけです。

---

## 2. 記事の新規作成 (`POST /api/v1/contents/{content_type}`)

指定したコンテンツタイプ（`article` や `page` など）の記事を作成します。

- **メソッド**: `POST`
- **パス**: `/api/v1/contents/{content_type}` (例: `/api/v1/contents/article`)
- **ヘッダー**:
    - `Authorization: Bearer <token>`
    - `Content-Type: application/json`

### 2.1 リクエストボディ (JSON)

```json
{
  "title": "AegisCMS API 経由での記事投稿ガイド",
  "slug": "posting-via-api-guide",
  "status": "published",
  "blocks": [
    {
      "name": "core/paragraph",
      "attributes": {},
      "text": "この記事は API を経由して自動投稿された記事です。",
      "children": []
    }
  ],
  "published_at": "2026-11-10T15:00:00Z",
  "fields": {
    "excerpt": "API を使って記事を作成・公開する手順の解説です。",
    "category_id": "cat-tech"
  }
}
```

### 2.2 パラメータ仕様

| フィールド     | 型      | 必須     | 既定値             | 説明                                                                                                                                                                                                                                                                |
| -------------- | ------- | -------- | ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `title`        | string  | **必須** | -                  | 記事のタイトル。                                                                                                                                                                                                                                                    |
| `slug`         | string  | **必須** | -                  | URL スラッグ（英数字・ハイフンなど）。同一 `content_type` 内で一意。                                                                                                                                                                                                |
| `status`       | string  | 任意     | `"draft"`          | 公開ステータス: `"draft"`, `"pending"`, `"published"`, `"scheduled"`, `"archived"` のいずれか。ほかの値は `400` です。省略すると、新規作成は `"draft"`、更新は現在の状態のままです。                                                                                |
| `blocks`       | array   | 任意     | `[]`               | 本文ブロックの配列（後述の [3. 本文ブロックの仕様](#3-本文ブロックの仕様-blocks) 参照）。                                                                                                                                                                           |
| `author_id`    | string  | 任意     | (ログインユーザー) | 著者のユーザー ID。ログインユーザー以外を指定できるのは `admin` のみ。                                                                                                                                                                                              |
| `published_at` | string  | 条件付   | `null`             | RFC 3339 形式の公開日時（例: `2026-11-10T12:00:00Z`）。`status: "scheduled"` の場合は必須。`"published"` では無視され、保存した時点で公開されます。                                                                                                                 |
| `unpublish_at` | string  | 任意     | `null`             | RFC 3339 形式の予約非公開（アーカイブ）日時。`published` または `scheduled` のときだけ使われ（公開日時より後にします）、ほかの状態では黙って無視されます。                                                                                                          |
| `publish`      | boolean | 任意     | `false`            | 保存した版を公開版にします。`"published"` で新規作成するときは、なくても公開されますが、すでに公開済みのコンテンツを**更新**するときは `true` が必要です。ないと、訪問者には前の版が見え続けます（[7.1](#71-記事の更新-put-apiv1contentscontent_typeslug_or_id)）。 |
| `review_note`  | string  | 任意     | `null`             | レビュー依頼・承認・差し戻しのメモ。監査ログや版の履歴に記録されます。                                                                                                                                                                                              |
| `fields`       | object  | 任意     | `{}`               | スキーマ（`schemas/*.yaml`）で定義されたカスタムフィールド（要約、カテゴリ、会員アクセス権など）。                                                                                                                                                                  |

> [!IMPORTANT]
> **カスタムフィールドの記述位置に関する注意**  
> スキーマで定義したカスタムフィールド（`excerpt` や `category_id` など）は、必ず `"fields": { ... }` の下にネストして記述してください。トップレベルに直接未知のキーを書くと、`400 Bad Request`（`unknown field`）が返されます。

### 2.3 ステータスの種類

- `"draft"`: 下書き。公開サイトには表示されず、管理画面や編集用 API からのみ閲覧可能。
- `"pending"`: レビュー待ち。`contributor` が執筆を完了し、編集者に承認・公開を依頼した状態。
- `"published"`: 公開。`"published"` で新規作成した記事は、公開サイトおよびヘッドレス配信 API にすぐ反映されます。あとの更新については [7.1](#71-記事の更新-put-apiv1contentscontent_typeslug_or_id) を参照してください。
- `"scheduled"`: 予約公開。`published_at` に指定した日時になると、バックグラウンドスケジューラによって自動的に `published` に切り替わります。
- `"archived"`: アーカイブ（非公開）。過去に公開されていたが現在は取り下げられた状態。

---

## 3. 本文ブロックの仕様 (`blocks`)

AegisCMS では記事本文を HTML 直書きではなく、構造化された AST ブロック（`BlockNode`）の配列として管理します（詳細は [本文ブロック & Markdown リファレンス](blocks.ja.md)）。

```json
{
  "name": "core/paragraph",
  "attributes": {},
  "text": "段落のテキスト",
  "children": []
}
```

### 代表的なブロック一覧

#### 段落 (`core/paragraph`)

```json
{
  "name": "core/paragraph",
  "attributes": {},
  "text": "これは段落です。**太字**や *斜体*、[リンク](https://example.com) のインライン書式が使えます。",
  "children": []
}
```

#### 見出し (`core/heading`)

`attributes.level` で 1〜6 を指定します（既定値は 2）。見出しの `id` は保存時に自動付与されます。

```json
{
  "name": "core/heading",
  "attributes": { "level": 2 },
  "text": "大見出しのタイトル",
  "children": []
}
```

#### コードブロック (`core/code`)

```json
{
  "name": "core/code",
  "attributes": {
    "language": "rust",
    "filename": "src/main.rs"
  },
  "text": "fn main() {\n    println!(\"Hello, AegisCMS!\");\n}",
  "children": []
}
```

#### 順不同リスト (`core/list`)

子は `core/list-item` を並べます。

```json
{
  "name": "core/list",
  "attributes": { "ordered": false },
  "children": [
    { "name": "core/list-item", "attributes": {}, "text": "第1項目", "children": [] },
    { "name": "core/list-item", "attributes": {}, "text": "第2項目", "children": [] }
  ]
}
```

#### 画像 (`core/image`)

```json
{
  "name": "core/image",
  "attributes": {
    "src": "/uploads/2026/11/9fc38a8e-architecture.png",
    "alt": "アーキテクチャ図"
  },
  "children": []
}
```

#### 注記 / コールアウト (`core/callout`)

`variant` は `"note"`, `"tip"`, `"important"`, `"warning"`, `"caution"` から選択します。

```json
{
  "name": "core/callout",
  "attributes": {
    "variant": "tip",
    "title": "ヒント"
  },
  "text": "API 投稿時はトークンの有効期限にご注意ください。",
  "children": []
}
```

#### 表 (`core/table`)

```json
{
  "name": "core/table",
  "attributes": {
    "header": true,
    "align": ["left", "center", "right"],
    "rows": [
      ["機能", "状態", "備考"],
      ["API 投稿", "安定版", "RESTful"],
      ["下書き保存", "安定版", "自動バージョン管理"]
    ]
  },
  "children": []
}
```

#### 会員限定区切り (`core/paywall`)

これ以降のブロックは、権限を持つ会員にのみ表示されます（[会員アカウント](members.ja.md)）。

```json
{
  "name": "core/paywall",
  "attributes": {},
  "children": []
}
```

---

## 4. カスタムフィールドとメタデータ (`fields`)

`fields` オブジェクトには、対象のスキーマ（`schemas/{content_type}.yaml`）で定義されたフィールドを設定します。

```json
"fields": {
  "excerpt": "記事の一覧や SNS シェア用に使用される抜粋文です。",
  "category_id": "cat-tech",
  "access": "public",
  "custom_meta": {
    "seo": {
      "canonical_url": "https://example.com/original-article"
    }
  }
}
```

- **`excerpt`**: 記事の要約（一覧画面や RSS/Atom フィード、SEO メタタグで使用）。
- **`category_id`**: 所属するカテゴリの ID（カテゴリとのリレーションが定義されている場合）。
- **`access`**: 会員限定設定（`"public"`（全員公開）, `"free"`（無料会員以上）, `"paid"`（有料会員限定））。設定にない値を入れると `400 Bad Request` になります。
- **`custom_meta`**: 任意の構造化 JSONB メタデータ。

---

## 5. 画像・メディアのアップロード

記事内に画像を挿入する場合は、先にメディアアップロード API を呼び出してストレージキーや URL を取得します（詳細は [メディア管理](media.ja.md) を参照）。

### アップロード API

- **メソッド**: `POST`
- **パス**: `/api/v1/storage/upload`
- **形式**: `multipart/form-data`
- **フィールド名**: `file`

```bash
curl -X POST "http://localhost:8080/api/v1/storage/upload" \
  -H "Authorization: Bearer <token>" \
  -F "file=@./screenshot.png"
```

**成功レスポンス (201 Created):**

```json
{
  "key": "2026/11/9fc38a8e-screenshot.png",
  "url": "/uploads/2026/11/9fc38a8e-screenshot.png",
  "size": 70,
  "content_type": "image/png"
}
```

取得した `url` を `core/image` ブロックの `attributes.src` に設定します。サーバーがファイル名の先頭に短いランダムな文字列を付けるので、必ず返ってきた `key` と `url` を使い、自分で組み立てないでください。

---

## 6. タグの紐付け (`PUT /api/v1/contents/{content_type}/{slug}/tags`)

投稿した記事にタグを紐付けるには、タグ更新 API を呼び出します（詳細は [タグ機能](tags.ja.md) を参照）。

- **メソッド**: `PUT`
- **パス**: `/api/v1/contents/{content_type}/{slug_or_id}/tags`
- **リクエストボディ**:
    ```json
    {
      "tag_ids": [
        "tag-rust-uuid",
        "tag-api-uuid"
      ]
    }
    ```

※既存のタグ一覧は `GET /api/v1/tags` で取得できます。タグを新規登録する場合は `POST /api/v1/tags` を実行します。

---

## 7. 記事の更新・削除

### 7.1 記事の更新 (`PUT /api/v1/contents/{content_type}/{slug_or_id}`)

記事の内容を上書き更新します。本文やフィールドは全体が新しい値に置き換わります（送信しなかったカスタムフィールドは削除されます）。

- **メソッド**: `PUT`
- **パス**: `/api/v1/contents/{content_type}/{slug_or_id}`
- **リクエストボディ**: 作成時と同様の JSON ペイロード。

> [!WARNING]
> **`status` は省略すると維持されますが、`blocks` と `fields` は維持されません。** 更新では、本文とカスタムフィールドが送った内容に置き換わるので、記事全体を送ってください。`status` を省略すると、記事は今の状態のままです（公開中なら公開のまま、予約公開なら予約の日時も保たれます）。変えたいときだけ `status` を送ります。

> [!IMPORTANT]
> **公開済みの記事を更新しただけでは、公開ページは変わりません。** 更新は作業用のコピーとして保存され、`"publish": true` を送るまで、訪問者には前の公開版が見え続けます。公開ページ・フィード・検索は、公開版に従います。
>
> ```bash
> curl -X PUT "http://localhost:8080/api/v1/contents/article/api-post-test" \
>   -H "Authorization: Bearer $AEGIS_TOKEN" \
>   -H "Content-Type: application/json" \
>   -d '{"title": "新しいタイトル", "slug": "api-post-test", "status": "published", "publish": true, "blocks": []}'
> ```

> [!NOTE]
> 記事を更新するたびに新しいリビジョン（バージョン）が自動的に保存されます。管理画面から過去のリビジョンへの復元が可能です。

### 7.1a 版の内容の取得 (`GET /api/v1/contents/{content_type}/{slug_or_id}/versions/{revision}`)

保存済みの 1 つの版の内容（`title`・`slug`・`blocks`・`fields`、版の `revision`・`created_at`・`created_by`・`change_note`）を返します。何も変更しません。版の一覧（`.../versions`）と同じく、ログインしていれば取得できます。`status`・公開日時・著者は、復元しても戻らない（現在の値のまま）ので返しません。存在しない版は 404 です。

管理画面は、版履歴の「確認」でこれを呼び、その版のプレビューと、いまエディターにある内容との差分（タイトル・スラッグ・カスタムフィールド・本文の Markdown の行単位）を表示します。

### 7.2 記事の削除 (`DELETE /api/v1/contents/{content_type}/{slug_or_id}`)

指定した記事を削除します。`editor` 以上が必要です。`contributor` は自分の `draft` / `pending` だけ削除でき、`reviewer` は削除できません。

- **メソッド**: `DELETE`
- **パス**: `/api/v1/contents/{content_type}/{slug_or_id}`
- **成功時**: `204 No Content`

---

## 8. 実装コード例

例では、API トークン（[1.3](#13-スクリプトciエージェント向けの-api-トークン)）を環境変数 `AEGIS_TOKEN` から読みます。

### 8.1 cURL による投稿手順

```bash
export AEGIS_TOKEN='aegis_pat_...'

# 記事を新規投稿
curl -i -X POST "http://localhost:8080/api/v1/contents/article" \
  -H "Authorization: Bearer $AEGIS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "title": "API 投稿テスト",
    "slug": "api-post-test",
    "status": "published",
    "blocks": [
      {
        "name": "core/heading",
        "attributes": { "level": 2 },
        "text": "自動投稿の概要",
        "children": []
      },
      {
        "name": "core/paragraph",
        "attributes": {},
        "text": "この記事は cURL スクリプトから自動投稿されました。",
        "children": []
      }
    ],
    "fields": {
      "excerpt": "API 経由の投稿テストです。"
    }
  }'
```

トークンではなくユーザー名とパスワードを使うときは、先にログインして、レスポンスの `id` を使います。

```bash
TOKEN=$(curl -s -X POST "http://localhost:8080/api/v1/auth/session" \
  -H "Content-Type: application/json" \
  -d '{"username": "editor_user", "password": "your-password"}' | jq -r .id)
```

### 8.2 Python スクリプト例

```python
import os
import requests

BASE_URL = "http://localhost:8080/api/v1"

headers = {
    "Authorization": f"Bearer {os.environ['AEGIS_TOKEN']}",
    "Content-Type": "application/json",
}

article_payload = {
    "title": "Python から投稿した記事",
    "slug": "python-auto-post",
    "status": "published",
    "blocks": [
        {
            "name": "core/heading",
            "attributes": {"level": 2},
            "text": "はじめに",
            "children": [],
        },
        {
            "name": "core/paragraph",
            "attributes": {},
            "text": "Python の requests ライブラリで投稿しました。",
            "children": [],
        },
        {
            "name": "core/code",
            "attributes": {"language": "python", "filename": "post.py"},
            "text": "print('Hello, AegisCMS!')",
            "children": [],
        },
    ],
    "fields": {
        "excerpt": "Python スクリプトによる自動投稿のサンプルです。"
    },
}

res = requests.post(f"{BASE_URL}/contents/article", headers=headers, json=article_payload)
if res.status_code == 201:
    article = res.json()
    print(f"記事が作成されました: ID={article['id']}, URL=/articles/{article['slug']}")
else:
    print(f"作成に失敗しました ({res.status_code}): {res.text}")
```

### 8.3 TypeScript / Node.js スクリプト例

```typescript
interface BlockNode {
  name: string;
  attributes?: Record<string, unknown>;
  text?: string;
  children?: BlockNode[];
}

async function publishArticle() {
  const BASE_URL = 'http://localhost:8080/api/v1';
  const token = process.env.AEGIS_TOKEN;

  const payload = {
    title: 'TypeScript から自動投稿した記事',
    slug: 'typescript-auto-post',
    status: 'published',
    blocks: [
      {
        name: 'core/heading',
        attributes: { level: 2 },
        text: 'TypeScript との連携',
        children: [],
      },
      {
        name: 'core/paragraph',
        attributes: {},
        text: 'Node.js / Bun から直接記事を作成できます。',
        children: [],
      },
    ] as BlockNode[],
    fields: {
      excerpt: 'TypeScript による投稿スクリプトのサンプルです。',
    },
  };

  const createRes = await fetch(`${BASE_URL}/contents/article`, {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${token}`,
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(payload),
  });

  if (createRes.status === 201) {
    const data = await createRes.json();
    console.log('投稿しました:', data.id, data.slug);
  } else {
    const error = await createRes.json();
    console.error('投稿エラー:', createRes.status, error);
  }
}

publishArticle();
```

---

## 9. エラーハンドリング (Error Handling)

AegisCMS の REST API は、エラー時に標準的な HTTP ステータスコードと英語のエラーメッセージ（JSON）を返します。

```json
{
  "error": "Unpublish date ('unpublish_at') must be after publication date"
}
```

### 主なステータスコード一覧

| ステータスコード            | 意味                    | 主な原因と対策                                                                                                                                                                                                                                                                                                                                                   |
| --------------------------- | ----------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `400 Bad Request`           | 値が不正                | ・`status` が `draft`, `pending`, `scheduled`, `published`, `archived` のどれでもない<br>・`published_at` または `unpublish_at` の日時形式（RFC 3339）が不正<br>・`published_at` なしで `status: "scheduled"` を指定した<br>・`unpublish_at` が `published_at` より後ではない<br>・`access` に未定義の会員レベルを指定した<br>・`tag_ids` のタグ ID が存在しない |
| `401 Unauthorized`          | 認証エラー              | ・`Authorization` ヘッダーがない、またはトークンが無効・期限切れ                                                                                                                                                                                                                                                                                                 |
| `403 Forbidden`             | 権限不足                | ・`contributor` が `published` や `scheduled` で保存しようとした（`draft` または `pending` のみ可）<br>・`admin` 以外のユーザーが他人の `author_id` を指定した<br>・`contributor` が他人のコンテンツを変更しようとした<br>・`reviewer` がコンテンツを作成・削除しようとした<br>・ログインセッションが必要な操作を API トークンで行った                           |
| `404 Not Found`             | リソース未存在          | ・指定した `content_type` のスキーマが登録されていない<br>・更新対象の記事が存在しない                                                                                                                                                                                                                                                                           |
| `409 Conflict`              | 衝突・競合              | ・指定した `slug` が、同じ `content_type` の別の記事ですでに使われている（または、別の記事のスラッグに変更しようとした）                                                                                                                                                                                                                                         |
| `422 Unprocessable Entity`  | ボディ不正 / フック拒否 | ・必須フィールド（`title`, `slug`）が欠落している<br>・トップレベルに未知のキーが含まれている（カスタムフィールドは `fields` 内に置く）。メッセージに使えるキーが列挙されます<br>・登録された Wasm プラグインの `on-before-save` フックによって保存が拒否された                                                                                                  |
| `500 Internal Server Error` | サーバー内部エラー      | ・データベースエラーや予期せぬ障害                                                                                                                                                                                                                                                                                                                               |

---

## 10. 公開後の自動処理 (Side Effects)

記事が公開された場合（`published` ステータスで作成した場合、または `"publish": true` で更新した場合）、サーバー内部で以下の処理が自動的に実行されます。

1. **キャッシュのパージ (Cache Invalidation)**:
    - 公開サイトの該当記事ページ、一覧ページ、フィード（`/feed.xml`）の CDN / 内部キャッシュが自動的に破棄されます。
2. **全文検索インデックスの更新 (`aegis-search`)**:
    - tantivy 検索エンジンのインデックスキューに通知され、即座に検索結果へ反映されます（[サイト内検索](search.ja.md)）。
3. **プラグインフックの呼び出し**:
    - プラグインが登録している `on-content-published` などの Wasm フックが自動的に発火します（[プラグインフック仕様](plugin_hooks.ja.md)）。
4. **操作監査ログの記録 (`aegis-audit`)**:
    - `content.create` または `content.update` イベントが実行ユーザー・IP アドレスとともに監査ログに記録されます（[監査ログ](audit.ja.md)）。
