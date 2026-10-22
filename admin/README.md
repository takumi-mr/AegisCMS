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
  - エディタで編集中のブロック構造からレンダリングされた HTML を即座に確認可能。
  - レンダリング結果をタブでプレビューと HTML ソースコード表示に切り替え可能。
- **プレミアム・グラスモフィズム UI**:
  - TailwindCSS 等のフレームワークに依存せず、ゼロランタイムの Vanilla CSS と CSS 変数（デザインシステムトークン）で構築。
  - 目に優しく視認性の高いダークテーマ、美しいすりガラス効果（Glassmorphism）、洗練されたマイクロインタラクション。
  - タイポグラフィに `Plus Jakarta Sans` / `Outfit` / `JetBrains Mono` を採用。
- **画像アップロード**:
  - 画像ブロックにドラッグ＆ドロップ、またはクリックでファイルを選ぶと `/api/v1/storage/upload` へアップロードし、URL をブロックに自動反映（alt が空ならファイル名から補完）。
  - 画像以外のファイルや、サーバーの上限（`[storage].max_upload_mb`）を超えるファイルはエラーを表示。
- **カテゴリ選択 & 管理**:
  - 記事エディタでカテゴリ（`category_id` リレーション）をドロップダウンから選択。既存記事を開くと選択状態を復元。
  - **Manage** ボタンのモーダルでカテゴリの一覧表示（色・説明つき）と新規作成（slug 自動生成、色指定）。作成したカテゴリはそのまま選択される。
- **監査ログ閲覧画面 (Audit Logs)**:
  - ヘッダーの切り替えで、監査ログ（`GET /api/v1/audit-logs`）を一覧表示。action / outcome / actor / resource / 期間での絞り込み、ページ送り、イベント詳細（JSON）の展開に対応。
  - 閲覧には `admin` ロールのセッショントークンが必要（詳細は [監査ログリファレンス](../docs/audit.md#6-admin-studio-の閲覧画面)）。
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

---

## 📁 ディレクトリ構成

```text
admin/
├── public/                 # 静的ファビコン・アセット
├── src/
│   ├── api/
│   │   └── client.ts       # REST API Gateway 連携クライアント
│   ├── assets/             # 静的画像・ベクター素材
│   ├── components/         # UI コンポーネント群
│   │   ├── Header.tsx      # ヘッダー・画面切り替え・サーバー状態インジケーター
│   │   ├── AuditLogView.tsx # 監査ログ閲覧画面
│   │   ├── CategoryManager.tsx # カテゴリ一覧・作成モーダル
│   │   ├── ImageUploader.tsx # 画像ブロックのアップロード欄（ドラッグ＆ドロップ）
│   │   ├── ContentList.tsx # 記事・コンテンツ一覧サイドバー
│   │   ├── BlockItem.tsx   # 各種 AST ブロックエディタ
│   │   └── Preview.tsx     # リアルタイム HTML / ソースコードプレビュー
│   ├── types/
│   │   └── index.ts        # AST ノード・スキーマ・コンテンツ型定義
│   ├── App.tsx             # メインアプリケーションコンポーネント
│   ├── index.css           # グローバルデザイントークン & グラスモフィズムスタイル
│   └── main.tsx            # エントリーポイント
├── index.html              # HTML テンプレート
├── package.json
└── vite.config.ts
```

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
※ コアサーバー（`cargo run -p aegis-server -- run`）が `http://127.0.0.1:8080` で起動している場合、CORS 設定により自動的に接続されます。

### 3. 型検証 & 本番ビルド

```bash
npm run build
```

TypeScript の型チェック（`tsc -b`）を実行後、最適化された静的アセットが `dist/` に出力されます。  
生成された `dist/` は、コアサーバー起動時に自動的に読み込まれ、`http://127.0.0.1:8080/` にて配信されます。

### 4. ビルドプレビュー

```bash
npm run preview
```

---

## 🔗 関連リンク

- [AegisCMS 全体設計書 (README.md)](../README.md)
- [エージェント運用規約 (AGENTS.md)](../AGENTS.md)
- [開発進捗 & TODOリスト (TODO.md)](../TODO.md)
