# AegisCMS Admin Studio

**AegisCMS Admin Studio** は、次世代セキュアCMS「AegisCMS」の公式管理画面 SPA（Single Page Application）です。  
宣言的 YAML スキーマと連動したコンテンツ管理、AST（抽象構文木）ベースのブロックエディタ、および Wasm プラグインによる拡張性を直感的に操作できるモダンな管理インターフェースを提供します。

---

## 🌟 主な特徴

- **ゼロトラスト・ブロックエディタ**:
  - 見出し（H1〜H6）、段落、コードブロック（シンタックス指定対応）、画像、引用、カスタムHTMLブロックの追加・編集。
  - 直感的なブロック並び替え（Up / Down）と安全なノード削除。
  - AST 構造（`BlockNode`）を直接編集し、生の不正スクリプト注入を構造的に防止。
- **リアルタイム HTML プレビュー**:
  - 編集中のブロックをサーバーのプレビュー API（`POST /api/v1/preview`）でレンダリングし、保存して公開ページで表示した場合と同じ HTML（見出しの id、プラグインの AST 変換、リンク・画像の書き換えを含む）を表示。入力が 300ms 止まるごとに更新し、更新待ちの間は前回の結果を薄く表示。
  - **Themed Page** タブでは、テーマのテンプレートと `on-render-page` フックを適用したページ全体を、スクリプトを無効にした `sandbox` 付き `iframe` で表示（タイトル・slug・カテゴリの変更も反映）。
  - レンダリング結果をタブでプレビューと HTML ソースコード表示に切り替え可能。
- **プレミアム・グラスモフィズム UI**:
  - TailwindCSS 等のフレームワークに依存せず、ゼロランタイムの CSS Modules と CSS 変数（デザインシステムトークン）で構築。
  - 目に優しく視認性の高いダークテーマ、美しいすりガラス効果（Glassmorphism）、洗練されたマイクロインタラクション。
  - タイポグラフィに `Plus Jakarta Sans` / `Outfit` / `JetBrains Mono` を採用。
- **画像アップロード**:
  - 画像ブロックにドラッグ＆ドロップ、またはクリックでファイルを選ぶと `/api/v1/storage/upload` へアップロードし、URL をブロックに自動反映（alt が空ならファイル名から補完）。
  - 画像以外のファイルや、サーバーの上限（`[storage].max_upload_mb`）を超えるファイルはエラーを表示。
- **メディアライブラリ**:
  - 画像ブロックの **Choose from media library** で、アップロード済み画像（`GET /api/v1/storage/objects?content_type=image/`）を新しい順にサムネイル一覧表示。ファイル名検索・追加読み込み（24 件ずつ）に対応。
  - 画像を選ぶと URL をブロックに反映（alt が空ならファイル名から補完）。同じ画像を再アップロードせずに再利用できる。
- **カテゴリ選択 & 管理**:
  - 記事エディタでカテゴリ（`category_id` リレーション）をドロップダウンから選択。既存記事を開くと選択状態を復元。
  - **Manage** ボタンのモーダルでカテゴリの一覧表示（色・説明つき）と新規作成（slug 自動生成、色指定）。作成したカテゴリはそのまま選択される。
  - サイドバーの記事一覧にカテゴリバッジを表示し、カテゴリ（All / Uncategorized / 各カテゴリ）で絞り込み。
- **監査ログ閲覧画面 (Audit Logs)**:
  - ヘッダーの切り替えで、監査ログ（`GET /api/v1/audit-logs`）を一覧表示。action / outcome / actor / resource / 期間での絞り込み、ページ送り、イベント詳細（JSON）の展開に対応。
  - `admin` ロールでログインした場合のみ表示（詳細は [監査ログリファレンス](../docs/audit.md#6-admin-studio-の閲覧画面)）。
- **ログイン & ロール**:
  - 起動時にログイン画面を表示し、ユーザー名 / パスワードで `POST /api/v1/auth/session` にログイン。発行されたトークンはタブを閉じるまで `sessionStorage` に保持し、すべての API 呼び出しに `Authorization: Bearer` として付与する（再読み込みしてもログイン状態を維持）。
  - ヘッダーに表示名とロールを表示し、ログアウトボタンでセッションを破棄。API が 401（期限切れ・失効）を返したらログイン画面に戻る。
  - ロールに応じて操作を制限: `viewer` は Save・画像アップロード・カテゴリ作成が無効、Audit Logs タブは `admin` のみ表示（サーバー側でも同じ権限を検証。[認証 & ユーザー管理](../docs/authentication.md)）。
- **多言語対応（英語 / 日本語）**:
  - ヘッダーとログイン画面の言語メニューで切り替え。選んだ言語はブラウザに保存し、初回はブラウザの言語（日本語なら日本語、それ以外は英語）で表示。日付・金額の書式も言語に合わせる。
  - 文言は [i18next](https://www.i18next.com/) の JSON（`src/locales/<言語>/translation.json`）で、翻訳サービスにそのまま渡せる（[多言語対応](#-多言語対応-i18n)）。
- **Core Engine との透過的連携**:
  - スキーマ一覧（`/api/v1/schemas`）および記事一覧・詳細・作成（`/api/v1/contents/{content_type}`）とのリアルタイム同期。
  - バックエンドサーバーのヘルスチェック（`/api/v1/health`）による接続ステータス表示。
  - 本番ビルド成果物（`dist/`）は、Rust 製コアサーバー（`aegis-server`）から単一ポート（8080）で自動配信。

---

## 🏗️ 技術スタック

| 分類 | 採用技術 | 選定理由 |
| :--- | :--- | :--- |
| **フレームワーク** | React 19 + TypeScript | 型安全性と堅牢なコンポーネント指向 UI |
| **ビルドツール** | Vite 6 | 高速な HMR（Hot Module Replacement）と最小限のバンドルサイズ |
| **スタイリング** | Vanilla CSS (CSS Variables) | 高いデザイン柔軟性、フレームワーク非依存、ゼロランタイム |
| **アイコン** | Lucide React | 軽量で一貫性のあるモダンアイコンセット |
| **多言語対応** | i18next + react-i18next | 翻訳サービス（Crowdin・Lokalise など）が扱える JSON 形式、複数形・差し込み |

---

## 📁 ディレクトリ構成

```text
admin/
├── src/
│   ├── api/
│   │   ├── client.ts       # REST API Gateway 連携クライアント（Bearer 付与・401 通知）
│   │   └── session.ts      # セッショントークンの保持（sessionStorage）
│   ├── components/         # 画面単位のコンポーネント（各 *.tsx の隣に同名の *.module.css）
│   │   ├── ui/             # 共通部品
│   │   │   ├── Button.tsx  # ボタン（primary / secondary / tool / icon）
│   │   │   ├── Field.tsx   # ラベル付き入力欄 Field と横並びの FieldRow
│   │   │   ├── Modal.tsx   # モーダル（Escape・外側クリックで閉じる、フォーカストラップ）
│   │   │   ├── LanguageSelect.tsx # 言語の切り替え
│   │   │   ├── RoleLabel.tsx # ロールの表示名
│   │   │   └── BrandMark.tsx # ロゴマーク
│   │   ├── Header.tsx      # ヘッダー・画面切り替え・サーバー状態・ログインユーザー
│   │   ├── LoginView.tsx   # ログイン画面
│   │   ├── AuditLogView.tsx # 監査ログ閲覧画面
│   │   ├── CategoryManager.tsx # カテゴリ一覧・作成モーダル
│   │   ├── MediaLibrary.tsx # アップロード済み画像の選択モーダル
│   │   ├── ImageUploader.tsx # 画像ブロックのアップロード欄（ドラッグ＆ドロップ）
│   │   ├── ContentList.tsx # 記事・コンテンツ一覧サイドバー
│   │   ├── BlockItem.tsx   # 各種 AST ブロックエディタ
│   │   └── Preview.tsx     # リアルタイム HTML / ソースコードプレビュー
│   ├── i18n/               # i18next の初期化・言語の検出と保存（index.ts）、キーの型（i18next.d.ts）
│   ├── locales/            # 翻訳: en/translation.json（原文）、ja/translation.json
│   ├── utils/              # カテゴリ・メディア・ロール判定、日付の書式（format.ts）、cx などのヘルパー
│   ├── types/
│   │   ├── index.ts        # AST ノード・スキーマ・コンテンツ型定義
│   │   └── schemas.ts      # スキーマから自動生成された型（task codegen:ts）
│   ├── App.tsx             # メインアプリケーションコンポーネント
│   ├── App.module.css
│   ├── index.css           # グローバル: デザイントークン（CSS 変数）・リセット・基本要素のみ
│   └── main.tsx            # エントリーポイント
├── scripts/check-locales.mjs # 全言語のキーが英語と同じか検証（npm run lint で実行）
├── index.html              # HTML テンプレート
├── package.json
└── vite.config.ts
```

### スタイルの書き方

- コンポーネントのスタイルは隣の `*.module.css`（CSS Modules）に書き、`import styles from './X.module.css'` で `className={styles.foo}` と参照します。クラス名はビルド時にファイルごとにスコープされるため、他のコンポーネントと衝突しません。
- 色・余白・角丸・フォントなどは `index.css` の CSS 変数（`var(--bg-surface)` など）を使います。`style={{...}}` のインライン指定は、カテゴリ色のように実行時に決まる値だけにします。
- ボタン・入力欄・モーダルは `components/ui/` の共通部品を使います。親から共通部品の見た目を調整するときは、`className` を渡し、親のモジュールで `.parent .child` のように子孫セレクタで指定します（CSS の読み込み順に依存せず上書きできるため）。
- 複数のクラスの結合には `utils/cx.ts` の `cx(styles.a, active && styles.b)` を使います。

---

## 🌐 多言語対応 (i18n)

画面の文言はコードに直接書かず、`src/locales/<言語>/translation.json` のキーで参照します。英語（`en`）が原文で、ほかの言語はそれを翻訳したものです。

```tsx
import { useTranslation } from 'react-i18next';

const { t } = useTranslation();
<span>{t('orders.markPaid')}</span>                        // 支払い済みにする
t('orders.updated', { number: order.number, status })     // 差し込み: "注文 {{number}}: {{status}}"
t('media.inUse', { count: 3 })                             // 複数形: media.inUse_one / media.inUse_other
<Trans i18nKey="profile.signedInAs" values={{ username }} components={{ code: <code /> }} />
```

- **キーは型で検査されます**（`src/i18n/i18next.d.ts`）。英語の JSON に無いキーを `t()` に渡すと `npm run build`（`tsc`）が失敗します。
- **訳し漏れは lint で検出します**。`npm run lint` の最後に `scripts/check-locales.mjs` が、各言語のキーが英語と同じか（無いキー・余分なキー・空の値）を確かめます。複数形は言語ごとに形が違うため（英語は `_one` / `_other`、日本語は `_other` だけ）、まとめて 1 つのキーとして比べます。
- コンポーネントの外（API クライアントのエラーメッセージなど）では `import i18n from '../i18n'` の `i18n.t()` を使います。`useEffect` の中のエラーメッセージも `i18n.t()` にします（`t` を依存配列に入れると、言語を切り替えたときに読み込み直してしまうため）。
- 日付は `utils/format.ts` の `formatDate` / `formatDateTime`、金額は `utils/money.ts` の `formatMoney` を使うと、表示中の言語の書式になります。
- サーバーが返すエラーメッセージ（API の `error`）は英語のまま表示します。画面側で言い換えているのは、ログインの失敗・権限不足など一部だけです。

### 言語を追加する

1. `src/locales/<言語コード>/translation.json` を英語のファイルから作って翻訳する
2. `src/i18n/index.ts` の `LANGUAGES` と `resources` に加える
3. `npm run lint` でキーがそろっているか確かめる

### 翻訳サービスと連携する

ファイルは i18next の JSON（v4、ネストしたキー）なので、Crowdin・Lokalise・Weblate などの「i18next JSON」形式でそのまま読み書きできます。例えば Crowdin なら:

```yaml
# crowdin.yml（リポジトリのルート）
files:
  - source: /admin/src/locales/en/translation.json
    translation: /admin/src/locales/%two_letters_code%/translation.json
```

翻訳サービスから戻ってきたファイルは、`npm run format` で整形してから `npm run lint` / `npm run build` を通してください。

---

## 🚀 開発 & ビルド手順

### 1. 依存パッケージのインストール

```bash
npm install
```

### 2. 開発サーバーの起動 (スタンドアロン開発)

```bash
npm run dev
```

デフォルトで `http://localhost:5173/` が起動します。  
※ 開発サーバーは `http://127.0.0.1:8080` のコアサーバー（`cargo run -p aegis-server -- run`）に接続します（CORS は許可済み）。別の場所のサーバーに接続する場合は `VITE_AEGIS_SERVER_URL=https://example.com/cms npm run dev` のように指定します。

### 3. 型検証 & 本番ビルド

```bash
npm run build
```

TypeScript の型チェック（`tsc -b`）を実行後、最適化された静的アセットが `dist/` に出力されます。  
生成された `dist/` は、コアサーバー起動時に自動的に読み込まれ、`/admin/` で配信されます。アセットは相対パス（`base: './'`）で参照し、API の接続先はバンドルの URL（`{サーバー}/admin/assets/*.js`）から求めるため、リバースプロキシ配下・任意のサブドメイン・`[server].base_path` のサブパスでも再ビルドせずに動きます。Admin Studio だけを別の場所（`admin.example.com` など）で配信する場合は、ビルド時に `VITE_AEGIS_SERVER_URL` で AegisCMS サーバーの URL を指定してください（[docs/configuration.md](../docs/configuration.md#43-admin-studio-の接続先)）。

### 4. ビルドプレビュー

```bash
npm run preview
```

### 5. 静的解析 (oxlint)

```bash
npm run lint   # oxlint --deny-warnings（警告 1 件でも失敗。CI でも実行）
```

ルールは `.oxlintrc.json`。Rust 側の Clippy（`-D warnings`）と同じくゼロ警告ポリシーです。

### 6. コードフォーマット (Prettier)

```bash
npm run format        # 整形を適用
npm run format:check  # 整形済みか検証（CI でも実行）
```

設定は `.prettierrc.json`（シングルクォート、1 行 100 文字、ES5 の末尾カンマ）。インデントは `.editorconfig`（2 スペース）に従います。自動生成ファイル `src/types/schemas.ts` と Markdown は対象外です（`.prettierignore`）。

**VS Code で保存時に自動整形する**: リポジトリ直下の `.vscode/settings.json` で、TS / TSX / JS / CSS / HTML / JSON の保存時に Prettier で整形するよう設定済みです。

1. 推奨拡張機能 **Prettier - Code formatter**（`esbenp.prettier-vscode`）をインストール（`.vscode/extensions.json` に登録済みのため、開くと推奨表示されます）
2. `npm install`（Prettier は `admin/node_modules` のものが使われ、バージョンが固定されます）
3. リポジトリのルート（`AegisCMS/`）を VS Code で開く

`prettier.requireConfig` を有効にしているため、Prettier 設定の無い `admin/` 外のファイル（`config/` 等の JSON）は整形されません。

---

## 🔗 関連リンク

- [AegisCMS 全体設計書 (README.md)](../README.md)
- [エージェント運用規約 (AGENTS.md)](../AGENTS.md)
- [開発進捗 & TODOリスト (TODO.md)](../TODO.md)
