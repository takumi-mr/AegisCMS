[← Back to the documentation index](README.md)

**English** | [日本語](benchmark.ja.md)

# Benchmark Comparison with WordPress

A record of running AegisCMS and WordPress in Docker environments with the same conditions and comparing them, and the steps to reproduce it. Its purpose is a **comparison with another CMS**, separate from the server's standalone HTTP benchmark (`task bench`, `scripts/benchmark.mjs`).

> The numbers are a relative guide in this environment (Docker Desktop on Windows 11, WSL2). They vary greatly with the machine, theme, plugins, and cache settings, so run `docker/bench/` in your own environment to check.

## 1. Results (2026-11-09, median of 3 rounds at 50 concurrent for 10 seconds)

| Page           |        AegisCMS (no cache) |    WordPress (no cache) |    AegisCMS + nginx cache |   WordPress + nginx cache |
| -------------- | -------------------------: | ----------------------: | ------------------------: | ------------------------: |
| List (top)     |  **884 req/s** (p50 65 ms) | 33 req/s (p50 1,400 ms) | 10,657 req/s (p50 2.1 ms) | 11,992 req/s (p50 1.9 ms) |
| Single article | **421 req/s** (p50 107 ms) | 35 req/s (p50 1,300 ms) | 11,316 req/s (p50 2.0 ms) | 11,999 req/s (p50 2.0 ms) |

- **Without cache** (the speed for requests that miss the cache, the first request, and right after a purge): AegisCMS is about 27 times faster on the list and about 12 times on the article. Response times are over 1 second versus around 0.1 seconds, an order of magnitude apart.
- **With cache** (nginx just returns the response in both cases): about the same. The difference is around 10%, within the range of differences in nginx settings and page size. When putting a CDN in front, a hit is close to this too.
- In other words, when operating on the premise of caching, the difference between the CMSs shows up only in **the load at cache misses and updates (purges)**. AegisCMS is light there.

## 2. Conditions

| Item      | AegisCMS                                                                                                                  | WordPress                                                                                                                                                        |
| --------- | ------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Version   | This repository's main (2026-11-09, after #314)                                                                           | WordPress 6.9.4 (`wordpress:6-php8.3-fpm`), PHP 8.3, MariaDB 11.8                                                                                                |
| Setup     | 1 container (SQLite). No plugins, default theme                                                                           | php-fpm + nginx + MariaDB. No plugins, default theme (twentytwentyfive)                                                                                          |
| Data      | 2,000 articles (about 4 KB; about 10 blocks such as headings, paragraphs, code, lists)                                    | 2,000 posts (about 2.9 KB; headings, paragraphs, lists, code)                                                                                                    |
| Page size | List about 38 KB, article about 35 KB                                                                                     | List about 104 KB, article about 88 KB (the block theme's CSS is in the HTML)                                                                                    |
| Resources | 2 CPUs, 2 GB                                                                                                              | php-fpm 2 CPUs, 2 GB (`pm.static`, 16 processes), MariaDB 2 CPUs, 2 GB, nginx 2 CPUs                                                                             |
| Load      | `wrk -t4 -c50 -d10s` (from inside the same Docker network; after a 3-second warm-up, the 4 setups alternate for 3 rounds) | Same as left                                                                                                                                                     |
| Cache     | nginx `proxy_cache` (10 minutes)                                                                                          | nginx `fastcgi_cache` (10 minutes; logged-in users, commenters, requests with a query, and the admin screen are not passed through — WordPress's standard setup) |

- WordPress has OPcache enabled (the official image's default). No object cache (Redis, etc.) or page cache plugin is installed (the page cache is on the nginx side).
- WordPress is slow without cache because on every request PHP loads WordPress core and the theme, queries the DB, and renders the block theme (about 60 ms/request on 2 CPUs). With more plugins, it gets even slower.
- The articles are slightly bigger for AegisCMS (4 KB versus 2.9 KB).

## 3. Notes When Putting AegisCMS Behind a Cache

These are things learned while preparing the measurement. They are reflected in the `docker/bench/` settings too.

1. **By default, public pages are returned with `Cache-Control: no-cache, no-store`** (a safe default that keeps them out of CDNs and caches). To let a front cache store them, set `[cdn].enabled = true` and decide `cache_control` such as `public, max-age=0, s-maxage=600` ([configuration.md](configuration.md)). Members-only pages are `private, no-store` regardless of settings.
2. **The response headers of list pages are large**: `Cache-Tag` and `Surrogate-Key` are added for as many tags as the articles have, about 1.5 KB each on a list (10 items), and about 3.9 KB for the headers overall. This is close to nginx's `proxy_buffer_size` default (4 KB), and exceeding it gives a 502. When putting nginx in front, use `proxy_buffer_size 16k; proxy_buffers 8 16k; proxy_busy_buffers_size 32k;` (`docker/bench/nginx-aegis-cache.conf`). If you do not use the tag headers (no CDN API purging), `[cdn].emit_headers = false` stops them from being added.
3. There is no mechanism to clear nginx's cache automatically on updates (purging by `Cache-Tag` is possible with Cloudflare, Fastly, and webhooks). With nginx, rely on the TTL or purge manually.

## 4. Steps

Prerequisites: Docker (Compose v2) and Node.js. Work inside `docker/bench/`.

```bash
cd docker/bench
docker compose build aegis
docker compose up -d --wait db wp wp-nocache wp-cache aegis aegis-cache

# WordPress: install and 2,000 posts (use about 3 KB of HTML as the content for all of them)
docker compose run --rm wpcli wp core install --url=http://wp.test --title=Bench \
  --admin_user=admin --admin_password=pw123456789 --admin_email=a@example.com --skip-email
MSYS_NO_PATHCONV=1 docker compose run --rm wpcli wp rewrite structure '/%postname%/' --hard
# Generating the posts: wp post generate --count=2000 --post_status=publish --post_author=1 --post_content < content.html
#   (--post_content reads the body from standard input. If you pass nothing, the body is empty)

# AegisCMS: setup and 2,000 articles
curl -s -XPOST localhost:18311/api/v1/setup -H 'content-type: application/json' \
  -d '{"admin_username":"admin","admin_password":"AdminPassword123!"}'
AEGIS_BENCH_PASSWORD='AdminPassword123!' node ../../scripts/seed-benchmark.mjs \
  --url http://127.0.0.1:18311 --user admin -n 2000

# Measure (concurrency, seconds, rounds). The URLs can be changed with POST_WP / POST_AEGIS
bash run.sh 50 10 3 > results.txt

docker compose down -v   # clean up
```

- In Git Bash (Windows), arguments starting with `/` are rewritten as paths. Add `MSYS_NO_PATHCONV=1` (`run.sh` already has it).
- WordPress is called with `Host: wp.test` (because the site URL is `http://wp.test`; a different host name gets redirected).
- The ports are 18311 for AegisCMS directly and 18312 via the cache, and 18301 for WordPress (no cache) and 18302 (with cache). They are for checking; measurement is done from inside the Docker network.

## 5. Limits of This Comparison

- **Different from native numbers**: even for the same AegisCMS, it is slower in Docker (2 CPUs, WSL2) than when run directly on the host (421 req/s for an article; in another measurement run directly on the host it was around 1,800 req/s. The CPU allocation and the Docker layers differ). Read the comparison as relative values within the same Docker conditions.
- WordPress is a minimal setup (no plugins, default theme). Real sites usually get heavier with plugins, page builders, WooCommerce, and so on. Conversely, with a lightweight classic theme it would not be this slow.
- The load is repeated requests to one page, and the no-cache numbers are with the DB and file caches warm. It does not reflect operation with many updates or the real access distribution where pages are scattered.
- Writes (posting, comments), the admin screen, search, and member features were not measured.
