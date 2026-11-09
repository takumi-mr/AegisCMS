#!/bin/bash
# 比較ベンチマークの実行（docker compose up -d --wait の後、docs/benchmark.md の手順でデータを入れてから）。
# 使い方: bash run.sh [並行数] [秒] [ラウンド数] > results.txt
C=${1:-50}; D=${2:-10}; R=${3:-3}
NET=aegisbench_default
POST_WP=${POST_WP:-/post-1990/}
POST_AEGIS=${POST_AEGIS:-/articles/bench-article-5}
declare -A TARGET HOSTH
TARGET[aegis]="http://aegis:8080";        HOSTH[aegis]="Host: aegis"
TARGET[aegis-cache]="http://aegis-cache"; HOSTH[aegis-cache]="Host: aegis"
TARGET[wp]="http://wp-nocache";           HOSTH[wp]="Host: wp.test"
TARGET[wp-cache]="http://wp-cache";       HOSTH[wp-cache]="Host: wp.test"
path_of() { # 名前 ページ
  case "$2" in
    home) echo "/";;
    post) case "$1" in aegis*) echo "$POST_AEGIS";; *) echo "$POST_WP";; esac;;
  esac
}
for round in $(seq 1 $R); do
  for page in home post; do
    for name in aegis aegis-cache wp wp-cache; do
      url="${TARGET[$name]}$(path_of $name $page)"
      # ウォームアップ（キャッシュを温める）
      MSYS_NO_PATHCONV=1 docker compose run --rm -T wrk -t2 -c10 -d3s -H "${HOSTH[$name]}" "$url" > /dev/null 2>&1
      out=$(MSYS_NO_PATHCONV=1 docker compose run --rm -T wrk -t4 -c$C -d${D}s --latency -H "${HOSTH[$name]}" "$url" 2>&1)
      rps=$(echo "$out" | awk '/Requests\/sec/{print $2}')
      p50=$(echo "$out" | awk '$1=="50%"{print $2}')
      p99=$(echo "$out" | awk '$1=="99%"{print $2}')
      non=$(echo "$out" | awk '/Non-2xx/{print $NF}')
      echo "$round $page $name rps=$rps p50=$p50 p99=$p99 non2xx=${non:-0}"
    done
  done
done
