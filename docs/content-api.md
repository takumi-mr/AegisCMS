[← ドキュメント一覧に戻る](README.md)

# 記事・コンテンツ投稿 API (Content Management API)

外部ツール、CI/CD パイプライン、静的サイト連携、CLI スクリプトなどから AegisCMS に記事や固定ページなどのコンテンツを自動投稿・編集・公開するための REST API ガイドです。

認証なしで公開データのみを取得する[ヘッドレス配信 API](headless.md)（読み取り専用）とは異なり、認証トークン（Bearer トークン）を用いてコンテンツの作成・下書き保存・更新・即時公開・予約公開・タグ付け・削除を行います。

---

## 1. 認証と権限 (Authentication & Roles)

コンテンツの作成・更新 API には、すべて `Authorization: Bearer <token>` ヘッダーが必要です。

### 1.1 ログインとトークンの取得

まず、登録済みアカウントでセッションを発行します。

```bash
curl -X POST "http://localhost:8080/api/v1/auth/session" \
  -H "Content-Type: application/json" \
  -d '{
    "username": "editor_user",
    "password": "your-secure-password"
  }'
```

**成功レスポンス (200 OK):**
```json
{
  "token": "sess_01HZX...",
  "user": {
    "id": "u-admin-1",
    "username": "editor_user",
    "role": "editor"
  }
}
```

以降のリクエストでは、取得したトークンをヘッダーに指定します：
```http
Authorization: Bearer sess_01HZX...
```

### 1.2 ロールと実行可能操作

操作に必要なロールは以下のとおりです（詳細は [認証 & ユーザー管理](authentication.md) を参照）。

| 操作 | 必要なロール | 備考 |
|---|---|---|
| 下書き (`draft`) / レビュー待ち (`pending`) の作成 | `contributor` 以上 | 自身が著者となるコンテンツのみ |
| 公開 (`published`) / 予約公開 (`scheduled`) での作成 | `editor` 以上 | 即時公開または日時指定公開 |
| 既存コンテンツの更新・公開 | `editor` 以上 | `contributor` は自身の `draft` / `pending` のみ編集可能 |
| 他のユーザーを著者 (`author_id`) に指定 | `admin` のみ | 省略時はログインユーザーが著者になる |
| コンテンツの削除 | `editor` 以上 | - |
| タグの紐付け (`PUT .../tags`) | `contributor` 以上 | `contributor` は自身の `draft` / `pending` のみ |

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

| フィールド | 型 | 必須 | 既定値 | 説明 |
|---|---|---|---|---|
| `title` | string | **必須** | - | 記事のタイトル。 |
| `slug` | string | **必須** | - | URL スラッグ（英数字・ハイフンなど）。同一 `content_type` 内で一意。 |
| `status` | string | 任意 | `"draft"` | 公開ステータス。`"draft"`, `"pending"`, `"published"`, `"scheduled"`, `"archived"` のいずれか。 |
| `blocks` | array | 任意 | `[]` | 本文ブロックの配列（後述の [3. 本文ブロックの仕様](#3-本文ブロックの仕様-blocks) 参照）。 |
| `author_id` | string | 任意 | (ログインユーザー) | 著者のユーザー ID。ログインユーザー以外を指定できるのは `admin` のみ。 |
| `published_at` | string | 条件付 | `null` | RFC 3339 形式の公開日時（例: `2026-11-10T12:00:00Z`）。`status: "scheduled"` の場合は必須。 |
| `unpublish_at` | string | 任意 | `null` | RFC 3339 形式の予約非公開（アーカイブ）日時。`published` または `scheduled` のみ指定可能（公開日時より未来であること）。 |
| `publish` | boolean | 任意 | `false` | 明示的に公開版ポインターを最新の作業版へ切り替えるフラグ。 |
| `review_note` | string | 任意 | `null` | レビュー依頼・承認・差し戻しのメモ。監査ログや版の履歴に記録されます。 |
| `fields` | object | 任意 | `{}` | スキーマ（`schemas/*.yaml`）で定義されたカスタムフィールド（要約、カテゴリ、会員アクセス権など）。 |

> [!IMPORTANT]
> **カスタムフィールドの記述位置に関する注意**  
> スキーマで定義したカスタムフィールド（`excerpt` や `category_id` など）は、必ず `"fields": { ... }` の下にネストして記述してください。トップレベルに直接未知のキーを書くと、`400 Bad Request`（`unknown field`）が返されます。

### 2.3 ステータスの種類

- `"draft"`: 下書き。公開サイトには表示されず、管理画面や編集用 API からのみ閲覧可能。
- `"pending"`: レビュー待ち。`contributor` が執筆を完了し、編集者に承認・公開を依頼した状態。
- `"published"`: 公開。公開サイトおよびヘッドレス配信 API に即時反映されます。
- `"scheduled"`: 予約公開。`published_at` に指定した日時になると、バックグラウンドスケジューラによって自動的に `published` に切り替わります。
- `"archived"`: アーカイブ（非公開）。過去に公開されていたが現在は取り下げられた状態。

---

## 3. 本文ブロックの仕様 (`blocks`)

AegisCMS では記事本文を HTML 直書きではなく、構造化された AST ブロック（`BlockNode`）の配列として管理します（詳細は [本文ブロック & Markdown リファレンス](blocks.md)）。

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
    "src": "/uploads/media/2026/11/architecture.png",
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
これ以降のブロックは、権限を持つ会員にのみ表示されます（[会員アカウント](members.md)）。
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

記事内に画像を挿入する場合は、先にメディアアップロード API を呼び出してストレージキーや URL を取得します（詳細は [メディア管理](media.md) を参照）。

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
  "key": "media/2026/11/screenshot.png",
  "url": "/uploads/media/2026/11/screenshot.png",
  "size": 1048576,
  "content_type": "image/png"
}
```

取得した `url` を `core/image` ブロックの `attributes.src` に設定します。

---

## 6. タグの紐付け (`PUT /api/v1/contents/{content_type}/{slug}/tags`)

投稿した記事にタグを紐付けるには、タグ更新 API を呼び出します（詳細は [タグ機能](tags.md) を参照）。

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

> [!NOTE]
> 記事を更新するたびに新しいリビジョン（バージョン）が自動的に保存されます。管理画面から過去のリビジョンへの復元が可能です。

### 7.2 記事の削除 (`DELETE /api/v1/contents/{content_type}/{slug_or_id}`)

指定した記事を削除します（`editor` 以上の権限が必要です）。

- **メソッド**: `DELETE`
- **パス**: `/api/v1/contents/{content_type}/{slug_or_id}`
- **成功時**: `204 No Content` または `200 OK`

---

## 8. 実装コード例

### 8.1 cURL による投稿手順

```bash
# 1. ログインしてトークンを取得
TOKEN=$(curl -s -X POST "http://localhost:8080/api/v1/auth/session" \
  -H "Content-Type: application/json" \
  -d '{"username": "admin", "password": "your-password"}' | jq -r .token)

# 2. 記事の新規投稿
curl -i -X POST "http://localhost:8080/api/v1/contents/article" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "title": "API 投稿のテスト",
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
        "text": "cURL スクリプトから自動投稿された記事です。",
        "children": []
      }
    ],
    "fields": {
      "excerpt": "API 経由での投稿テストです。"
    }
  }'
```

### 8.2 Python スクリプト例

```python
import requests

BASE_URL = "http://localhost:8080/api/v1"

# 1. ログインしてセッショントークンを取得
login_res = requests.post(f"{BASE_URL}/auth/session", json={
    "username": "editor_user",
    "password": "your-password"
})
login_res.raise_for_status()
token = login_res.json()["token"]

headers = {
    "Authorization": f"Bearer {token}",
    "Content-Type": "application/json"
}

# 2. 記事作成リクエスト
article_payload = {
    "title": "Python から投稿した記事",
    "slug": "python-auto-post",
    "status": "published",
    "blocks": [
        {
            "name": "core/heading",
            "attributes": {"level": 2},
            "text": "はじめに",
            "children": []
        },
        {
            "name": "core/paragraph",
            "attributes": {},
            "text": "Python の requests ライブラリを使用して投稿されました。",
            "children": []
        },
        {
            "name": "core/code",
            "attributes": {"language": "python", "filename": "post.py"},
            "text": "print('Hello, AegisCMS!')",
            "children": []
        }
    ],
    "fields": {
        "excerpt": "Python スクリプトによる自動投稿のサンプルです。"
    }
}

res = requests.post(f"{BASE_URL}/contents/article", headers=headers, json=article_payload)
if res.status_code == 201:
    article = res.json()
    print(f"記事が作成されました: ID={article['id']}, URL=/{article['slug']}")
else:
    print(f"作成失敗 ({res.status_code}): {res.text}")
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

  // 1. ログイン
  const sessionRes = await fetch(`${BASE_URL}/auth/session`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      username: 'editor_user',
      password: 'your-password'
    }),
  });

  if (!sessionRes.ok) {
    throw new Error(`Login failed: ${sessionRes.statusText}`);
  }
  const { token } = await sessionRes.json();

  // 2. 記事作成
  const payload = {
    title: 'TypeScript による自動投稿記事',
    slug: 'typescript-auto-post',
    status: 'published',
    blocks: [
      {
        name: 'core/heading',
        attributes: { level: 2 },
        text: 'TypeScript 連携',
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
      excerpt: 'TypeScript による投稿スクリプトのサンプル。',
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
    console.log('投稿成功:', data.id, data.slug);
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

| ステータスコード | 意味 | 主な原因と対策 |
|---|---|---|
| `400 Bad Request` | リクエスト形式不正 | ・必須フィールド（`title`, `slug`）が欠落している<br>・トップレベルに未知のキーが含まれている（カスタムフィールドは `fields` 内に配置すること）<br>・`published_at` や `unpublish_at` の日時フォーマット（RFC 3339）が不正<br>・`access` に未定義の会員レベルを指定した |
| `401 Unauthorized` | 認証エラー | ・`Authorization` ヘッダーがない、またはトークンが無効・期限切れ |
| `403 Forbidden` | 権限不足 | ・`contributor` が直接 `published` で作成しようとした（`draft` または `pending` のみ可）<br>・`admin` 以外のユーザーが他人の `author_id` を指定した<br>・他人のコンテンツを編集しようとした |
| `404 Not Found` | リソース未存在 | ・指定した `content_type` のスキーマが登録されていない<br>・更新対象の記事が存在しない |
| `409 Conflict` | 衝突・競合 | ・指定した `slug` が同じ `content_type` 内ですでに使用されている |
| `422 Unprocessable Entity` | バリデーション / フック拒否 | ・登録された Wasm プラグインの `on-before-save` フックによって保存が拒否された |
| `500 Internal Server Error` | サーバー内部エラー | ・データベースエラーや予期せぬ障害 |

---

## 10. 公開後の自動処理 (Side Effects)

記事が `published` ステータスで作成・更新された場合、サーバー内部で以下の処理が自動的に実行されます。

1. **キャッシュのパージ (Cache Invalidation)**:
   - 公開サイトの該当記事ページ、一覧ページ、フィード（`/feed.atom`）の CDN / 内部キャッシュが自動的に破棄されます。
2. **全文検索インデックスの更新 (`aegis-search`)**:
   - tantivy 検索エンジンのインデックスキューに通知され、即座に検索結果へ反映されます（[サイト内検索](search.md)）。
3. **プラグインフックの呼び出し**:
   - プラグインが登録している `on-content-published` などの Wasm フックが自動的に発火します（[プラグインフック仕様](plugin_hooks.md)）。
4. **操作監査ログの記録 (`aegis-audit`)**:
   - `content.create` または `content.update` イベントが実行ユーザー・IP アドレスとともに監査ログに記録されます（[監査ログ](audit.md)）。

