# バージョニングとリリース

AegisCMS は [Semantic Versioning 2.0.0](https://semver.org/lang/ja/) に従います。バージョンの上げ方とタグ・CHANGELOG は [release-please](https://github.com/googleapis/release-please) が Conventional Commits から自動で行います。

## 1. 何にバージョンを付けるか

| 対象 | バージョン | 置き場所 | タグ |
|---|---|---|---|
| **コア**: 全 `aegis-*` crate・Rust SDK（`aegis-plugin-sdk`）・Admin Studio・Theme Builder・Plugin Builder | 一括（同じ番号） | ルートの `Cargo.toml` の `[workspace.package]`、各 `package.json` | `v0.2.0` |
| **AssemblyScript SDK** `@takumi-mr/plugin-sdk` | 個別 | `sdk/assemblyscript/package.json` | `sdk-assemblyscript-v0.2.0` |
| **TinyGo SDK** | 個別 | タグそのもの（Go モジュールの規約） | `sdk/tinygo/v0.2.0` |
| **公式プラグイン** `plugins/official-*` | プラグインごとに個別 | `Cargo.toml` と `plugin.manifest.json` の `version` | `plugin-official-ads-v0.1.1` |

- コアの crate は `version.workspace = true` で 1 つの番号を共有します。個別に番号を持たせないでください。
- `sdk/templates/` と `sdk/examples/` はコピー元のひな形で、単独ではリリースしません（バージョンは `0.1.0` のままで構いません）。ただし Rust テンプレートの `aegis-plugin-sdk` へのバージョン要求はコアに追従させます。
- 1.0 になるまでは、機能追加（`feat`）も破壊的変更もマイナー（`0.x`）を上げ、修正（`fix`・`perf`）はパッチを上げます。

## 2. プラグインの互換性

プラグインと本体の互換性は、2 つの項目で宣言します（`plugin.manifest.json`）。

- **`min_aegis_version`**: 動かせる本体の最小バージョン（SemVer）。本体がこれより古いと読み込まない。
- **`abi_version`**: プラグインが前提にする **プラグイン ABI のバージョン**（整数）。ホスト API・フックの入出力に互換性のない変更を入れたときだけ、`aegis_core::PLUGIN_ABI_VERSION` を上げます。ホストが対応する範囲（1 以上、`PLUGIN_ABI_VERSION` 以下）を超えると読み込まない。省略したプラグインは検査しない。

ABI の中身は [plugin_abi_specification.md](plugin_abi_specification.md) にあります。互換性を壊さない追加（新しいフック・ホスト API の追加）では ABI のバージョンを上げません。その追加を使うプラグインは `min_aegis_version` を上げて宣言します。

サーバーは、管理者のセッションの `GET /api/v1/health` に `version`（本体）と `plugin_abi_version` を返します（公開側には返しません）。Admin Studio のヘッダーのゲートウェイ表示に出るツールチップで本体のバージョンを確認できます。

## 3. コミットメッセージとバージョンの上がり方

[CLAUDE.md の 4](../CLAUDE.md) の Conventional Commits がそのままバージョンに反映されます。

| コミット | 上がるバージョン |
|---|---|
| `fix:` / `perf:` | パッチ |
| `feat:` | マイナー（1.0 前も同じ） |
| `feat!:` や本文の `BREAKING CHANGE:` | 1.0 前はマイナー、1.0 以降はメジャー |
| `docs` / `style` / `test` / `chore` | 上がらない（CHANGELOG にも出ない） |

コミットがどの対象に数えられるかは、**変更したファイルのパス**で決まります（スコープではありません）。`plugins/official-ads/` の変更はそのプラグインだけを、`sdk/assemblyscript/` の変更はその SDK だけを、それ以外（`plugins/`・`sdk/` の外）はコアを上げます。

## 4. リリースの流れ

1. `main` に変更をマージすると、`Release Please` ワークフローがリリース PR（バージョン更新 + 各 `CHANGELOG.md`）を作る、または更新する。
2. リリース PR のバージョンと CHANGELOG を確かめてマージする。`Cargo.lock` はワークフローが同じ PR に追従させる。
3. マージするとタグと GitHub Release が作られる。AssemblyScript SDK は GitHub Packages に自動で公開され、TinyGo SDK はタグ（`sdk/tinygo/vX.Y.Z`）が配布になる。

リリース PR にも CI を走らせるため（`GITHUB_TOKEN` で作った PR では CI が動かない）、リポジトリの secret に `RELEASE_PLEASE_TOKEN`（PAT）を登録しています。Fine-grained PAT なら、対象リポジトリに **Contents** と **Pull requests** の Read and write が要ります（足りないと PR のブランチ作成で失敗する）。未登録でも `GITHUB_TOKEN` で動きますが、リリース PR の CI は動きません。

初めてリリース PR を作るときは、それまでに変更したパスすべてが対象になります。新しく仕組みを入れた直後は、全プラグインと TinyGo SDK が同時に上がるのはこのためです（次回からは変更したパスだけ）。

## 5. 整合性の検査

`task version:check`（`node scripts/check-versions.mjs`）が、次を確かめます。`task check` と CI でも実行します。

- `release-please-config.json` と `.release-please-manifest.json` と、ディスク上の `plugins/official-*` が一致している（プラグインを足したら両方に登録する）
- ルートの `Cargo.toml`、各 crate の `version.workspace = true`、3 つのフロントエンドの `package.json` / `package-lock.json`、SDK のバージョン要求が、コアのバージョンと一致している
- 各公式プラグインの `Cargo.toml` と `plugin.manifest.json` の `version` が一致している
- すべての `plugin.manifest.json` に `abi_version` があり、ホストが扱える範囲に収まっている。`min_aegis_version` がコアより新しくない

バージョンを手で上げることは基本的にありません（リリース PR に任せる）。手で直すときも、このコマンドで抜けを確かめてください。
