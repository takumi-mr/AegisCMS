[English](versioning.md) | **日本語**

# バージョニングとリリース

AegisCMS は [Semantic Versioning 2.0.0](https://semver.org/lang/ja/) に従います。バージョンの上げ方とタグ・CHANGELOG は [release-please](https://github.com/googleapis/release-please) が Conventional Commits から自動で行います。

## 1. 何にバージョンを付けるか

| 対象                                                                                                      | バージョン           | 置き場所                                                          | タグ                         |
| --------------------------------------------------------------------------------------------------------- | -------------------- | ----------------------------------------------------------------- | ---------------------------- |
| **コア**: 全 `aegis-*` crate・Rust SDK（`aegis-plugin-sdk`）・Admin Studio・Theme Builder・Plugin Builder | 一括（同じ番号）     | ルートの `Cargo.toml` の `[workspace.package]`、各 `package.json` | `v0.2.0`                     |
| **AssemblyScript SDK** `@takumi-mr/plugin-sdk`                                                            | 個別                 | `sdk/assemblyscript/package.json`                                 | `sdk-assemblyscript-v0.2.0`  |
| **TinyGo SDK**                                                                                            | 個別                 | タグそのもの（Go モジュールの規約）                               | `sdk/tinygo/v0.2.0`          |
| **公式プラグイン** `plugins/official-*`                                                                   | プラグインごとに個別 | `Cargo.toml` と `plugin.manifest.json` の `version`               | `plugin-official-ads-v0.1.1` |

- コアの crate は `version.workspace = true` で 1 つの番号を共有します。個別に番号を持たせないでください。
- `sdk/templates/` と `sdk/examples/` はコピー元のひな形で、単独ではリリースしません（バージョンは `0.1.0` のままで構いません）。ただし Rust テンプレートの `aegis-plugin-sdk` へのバージョン要求はコアに追従させます。
- 1.0 になるまでは、機能追加（`feat`）も破壊的変更もマイナー（`0.x`）を上げ、修正（`fix`・`perf`）はパッチを上げます。

## 2. プラグインの互換性

プラグインと本体の互換性は、2 つの項目で宣言します（`plugin.manifest.json`）。

- **`min_aegis_version`**: 動かせる本体の最小バージョン（SemVer）。本体がこれより古いと読み込まない。
- **`abi_version`**: プラグインが前提にする **プラグイン ABI のバージョン**（整数）。ホスト API・フックの入出力に互換性のない変更を入れたときだけ、`aegis_core::PLUGIN_ABI_VERSION` を上げます。ホストが対応する範囲（1 以上、`PLUGIN_ABI_VERSION` 以下）を超えると読み込まない。省略したプラグインは検査しない。

ABI の中身は [plugin_abi_specification.md](plugin_abi_specification.ja.md) にあります。**ABI 2**: `on-render-page` のスクリプトを、HTML の文字列ではなく構造化した `scripts` でだけ足せるようにした（`extra_head`・`prepend_content`・`append_content` の中の `<script>` と `on*` 属性は、ホストが取り除く）。`scripts` を使うプラグインは `abi_version: 2` と書く。ABI 1 のプラグインも読み込めるが、`<script>` は動かなくなる。経緯は [csp.md](csp.ja.md)。

互換性を壊さない追加（新しいフック・ホスト API の追加）では ABI のバージョンを上げません。その追加を使うプラグインは `min_aegis_version` を上げて宣言します。

サーバーは、管理者のセッションの `GET /api/v1/health` に `version`（本体）と `plugin_abi_version` を返します（公開側には返しません）。Admin Studio のヘッダーのゲートウェイ表示に出るツールチップで本体のバージョンを確認できます。

## 3. コミットメッセージとバージョンの上がり方

[CLAUDE.md の 4](../CLAUDE.md) の Conventional Commits がそのままバージョンに反映されます。

| コミット                             | 上がるバージョン                     |
| ------------------------------------ | ------------------------------------ |
| `fix:` / `perf:`                     | パッチ                               |
| `feat:`                              | マイナー（1.0 前も同じ）             |
| `feat!:` や本文の `BREAKING CHANGE:` | 1.0 前はマイナー、1.0 以降はメジャー |
| `docs` / `style` / `test` / `chore`  | 上がらない（CHANGELOG にも出ない）   |

> [!WARNING]
> **コミットメッセージ・PR のタイトルと本文に、生の HTML（特に `<script>` と `<style>`）を書かないでください。** リリース PR の本文はコミットメッセージから作られ、release-please はそれを HTML として読みます。`<script>` が 2 つ以上あるなどで読み取りが崩れると、リリース PR をマージしても**タグも GitHub Release も作られず**、`autorelease: pending` のまま残ります。以降のリリース PR も「untagged, merged release PRs outstanding」で作られなくなります（ワークフローは success と表示されるので気づきにくい）。バッククォートで囲んでも防げません。「script 要素」のように言葉で書いてください。`破壊的変更`の説明（`BREAKING CHANGE:`）で特に起きやすいです。PR では `scripts/check-release-notes-html.mjs`（`commit-lint.yml`）が検査します。
>
> すでにリリース PR をマージして止まった場合は、そのリリース PR の本文の `<` を `&amp;lt;` に直して（`<script` → `&amp;lt;script`）、main 上の Release Please のワークフローを再実行します（`&lt;` では、release-please が本文を読み直すときに元に戻ってしまい直りません）。

コミットがどの対象に数えられるかは、**変更したファイルのパス**で決まります（スコープではありません）。`plugins/official-ads/` の変更はそのプラグインだけを、`sdk/assemblyscript/` の変更はその SDK だけを、それ以外（`plugins/`・`sdk/` の外）はコアを上げます。

## 4. リリースの流れ

1. `main` に変更をマージすると、`Release Please` ワークフローがリリース PR（バージョン更新 + 各 `CHANGELOG.md`）を作る、または更新する。
2. リリース PR のバージョンと CHANGELOG を確かめてマージする。`Cargo.lock` はワークフローが同じ PR に追従させる。
3. マージするとタグと GitHub Release が作られる。さらに `release-please.yml` の `paths_released` 出力に基づき、リリースされた対象パスの公開ワークフローが自動発火する：
    - **AssemblyScript SDK** (`sdk/assemblyscript`): GitHub Packages (`@takumi-mr/plugin-sdk`) へ npm パッケージが自動公開される (`release-sdk.yml`)。
    - **コア** (`.`): GitHub Container Registry (`ghcr.io/takumi-mr/aegiscms`) へ公式 Docker イメージが自動でビルド＆プッシュされる (`release-docker.yml`)。
    - **コア** (`.`): GitHub Release に Linux (x86_64) と Windows (x86_64) の実行バイナリ（Admin Studio 埋め込み、`config`・`schemas`・`plugins`・`themes` 同梱）と SHA-256 が添付される (`release-binaries.yml`)。既存タグへの再実行は `workflow_dispatch` で `tag` を渡す。
    - **TinyGo SDK** (`sdk/tinygo`): 作成されたタグ (`sdk/tinygo/vX.Y.Z`) 自体が Go モジュールとしての配布となる。

リリース PR にも CI を走らせるため（`GITHUB_TOKEN` で作った PR では CI が動かない）、リポジトリの secret に `RELEASE_PLEASE_TOKEN`（PAT）を登録しています。Fine-grained PAT なら、対象リポジトリに **Contents** と **Pull requests** の Read and write が要ります（足りないと PR のブランチ作成で失敗する）。未登録でも `GITHUB_TOKEN` で動きますが、リリース PR の CI は動きません。

初めてリリース PR を作るときは、それまでに変更したパスすべてが対象になります。新しく仕組みを入れた直後は、全プラグインと TinyGo SDK が同時に上がるのはこのためです（次回からは変更したパスだけ）。

## 5. 整合性の検査

`task version:check`（`node scripts/check-versions.mjs`）が、次を確かめます。`task check` と CI でも実行します。

- `release-please-config.json` と `.release-please-manifest.json` と、ディスク上の `plugins/official-*` が一致している（プラグインを足したら両方に登録する）
- ルートの `Cargo.toml`、各 crate の `version.workspace = true`、3 つのフロントエンドの `package.json` / `package-lock.json`、SDK のバージョン要求が、コアのバージョンと一致している
- 各公式プラグインの `Cargo.toml` と `plugin.manifest.json` の `version` が一致している
- すべての `plugin.manifest.json` に `abi_version` があり、ホストが扱える範囲に収まっている。`min_aegis_version` がコアより新しくない

バージョンを手で上げることは基本的にありません（リリース PR に任せる）。手で直すときも、このコマンドで抜けを確かめてください。

## 6. テーブル定義の更新方針

システムテーブル（`_aegis_*`）の定義は、sqlx の migrate（番号つきの SQL ファイル）で作り、変えます。**手順は前向きに足していくだけ**で、メジャーバージョンでも整理し直しません。どのバージョンの DB からでも、新しい版を起動すれば、まだ適用していない手順が順に流れます。

- **置き場所**: [`crates/aegis-db/migrations/`](../crates/aegis-db/migrations) の `sqlite/`・`postgres/`・`mysql/`。方言ごとに SQL を手で書く（型や索引の書き方が違うため）。ファイル名は `NNNN_説明.sql`（4 桁の連番）で、**3 つの方言で同じ番号・同じ名前**にそろえる（そろっていないとテストが落ちる）。
- **`0001_baseline`**: v0.4 のシステムテーブルの定義。すべて `IF NOT EXISTS` なので、v0.4 が作った DB は何も変えずに「適用済み」として採用される（バックアップ → 復元は要らない）。
- **記録と検査**: 適用済みの手順は DB の `_sqlx_migrations` に、チェックサムつきで残る。次の 2 つは起動を止める。
    - **公開した手順のファイルを書き換えた**（チェックサムが違う）。直したいときは、新しい手順を足す。
    - **DB が、今のバイナリの知らない手順まで適用済み**（新しい版で更新した DB を古い版で開いた）。古い版では動かさない。戻すときは、更新前に取ったバックアップから復元する。
- **起動時に自動で流す**（`init`・`run`・`task migrate`）。同時に複数台が起動しても、PostgreSQL・MySQL は sqlx が排他する。
- **巻き戻し（down）は作らない**。戻すときは、更新前のバックアップ（`task dump`）から復元する。

### 6.1 手順の書き方

- **列・表・索引の追加を基本にする**。NULL 可か、デフォルト値つきの列を足す。複数台を順番に更新するとき、更新前の台が新しい列に当たっても動くようにするため。
- **名前や型の変更、列の削除は、段階に分ける**。新しい列を足してコピーし、アプリが新しい列だけを使うようになった版の**次の手順**で古い列を消す（SQLite の `DROP COLUMN` は 3.35 以降）。
- **1 つのファイルに 1 つの変更**を書き、何度流しても同じ結果になる書き方（`IF NOT EXISTS` など）にする。**MySQL は DDL を 1 文ずつコミットして戻せない**。途中で失敗した手順は、sqlx が「失敗」と記録して起動を止める。原因を直し、`_sqlx_migrations` のその行を消してから、起動し直す。
- 手順の中でデータを書き換えるとき（`UPDATE`）は、3 つの DB で同じ結果になるようにする。

### 6.2 YAML のスキーマで宣言するコンテンツの表

`content_*`（YAML のスキーマで宣言する表）は、この仕組みの対象ではありません。起動時に、既存の表に無い列だけを `ALTER TABLE ... ADD COLUMN` で足す、これまでの仕組みのままです（[architecture.md の「スキーマ進化」](architecture.ja.md)）。

### 6.3 バックアップ・復元との関係

手順を前向きに足すので、バージョンをまたぐ更新に、バックアップ → 復元は要りません。バックアップは、更新前の保険（戻すとき）と、別の DB・サーバーへの引っ越しに使います。復元は、今のスキーマに合わないカスタムフィールドを、警告を出して落とします（[architecture.md の 17](architecture.ja.md#17-バックアップとリストア-aegis-backup)）。

### 6.4 v0.4 より前の DB

0.3.x までの DB は、`0001_baseline` が「作成済み」として何もしないので、足りない列を足せません。`init` が、v0.4 で足した列（`REQUIRED_COLUMNS`）の有無を確かめ、無ければ、0.3.x でバックアップを取って新しい版に復元する手順を示して起動を止めます。
