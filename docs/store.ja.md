[← ドキュメント一覧に戻る](README.ja.md)

[English](store.md) | **日本語**

# パッケージストア（設計・未実装）

> [!NOTE]
> この文書は設計メモです。実装済みはカタログの取得・検証、一覧の API（`GET /api/v1/store/packages`）、ストアからのインストール（`POST /api/v1/store/install`）まで（設定と API は [configuration.md の 2.28](configuration.ja.md#228-store--パッケージストア)）。Admin Studio の画面（テーマ・プラグインの画面の**ストア**のタブ。[admin-settings.md](admin-settings.ja.md#ストアテーマプラグインの画面)）まで。未決事項が決まったら仕様の文書に書き直します。

Admin Studio から、配布されているプラグインとテーマを選んでインストール・更新できるようにします。配る側は**静的なファイルだけ**（署名したカタログと ZIP）で、専用の HTTP サーバーは作りません。CMS 側は、カタログを読み、ZIP を取得して、既存の ZIP インストール（[プラグインのインストールと署名](plugin_hooks.ja.md#74-zip-パッケージからのインストールと署名)）に渡します。

## 1. 範囲

- **第 1 段階（当面）**: 配るのは自分（公式）のプラグインとテーマだけ。審査・作者の登録・有料販売はしない。
- **第 2 段階（必要になったら）**: 第三者のプラグインを受け入れる（[6](#6-将来-第三者のパッケージを受け入れる)）。このとき初めて動的なサーバーを考える。

第 2 段階で作り直しにならないよう、カタログの形式と CMS 側の設定は第 1 段階から次を満たしておきます。

- カタログに形式のバージョン（`schema_version`）を持たせる
- 各エントリに作者（`publisher`）の欄を持たせる（当面は自分だけ）
- CMS は GitHub を前提にしない（読むのはカタログの URL と、カタログに書かれた ZIP の URL だけ）
- カタログの URL を複数持てる設定にする

## 2. 構成

本体のリポジトリとは別のリポジトリ（例: `aegis-store`）で配ります。本体の Releases は release-please がタグと CHANGELOG を管理しているので、プラグインのタグを混ぜません。

```text
 aegis-store リポジトリ
   ├─ GitHub Pages:    https://<owner>.github.io/aegis-store/catalog.json
   │                   https://<owner>.github.io/aegis-store/catalog.json.sig
   └─ GitHub Releases: https://github.com/<owner>/aegis-store/releases/download/<tag>/<file>.zip
                         （ダウンロードは release-assets.githubusercontent.com へ 302 で転送される）

 AegisCMS（各サイト）
   catalog.json を取得 → 署名・巻き戻しを検証 → 一覧を表示
   → ZIP を取得 → sha256 を照合 → 既存の ZIP インストール（展開の制限・min_aegis_version・signature.json の検証）
```

- **カタログは Pages に置く**: 更新し続けるファイルなので、Releases のアセット（差し替えが手間）や `raw.githubusercontent.com`（数分のキャッシュ）には置かない。
- **GitHub API は使わない**: 認証なしの API は 1 時間に 60 回までで、サイトが多いと当たる。一覧はカタログから読み、ZIP はカタログに書いた直接の URL から取る。
- **タグはパッケージごとに名前を分ける**: `<id>-v<version>`（例: `seo-tools-v1.2.0`、`theme-minimal-v0.3.1`）。
- **出したバージョンのアセットは差し替えない**: 直すときは新しいバージョンを出す（カタログの sha256 と食い違わないように）。

## 3. カタログの形式

```json
{
  "schema_version": 1,
  "sequence": 42,
  "generated_at": "2026-11-28T00:00:00Z",
  "expires_at": "2027-02-28T00:00:00Z",
  "packages": [
    {
      "id": "seo-tools",
      "kind": "plugin",
      "name": "SEO Tools",
      "description": "…",
      "publisher": "official",
      "versions": [
        {
          "version": "1.2.0",
          "min_aegis_version": "0.9.0",
          "capabilities": { "local_scripts": false },
          "url": "https://github.com/<owner>/aegis-store/releases/download/seo-tools-v1.2.0/seo-tools-1.2.0.zip",
          "sha256": "<64 文字の hex>",
          "size": 123456,
          "public_key": "<ZIP の signature.json の Ed25519 公開鍵、32 バイトの hex>"
        }
      ]
    }
  ]
}
```

- `catalog.json.sig` は、`catalog.json` のバイト列そのものに対する Ed25519 署名（64 バイトの hex）。カタログの鍵と ZIP の鍵は分ける（[5](#5-鍵の管理)）。
- `sequence` は公開のたびに 1 以上増やす。CMS は最後に受け入れた値を覚え、それより小さいカタログを拒否する（古い・脆弱なバージョンを「最新」と偽る巻き戻しへの対策）。
- `expires_at` を過ぎたカタログは拒否する（配信元を止めて古い一覧を見せ続ける攻撃への対策）。CI で定期的に作り直して延ばす。
- `capabilities` はマニフェストからの写しで、インストール前の確認画面に出すためのもの。インストール時に ZIP の中のマニフェストと食い違えば拒否する。
- 知らない `schema_version` のカタログは読まない。項目の追加は同じ `schema_version` のまま行い、CMS は知らない項目を無視する。

## 4. CMS 側の処理

### 4.1 設定

```toml
[store]
enabled = true

[[store.catalogs]]
name = "official"
url = "https://<owner>.github.io/aegis-store/catalog.json"
public_key = "<カタログ署名の Ed25519 公開鍵、32 バイトの hex>"
```

- カタログは配列で、各カタログの鍵は、そのカタログにだけ使う。
- `[[store.catalogs]]` を書かなければ、公式のカタログ（AegisCMS-Store。URL と公開鍵は `store.rs` の `OFFICIAL_CATALOG_*`）を読む。書くと置き換わる。`enabled` の既定は `false`（黙って外部に通信しない）。
- 公式のカタログの公開鍵はサーバーに組み込まれているので、鍵を替えると、新しい鍵を持つ版に更新したサーバーだけが読める。カタログの秘密鍵は GitHub の secret とは別に控えておく。
- 受け入れた `sequence` は DB（`_aegis_settings` の `store.catalog_sequences`）に記録する。複数のサーバーで共有され、記録は下げない。バックアップから復元すると記録も復元時点の値に戻るが、検査が緩むだけで、次に受け入れたカタログから元に戻る。
- ZIP の `signature.json` の鍵は、カタログのエントリの `public_key` と一致することを確かめる。`[plugins]` / `[theme]` の `require_signatures`・`trusted_public_keys` はそのまま効く（ストア経由でも緩めない）。

### 4.2 流れ

1. カタログと `.sig` を取得し、署名・`schema_version`・`sequence`・`expires_at` を確かめる。
2. Admin Studio に一覧を出す。インストール済みのものは、バージョンを比べて更新を知らせる。互換性のないもの（`min_aegis_version`）は選べないようにする。
3. インストール時に `capabilities` を見せ、確認をはさむ。
4. ZIP を取得し、`size` と `sha256` を照合する。
5. 既存の処理（`extract_zip_safely` → `check_min_aegis_version` → `verify_package_signature`）に渡してインストールする。有効化はしない（既存と同じ）。監査ログには取得元のカタログと URL を残す。

### 4.3 外向きの通信の制限

- `https://` だけ。リダイレクトは回数を絞り、転送先も `https://` だけにする。中身の正しさは sha256 と署名で確かめるので、届いたホストは信頼の根拠にしない。
- 応答の大きさの上限（カタログ・ZIP それぞれ）とタイムアウト。
- URL は admin が設定ファイルに書くものなので SSRF の余地は小さいが、上の制限は入れる。

## 5. 鍵の管理

- 秘密鍵はリポジトリに置かない。CI で署名するなら Actions の secret に入れる。
- **カタログの鍵**と **ZIP の鍵**は分ける。ZIP の鍵だけが漏れても、カタログに無い ZIP は配れない。カタログの鍵だけが漏れても、ZIP の署名は偽れない。
- 鍵を替えるときは、CMS の設定（`store.catalogs[].public_key`）を新旧の両方を受け入れられる形にしてから移る（未決。[8](#8-未決事項)）。

公開の流れ（CI の例）: タグを push → ビルド → ZIP に署名 → Release を作る → `catalog.json` を作り直して署名 → Pages に出す。手で行うのはタグの push だけにする。ダウンロード数は Releases のアセットの数で見られる。

## 6. 将来: 第三者のパッケージを受け入れる

作者に提出してもらい、ストア側で確かめてから署名して公開する流れになります。ただし**提出してもらうのは ZIP（ビルド済みの成果物）ではなくソース**にします。

**オープンソースは強制しません**。ソースを渡すのは審査とビルドのためで、公開するためではありません。

- ソースは非公開のまま渡せる（作者の非公開リポジトリをストアの CI だけが読めるようにする、非公開の提出口からアーカイブを送る、など）。ストアはソースを再配布しない。
- 利用者に配るのはビルドした ZIP（Wasm・テンプレートなど）だけで、ソースは含まない。
- ライセンスは作者が決める（プロプライエタリ・有料でもよい）。カタログのエントリに `license`（SPDX の識別子か `proprietary`）の欄を足し、一覧に出す。

ソースを提出する流れの詳細は次のとおりです。

- **ソースを受け取り、ストアの CI でビルドする**: Wasm はバイナリなので、ZIP を受け取ってもソースと一致するかを確かめられない。ソース（リポジトリとコミット）を受け取り、固定したツールチェーンでストアがビルドすれば、審査したソースと配るバイナリが一致する。審査はソースの差分を読めばよくなる。
- **署名は二重にする**: 作者の鍵（「作者が出した」）とストアの鍵（「ストアが審査した」）。カタログのエントリには作者の公開鍵を載せる（第 1 段階からある `publisher`・`public_key` の欄）。
- **審査で見るもの**: `capabilities`（とくに `local_scripts`・外部ホスト）、マニフェストの変更、前のバージョンからの差分。
- **取り下げ**: 脆弱なバージョンをカタログから外すか、`yanked` の印を付ける。CMS はインストール済みのものに警告を出す。
- このとき初めて、作者の登録・提出の受付・審査の状態を持つ動的なサーバー（または GitHub の Pull Request を提出口にする運用）が要る。有料販売もこの段階で考える。

### 6.1 審査済みとバイナリのみの 2 区分（案）

ソースを審査に出すこと自体を嫌がる作者のために、ZIP だけで出せる区分も用意する案です。

| 区分         | 提出物                 | 署名                  | 許可する権限                                                                    |
| ------------ | ---------------------- | --------------------- | ------------------------------------------------------------------------------- |
| 審査済み     | ソース（非公開でよい） | 作者の鍵 + ストアの鍵 | 制限なし（`local_scripts`・外部ホストなども、審査で認めれば許可）               |
| バイナリのみ | ZIP                    | 作者の鍵だけ          | 弱い権限だけ（`local_scripts`・外部ホストなど強い権限を宣言したものは載せない） |

- **ストアの鍵で署名するのは審査済みだけ**。中身を確かめていないものにストアの署名を付けない。バイナリのみのものは、ストアは「作者の鍵で署名されていること」と「宣言した権限が弱いこと」だけを確かめて載せる。
- **バイナリのみでも配れる根拠は Wasm のサンドボックス**。宣言していない権限は実行時に止まるので、権限を絞れば、ソースを見なくても被害の範囲を限られる。逆に、強い権限を持つものはソースを見ない限り載せない。
- カタログのエントリに区分（`review`: `"reviewed"` / `"unreviewed"`）を持たせ、Admin Studio の一覧とインストール前の確認画面に出す。運用者がバイナリのみの区分を設定で受け入れないようにもできる（例: `[store] allow_unreviewed = false`）。

## 7. 公式パッケージの別リポジトリへの移行（計画）

本体のテストに使っていない公式プラグイン・テーマと、外部のサービスに依存するものは、本体から別のリポジトリ（例: `AegisCMS-Plugins`）へ移し、ストアから配る。AegisCMS-Store の `store.json` は取得元（`sources`）を複数持てるので、移したものはそのリポジトリのタグから配る。

| 区分             | 対象                                                                                                                                                                                                                                              | 理由                                                                                                                                                                                               |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 本体に残す       | `official-blocks`・`official-code-enhancer`・`official-page-toc`・`official-post-navigation`・`official-shop-rules`・`official-order-notifier`、テーマ `default`                                                                                  | 本体のコード・既定値・テスト（Admin Studio の E2E、EC・通知、プラグイン管理）が名前で使う。`official-post-navigation` は本体（`handlers/public.rs`）が設定を読む。`default` はテーマエンジンの前提 |
| 先に移す         | `official-ads`・`official-clarity`・`official-embeds`・`official-link-embed`・`official-charts`・`official-share-buttons`・`official-reading-time`・`official-back-to-top`・`official-code-lines`・`official-lightbox`・`official-external-links` | 本体で使うのは各プラグインの動作のテスト（`aegis-runtime/tests/sdk_plugins.rs`）だけで、それはプラグインと一緒に移せる                                                                             |
| 準備してから移す | `official-mermaid`・`official-plantuml`、テーマ `default` 以外                                                                                                                                                                                    | mermaid・plantuml は `config/default.toml` で既定で有効（無いとサーバーが起動しない）。テーマの翻訳文は本体の `aegis-theme/src/messages.rs` にあり、テーマ側に持たせる仕組みが要る                 |

順番:

1. ストアが複数の取得元から配れるようにする（済。AegisCMS-Store の `store.json` の `sources`）。
2. ストアからのインストール API を作る（`POST /api/v1/store/install`）。同梱をやめても、利用者がストアから入れ直せるようにするため。
3. 新しいリポジトリに「先に移す」ものを、各プラグインのテストと一緒に移す（済。[AegisCMS-Plugin](https://github.com/takumi-mr/AegisCMS-Plugin) に本体 v0.12.0 から移し、ストアはそこから配っている）。テストは本体の `aegis-runtime` をタグで参照して動かす。ストアの `store.json` で取得元を切り替える（バージョンを上げてから出す）。
4. 本体から消す（v0.13.0 で済）（ディレクトリ・workspace のメンバー・Taskfile・release-please の設定・`sdk_plugins.rs` の該当テスト・ドキュメント）。同梱をやめるのは利用者にとって破壊的変更なので `feat!:` で出し、移行の手順（ストアから入れ直す）を書く。
5. mermaid・plantuml を既定で有効にするのをやめてから移す。テーマは翻訳をテーマ側に持てるようにしてから移す。

## 8. 未決事項

- 更新の確認の頻度と、取得したカタログのキャッシュの置き場。今はストアのタブを開くたびにカタログを取得し、更新はそのタブにだけ出す（「インストール済み」のタブなどにはまだ出さない）。
- 鍵の切り替えの手順（カタログに次の鍵を予告する欄を持たせるか）。
