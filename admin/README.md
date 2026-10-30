# AegisCMS Admin Studio

**AegisCMS Admin Studio** は、次世代セキュアCMS「AegisCMS」の公式管理画面 SPA（Single Page Application）です。  
宣言的 YAML スキーマと連動したコンテンツ管理、AST（抽象構文木）ベースのブロックエディタ、および Wasm プラグインによる拡張性を直感的に操作できるモダンな管理インターフェースを提供します。

---

## 🌟 主な特徴

- **ゼロトラスト・ブロックエディタ**:
  - 見出し（H1〜H6）、段落、リスト、コードブロック（シンタックス指定対応）、画像、表（`TableBlockEditor.tsx`。行・列の追加と削除、見出し行、列の揃え）、目次、引用、注記、折りたたみ（Q&A）、区切り線、カスタムHTMLブロックの追加・編集（ブロックの仕様は [本文のブロック](../docs/blocks.md)）。
  - 有効なプラグインのカスタムブロック（`GET /api/v1/blocks`）もツールバーに並び、マニフェストで宣言された入力欄から作ったフォーム（`PluginBlockEditor.tsx`）で編集する（[本文のブロック 4](../docs/blocks.md#4-プラグインのブロック)）。
  - 段落・リスト・表のセルなどでは `**太字**`・`*斜体*`・`` `コード` ``・`[リンク](URL)` のインライン書式が使える。
  - 直感的なブロック並び替え（Up / Down）と安全なノード削除。
  - AST 構造（`BlockNode`）を直接編集し、生の不正スクリプト注入を構造的に防止。
- **リアルタイム HTML プレビュー**:
  - 編集中のブロックをサーバーのプレビュー API（`POST /api/v1/preview`）でレンダリングし、保存して公開ページで表示した場合と同じ HTML（見出しの id、プラグインの AST 変換、リンク・画像の書き換えを含む）を表示。入力が 300ms 止まるごとに更新し、更新待ちの間は前回の結果を薄く表示。
  - **Themed Page** タブでは、テーマのテンプレートと `on-render-page` フックを適用したページ全体を `iframe`（`srcdoc`、`sandbox="allow-scripts"`。プラグインの Mermaid 描画などのスクリプトは動くが、Admin Studio のオリジンには入れない）で表示（タイトル・slug・カテゴリの変更も反映）。
  - レンダリング結果をタブでプレビューと HTML ソースコード表示に切り替え可能。
- **予約公開・予約非公開化**: エディタ・商品フォームの状態で「予約公開」を選び、公開日時（`datetime-local`）を入れて保存すると、その日時にサーバーが公開する。また、公開中・予約公開のコンテンツに非公開予定日時（`unpublish_at`）を指定することで期間限定公開が可能。一覧にはステータスバッジに加え非公開予定アイコン（ツールチップに日時）を表示（[予約公開・予約非公開化](../docs/theme_and_publishing_guide.md#予約公開スケジュール公開)）。
- **公開スケジュール・カレンダー (Calendar)**: ヘッダーの「カレンダー」タブ（`ScheduleCalendar.tsx`）から、全コンテンツタイプの公開予定・非公開予定・公開中アイテムを月間カレンダーおよびアジェンダ形式で一覧表示。コンテンツタイプ・予定種別による絞り込み、詳細モーダルからの非公開日時のクイック編集・解除、エディタへの直接遷移に対応。
- **アクセス解析 (Analytics)**: `[analytics].enabled = true` のときだけ出るヘッダーの「アクセス解析」タブ（`AnalyticsView.tsx`）で、過去 7 / 30 / 90 日のページビュー・訪問者の推移（グラフと表。グラフは矢印キーでも読める）、よく見られたページ・参照元・端末を表示（[docs/analytics.md](../docs/analytics.md)）。
- **問い合わせ (Inbox)**: `[contact].enabled = true` のとき editor 以上に出るヘッダーの「問い合わせ」タブ（`ContactInbox.tsx`）で、問い合わせフォームから届いたメッセージを新着・既読・対応済み・スパムに分けて一覧・詳細表示・メールで返信・まとめて状態の変更・削除。本文には「問い合わせフォーム」ブロック（`core/contact-form`、Markdown では `[CONTACT]`）を置ける（[docs/contact.md](../docs/contact.md)）。
- **Markdown 編集 & 貼り付け**:
  - エディタ上部の **Blocks / Markdown** で編集方法を切り替える。ブロックと Markdown は相互に変換され（`utils/markdown.ts` の `blocksToMarkdown` / `markdownToBlocks`。見出し・段落・コード・画像・リスト・表・目次（`[TOC]`）・引用・注記（`> [!NOTE]`・`:::note`）・折りたたみ（`:::details`）・区切り線・プラグインのブロック（`::: <名前> {属性の JSON}`））、どちらで編集してもプレビューと保存は同じブロックから行う。
  - Markdown エディタ（`MarkdownEditor.tsx`）: 書式の挿入ツールバー、Tab でのインデント、Ctrl+S で保存、行数・文字数・ブロック数の表示。
  - ブロックエディタに Markdown を貼り付けるとブロックに変換して追加する。**Paste Markdown**（`PasteMarkdownModal.tsx`）ではクリップボードを読み込み、変換結果の内訳を確かめてから末尾に追加か全置換を選ぶ。
- **商品・注文 (Products / Orders)**: 商品の作成・編集（価格・在庫・SKU・画像）と、注文の一覧・詳細・状態の変更（入金確認・発送・取り消し）。タブは EC が有効（`[commerce].enabled`）なときだけ表示する（[EC](../docs/commerce.md#33-admin-studio)）。
- **コメント (Comments)**: 読者のコメントの承認・承認の取り消し・スパム・ゴミ箱・削除（1 件・まとめて）、本文の編集、返信、コンテンツごとの受付の停止・再開、承認制の切り替え（`admin`）。タブはコメントが有効（`[comments].enabled`。`GET /api/v1/comments/settings` が 404 でない）かつ `editor` 以上のときだけ表示する。URL の `#comments`（通知メールのリンク）でこのタブが開く（[コメント](../docs/comments.md#4-admin-studio)）。
- **サイト (Settings › Site)**: 公開サイトの名前と favicon の変更（`GET` / `PUT /api/v1/site`、`admin` のみ。`SiteManager.tsx`）。favicon はメディアライブラリから選ぶ・アップロードする・URL を入力する。空にして保存すると設定ファイル（`[theme]`）の値に戻る（[テーマと公開](../docs/theme_and_publishing_guide.md#サイト名と-faviconadmin-studio)）。
- **システム運用設定 (Settings › System)**: メンテナンスモードの切り替え（案内文の編集）、アセットアップロード上限（MB）、CDN/ブラウザキャッシュの有効化とTTL、ユーザー自己登録受付の停止・再開、連続ログイン失敗時のロック回数、コメント受付および掲示板投稿の受付停止・再開をサーバー再起動なしで動的に変更（`GET` / `PUT /api/v1/system/settings`、`admin` のみ。`SystemManager.tsx`。[システム設定](../docs/configuration.md#53-システム運用設定-system)）。既定値へのリセットボタン付き。
- **通知 (Settings › Notifications)**: 配信の状態（チャネル・形式・送信待ちの保存先）と、送信待ち・送れなかった通知の一覧（再送・破棄）、見本の値でのテスト送信（チャネルごとの結果を表示）、テンプレートの編集（種類と宛先ごとの件名・本文・HTML、共通の部品）と保存前の内容でのプレビュー（HTML は `iframe` の `srcdoc`、`sandbox=""` でスクリプトを動かさない）、元に戻す。`admin` のみ（`NotificationManager.tsx`。[通知 6](../docs/notifications.md#6-admin-studio-から管理する)）。
- **テーマ (Settings › Themes)**: インストール済みテーマのカード（プレビュー画像・タグ・作者）、ワンクリックでの切り替え（`PUT /api/v1/themes/active`、`admin`）、公開サイトの確認。マーケットプレイスのタブは将来の機能の予告（ZIP の導入は未実装）。
- **ユーザー管理 (Settings › Users) & プロフィール**:
  - `admin` はユーザーの一覧・作成・編集・ロール変更・無効化・パスワードのリセット・削除（`UserManager.tsx`、[認証 3](../docs/authentication.md#3-ユーザー管理-api)）。
  - **ユーザー招待 (Invite user)**: 管理者がメールアドレスとロールを指定して招待メールを送信するモーダル（`InviteUserModal`）を提供（[招待リンク](../docs/authentication.md#10-ユーザー招待リンク)）。
  - **承認待ちステータス**: 自己登録で承認待ち状態のアカウント（`pending_approval: true`）には専用の「承認待ち (Pending approval)」バッジを表示し、編集モーダルからワンクリックで「承認して有効化」できる。
  - ヘッダーのアバター + 表示名から **My profile**（`ProfileModal.tsx`）: アバター画像・表示名・メールアドレス・パスワードの変更（SSO のユーザーにはパスワード欄を出さない）。アバターが無い・読めないときはイニシャルを表示（`ui/Avatar.tsx`）。
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
  - カテゴリの一覧表示（色・説明つき）と新規作成（slug 自動生成、色指定）は **Settings › Categories**。エディタの **Manage** ボタンからも開ける。
  - サイドバーの記事一覧にカテゴリバッジを表示する。
- **コンテンツ一覧の検索・絞り込み・ページ送り（サイドバー）**: タイトル・スラッグ・要約（`excerpt`）の検索ボックス（入力が 300ms 止まると検索、Enter ですぐ検索）、カテゴリ（All / Uncategorized / 各カテゴリ。記事のみ）と状態（Draft / Scheduled / Published）の絞り込み、1 ページ 20 件のページ送りに対応。検索・絞り込み・ページ送りはサーバーで行い（`GET /api/v1/contents/{type}?view=summary&q=&category=&status=&limit=&offset=`）、一覧は要約（タイトル・スラッグ・状態・カテゴリ・要約。ブロックと描画済み HTML は含まない）だけを取得する。記事を選ぶと、その 1 件の全体を取得してエディタに開く。種類（記事 / 固定ページ）を切り替えると検索と絞り込みは戻る。
- **設定画面 (Settings)**:
  - サイト全体の管理（サイト名と favicon・SEO・通知・テーマ・プラグイン・カテゴリ・ユーザー・監査ログ）はヘッダーの **Settings** にまとめ、左のメニューで切り替える。ヘッダーのタブは Editor / Products / Orders / Comments / Settings（Products / Orders は EC が有効かつ `editor` 以上のとき、Comments はコメントが有効かつ `editor` 以上のとき）。
  - ロールに応じて項目を出し分け: Themes・Plugins・Categories は全員（カテゴリの作成は `editor` 以上）、Site・Users・Audit Logs は `admin` のみ。
- **監査ログ閲覧画面 (Settings › Audit Logs)**:
  - 監査ログ（`GET /api/v1/audit-logs`）を一覧表示。action / outcome / actor / resource / 期間での絞り込み、ページ送り、イベント詳細（JSON）の展開に対応。
  - `admin` ロールでログインした場合のみ表示（詳細は [監査ログリファレンス](../docs/audit.md#6-admin-studio-の閲覧画面)）。
- **ログイン & ロール**:
  - 起動時にログイン画面を表示し、ユーザー名 / パスワードで `POST /api/v1/auth/session` にログイン。発行されたトークンはタブを閉じるまで `sessionStorage` に保持し、すべての API 呼び出しに `Authorization: Bearer` として付与する（再読み込みしてもログイン状態を維持）。
  - ヘッダーに表示名とロールを表示し、ログアウトボタンでセッションを破棄。API が 401（期限切れ・失効）を返したらログイン画面に戻る。
  - サーバーでアカウント登録（`[auth.registration]`）が有効なら、ログイン画面に「アカウントを作成」を表示（`RegisterForm.tsx`）。確認メールのリンク（`#verify-email=<トークン>`）で開くと、トークンをアドレスバーから消してアカウントを作り、結果（ログインできる / 承認待ち / リンクが無効）を表示する（[認証 7](../docs/authentication.md#7-アカウント登録)）。
  - **パスワード再設定 (`PasswordResetForms.tsx`)**: `[auth.password_reset]` 有効時、ログイン画面の「パスワードをお忘れですか？」からメールアドレスを送信し、届いた再設定メールのリンク（`#reset-password=<トークン>`）から新パスワードを再設定（[認証 9](../docs/authentication.md#9-パスワードの再設定)）。
  - **招待承諾 (`AcceptInviteForm.tsx`)**: 管理者から招待されたユーザーはメール内のリンク（`#invite=<トークン>`）を開き、招待情報（メール・ロール）を確認した上でユーザー名・パスワードを設定して即時有効アカウントを開設（[認証 10](../docs/authentication.md#10-ユーザー招待リンク)）。
  - ロールに応じて操作を制限: `viewer` は Save・画像アップロード・カテゴリ作成が無効、設定画面の Users / Audit Logs は `admin` のみ表示（サーバー側でも同じ権限を検証。[認証 & ユーザー管理](../docs/authentication.md)）。
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
│   ├── components/         # 画面単位のコンポーネント（各 *.tsx の隣に同名の *.module.css、Storybook の *.stories.tsx）
│   │   ├── ui/             # 共通部品
│   │   │   ├── Button.tsx  # ボタン（primary / secondary / tool / icon）
│   │   │   ├── Field.tsx   # ラベル付き入力欄 Field と横並びの FieldRow
│   │   │   ├── Modal.tsx   # モーダル（Escape・外側クリックで閉じる、フォーカストラップ）
│   │   │   ├── LanguageSelect.tsx # 言語の切り替え
│   │   │   ├── RoleLabel.tsx # ロールの表示名
│   │   │   ├── Avatar.tsx  # アバター（画像が無ければイニシャル）
│   │   │   └── BrandMark.tsx # ロゴマーク
│   │   ├── Header.tsx      # ヘッダー・画面切り替え・サーバー状態・ログインユーザー
│   │   ├── LoginView.tsx   # ログイン画面
│   │   ├── RegisterForm.tsx # アカウント登録（ログイン画面から）
│   │   ├── PasswordResetForms.tsx # パスワード再設定要求および確定フォーム
│   │   ├── AcceptInviteForm.tsx # 招待リンク承諾・初期アカウント設定
│   │   ├── SettingsView.tsx # 設定画面（左メニューで下の各画面を切り替え）
│   │   ├── SiteManager.tsx # 公開サイトの名前と favicon
│   │   ├── ThemeManager.tsx # テーマ一覧・切り替え
│   │   ├── UserManager.tsx # ユーザー管理
│   │   ├── ProfileModal.tsx # 自分のプロフィール（AvatarPicker.tsx でアバターを選ぶ）
│   │   ├── ProductManager.tsx # 商品
│   │   ├── OrderManager.tsx # 注文
│   │   ├── AnalyticsView.tsx # アクセス解析のダッシュボード
│   │   ├── ContactInbox.tsx # 問い合わせの受信箱
│   │   ├── CommentManager.tsx # コメントの審査・返信・承認の設定
│   │   ├── AuditLogView.tsx # 監査ログ閲覧画面
│   │   ├── CategoryManager.tsx # カテゴリ一覧・作成
│   │   ├── MediaLibrary.tsx # アップロード済み画像の選択モーダル
│   │   ├── ImageUploader.tsx # 画像ブロックのアップロード欄（ドラッグ＆ドロップ）
│   │   ├── ContentList.tsx # 記事・コンテンツ一覧サイドバー
│   │   ├── BlockItem.tsx   # 各種 AST ブロックエディタ
│   │   ├── MarkdownEditor.tsx # Markdown 編集モード
│   │   ├── PasteMarkdownModal.tsx # Markdown の貼り付け
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
├── .storybook/             # Storybook の設定（main.ts・preview.tsx）と API のモック（mockApi.ts）
├── e2e/                    # Playwright の E2E テスト（*.spec.ts）と使い捨てのサーバーの起動（start-server.mjs）
├── scripts/check-locales.mjs # 全言語のキーが英語と同じか検証（npm run lint で実行）
├── index.html              # HTML テンプレート
├── package.json
├── playwright.config.ts
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
- サーバーが返すエラーメッセージ（API の `error`）は英語のまま表示します。画面側で言い換えているのは、ログインの失敗・権限不足・アカウント登録の失敗（API の `code`）など一部だけです。

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

### i18n の細かな決まり

- 言語は `localStorage` の `aegis.admin.language`、無ければ `navigator.languages`（`ja` 以外は `en`）。`<html lang>` も切り替える。
- ログイン画面のお知らせ（期限切れ・ログアウト）と SSO のエラーは、文字列ではなく理由のコードで持ち、表示するときに訳す（言語を切り替えても追従する）。
- 監査ログの action 名・テーマの説明（`theme.json`）・プラグインが付ける明細名（`Coupon WELCOME10` など）はデータなので訳さない。
- 日本語はラベルが長いため、ヘッダーの要素は `nowrap` にし、幅 1680px 未満でバッジ、1480px 未満でゲートウェイの文言を隠す（状態の点とツールチップは残す）。

---

## 🧩 設計メモ

- **API の接続先**: `src/api/client.ts` の `SERVER_URL`（オリジン + `[server].base_path`）は、ビルド時の `VITE_AEGIS_SERVER_URL` → 開発サーバーなら `http://127.0.0.1:8080` → 本番ビルドはバンドルの URL（`{サーバー}/admin/assets/*.js` から `../../`）の順に決まる。ドメインやベースパスごとに再ビルドする必要はない。サーバーが返すルート起点の URL（アバターなど）は `serverAssetUrl` でオリジンを付けて表示する。
- **コンテンツの型**: 自動生成の `types/schemas.ts`（`ArticleContent` など）はカスタムフィールドをトップレベルに持つが、コンテンツ API の応答はカスタムフィールドを `fields`、解決済みリレーションを `relations` に入れて返す。画面では `types/index.ts` の `ContentItem` を使う。作成時のペイロード（`CreateArticlePayload` など）はトップレベルで正しい。エディタの一覧は要約（`ContentSummary`。ブロックと描画済み HTML なし）を `listContentSummaries` で 1 ページずつ取得し、カテゴリのような小さな一覧は `getAllContentSummaries` で全件の要約を取る。`getContents`（全体のデータを 500 件ずつ全件）は、商品の編集が本文（`blocks`）を使う商品管理だけが使う。サーバーは 1 件ごとにブロックを描画するので、一覧に全体は要らないときは使わない。
- **保存**: 開いている記事は `PUT`（更新）、新規は `POST`。
- **Themed Page プレビュー**: サーバーの `page_html` に `<base href="{SERVER_URL}/">` を差し込んで `srcdoc` に入れる（テーマのアセットやアップロード画像をサーバーから読むため）。`sandbox` に `allow-same-origin` を付けてはいけない。`srcdoc` の文書は親と同じオリジンになるため、テーマやプラグインが出力したスクリプトから `sessionStorage` のセッショントークンを読めてしまう。付けないと文書のオリジンは不透明（`null`）になり、`localStorage` などのストレージに触れると例外になる（`cart.js` は try/catch で受けてメモリ上に持つ）。CDN の通常の `<script>` は読み込めるが、`type="module"` や CORS が要る読み込み（`crossorigin` 付きのフォントなど）はサーバー側で `Access-Control-Allow-Origin` が要る。
- **画面の高さとスクロール**: ルートの `.app` は `height: 100vh; overflow: hidden`。全幅の画面（監査ログ・テーマなど）は直下に `flex: 1; overflow-y: auto` のラッパーを置き、下に余白を取ってスクロールさせる。
- **設定画面 (`SettingsView.tsx`)**: 各項目は既存の画面コンポーネント（`SiteManager` / `SeoManager` / `NotificationManager` / `ThemeManager` / `PluginManager` / `CategoryManager` / `UserManager` / `AuditLogView`）をそのまま表示する。項目を足すときは `SECTIONS` と `settings.sections.<id>` の訳を足す。選んでいる項目は `App` が持つ（エディタの **Manage** から `categories` を開くため）。ロールで見えない項目を選んでいたときは最初の項目を表示する。エディタの状態も `App` にあるので、設定画面へ移って戻っても編集中の内容は消えない。
- **モバイル・レスポンシブ対応**: 画面幅 900px 以下の狭小端末では、ヘッダーをコンパクト化しハンバーガーメニュー内にタブ・接続状態・言語切替・アカウント操作を統合。エディタ画面は 3 列固定（300px/可変/480px）からペイン切り替えタブ（記事一覧 / 編集 / プレビュー）へと切り替わり、一覧から記事を選択すると自動的に編集ペインへ遷移。フィールド行の折り返し、カレンダーやテーブルの横スクロール、各種管理画面の単一カラム化により、スマートフォン・タブレットでも UI が崩れず直感的に操作できます。

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

### 6. E2E テスト (Playwright) と Storybook

```bash
npm run test:e2e         # サーバーと Admin Studio をビルドし、ポート 18080 の使い捨てサーバーでテスト
npm run storybook        # http://localhost:6006
npm run build-storybook  # storybook-static/
```

初回は `npx playwright install chromium` でブラウザを入れてください。詳しくは [開発ガイド 8](../docs/development.md#8-admin-studio-の-e2e-テストと-storybook)。

### 7. コードフォーマット (Prettier)

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
