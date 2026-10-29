#!/bin/sh
# AegisCMS + Caddy で sites.toml のサイトを HTTPS で公開する。使い方は README.md
#
#   ./deploy.sh up                    イメージを用意し、設定を生成して起動（もう一度実行すると設定を反映）
#   ./deploy.sh generate              設定（compose.yaml・Caddyfile・sites/*/aegis.toml）だけを生成
#   ./deploy.sh build                 AegisCMS のイメージをビルドし直す（リポジトリのソースから）
#   ./deploy.sh user <サイト> [名前]   サイトの管理者を作る（既定の名前は admin。パスワードを聞かれる）
#   ./deploy.sh ps | logs [サイト] | restart [サイト] | down
#
# AegisCMS のイメージは環境変数 AEGIS_IMAGE（既定 aegiscms:latest）
set -eu
cd "$(dirname "$0")"
IMAGE="${AEGIS_IMAGE:-aegiscms:latest}"
export AEGIS_IMAGE="$IMAGE"

compose() {
	docker compose -f compose.yaml "$@"
}

build() {
	docker build -t "$IMAGE" ..
}

generate() {
	if [ ! -f sites.toml ]; then
		echo "sites.toml がありません。cp sites.example.toml sites.toml して書き換えてください" >&2
		exit 1
	fi
	# イメージの中の aegis-server で生成する（サーバーには Docker だけあればよい）。
	# 生成したファイルの持ち主をこのユーザーにする
	docker run --rm --user "$(id -u):$(id -g)" -e RUST_LOG=warn \
		-v "$PWD:/deploy" -w /deploy "$IMAGE" deploy generate --dir /deploy
}

case "${1:-up}" in
up)
	docker image inspect "$IMAGE" >/dev/null 2>&1 || build
	generate
	compose up -d --remove-orphans
	# 設定の変更を反映する（Caddy は止めずに読み直す）
	compose exec -T caddy caddy reload --config /etc/caddy/Caddyfile >/dev/null 2>&1 || true
	compose ps
	;;
generate) generate ;;
build) build ;;
user)
	if [ $# -lt 2 ]; then
		echo "使い方: ./deploy.sh user <サイト> [ユーザー名]" >&2
		exit 1
	fi
	compose run --rm "$2" user add "${3:-admin}" --role admin
	;;
ps) compose ps ;;
logs)
	shift
	compose logs -f "$@"
	;;
restart)
	shift
	compose restart "$@"
	;;
down) compose down ;;
*)
	sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'
	exit 1
	;;
esac
