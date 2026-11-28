[English](README.md) | **日本語**

# テスト用のテーマ

AegisCMS 本体のテストのためのテーマです。**同梱しません**。**ストアにも載せません**（公式のテーマは [AegisCMS-Plugin](https://github.com/takumi-mr/AegisCMS-Plugin/tree/main/themes) にあります）。

| テーマ       | すること                                                         | 使うテスト                                            |
| ------------ | ---------------------------------------------------------------- | ----------------------------------------------------- |
| `test-fonts` | `theme.json` の `csp` で Google Fonts を宣言する。見た目は最小限 | E2E（`themes.spec.ts`）: 切り替え、外部のホストの表示 |

E2E のサーバー（`admin/e2e/start-server.mjs`）が、このディレクトリをインストール済みのテーマ（`target/e2e/themes`）に写します。
