[← ドキュメント一覧に戻る](README.ja.md)

[English](benchmark.ja.md) | **日本語**

# WordPress との比較ベンチマーク

AegisCMS と WordPress を、同じ条件の Docker 環境で動かして比べた記録と、その再現手順です。サーバー単体の HTTP ベンチマーク（`task bench`、`scripts/benchmark.mjs`）とは別の、**他の CMS との比較**が目的です。

> 数字は、この環境（Windows 11 の Docker Desktop・WSL2）での相対的な目安です。マシン・テーマ・プラグイン・キャッシュの設定で大きく変わるので、自分の環境で `docker/bench/` を動かして確かめてください。

## 1. 結果（2026-11-09、並行 50・10 秒・3 ラウンドの中央値）

| ページ         |  AegisCMS（キャッシュなし） | WordPress（キャッシュなし） | AegisCMS + nginx キャッシュ | WordPress + nginx キャッシュ |
| -------------- | --------------------------: | --------------------------: | --------------------------: | ---------------------------: |
| 一覧（トップ） |  **884 req/s**（p50 65 ms） |    33 req/s（p50 1,400 ms） |  10,657 req/s（p50 2.1 ms） |   11,992 req/s（p50 1.9 ms） |
| 記事 1 件      | **421 req/s**（p50 107 ms） |    35 req/s（p50 1,300 ms） |  11,316 req/s（p50 2.0 ms） |   11,999 req/s（p50 2.0 ms） |

- **キャッシュなし**（キャッシュに載らないリクエスト、初回、パージ直後の速さ）は、AegisCMS が一覧で約 27 倍、記事で約 12 倍。応答時間は 1 秒以上と 0.1 秒前後で、桁が違う。
- **キャッシュあり**（どちらも nginx が応答を返すだけ）は、ほぼ同じ。差は 1 割ほどで、nginx の設定・ページの大きさの差の範囲。CDN を前に置く場合も、ヒット時はこちらに近い。
- つまり、キャッシュを前提にした運用では、CMS の違いは**キャッシュのミスと更新（パージ）のときの負荷**にだけ出る。AegisCMS はそこが軽い。

## 2. 条件

| 項目           | AegisCMS                                                                                                        | WordPress                                                                                                          |
| -------------- | --------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| バージョン     | このリポジトリの main（2026-11-09、#314 の後）                                                                  | WordPress 6.9.4（`wordpress:6-php8.3-fpm`）・PHP 8.3・MariaDB 11.8                                                 |
| 構成           | 1 コンテナ（SQLite）。プラグインなし・既定のテーマ                                                              | php-fpm + nginx + MariaDB。プラグインなし・既定のテーマ（twentytwentyfive）                                        |
| データ         | 記事 2,000 件（約 4 KB。見出し・段落・コード・リストなど約 10 ブロック）                                        | 投稿 2,000 件（約 2.9 KB。見出し・段落・リスト・コード）                                                           |
| ページの大きさ | 一覧 約 38 KB・記事 約 35 KB                                                                                    | 一覧 約 104 KB・記事 約 88 KB（ブロックテーマの CSS が HTML に入る）                                               |
| 資源           | 2 CPU・2 GB                                                                                                     | php-fpm 2 CPU・2 GB（`pm.static`・16 プロセス）、MariaDB 2 CPU・2 GB、nginx 2 CPU                                  |
| 負荷           | `wrk -t4 -c50 -d10s`（同じ Docker ネットワークの中から。3 秒のウォームアップのあと、4 構成を交互に 3 ラウンド） | 同左                                                                                                               |
| キャッシュ     | nginx `proxy_cache`（10 分）                                                                                    | nginx `fastcgi_cache`（10 分。ログイン中・コメント投稿者・クエリ付き・管理画面は通さない、WordPress の定番の設定） |

- WordPress は OPcache が有効（公式イメージの既定）。オブジェクトキャッシュ（Redis など）・ページキャッシュのプラグインは入れていない（ページキャッシュは nginx 側）。
- WordPress のキャッシュなしが遅いのは、1 リクエストごとに PHP が WordPress 本体とテーマを読み込み、DB を引き、ブロックテーマを描画するため（2 CPU で約 60 ms/リクエスト）。プラグインが増えると、さらに遅くなる。
- 記事のサイズは、AegisCMS のほうがやや大きい（4 KB 対 2.9 KB）。

## 3. AegisCMS をキャッシュの後ろに置くときの注意

計測の準備で分かったことです。`docker/bench/` の設定にも反映してあります。

1. **既定では、公開ページは `Cache-Control: no-cache, no-store` で返る**（CDN・キャッシュに載せない安全側の既定）。前段のキャッシュに載せるなら `[cdn].enabled = true` にして、`cache_control` を `public, max-age=0, s-maxage=600` のように決める（[configuration.md](configuration.ja.md)）。会員限定ページは、設定に関わらず `private, no-store`。
2. **一覧ページの応答ヘッダーが大きい**: `Cache-Tag` と `Surrogate-Key` が、記事のタグの分だけ付き、一覧（10 件）で約 1.5 KB ずつ、ヘッダー全体で約 3.9 KB になる。nginx の `proxy_buffer_size` の既定（4 KB）に近く、超えると 502 になる。nginx を前に置くときは `proxy_buffer_size 16k; proxy_buffers 8 16k; proxy_busy_buffers_size 32k;` にする（`docker/bench/nginx-aegis-cache.conf`）。タグのヘッダーを使わない（CDN の API パージをしない）なら、`[cdn].emit_headers = false` で付かなくなる。
3. 更新時に nginx のキャッシュを自動で消す仕組みはない（`Cache-Tag` でパージできるのは Cloudflare・Fastly・webhook）。nginx では TTL に任せるか、手動でパージする。

## 4. 手順

前提: Docker（Compose v2）と Node.js。`docker/bench/` の中で作業します。

```bash
cd docker/bench
docker compose build aegis
docker compose up -d --wait db wp wp-nocache wp-cache aegis aegis-cache

# WordPress: インストールと投稿 2,000 件（内容は約 3 KB の HTML を全件に使う）
docker compose run --rm wpcli wp core install --url=http://wp.test --title=Bench \
  --admin_user=admin --admin_password=pw123456789 --admin_email=a@example.com --skip-email
MSYS_NO_PATHCONV=1 docker compose run --rm wpcli wp rewrite structure '/%postname%/' --hard
# 投稿の生成: wp post generate --count=2000 --post_status=publish --post_author=1 --post_content < 内容.html
#   （--post_content は標準入力から本文を読む。何も渡さないと本文が空になる）

# AegisCMS: セットアップと記事 2,000 件
curl -s -XPOST localhost:18311/api/v1/setup -H 'content-type: application/json' \
  -d '{"admin_username":"admin","admin_password":"AdminPassword123!"}'
AEGIS_BENCH_PASSWORD='AdminPassword123!' node ../../scripts/seed-benchmark.mjs \
  --url http://127.0.0.1:18311 --user admin -n 2000

# 計測（並行数・秒・ラウンド数）。URL は POST_WP / POST_AEGIS で変えられる
bash run.sh 50 10 3 > results.txt

docker compose down -v   # 片付け
```

- Git Bash（Windows）では、`/` で始まる引数がパスとして書き換えられる。`MSYS_NO_PATHCONV=1` を付ける（`run.sh` は付けてある）。
- WordPress は `Host: wp.test` で呼ぶ（サイトの URL を `http://wp.test` にしてあるため。別のホスト名だとリダイレクトされる）。
- ポートは、AegisCMS 直 18311・キャッシュ経由 18312、WordPress（キャッシュなし）18301・（キャッシュあり）18302。確認用で、計測は Docker ネットワークの中から行う。

## 5. この比較の限界

- **ネイティブの数字とは別物**: 同じ AegisCMS でも、Docker（2 CPU・WSL2）では、ホストで直接動かしたときより遅い（記事で 421 req/s。ホストで直接動かした別の計測では 1,800 req/s 前後。CPU の割り当てと、Docker の層が違う）。比較は同じ Docker の条件の中の相対値として読む。
- WordPress は最小構成（プラグインなし・既定のテーマ）。実際のサイトはプラグイン・ページビルダー・WooCommerce などで、さらに重くなるのが普通。逆に、軽量なクラシックテーマなら、ここまで遅くはならない。
- 負荷は 1 ページへの繰り返しで、キャッシュなしの数字は DB やファイルのキャッシュが温まった状態。更新の多い運用や、ページがばらける実アクセスの分布は反映していない。
- 書き込み（投稿・コメント）・管理画面・検索・会員機能は測っていない。
