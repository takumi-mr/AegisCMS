# AegisCMS ドキュメントポータル (Documentation Index)

AegisCMS の設計、設定、機能仕様、拡張開発（テーマ・プラグイン）、運用に関する公式ドキュメント集です。  
知りたい内容や担当ロール（サイト運用者、テーマ制作者、プラグイン開発者、コア開発者）に応じて最適なドキュメントを参照してください。

---

## 🗺️ ドキュメントマップ

### 1. 🏗️ コアアーキテクチャ & 設計
| ドキュメント | 概要 | 対象 |
|---|---|---|
| [アーキテクチャ設計書 (`architecture.md`)](architecture.md) | 全クレート構成、DB抽象化、公開パイプライン、キャッシュ/セッション、Wasmランタイム、バックアップなどシステム全体の詳細設計 | コア開発者・運用者 |
| [設定ファイルリファレンス (`configuration.md`)](configuration.md) | `config/default.toml` の全項目リファレンス、環境変数オーバーライド、Admin Studio からの動的システム設定（`system.*`） | 運用者・コア開発者 |
| [開発ガイド (`development.md`)](development.md) | `task` コマンド、ビルド手順、CI、ローカルDB検証、E2Eテスト、Storybook、CLIリファレンス、開発Tips | コア開発者 |
| [REST API 仕様書 (`openapi.json`)](openapi.json) | OpenAPI 3.0 準拠の REST API 仕様定義ファイル | API連携者・開発者 |

### 2. 🔐 セキュリティ・認証・監査
| ドキュメント | 概要 | 対象 |
|---|---|---|
| [認証 & ユーザー管理 (`authentication.md`)](authentication.md) | ローカル認証（Argon2id）、セッション管理、OpenID Connect、SAML 2.0、アカウント登録・招待フロー | 運用者・コア開発者 |
| [監査ログリファレンス (`audit.md`)](audit.md) | 操作監査ログの出力先（DB/ログ/Prometheus）、イベント種別一覧、保持期間、Admin Studio での閲覧 | 運用者・監査者 |
| [セキュリティ評価レポート (`security-assessment-report.md`)](security-assessment-report.md) | 内部検査、OWASP ZAP スキャン結果、脆弱性対策状況とステータス | セキュリティ担当 |
| [ペネトレーションテスト手順 (`penetration-testing.md`)](penetration-testing.md) | Docker Compose を用いた安全なローカル検証環境の構築と脆弱性スキャン手順 | セキュリティ担当 |

### 3. 🎨 テーマ開発 & フロントエンド
| ドキュメント | 概要 | 対象 |
|---|---|---|
| [テーマ作成ガイド (`theme_authoring_guide.md`)](theme_authoring_guide.md) | テーマのディレクトリ構成、`theme.json`、テンプレート変数、CSS設計、`theme dev`・Theme Builder の使い方 | テーマ制作者 |
| [コンテンツ公開 & テーマエンジン開発ガイド (`theme_and_publishing_guide.md`)](theme_and_publishing_guide.md) | MiniJinja 3層フォールバック機構、予約公開/非公開、Atom フィード、公開ルーティング設計 | テーマ制作者・開発者 |
| [本文ブロック & Markdown リファレンス (`blocks.md`)](blocks.md) | 見出し、段落、表、TOC、コールアウト、カスタム HTML（sanitized / sandbox）、プラグインブロック仕様 | コンテンツ作成者・制作者 |

### 4. 🔌 WebAssembly プラグイン拡張
| ドキュメント | 概要 | 対象 |
|---|---|---|
| [Wasm プラグイン開発公式ガイド (`plugin_sdk_guide.md`)](plugin_sdk_guide.md) | Rust / AssemblyScript / TinyGo SDK によるプラグイン開発、ビルド、テスト、配布・署名手順 | プラグイン開発者 |
| [プラグインフック & ホスト API リファレンス (`plugin_hooks.md`)](plugin_hooks.md) | 利用可能な全フック仕様、データ構造、マニフェスト定義、ホスト API、動的実行順制御 | プラグイン開発者 |
| [共通 ABI 仕様書 (`plugin_abi_specification.md`)](plugin_abi_specification.md) | Wasm モジュールとホスト間の低レベル ABI シグネチャ、メモリレイアウト、データ交換規約 | SDK開発者 |

### 5. 🧩 組み込み機能・サービス
| ドキュメント | 概要 | 対象 |
|---|---|---|
| [メディア & ストレージ管理 (`media.md`)](media.md) | アップロード処理、画像最適化、ファイルサイズ制限、ストレージ操作 API | 運用者・開発者 |
| [サイト内検索 (`search.md`)](search.md) | tantivy を用いた全文検索、インデックス構築・自動更新、検索 API / 画面表示 | 運用者・開発者 |
| [アクセス解析 (`analytics.md`)](analytics.md) | Cookie を使用しないプライバシー配慮型軽量アクセス解析、統計収集と Admin Studio 表示 | 運用者 |
| [問い合わせフォーム (`contact.md`)](contact.md) | 設定ファイルによるフォーム動的生成、ボット対策、通知連携、Admin Studio 受信箱 | 運用者・制作者 |
| [ニュースレター (`newsletter.md`)](newsletter.md) | メールアドレスの申し込み（ダブルオプトイン）、配信停止リンク、公開中の記事の一斉配信、Admin Studio での管理 | 運用者・制作者 |
| [リダイレクト (`redirects.md`)](redirects.md) | 旧 URL から新 URL への 301 / 302 転送、slug 変更時の自動の規則、API、Admin Studio での管理 | 運用者・制作者 |
| [タグ (`tags.md`)](tags.md) | どのコンテンツにも付けられる管理されたタグ、API、Admin Studio での管理と付与、バックアップ | 運用者・制作者・開発者 |
| [コメント管理 (`comments.md`)](comments.md) | 記事・商品へのコメント受付、モデレーション（承認制・NGワード・IP制限）、通知 | 運用者・制作者 |
| [掲示板システム (`forum.md`)](forum.md) | スレッド式掲示板、独立 DB 構成、キーセットページネーション、通報・モデレーション | 運用者・制作者 |
| [EC・コマース機能 (`commerce.md`)](commerce.md) | 商品・バリエーション、カート、在庫引き当て、注文ステータス遷移、決済連携 | 運用者・開発者 |
| [通知サービス (`notifications.md`)](notifications.md) | メール（SMTP）および Webhook によるイベント通知、多言語テンプレート、キュー配信 | 運用者・開発者 |

---

## 🎯 ロール別おすすめ読書順

### 👩‍💼 サイト運用者 / 管理者
1. [設定ファイルリファレンス (`configuration.md`)](configuration.md) - 基本設定および管理画面からの動的設定（`system.*`）
2. [認証 & ユーザー管理 (`authentication.md`)](authentication.md) - ユーザー登録、ロール権限、SSO 連携
3. [監査ログ (`audit.md`)](audit.md) / [アクセス解析 (`analytics.md`)](analytics.md) - 運用監視とアクセス統計

### 🎨 Webデザイナー / テーマ制作者
1. [テーマ作成ガイド (`theme_authoring_guide.md`)](theme_authoring_guide.md) - テーマ構成と `theme dev` / Theme Builder による高速開発
2. [本文ブロック & Markdown リファレンス (`blocks.md`)](blocks.md) - 記事本文に挿入される要素と CSS スタイリング
3. [コンテンツ公開 & テーマエンジン開発ガイド (`theme_and_publishing_guide.md`)](theme_and_publishing_guide.md) - 公開URLルーティングと MiniJinja 詳細仕様

### 🧩 プラグイン拡張開発者
1. [Wasm プラグイン開発公式ガイド (`plugin_sdk_guide.md`)](plugin_sdk_guide.md) - テンプレートからの開発開始とデバッグ
2. [プラグインフック & ホスト API リファレンス (`plugin_hooks.md`)](plugin_hooks.md) - フックとホスト API の完全仕様
3. [共通 ABI 仕様書 (`plugin_abi_specification.md`)](plugin_abi_specification.md) - 低レベルメモリ ABI（新言語 SDK 開発時など）

### 🛠️ AegisCMS コア開発者 / コントリビューター
1. [エージェント運用規約 (`AGENTS.md`)](../AGENTS.md) - ブランチ規約、コミット規約、PR 検証ルール
2. [開発ガイド (`development.md`)](development.md) - `task` コマンド、テスト実行、ローカル検証環境
3. [アーキテクチャ設計書 (`architecture.md`)](architecture.md) - システム全体のアーキテクチャと内部構造
4. [タスク管理 (`TODO.md`)](../TODO.md) - 現在のステータスと未着手タスク

