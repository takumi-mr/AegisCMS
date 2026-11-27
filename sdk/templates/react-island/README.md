# React Island Block Plugin Template

Admin Studio の記事エディタから差し込める **React アイランド方式のカスタムブロックプラグイン** テンプレートです。

## 構成

- `plugin.manifest.json`: Admin Studio のエディタ用ブロック定義と CSP capability 宣言。同梱した `assets/island.js` を読み込むため `capabilities.local_scripts: true` を宣言しています（無いとホストがスクリプトを捨てます）
- `src/lib.rs`: `render-blocks`（マウント先 `<div data-aegis-island="react-counter" data-props="...">` を出力）および `on-render-page`（該当ブロックがある記事だけ `/plugins/<name>/assets/island.{js,css}` を注入）
- `frontend/`: React + Vite プロジェクト（`npm run build` で `../assets/island.js` と `../assets/island.css` を生成）

## ビルドと導入

```bash
# 1. フロントエンド (React) をビルドして assets/ に出力
cd frontend && npm install && npm run build && cd ..

# 2. Wasm プラグインをビルド
cargo build --target wasm32-unknown-unknown --release
cp target/wasm32-unknown-unknown/release/aegis_plugin_react_island_template.wasm plugin.wasm
```

`plugin.manifest.json`、`plugin.wasm`、`assets/` を `plugins/<プラグイン名>/` に配置して有効化すると、Admin Studio のエディタに「React カウンター」ブロックが追加されます。

## 外部 API と通信する場合

ブラウザ側の React コンポーネントから外部 API に `fetch` したい場合は、`plugin.manifest.json` の `capabilities.csp.connect_domains` に通信先ホストを宣言してください（未宣言の外部ドメインは Strict CSP により遮断されます）。

```json
"capabilities": {
  "csp": {
    "connect_domains": ["api.example.com"]
  }
}
```
