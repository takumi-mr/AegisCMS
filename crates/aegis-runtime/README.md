# aegis-runtime

Wasmtime を基盤とするゼロトラスト WebAssembly (Wasm) プラグイン実行エンジンおよびスーパーバイザー。

## 概要

AegisCMS では、従来のモノリシック CMS のようにプラグインがホスト本体やデータベースの生メモリに直接アクセスすることを許しません。すべてのプラグインは Bytecode Alliance の `wasmtime` による独立したリニアメモリ空間内で隔離実行されます。

`aegis-runtime` は、プラグイン Wasm バイナリのロード、インスタンスプール管理、ホスト API 関数のバインド、Fuel（燃料）消費計算やエポック割り込みによるタイムアウト・リソース枯渇監視、およびマニフェストに基づくケイパビリティ検証（Zero-Trust Capability Guard）を提供します。

## 主な責務・提供機能

- **Wasm 実行エンジン (`wasm::WasmEngine`)**:
  - `wasmtime` による JIT コンパイルとインスタンス生成。
  - Fuel（命令数）制限による CPU バウンドな DoS 対策。
  - Epoch ベースの割り込みによる確実な実行タイムアウト制御。
  - メモリ最大割り当てサイズ（線形メモリ制限）の強制。
  - フック実行結果のキャッシュ機構 (`ResultCache`) によるレンダリング最適化。
- **ケイパビリティガード (`CapabilityGuard`)**:
  - プラグインの `plugin.manifest.json` に記述された権限ホワイトリスト（外部許可ドメイン、KVS 名前空間、許可フック等）の実行時検証。
  - 未許可リソースへのアクセスを即時拒絶。
- **ホストサービス・API バインディング (`services`, `wasm::host_state`)**:
  - 構造化ロギング (`log_*` -> ホストの `tracing`)。
  - プラグイン個別 KVS ストレージアクセス。
  - ドメイン検証付き HTTP クライアント。
  - Admin Studio からの設定値読み込み。
- **プラグインスーパーバイザー (`PluginSupervisor`)**:
  - `plugins/` ディレクトリ配下の全プラグインのライフサイクル管理、有効化/無効化、順序制御。
  - 各種フック（`on_before_save`, `on_render_page`, `calculate_order` 等）のオーケストレーション。

## モジュール構成

| モジュール | 説明 |
| :--- | :--- |
| `wasm` | Wasmtime エンジン、インスタンスプール、ホストステート、ローダー、キャッシュ |
| `guard` | マニフェスト権限検証ガード (`CapabilityGuard`) |
| `plugin` | プラグインライフサイクル・スーパーバイザー (`PluginSupervisor`, `AegisPlugin`) |
| `services` | ホストが提供する各種サービスインターフェース (`HostServices`, `LocalHostServices`) |
| `config` | プラグインランタイム設定 (`PluginsConfig`, `RuntimeConfig`) |
