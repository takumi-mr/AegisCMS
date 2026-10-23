# サンプルショップ「朝凪珈琲」

AegisCMS の EC 機能（[docs/commerce.md](../../docs/commerce.md)）で作った、コーヒー豆のオンラインショップのサンプルです。商品・注文・店名はすべて架空のものです。

| 内容 | 場所 |
| --- | --- |
| 設定（ショップ・送料とクーポン・テーマ・銀行振込の案内・通知） | [`aegis.toml`](aegis.toml) |
| テーマ「Roastery」（トップページ・記事・固定ページ。ショップのページは組み込みのテンプレートにテーマの色を合わせる） | [`themes/roastery/`](../../themes/roastery) |
| 投入するデータ（カテゴリ 4・商品 9・記事 3・固定ページ 2）と投入スクリプト | [`seed.mjs`](seed.mjs) |
| 商品画像 | [`images/`](images) |
| データベースとアップロードしたファイル（Git の管理外） | `shop.db`・`uploads/`（起動すると作られる） |

## 起動する

リポジトリのルートで実行します。Rust・Node.js 18 以上と [go-task](https://taskfile.dev) が必要です。

```bash
# Admin Studio をビルド（初回のみ）
task build:admin

# 管理者 owner（パスワード sample-password）を作ってサーバーを起動
task demo:shop

# 別のターミナルで商品・記事・ページを投入
task demo:shop:seed
```

<http://127.0.0.1:8080/> がショップ、<http://127.0.0.1:8080/admin/> が管理画面（`owner` / `sample-password`）です。

go-task を使わない場合:

```bash
AEGIS_USER_PASSWORD=sample-password cargo run -p aegis-server -- -c examples/coffee-shop/aegis.toml user add owner --role admin
cargo run -p aegis-server -- -c examples/coffee-shop/aegis.toml run
node examples/coffee-shop/seed.mjs      # 別のターミナルで
```

`seed.mjs` は作成済みの slug を飛ばすので、何度実行しても構いません。別のサーバーやユーザーに投入するときは `--url` / `--user` / `--password`（または環境変数 `AEGIS_SEED_PASSWORD`）を指定します。

最初からやり直すときは、サーバーを止めて `examples/coffee-shop/` の `shop.db*` と `uploads/` を削除します。

## 試せること

| 機能 | 試し方 |
| --- | --- |
| 商品一覧・商品ページ | `/products`。ハウスブレンドは値引き前の価格（打ち消し線）、デカフェは在庫切れ（Sold out）、ドリップバッグは残りわずか（Only 3 left）の表示 |
| カート・見積もり | 商品ページの「Add to cart」→ `/cart`。合計はサーバーが計算する |
| 送料（プラグイン `official-shop-rules`） | 5,000円未満は送料 650円、以上は無料 |
| クーポン（同上） | `WELCOME10`（10% オフ）、`COFFEE300`（300円引き）。それ以外は 422 で「無効なクーポン」 |
| 注文と在庫 | 注文すると在庫が減り、注文確認ページに振込先が出る。72 時間支払いがなければ自動で取り消して在庫を戻す |
| 注文の管理 | 管理画面の **Orders** で支払い済み → 発送 → 完了と進める。顧客の注文確認ページの状態も変わる |
| 通知（[docs/notifications.md](../../docs/notifications.md)） | 注文・入金確認・発送・取り消しのメールを、サンプルではサーバーのログに出す（`aegis::notify`）。発送にするときのメモ（追跡番号など）は発送のメールに入る。SMTP サーバーがあれば `channels = ["email"]` で実際に送れる |
| 商品の管理 | 管理画面の **Products** で価格・在庫・画像を編集 |
| 監査ログ | 管理画面の **Audit Logs** に注文（`order.create` / `order.update`）や商品の変更が残る |

## カード決済（Stripe）にする

`aegis.toml` の `[commerce]` を次のように変えると、Stripe Checkout で支払うようになります（テストモードのキーで試せます）。詳しくは [docs/commerce.md「4.2 stripe」](../../docs/commerce.md)。

```toml
[commerce]
enabled = true
currency = "jpy"
provider = "stripe"

[commerce.stripe]
# AEGIS_STRIPE_SECRET_KEY / AEGIS_STRIPE_WEBHOOK_SECRET でも渡せる
secret_key = "sk_test_..."
webhook_secret = "whsec_..."
```
