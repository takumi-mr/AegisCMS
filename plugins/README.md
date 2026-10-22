# plugins/

AegisCMS にインストールした Wasm プラグインを置くディレクトリです（`[paths].plugins_dir`）。

- 1 プラグイン = 1 ディレクトリ（`plugins/<name>/`）。`<name>` は `plugin.manifest.json` の `name` と同じにします。
- 置いただけでは読み込まれません。設定ファイルの `[plugins].enabled` に名前を追加すると有効になります。
- インストール済みのプラグインと有効・無効は `aegis plugin list` で確認できます。

```text
plugins/
└── my-plugin/
    ├── plugin.manifest.json   # "name": "my-plugin"
    └── plugin.wasm
```

プラグインの作り方は [docs/plugin_sdk_guide.md](../docs/plugin_sdk_guide.md)、テンプレートとサンプルは [`sdk/`](../sdk) にあります。
