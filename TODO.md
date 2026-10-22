# AegisCMS 開発進捗 & TODOリスト (TODO.md)

> [!NOTE]
> 作業を再開する際は、まず本ドキュメントの「現在の状況と直近の再開手順」および「TODOタスク一覧」を確認してください。  
> **ルール**: 各修正・実装ステップの終了後には、必ず本ドキュメントを更新してください。  
> **アーカイブ**: これまでに完了したタスク・実装実績は [docs/completed_tasks.md](file:///d:/projects/cms/docs/completed_tasks.md) に整理・記録されています。

---

## 1. 現在の状況と直近の再開手順

- **現在のステータス**: 公式 Code & Diagram Enhancer Wasm プラグイン（`plugins/official-code-enhancer/`: Mermaid 自動レンダリング、インラインコード整形、外部読み込み対応 6 種マルチコードテーマ、ワンクリックコピー）、コンテンツエディタの Markdown 編集モード & スマート貼り付けサポート（Blocks ⇄ Markdown 双方向切り替え、リアルタイムプレビュー同期、クリップボードからのスマートペースト＆インポートモーダル、リストブロック対応）、公開コンテンツ向けマルチテーマシステム（4種類のテーマ: `default`, `cyber-neon`, `minimal-editorial`, `emerald-luxury`）、将来のテーママーケットプレイスを見据えた独立テーマパッケージ仕様 (`theme.json` マニフェスト、プレビュー画像、`templates/`、`assets/style.css`)、スレッドセーフな動的テーマ切り替え API (`GET /api/v1/themes`, `PUT /api/v1/themes/active`)、Admin Studio のテーマ管理画面 (Themes & Marketplace Preview)、動的アセット配信ハンドラー、Wasm プラグイン共通 ABI 仕様書 & 公式 SDK、MiniJinja テンプレートエンジン、ローカル認証・認可 (RBAC)、ログインレート制限、監査ログ記録、メディアライブラリ UI の実装が完了。
- **動作確認済み**:
  - `task check` による全自動検証（Rust テスト 93 件パス、rustfmt、Clippy ゼロ警告、Admin Studio 本番ビルド、Prettier、oxlint ゼロ警告）すべてパス。
  - 公式 Wasm プラグイン `official-code-enhancer` (`plugins/official-code-enhancer/`):
    - `task plugin:build:code-enhancer` による `plugin.wasm` (253 KiB) 生成と Wasm サンドボックス動作。
    - Mermaid 自動検出（`transform-content-ast`）、Mermaid.js CDN 注入・SVG 自動描画（`on-render-page`）。
    - XSS 安全なインラインコードスマート整形（TextNode 分割）、ワンクリックコードコピーボタン。
    - 6 つのビルトインプリセット（`dracula`, `github-dark`, `one-dark`, `nord`, `github-light`, `monokai`）。
    - 外部 CSS URL（CDN Prism/Highlight.js テーマ: `code_theme_url`）およびカスタム CSS 注入（`code_theme_css`）サポート。
    - 動的テーマ切り替え API (`window.AegisCodeEnhancer.setThemeUrl`)。
  - テーマシステム & マーケットプレイス準備 (`crates/aegis-theme`, `crates/aegis-server`, `themes/`, `admin/`):
    - 独立テーマパッケージ仕様: `theme.json`（ID、バージョン、作者、タグ、ライセンス、最小バージョン等）、`preview.svg`、`templates/`、`assets/style.css`。将来のマーケットからの ZIP 導入を見据えた `validate_theme_package` 機構。
    - 4つの公式テーマ:
      - `default` (Aegis Horizon): 宇宙感・インディゴ/パープルグラデーションの公式ダークテーマ。
      - `cyber-neon` (Tokyo Cyber Neon): ネオンシアン/ピンク、サイバーグリッド、グラスモーフィズム、等幅フォントの超近未来サイバーパンクテーマ。
      - `minimal-editorial` (Nordic Minimal Editorial): 北欧エディトリアル風、上質なセリフタイポグラフィ、アイボリー背景、長文読書に極上のライトテーマ。
      - `emerald-luxury` (Emerald & Obsidian Studio): 黒曜石・エメラルド・シャンパンゴールドの高級感あふれるラグジュアリースタジオテーマ。
    - スレッドセーフな動的テーマ切り替え API (`PUT /api/v1/themes/active`): 管理者権限で即座に公開サイトの見た目とアセットを切り替え、監査ログ `theme.switch` に記録。
    - 動的アセット配信 (`GET /theme/assets/{*path}`): 切り替えられたテーマの CSS/画像を自動解決・配信（パストラバーサル防御付き）。
    - Admin Studio テーマ管理画面 (`ThemeManager.tsx`): インストール済みテーマのカード表示、美麗なプレビュー画像、ワンクリック適用、ライブサイト確認、将来のテーママーケットプレイス（Marketplace v1 Preview & ZIP インポート案内）画面。
    - Wasm プラグインエコシステム:
      - 言語非依存の共通 ABI 仕様書 (`docs/plugin_abi_specification.md`)。
      - 公式 Code & Diagram Enhancer プラグイン (`plugins/official-code-enhancer/`)。
      - Rust / AssemblyScript / TinyGo 公式テンプレート (`sdk/templates/*`) とサンプル (`sdk/examples/*`)。`plugins/` はインストール先で、`[plugins].enabled` に書いたものだけを読み込む。
  - 認証・認可・セキュリティ:
    - ローカル認証 (`LocalAuthProvider`: Argon2id)、セッション管理、RBAC 認可 (`RequireRole`)、レート制限、監査ログ。

### 🚀 次に作業を再開するときの着手項目
**認証・認可の仕上げ（合意済みの順番: ①ローカル認証 → ②認証済みセッション発行 → ③API の認可 → ④ログイン画面 → ⑤フック・OIDC/SAML。①〜④、ログイン試行のレート制限、権限拒否（401 / 403）の監査ログ記録、メディア削除、プラグイン SDK、Wasm 認証後フック（on-user-authenticated）、OIDC シングルサインオン、サブパス運用、コンテンツ向けマルチテーマ基盤は完了）**
1. **SAML 2.0 認証プロバイダ**（SAML は XML 署名検証のネイティブ依存が重いため後回しで合意）。

---

## 2. TODOタスク一覧

> 過去の完了済みタスク（コア基盤、スキーマ、Wasm SDK、テーマエンジン、ローカル認証、メディア削除、不具合修正など）は [docs/completed_tasks.md](file:///d:/projects/cms/docs/completed_tasks.md) に移動・集約しました。

### ⏳ 直近のタスク (Next Up)

#### 公式 Wasm プラグイン: Mermaid & コードテーマエンハンサー (`official-code-enhancer`) (Completed)
- [x] **公式 Code & Diagram Enhancer Wasm プラグインの実装 (`plugins/official-code-enhancer/`)**
  - **Mermaid 自動レンダリング**: `language: "mermaid"` コードブロックの検出と Mermaid.js (SVG) クライアント自動レンダリング連携。
  - **インラインコードのスマート整形**: 本文内の `` `inline code` `` を XSS 安全に `<code>` 要素へ自動整形。
  - **外部読み込み対応マルチテーマシステム**:
    - 6 つのビルトインプリセット（Dracula, GitHub Dark, One Dark, Nord, GitHub Light, Monokai）。
    - 外部 CSS URL / CDN スタイルシート読み込み対応（Prism.js / Highlight.js テーマ対応: `code_theme_url`）。
    - KVS / コンテキストによるカスタム CSS 注入対応 (`code_theme_css`)。
  - **コードブロック機能拡張**: ワンクリックコピーボタン、ハイライトスタイルシート適用。
  - **Taskfile / Workspace 連携**: Cargo ワークスペースへの登録、`task plugin:build:code-enhancer` タスク、テスト自動化。


#### コンテンツ向けマルチテーマ & マーケットプレイス設計 (Completed)
- [x] **将来のMarket連携を見据えたテーマパッケージ仕様 (`theme.json`)**
  - テーマメタデータマニフェスト (`ThemeManifest`, `ThemeInfo`)、`theme.json` 自動読み込み、ディレクトリからのフォールバック推論、将来の ZIP 展開を見据えたバリデーション (`validate_theme_package`)。
- [x] **かっこいいテーマのラインナップ作成 (`themes/`)**
  - `default` (Aegis Horizon): ダーク＆インディゴグラデーションの標準公式テーマ。
  - `cyber-neon` (Tokyo Cyber Neon): ネオンシアン/ピンク、サイバーグリッド、ハイテクグラスモーフィズム。
  - `minimal-editorial` (Nordic Minimal Editorial): ウォームアイボリー、洗練されたセリフ見出し、雑誌・長文読書に極上のライトテーマ。
  - `emerald-luxury` (Emerald & Obsidian Studio): 黒曜石・エメラルドグリーン・シャンパンゴールドのラグジュアリーダークテーマ。
  - 各テーマに美麗なベクターSVGプレビュー画像 (`preview.svg`)、スタイルシート (`assets/style.css`)、MiniJinja テンプレート群 (`templates/`) を同梱。
- [x] **スレッドセーフな動的テーマ切り替え基盤 (`crates/aegis-theme` & `aegis-server`)**
  - `ThemeEngine` のスレッドセーフ化 (`RwLock`)、`set_active_theme` による即時反映。
  - `GET /api/v1/themes`, `GET /api/v1/themes/active`, `PUT /api/v1/themes/active` (AdminSession, 監査ログ `theme.switch` 記録)。
  - `GET /theme/assets/{*path}` 動的アセット解決配信（パストラバーサル防止）、`GET /themes/{id}/preview` プレビュー画像配信。
- [x] **Admin Studio テーマ管理画面 (`admin/src/components/ThemeManager.tsx`)**
  - ナビゲーションに「Themes」タブ（Palette アイコン）を追加。
  - インストール済みテーマ一覧カード（SVGプレビュー、タグ、作者、Activeバッジ、ワンクリック切り替え、ライブサイトリンク）。
  - 将来のマーケットプレイスを見据えた「Theme Marketplace」タブ（コミュニティテーマ探索、カテゴリフィルター、ZIP インポート案内）。
  - 画面全体の縦スクロール対応（親の `height: 100vh; overflow: hidden;` に対し、`flex: 1; overflow-y: auto;` な `<main>` ラッパーを配置し、画面外の全テーマへのアクセス・操作性を確保）。

#### プラグインのインストール・有効化の分離（ストア対応の土台）
- [x] **SDK テンプレートを `plugins/` から `sdk/` へ移動**: テンプレート（`sdk/templates/{rust,assemblyscript,tinygo}`）とサンプル（`sdk/examples/slack-notifier`）が既定の構成・Docker イメージで有効になり、全記事を書き換えていた／AssemblyScript 版が `env.abort` のインポートで全保存を失敗させていた問題を解消。
- [x] **`[plugins].enabled` による有効化**: `plugins/<name>/`（`name` はマニフェストの `name`）にインストールし、設定に書いたものだけを書いた順（= フックの実行順）に読み込む。有効にしたプラグインが未インストール・読み込めない場合は起動を中止。`aegis plugin list`、`GET /api/v1/plugins`（admin）。
- [x] **マニフェストのストア用メタデータ**: `display_name` / `description` / `author` / `license` / `tags` / `homepage` / `repository` / `min_aegis_version`（`theme.json` と同じ項目、任意）。
- [x] **ランタイムの堅牢化**: 読み込み時にインポートを検査（`InstancePre`）、`_initialize` の呼び出し、オブジェクトを返すフックの部分更新（入力に上書き）、プラグイン障害時の保存 API を 400 → 500。
- [x] **テンプレートの修正**: AssemblyScript（`abort` の差し替え、本文を置き換えず追記、`PageRenderContext` のフィールド名をホストに合わせる）、TinyGo（`-target=wasm-unknown`、`PageRenderContext` のフィールド名）。`crates/aegis-runtime/tests/sdk_plugins.rs` で全テンプレートを実ホストで実行。
- [ ] **Admin Studio のプラグイン管理画面**（`GET /api/v1/plugins` の一覧表示。テーマ管理画面と同じ構成）。
- [ ] **実行時の有効・無効の切り替え**（現状は設定ファイル + 再起動。テーマの `PUT /api/v1/themes/active` のように API で切り替えるには、有効化リストの永続化（DB）と `PluginSupervisor` の差し替えが必要）。
- [ ] **ストアからのインストール**（パッケージの検証・展開先は `plugins/<name>/`。テーマの `validate_theme_package` に相当する検証と、署名・`min_aegis_version` の確認）。

#### 認証・認可基盤の完成
- [x] **Wasm 認証後フック (`on-user-authenticated`)**: 資格情報の確認後・セッション発行前に `UserAuthContext`（provider / user_id / email / display_name / role / metadata）をプラグインに渡し、ロール・表示名・metadata の変更をセッションに反映。`Err` でログイン拒否（403、`session.create` denied）、失敗時もログイン失敗（500、`plugin.error`）。ロール変更は `details.role_mapped` に記録。外部 IdP 対応時もこのフックを通す（`auth::hook::run_auth_hook`）。
- [x] **OpenID Connect によるシングルサインオン** (`[auth.oidc]`、Cargo feature `oidc`、既定で有効): 認可コードフロー + PKCE、ID トークン検証（JWKS 署名・iss・aud・exp・nonce・azp、非対称アルゴリズムのみ）。IdP アカウント（Issuer + sub）と `_aegis_users` を `_aegis_user_identities` で紐付け、初回ログイン時に自動作成（同名ユーザーがいれば `username_taken` で拒否）。`role_claim` + `role_mapping` でロールを同期、`on-user-authenticated` フックも適用。`aegis user link-oidc` で既存ユーザーを紐付け。Admin Studio のログイン画面に SSO ボタン、`#session=` / `#login_error=` の受け取り。[docs/authentication.md](docs/authentication.md#5-シングルサインオン-openid-connect)
- [x] **リバースプロキシ配下・サブドメイン / サブパスでの運用**: Admin Studio の API 接続先を配信元のサーバーに（`VITE_AEGIS_SERVER_URL` で別の場所も指定可、Docker は `--build-arg`）。`[server].base_path`（例: `/cms`）でサブパス公開に対応（ルーティング・テーマのリンク・アップロード URL・`/admin` リダイレクト・SSO の戻り先）。プロキシ配下の設定と nginx の例を [docs/configuration.md](docs/configuration.md#4-リバースプロキシ配下での運用) に追記。
- [x] **Markdown 編集モード & スマート貼り付けサポート (`admin/src/components/MarkdownEditor.tsx`, `PasteMarkdownModal.tsx`, `admin/src/utils/markdown.ts`)**:
  - **Blocks ⇄ Markdown 双方向切り替え**: エディタ上部に `[ 🧱 Blocks ]` `[ 📝 Markdown ]` セグメント切り替えタブを追加。ブロックと Markdown を完全にロスなく相互変換し、どちらのモードで編集してもリアルタイムにプレビュー（Live HTML / Themed Page / Block AST）と連動・保存。
  - **専用 Markdown エディタ**: 等幅フォント、行数・文字数・検出 AST ブロック数カウンター、Tab インデント挿入、Ctrl+S 保存、クイック書式挿入ツールバー（見出し、太字、斜体、コードブロック、リスト、画像、リンク、クリップボード貼り付け）。
  - **スマート貼り付け（Paste Everywhere）**: ブロックエディタのキャンバス上で Markdown を貼り付けた際に、自動で AST ブロック群（見出し、段落、コード、画像、リスト等）にパースして追加。
  - **貼り付けモーダル (`PasteMarkdownModal`)**: ツールバーの `[ 📋 Paste Markdown ]` から、クリップボード読み込み・ブロック変換サマリー確認・末尾追加または全置換を選択して安全にインポート可能。
  - **リストブロック対応**: `core/list`（順序付き/箇条書き）の `BlockItem` 編集 UI を追加し、ツールバーからもワンクリックでリスト作成可能に拡張。
- [ ] **SAML 2.0 認証プロバイダ**（SP-initiated、POST バインディング）。XML 署名検証（xmlsec 等）の依存とセキュリティ（XML Signature Wrapping 対策）を検討してから着手。

---

### 🔮 将来のロードマップ (Future)

- [ ] **PostgreSQL / libSQL 対応（分散ストレージモード）**
  - 分散 SQLite / libSQL (Turso) への対応検証およびクラスタリング環境の最適化。
- [ ] **On-demand ISR / Cache-Tag Purge によるエッジキャッシュ制御**
  - コンテンツ更新時の Cloudflare / CDN エッジキャッシュ無効化・タグパージ連動。

---

## 3. 技術メモ・申し送り事項 (Memo & Notes)

- **公式 Code & Diagram Enhancer Wasm プラグイン (`plugins/official-code-enhancer/`)**:
  - **Mermaid 自動レンダリング**:
    - `transform-content-ast` で `core/code` のうち `attributes.language == "mermaid"` のブロックを検出し、`data-diagram-type: "mermaid"` 属性を付与。
    - `on-render-page` で Mermaid.js (ESM 版) を注入し、クライアントサイドで `<pre><code class="language-mermaid">` をインタラクティブな SVG ダイアグラムへ自動置換レンダリング。選択されているテーマと Mermaid テーマ（`dark` / `default`）が自動連動。
  - **インラインコードのスマート整形（XSS 安全）**:
    - 本文（`.prose p`, `.prose li`, `.prose blockquote`）内の `` `inline code` `` を、DOM の `TreeWalker` と `document.createTextNode` / `code.textContent` を用いて分割置換。サーバー側の HTML エスケープやセキュリティリライターを破壊せず、かつ XSS リスク皆無でインラインコードタグ `<code>` へ自動整形。
  - **外部読み込み対応マルチテーマシステム**:
    - 6 種のビルトインプリセット（`dracula`, `github-dark`, `one-dark`, `nord`, `github-light`, `monokai`）。
    - 記事カスタムフィールドまたはテーマ変数から `code_theme` を解決。
    - **外部 CSS URL 読み込み (`code_theme_url`)**: Prism.js などの外部 CDN スタイルシート URL を指定すると、`<link rel="stylesheet">` を動的挿入。
    - **カスタム CSS 注入 (`code_theme_css`)**: 任意の CSS 文字列を直接追加・上書き可能。
    - **動的テーマ切り替え API**: `window.AegisCodeEnhancer.setThemeUrl(url)` により、ブラウザ上や Admin Studio プレビューから動的にスタイルシートを適用可能。
  - **ワンクリックコードコピー**:
    - `<pre>` ブロックの右上にホバー時に浮かび上がる「Copy」ボタンを動的付与。クリック時に `navigator.clipboard.writeText` でクリップボードへ保存し、`Copied!` 表示に遷移。

- **プラグインのインストールと有効化 (`aegis_runtime::WasmPluginLoader`)**:
  - インストール = `plugins_dir/<name>/plugin.manifest.json`（ディレクトリ名とマニフェストの `name` は一致必須、`aegis_core::is_valid_plugin_name`）。有効化 = `[plugins].enabled`（順序 = フックの実行順）。テーマの `themes/<id>/` + `[theme].active` と同じ考え方で、ストアからのインストールもこの形に展開する前提。
  - `load_enabled` は有効なものだけをコンパイルし、失敗したら起動を止める（fail-closed。アクセス制限系のプラグインが黙って外れるのを防ぐ）。無効なものは `discover` でマニフェストだけ読み、不備は警告ログ・`aegis plugin list`・`GET /api/v1/plugins` に出す。
  - `WasmPluginInstance::from_bytes` で `Linker::instantiate_pre` してインポートを検査する（以前は呼び出しのたびのインスタンス化で初めて失敗し、起動ログは「Successfully loaded」のままだった）。`_initialize` があればインスタンス化直後に呼ぶ（TinyGo `wasm-unknown`）。
  - オブジェクトを返すフック（`on_before_save` / `on_render_page` / `on_user_authenticated`）の返り値はトップレベルのキー単位で入力に上書きする（`parse_object_output`）。軽量 JSON しか持たない言語の SDK が一部のフィールドだけを返しても、記事一覧などが消えない。
  - テンプレートのビルド済み `plugin.wasm` はコミットしており、`crates/aegis-runtime/tests/sdk_plugins.rs` が実ホストで全フックを実行する。テンプレートを変えたら `task plugin:build*` で再ビルドしてコミットすること（AssemblyScript は npm、TinyGo は 0.30+ が必要）。

- **Markdown 編集モード & スマート貼り付けアーキテクチャ (`admin/src/utils/markdown.ts`, `MarkdownEditor.tsx`, `PasteMarkdownModal.tsx`)**:
  - **双方向変換と完全同期**:
    - `blocksToMarkdown(blocks)`: `core/heading` (#), `core/paragraph`, `core/code` (```lang), `core/image` (![]()), `core/list` (- / 1.) を標準 Markdown テキストへ直列化。
    - `markdownToBlocks(markdown)`: コードブロック、見出し (H1〜H6)、画像、リスト（順序付き/箇条書きの自動グループ化）、空行区切り段落を堅牢に走査し AST `BlockNode` 配列へ変換。
    - Markdown エディタで文字入力を行うたび即座に AST ブロックへ変換・同期されるため、右側の `<Preview>`（Live HTML / Themed Page / Block AST）がリアルタイムに更新され、そのまま保存 (`handleSave`) してもバックエンドの Wasm フックや DB 保存パイプラインを一切壊さず動作します。
  - **スマート貼り付け（Paste Everywhere & モーダル）**:
    - ブロックキャンバス上の `onPaste` イベントをインターセプトし、クリップボードの内容が Markdown（見出し、コードブロック、リスト、画像、複数段落など）である場合に自動で AST ブロック群にパースして追加。
    - ツールバーの `[Paste Markdown]` ボタンから開くモーダルでは、ワンクリッククリップボード読み込み（`navigator.clipboard.readText`）、解析されたブロック内訳プレビュー（例: `5 blocks: 1 H2, 3 Paragraphs, 1 Code`）、および「既存ブロックの末尾に追加」または「全置換」を安全に選択してインポート可能。
  - **リストブロックの編集対応**:
    - `BlockItem` に `core/list` のサポートを追加し、箇条書き（`- `）と番号付き（`1. `）の切り替え、1行1項目の直感的なリスト編集に対応。

- **OpenID Connect (`auth::oidc::OidcAuthProvider`)**:
  - `AuthProvider` の `initiate_login`（state / nonce / PKCE をキャッシュに 10 分保存して認可 URL を返す）と `authenticate`（`kind = "oidc_callback"`、`parameters` にコールバックのクエリ）で実装。ハンドラー（`handlers/oidc.rs`）は feature に依存せず `AppState::oidc: Option<OidcLogin>` を見る。
  - パスワードログインと SSO はセッション発行・監査・フックを `handlers::auth::complete_login` で共有する。
  - ID 連携テーブルの subject は `oidc_subject(issuer, sub)`（`{issuer}#{sub}`、末尾 `/` を除去）。Issuer を変えると既存の紐付けは使えなくなる。
  - トークンエンドポイントの認証は `client_secret_basic`（IdP が `token_endpoint_auth_methods_supported` で basic を挙げない場合のみ `client_secret_post`）。
  - テストは `auth::oidc_mock::MockIdp`（テスト内で起動する axum の IdP。鍵は起動ごとに P-256 を生成し、秘密鍵はリポジトリに置かない）。E2E は Python の簡易 IdP（対話フォーム付き）で確認した。
- **マルチ言語 Wasm プラグイン SDK 設計 (`docs/plugin_abi_specification.md`)**:
  - ホストとゲスト間は 32bit リニアメモリおよび `(ptr << 32) | len` の 64bit パック整数で疎結合に連携します。
  - **AssemblyScript テンプレート (`sdk/templates/assemblyscript/`)**:
    - `heap.alloc` によるアロケータ (`allocate(len: i32) -> i32`)、`String.UTF8` による文字列変換、外部 npm 依存なしのゼロ依存 JSON パーサー/シリアライザ (`assembly/sdk/json.ts`) を同梱。
    - ホスト API は `@external("host", ...)` でバインド。
  - **TinyGo テンプレート (`sdk/templates/tinygo/`)**:
    - Go 1.21+ / TinyGo の `//go:wasmimport host <func>` ディレクティブにより型安全にホスト関数をインポート。
    - ホストから渡されたメモリバッファが TinyGo GC により回収されないよう、`allocatedBuffers` マップによるポインタ追跡保護機構を内蔵。
    - Go 標準の `encoding/json` により柔軟に `PostContext` / `BlockNode` を構造体マッピング。
  - `Taskfile.yml` に `plugin:build:as` および `plugin:build:tinygo` を追加し、ワンコマンドでのビルドに対応。
- **認証後フック (`auth::hook::run_auth_hook`)**:
  - プロバイダに依存しない形で `AuthIdentity` を受け取り返すので、OIDC / SAML のコールバックからも同じ関数を呼ぶ。`provider` / `user_id` は変更不可（ユーザーのなりすましを防ぐ）、`role` は `auth::ROLES` のみ許可。
  - フックの拒否・失敗はログイン試行の失敗回数に数えない（`login_limiter.record_success` は資格情報確認の直後に呼ぶ）。失敗時にログインを通す（fail-open）と、プラグインで実装したアクセス制限を障害で迂回できてしまうため、失敗も拒否として扱う。
  - Wasm インスタンスのフック呼び出しは `WasmPluginInstance::call_hook`（インスタンス化・入力の書き込み・タイムアウト付き呼び出し・結果の読み出し）に集約した。拒否できるフック（`on_before_save` / `on_user_authenticated`）は `parse_object_output` で `{"error": ...}` を `PluginRejected` に変換する。
- **HTML の生成経路 (`crates/aegis-server/src/render.rs`)**:
  - 保存（`transform_blocks` → `render_blocks_html`）、公開ページ・コンテンツ API（保存済みブロックを `render_blocks_html`）、エディタのプレビュー（`transform_blocks` → `render_blocks_html`）はすべてこのモジュールを通します。レンダリング手順を変えるときはここだけを変更してください。
  - プレビューは編集のたびにプラグインの `transform-content-ast` を呼ぶため、このフックは副作用なしが前提です（`docs/plugin_hooks.md`）。
  - テーマ適用済みのページ全体は `render::render_page`（リレーション解決 → `on-render-page` → テンプレート）で組み立てます。公開ページ（`/articles/{slug}`、`/{slug}`）とプレビューの `page_html` が共通で使います。
  - Admin Studio は `page_html` に `<base href="{サーバーのオリジン}/">` を差し込んで `iframe`（`sandbox=""`）の `srcdoc` に表示します。テーマのアセット（`/theme/assets/*`）やアップロード画像は AegisCMS サーバーから読み込まれます。
- **フロントエンドのスタイル (CSS Modules)**:
  - 書き方は `admin/README.md` の「スタイルの書き方」を参照。共通部品の見た目を親から変えるときは子孫セレクタ（`.toolbar .searchField`）で指定し、CSS の出力順に依存しないようにしています。
  - 見た目を変えないリファクタリングの検証には、Playwright で全画面の `getComputedStyle`（約 90 プロパティ × 全要素）を取得して前後比較しました（13 画面で差分はキーフレーム名のハッシュ化のみ）。
- **ログイン試行のレート制限 (`auth::rate_limit::LoginRateLimiter`)**:
  - 失敗回数は `CacheBackend::increment`（存在しないキーは 0 から数え、TTL は作成時のみ設定する固定ウィンドウ）で数えます。Redis では `INCR` と初回のみの `EXPIRE` を Lua スクリプトで原子的に実行します。上限到達で `login:lock:{scope}:{id}` に解除時刻（UNIX 秒）を TTL 付きで書き、計数キーは削除します。ユーザー名のキーは小文字化した名前の SHA-256 です。
  - クライアント IP は `audit::ClientIp` extractor（`AuditContext` と同じ判定。`[audit].trust_forwarded_for` で `X-Forwarded-For` を優先）で取得します。監査ログが無効でも IP は取得されます。
  - チェック時のキャッシュエラーは安全側に倒して 500 を返し、失敗・成功の記録時のエラーは警告ログのみにしています。サーバー側エラー（DB 障害など）は失敗回数に数えません。
  - `MemoryCacheBackend` は読まれないまま期限切れになったキー（ランダムなユーザー名での試行など）が溜まらないよう、書き込み時にエントリ数が閾値（掃除後の 2 倍、最小 1024）を超えたら期限切れを一括削除します。
  - `cargo clippy --workspace --all-targets` は `sdk/templates/rust` のテストで `default_constructed_unit_structs` が出ます（main でも同じ。CI / `task check` は `--all-targets` なしなので未対応）。
- **Prettier / VS Code**:
  - Prettier は `.editorconfig` のインデント設定を読みます。`[*]` の 4 スペースが CSS / HTML に効いていたため、`[*.{ts,tsx,js,jsx,css,html}]` に CSS / HTML を追加しました。
  - VS Code の Prettier 拡張は既定でワークスペース直下の `.prettierignore` を探すため、`.vscode/settings.json` で `prettier.ignorePath` を `admin/.prettierignore` に向けています。`prettier.requireConfig` により設定ファイルの無い `admin/` 外の JSON 等は整形しません。
  - `src/types/schemas.ts`（`task codegen:ts` の生成物）と Markdown は整形対象外です。
- **ローカル認証 (`docs/authentication.md`)**:
  - セッションの `user_id` はユーザー名（変更不可）で、監査ログの actor もユーザー名になります。ユーザーの改名機能を作る場合はこの前提を見直してください。
  - ロール変更・無効化・パスワード変更は発行済みセッションに反映されません（セッションはキャッシュにのみ保存され、ユーザーとの紐付け索引が無いため）。即時失効が必要になったら、ユーザー単位のセッション索引か、リクエストごとのユーザー状態確認を追加します。
  - ロールの包含関係（`admin` ⊃ `editor` ⊃ `viewer`）は `auth::authorize::role_level` で一元管理しています。API に権限を付けるときはハンドラーの引数に `ViewerSession` / `EditorSession` / `AdminSession` を追加します（Body を読む extractor より前に置く）。`/metrics` は監視系のため公開のままです。
  - 以前の `POST /api/v1/auth/session`（`{"user_id", "role"}` で誰でも任意ロールのセッションを発行できた）は廃止。この形式は 422 になります。
- **メディア一覧 API (`GET /api/v1/storage/objects`)**:
  - ストレージ側に並び順・絞り込みの機能が無いため、サーバーは最大 10,000 件（`MAX_LIST_SCAN`）を走査してからメモリ上で絞り込み・新しい順の並び替え・ページングを行います。超過時はレスポンスの `truncated: true` で通知します（`q` / `content_type` は走査後の絞り込みなので、超過分を探すには `prefix` を使う）。
  - バックエンドはアップロード時の mime type を保持していないため、一覧の `content_type` は拡張子から推定します（`aegis_core::guess_content_type`）。ローカルでは隠しファイル・シンボリックリンクを除外、S3 では末尾 `/` の「フォルダ」オブジェクトを除外します。
  - 一覧は `viewer`、アップロード・削除は `editor` が必要です（[docs/authentication.md](file:///d:/projects/cms/docs/authentication.md)）。
- **メディア削除 API (`DELETE /api/v1/storage/objects/{key}`)**:
  - 存在確認に `StorageBackend::exists` を追加（既定はキーを prefix にした `list` での完全一致。ローカルは `metadata` で上書き）。S3 の DELETE は存在しないキーでも成功するため、404 を返すには事前の確認が必要。
  - 使用中の判定は全スキーマの `list_contents` を読み、`blocks_json` とカスタムフィールドの JSON に公開 URL が含まれるかの文字列一致。コンテンツ数が増えて遅くなった場合は、保存時に参照する URL を別テーブルに記録する方式に切り替える。
  - 使用中で拒否した `409` は監査に記録しない（入力不備と同じ扱い）。削除した場合は `details.references` に削除時点の参照元の件数を残す。
- **アップロードのサイズ上限**:
  - axum の `Multipart` は既定でリクエストボディを 2MB に制限します。`/api/v1/storage/upload` には `DefaultBodyLimit::max(config.storage.max_upload_bytes())` を個別に設定しているため、上限を変える場合は `[storage].max_upload_mb` を変更してください（リバースプロキシ側の上限、例: nginx の `client_max_body_size` も合わせる必要があります）。
- **Admin Studio のコンテンツ型と API レスポンスの差異**:
  - 自動生成の `admin/src/types/schemas.ts`（`ArticleContent` など）はカスタムフィールドをトップレベルに持ちますが、実際のコンテンツ API はカスタムフィールドを `fields`、解決済みリレーションを `relations` に入れて返します。Admin Studio では `types/index.ts` の `ContentItem`（`BaseContentItem` + `fields` / `relations`）を使ってください。作成時のペイロードはトップレベル（`CreateArticlePayload` など）で正しいです。
- **監査ログの保持期間**:
  - `AuditService::spawn_retention_task` が起動直後と `purge_interval_secs` ごとに `purge_expired` を実行し、`DatabaseAdapter::delete_audit_events_before` で `occurred_at < 基準時刻` のレコードを削除します。削除が 1 件以上あれば `audit.purge` を記録します。
  - 既定値は `retention_days = 0`（無期限）です。既存環境でアップデート後に監査ログが意図せず消えないよう、明示的に設定した場合のみ削除します。
- **Admin Studio の配信パスと API 接続先**:
  - Admin Studio は `/admin/` で配信されます（`/` はテーマエンジンの公開サイト。`/admin` はスラッシュ付きへ 307）。Vite の `base: './'` でアセットを相対参照し、`/admin/assets/*` から配信します（以前の `/assets` の配信は廃止）。
  - API の接続先は `admin/src/api/client.ts` の `SERVER_URL`（オリジン + ベースパス）で、ビルド時の `VITE_AEGIS_SERVER_URL` → 開発サーバーなら `http://127.0.0.1:8080` → 本番ビルドはバンドルの URL（`{サーバー}/admin/assets/*.js` の `../../`）の順に決まります。ドメイン・ベースパスごとの再ビルドは不要です（以前は `127.0.0.1:8080` 固定で、プロキシ越しにはブラウザから API に届かなかった）。
- **サブパス公開 (`[server].base_path`)**:
  - ルーターはすべて `/` 起点のまま、`app::with_base_path` が外側でベースパスを取り除いてから渡します（ベースパスの無いリクエストもそのまま受け付ける。ヘルスチェックやパスを取り除くプロキシのため）。新しいルートを追加するときにベースパスを意識する必要はありません。
  - 逆にサーバーが**出力する** URL にはベースパスが必要です。ルート起点のパスには `app::under_base_path` を使い、テンプレートではグローバル変数 `{{ base_path }}` を付けます（`ThemeEngine::with_base_path`）。現在の対象: テーマのリンク、ローカルストレージの公開 URL、`/admin` のリダイレクト、OIDC の `post_login_redirect`。
  - アップロード URL はベースパス込みで本文（`blocks_json`）に保存されるため、運用中に `base_path` を変えると既存の画像が切れます。本文中の手書きのルート起点リンクも書き換えません。必要になったら保存時は相対・描画時に付与する方式を検討してください。
  - 値は `ServerConfig::normalized_base_path` で正規化・検証（URL で安全な文字のみ）するので、テンプレートでは自動エスケープしない（`Value::from_safe_string`）。
- **カスタムフィールドの型付きバインド (PostgreSQL / MySQL)**:
  - `db::CustomFieldValue::from_json` がスキーマのフィールド型に合わせて値を変換し（jsonb → JSON、integer → i64、boolean → bool、その他は文字列）、変換できない値は `AegisError::Validation`（API は 400）を返します。SQLite は従来どおり文字列で保存します。
  - 読み出しも各アダプターの `custom_fields_from_row` でフィールド型ごとにデコードします。新しいフィールド型を追加する際は、`SchemaMigration` の型対応・`CustomFieldValue`・各アダプターのデコードを揃えて更新してください。
  - MySQL は `CREATE INDEX IF NOT EXISTS` に対応していないため、既存インデックスの重複エラー（1061）のみ無視しています。
- **監査ログ (`AuditSink`) の設計**:
  - 出力先は `aegis_core::AuditSink` トレイトで抽象化し、`AuditService` が `[audit].sinks` に列挙された全出力先へ順に配送します。出力先の失敗はエラーログに留め、リクエストは失敗させません。
  - `occurred_at` は UTC・マイクロ秒精度・`Z` 終端の RFC 3339 に正規化して保存するため、DB 上の文字列比較で期間検索できます。API の `since` / `until` も同じ形式に正規化してから比較します。
  - 操作主体はリクエストのセッションから解決する `AuditContext` エクストラクターで取得します（`FromRequestParts`、失敗しない）。クライアント IP 取得のため、サーバーは `into_make_service_with_connect_info::<SocketAddr>()` で起動しています。
  - セッショントークンは Bearer 認証情報のため保存せず、`auth::session_fingerprint`（SHA-256 先頭 16 桁）で突き合わせます。
- **設定ファイルの読み込み方針**:
  - `AegisConfig` 配下の全構造体は `#[serde(default)]` 付きで、任意のセクション・項目を省略しても `Default` 実装の値で補完されます。新しい設定項目を追加する際は、必ず `Default` 実装に既定値を追加してください。
  - `AegisConfig::load_or_default` はファイル不在時のみ既定値へフォールバックし、パースエラーは `Err` を返してサーバー起動を中止します（設定ミスに気付かず既定値で動き続けることを防ぐため）。
- **Wasm プラグイン公式 SDK の設計とメモリ境界**:
  - `#[aegis_plugin]` マクロは、ホストが要求する `allocate(len: i32) -> i32`、`on_before_save(ptr, len) -> i64`、`transform_content_ast(ptr, len) -> i64` を完全に自動生成し、ポインタパッキング（`(ptr << 32) | len`）と JSON 変換を隠蔽します。
  - `on_before_save` の結果 JSON は `PostContext` か `{"error": "<メッセージ>"}`（`abi::write_error_to_host`）。ホストは `error` キーを持つオブジェクトを拒否として扱う（`PostContext` に `error` フィールドは無いので既存プラグインと互換）。入力 JSON のパース失敗も拒否にしている（検証フックが黙って素通りしないように）。
  - `transform_content_ast` / `on_render_page` で `Err` を返した場合は、これまでどおりエラーログを出して入力をそのまま返す（そのプラグインの変更だけが無効になる）。これらで処理全体を失敗させたくなった場合は同じエンベロープを使える。
  - `sdk/templates/rust/plugin.wasm` は SDK 変更後に `task plugin:build` で再ビルドしてコミットする（`sdk/examples/slack-notifier/plugin.wasm` は手書き WAT から生成されるため SDK とは無関係）。
- **AssemblyScript プラグイン開発と TypeScript/IDE 型定義**:
  - AssemblyScript の独自型（`i32`, `i64`, `f32`, `bool` 等）は、`assemblyscript/std/assembly.json` の型定義 (`declare type i32 = number`) を介して IDE に認識されます。
  - `tsconfig.json` には TS 6.0 以降の非推奨オプションエラー (`TS5101`) を回避するための `"ignoreDeprecations": "6.0"` が必要です。
  - 公式規約に従い `assembly/tsconfig.json` とプロジェクトルートの `tsconfig.json` を整備し、ホストインポート関数のデコレータ (`@external`) には `// @ts-ignore` を付与して標準 TSServer との互換性を確保しています。
- **ステートレスセッション & キャッシュ設計**:
  - セッションデータは `SessionData` として JSON 直列化され、抽象化された `CacheBackend` に格納されます。
  - セッショントークンは `ses_{UUIDv4}` 形式で発行され、HTTP リクエストの `Authorization: Bearer <token>` または `X-Session-Id` ヘッダーから抽出されます。
  - バックエンドとして `backend = "redis"` を指定した場合は Redis クラスター/スタンドアロンでセッションが共有され、サーバープロセス自体は完全なステートレスとして水平スケール可能です。
  - デフォルトは `backend = "memory"` となっており、追加のミドルウェアや環境構築なしに即座にローカル実行・テストが可能です。
- **Admin Studio SPA 配信**:
  - `aegis-server` は起動時に `admin/dist/index.html` の存在をチェックし、存在すれば `tower_http::services::ServeDir` と `ServeFile` による SPA フォールバック配信を行います。
  - フロントエンド更新時は `npm --prefix admin run build` を実行すれば、サーバーの再コンパイル不要で即座に反映されます。
- **コンテンツ API ペイロード仕様**:
  - `POST /api/v1/contents/{content_type}` の JSON ペイロードはトップレベルに `title`, `slug`, `status`, `blocks`, `author_id` を持ちます。
  - `blocks` 配列内の各要素は `BlockNode` (`{ "name": "core/heading", "attributes": { "level": 1 }, "text": "...", "children": [] }`) 形式です。
- **Wasmtime と Windows 環境の非同期実行**:
  - Windows MSVC 環境では、Wasmtime の同期呼び出しで Trap が発生した場合に Rust 1.98 の `extern "C"` non-unwinding 境界チェックと競合して abort するケースがあります。
  - そのため `aegis-runtime` では `config.async_support(true)` を有効化し、`tokio::time::timeout` でタイムアウトを明示的に強制する多層防御設計を採用しています。
- **リニアメモリのデータ受け渡し**:
  - ホストからゲストへ文字列/JSONを渡す際は、必ずゲスト側がエクスポートする `allocate(len: i32) -> i32` を呼び出して安全なメモリ領域を確保してから書き込みます。
  - ゲストからホストへの返却は、上位32bitが `ptr`、下位32bitが `len` の `i64` パック形式で受け渡しを行っています。
- **データベーステーブル命名規則**:
  - スキーマ `content_type: article` の場合、テーブル名は `content_article` となります。
  - ASTブロック配列は `blocks_json TEXT NOT NULL DEFAULT '[]'` カラムに格納されます。
- **Docker コンテナ運用と再現性**:
  - 本番イメージは `Dockerfile` によるマルチステージビルド（`node:22.22-alpine` -> `rust:1.99.1-slim-bookworm` -> `debian:bookworm-slim`）で構築され、イメージサイズは約 163MB です。
  - ベースイメージはバージョンを固定しています（`latest` は Rust のリリースごとに変わってレイヤーキャッシュが無効になり、実行環境の bookworm と OS もずれるため）。Rust を更新する際は `Dockerfile` のタグを上げてください。
  - Rust のビルドは cargo-chef で「依存クレートのビルド（`cargo chef cook`）」と「ワークスペースのビルド」を分けており、`Cargo.toml` / `Cargo.lock` が変わらない限り依存のレイヤー（約 1.7GB）は CI の GitHub Actions キャッシュ（`type=gha`）から再利用されます。残りの所要時間の大半は release プロファイルの `lto = true` / `codegen-units = 1` によるリンク時最適化（約 3 分）です。
  - セキュリティのため、UID 10001 の非rootユーザー `aegis` でプロセスを実行します。データ永続化には `/app/data` をボリュームマウント（例: `-v aegis-data:/app/data`）してください。
  - コンテナ内でのリッスン設定は `config/docker.toml`（`host = "0.0.0.0"`, `port = 8080`）で定義され、環境変数 `AEGIS_CONFIG`、`HOST`、`PORT` でも動的に上書き可能です。
- **ストリーミング HTML セキュリティ & 最適化リライター (`lol-html`)**:
  - `crates/aegis-ast` 内の `StreamingHtmlRewriter` は、Cloudflare 製の CSS セレクタベース高速ストリーミングパーサー `lol-html` を使用しています。
  - 外部ドメイン判定された `<a>` タグに対し、`rel="noopener noreferrer"` を自動付与（既存の rel トークンを尊重しスマートマージ）します。
  - 同様に、未指定の `<img>` タグへ `loading="lazy"` および `decoding="async"` を強制インジェクションし、ゼロ・トラストなコンテンツセキュリティと Core Web Vitals パフォーマンス向上を同時に実現します。
  - `config/*.toml` の `[security]` セクション（`internal_domains` や `open_external_links_in_new_tab` など）から柔軟に挙動をカスタマイズ可能です。
- **スキーマ駆動コード生成 (Codegen & OpenAPI)**:
  - YAMLスキーマ (`schemas/*.yaml`) を修正・追加した場合、`task codegen`（または `task codegen:ts`, `task codegen:openapi`）を実行することで、フロントエンド用 TypeScript 型定義 (`admin/src/types/schemas.ts`) および OpenAPI 3.1 仕様書 (`docs/openapi.json`) をワンコマンドで自動更新できます。
  - サーバー稼働中は `GET /api/v1/openapi.json` エンドポイントからも最新の OpenAPI 3.1 仕様が JSON 形式で動的に配信され、Swagger UI / Redoc や外部 API クライアント生成ツールと容易に統合できます。
- **宣言的スキーマ進化 (SQLite 自動カラム追加マイグレーション)**:
  - `aegis-server` の起動時マイグレーション (`run_migrations`) は、テーブル作成 (`CREATE TABLE IF NOT EXISTS`) だけでなく、`PRAGMA table_info` を走査して既存テーブルの既存カラム定義を取得します。
  - YAML スキーマに新しく定義されたフィールド（例: `category_id`）が既存テーブルに存在しない場合、自動的に `ALTER TABLE {table_name} ADD COLUMN {col_name} {col_type}` を実行してからインデックスを生成します。
  - これにより、開発中やリリース後のスキーマ変更時でも、手動マイグレーションスクリプトを書くことなく、既存データを保持したまま安全にスキーマを進化させることが可能です。
- **動的リレーション自動解決 (Automatic Relation Resolution)**:
  - スキーマのフィールド定義で `type: relation` および `target: category.id` （`<target_schema>.<target_column>`）が指定されている場合、API レスポンスの `relations` マップに自動解決された関連リソース情報（`id`, `content_type`, `title`, `slug`）が埋め込まれます。
  - フロントエンドは個別に追加 API リクエストを発行することなく、単一のコンテンツ取得クエリでリレーション先のメタデータ（カテゴリ名やスラッグ）を即座に描画・利用可能です。
- **マルチRDBMS 抽象化アーキテクチャ (`DatabaseAdapter`)**:
  - `DatabaseAdapter` トレイトによって SQLite / PostgreSQL / MySQL の SQL 方言・プレースホルダ構文（`?1` vs `$1` vs `?`）・接続プール・カラム情報スキーマ走査（`PRAGMA table_info` vs `information_schema.columns`）の違いを完全に隠蔽。
  - ハンドラ層は `DatabaseAdapter` 経由でコンテンツ CRUD やリレーション解決を行うため、特定の RDBMS に一切ロックインされません。
  - `config/*.toml` の `[database]` セクションで `backend = "sqlite" | "postgres" | "mysql"` と `url` を指定するだけで、サーバー再コンパイル不要でデータベースを切り替え可能です。
- **マルチストレージ抽象化アーキテクチャ (`StorageBackend`)**:
  - `StorageBackend` トレイト（`put`, `get`, `delete`, `exists`, `list`, `public_url`）により、ローカルディスク/ブロックストレージ (`LocalStorageBackend`) と S3 互換オブジェクトストレージ (`S3StorageBackend`) を Config ベースで透過的に切り替え可能。
  - `S3StorageBackend` は AWS SigV4 認証プロトコルをネイティブ実装しているため、重量級の AWS SDK を追加することなく、AWS S3, Cloudflare R2, MinIO, Wasabi と完全互換で高速・軽量に動作します。
  - マルチパートファイルアップロード API (`POST /api/v1/storage/upload`) により、メディアやアセットの柔軟な保存と URL 返却に対応しています。
- **認証拡張方針（Crate 抽象化と Wasm プラグインフックの役割分担）**:
  - 認証・認可におけるセキュリティリスク（認証バイパス、暗号脆弱性、トークン漏洩）を防ぐため、認証判定や暗号処理（SAML XML-DSIG、OIDC JWT/JWKS 検証など）をサードパーティ Wasm プラグインには直接開放しません。
  - 代わりに `aegis-core` の `AuthProvider` トレイトによる Rust Crate（静的コンパイル / Cargo Features）として安全に実装・分離します。
  - 一方で、認証成功後のメタデータ付加、部署属性によるロールマッピング、Slack/Teams へのログイン監査通知などの安全な拡張ロジックのみを Wasm プラグイン（`on_user_authenticated` フック）に委譲する設計を採用します。
- **コンテンツ公開 & テーマエンジン・プラグイン書き換えアーキテクチャ**:
  - `MiniJinja` をコアエンジンとした `aegis-theme` クレートを導入。ビルトインのデフォルトテンプレート（`base.html`, `index.html`, `article.html`, `page.html`, `404.html`）をバイナリ内にコンパイル時埋め込みしているため、追加のテーマファイルなしでも即座に洗練されたモダン Web サイトを配信可能です。
  - テンプレートの探索は「プラグイン/インメモリ動的登録 (`register_template`)」→「ファイルシステムテーマ (`themes/{active}/templates/`)」→「ビルトインデフォルト」の 3 層フォールバック機構を採用。
  - プラグインフック `on_render_page` により、ページレンダリング直前に `PageRenderContext` をプログラマブルに書き換え可能（サイトタイトル修飾、カスタム CSS 注入、`<head>` タグ追加、テーマ変数の動的更新、テンプレート差し替え）。
  - 一般公開ルート（`GET /`, `GET /articles`, `GET /articles/{slug}`, `GET /{slug}`）と管理画面（`GET /admin` & `/assets`）を同一サーバー上でルーティング分離し、下書き記事はゼロトラストに 404 で遮断。記事本文は `StreamingHtmlRewriter` を透過しセキュリティ・パフォーマンス最適化（rel 属性付与、画像遅延読み込み等）が自動適用されます。
- **将来のマーケットプレイスを見据えた独立テーマパッケージ仕様 & 動的切り替え設計**:
  - 各テーマは `themes/{theme_id}/` ディレクトリ配下に完全カプセル化されます。
    - `theme.json`: テーマの一意識別子 (`id`)、表示名 (`name`)、バージョン (`version`)、説明 (`description`)、作者情報 (`author`)、ライセンス (`license`)、プレビュー画像ファイル (`preview_image`)、タグ配列 (`tags`)、ホームページ/リポジトリ URL、対応 AegisCMS 最小バージョン (`min_aegis_version`) を定義。
    - `preview.svg` / `preview.png`: 管理画面やマーケットプレイスで表示される高品質なベクター/ラスタープレビューサムネイル。
    - `assets/style.css`: テーマ固有の CSS・画像・フォント等の静的アセット。
    - `templates/`: MiniJinja テンプレート群 (`base.html`, `index.html`, `article.html`, `page.html`, `404.html`)。
  - `ThemeEngine` はスレッドセーフな `RwLock` 内部状態を持ち、`ThemeEngine::set_active_theme` を呼ぶことでサーバー再起動不要で即座にアクティブテーマおよびテンプレート探索ディレクトリを更新可能。
  - 動的アセット配信ハンドラー (`GET /theme/assets/{*path}`) により、アクティブテーマのアセットディレクトリをリクエスト時に動的に解決して配信。パストラバーサル（`..` やルートアクセス）を厳格にブロックし安全性を担保。
  - テーマ切り替え API (`PUT /api/v1/themes/active`) は `AdminSession` 必須の RBAC 認可と監査ログ `theme.switch`（変更前後のテーマID）の自動記録を内蔵。
  - 将来のマーケット導入を見据えたバリデーション API `ThemeEngine::validate_theme_package` を備え、ZIP 展開時や外部レジストリからのインポート時にパッケージの整合性を検証できます。
- **テンプレートフィルタの型安全性と None-safety (`date_short`, `truncate`)**:
  - MiniJinja のフィルタ定義において、引数型を `val: &str` にすると `None`（`Value::from(())`）や未定義値が渡った際に `invalid operation: value is not a string` ランタイムエラーが発生します。
  - そのため `engine.rs` では `val: minijinja::Value` を受け取り、`val.as_str()` でフォールバック（空文字等）を安全に処理する堅牢な実装を採用。記事の `author` や `published_at` が未設定の場合でも 500 エラーにならず安定描画されます。
- **Web アセット MIME タイプ解決 (`guess_content_type`) とスタイルシート配信**:
  - `aegis-core` の `guess_content_type` に Web アセット用の MIME 定義（`.css` -> `text/css`, `.js` -> `application/javascript`, `.woff2` -> `font/woff2`, `.wasm` -> `application/wasm` 等）を完備。
  - ブラウザの厳格なスタイルシート MIME チェック（`strict MIME checking`）により `application/octet-stream` では CSS の適用が拒否される問題を防ぎ、全テーマの外部 CSS が確実に解釈・レンダリングされます。
- **Admin Studio フルスクリーンビューのスクロール設計**:
  - `App.module.css` のルート `.app` は `height: 100vh; overflow: hidden;` で固定されているため、`AuditLogView` や `ThemeManager` などの全幅ビューは必ず直下に `flex: 1; overflow-y: auto;` を持つラッパー `<main>` を配置し、下部パディングを確保してスクロール可能にする必要があります。
