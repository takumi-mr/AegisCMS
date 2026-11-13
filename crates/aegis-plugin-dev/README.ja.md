# aegis-plugin-dev

[English](README.md) | **日本語**

AegisCMS Wasm プラグイン開発者向けのスタンドアロン・ローカル開発・テストサーバーおよび Plugin Builder 連携 API。

## 概要

`aegis-plugin-dev` は、本体サーバーのセットアップやデータベース、管理者認証を必要とせず、プラグイン単体（`plugin.manifest.json` と `plugin.wasm`）を即座に検証・テスト・デバッグするための開発ツールです。

`aegis-server plugin dev <dir>` コマンド（または本クレートの単体バイナリ）で起動し、Wasmtime サンドボックス上でフック関数を直接実行・検査できるほか、Plugin Builder GUI (`plugin-builder/`) とシームレスに連携します。

## 主な責務・提供機能

- **フックのダイレクト実行 (`POST /__dev/api/invoke`)**:
  - テストペイロード（JSON）を渡して、指定したフック（`on-before-save`, `transform-content-ast`, `calculate-order` 等）を直接実行。
  - 消費 Fuel（CPU 命令数）、実行時間（ミリ秒）、Wasm ログ出力（`log_*`）をリアルタイムに収集・可視化。
- **ホスト環境のモックと追跡**:
  - プラグイン固有 KVS のインメモリシミュレーションと、キー更新履歴の追跡。
  - 外部 HTTP リクエスト（`http::fetch`）のモック応答設定と、実際の発行履歴検査。
  - プラグイン設定値（`config::get`）の動的インジェクション。
- **HTML ライブプレビュー (`POST /__dev/api/preview`)**:
  - `on-render-page` や `render-blocks` などのフロントエンド系フックについて、サンプルの HTML やブロック AST を入力として即座に描画結果をプレビュー。
- **自動再コンパイルとホットリロード**:
  - Wasm バイナリやマニフェストの更新を検知し、インスタンスを自動リロード。
- **Plugin Builder UI 連携 API**:
  - ブラウザ上の Plugin Builder（React + Vite）からプラグインのテスト実行やマニフェスト編集を行うための REST API を提供。

## 使い方

```bash
# AegisCMS 統合コマンドから起動
aegis-server plugin dev plugins/my-plugin

# ポート指定
aegis-server plugin dev plugins/my-plugin --port 4000
```
