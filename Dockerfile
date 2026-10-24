# ==========================================
# Stage 1: Frontend Builder (Admin Studio)
# ==========================================
FROM node:22.22-alpine AS frontend-builder
WORKDIR /build/admin

# キャッシュ最適化のため package.json を先行コピー
COPY admin/package.json admin/package-lock.json ./
RUN npm ci

# フロントエンドソースコードをコピーしてビルド
COPY admin/ ./
# Admin Studio を別の場所で配信する場合のみ AegisCMS サーバーの URL（例: https://example.com/cms）を指定する
# （未指定なら配信元のサーバーに接続。docs/configuration.md の「リバースプロキシ配下での運用」）
ARG VITE_AEGIS_SERVER_URL=""
ENV VITE_AEGIS_SERVER_URL=${VITE_AEGIS_SERVER_URL}
RUN npm run build

# ==========================================
# Stage 2: Rust Builder (Aegis Core Server)
# ==========================================
# ベースイメージはバージョンを固定する（`latest` は Rust のリリースごとに変わり、
# レイヤーキャッシュが無効になるうえ、実行環境 (bookworm) と OS がずれるため）
FROM rust:1.99.1-slim-bookworm AS chef
WORKDIR /build

# ビルドツール（OpenSSLやビルド補助）と cargo-chef の準備
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/* \
    && cargo install cargo-chef --version 0.1.78 --locked

# 2-1. 依存関係のレシピ (recipe.json) を作成
#      Cargo.toml / Cargo.lock から依存だけを抜き出すため、ソースの変更ではレシピは変わらない
FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
COPY sdk/ ./sdk/
COPY plugins/ ./plugins/
RUN cargo chef prepare --recipe-path recipe.json

# 2-2. 依存クレートだけを先にビルド（レシピが同じならこのレイヤーはキャッシュされる）
FROM chef AS rust-builder
# ビルドプロファイル。CI のビルド検証では LTO を省いた ci-release を渡して時間を短縮する
ARG CARGO_PROFILE=release
COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --profile ${CARGO_PROFILE} -p aegis-server --recipe-path recipe.json

# 2-3. ソースコードをコピーして本番用最適化リリースビルド（ワークスペースのクレートのみ再コンパイル）
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
COPY schemas/ ./schemas/
COPY sdk/ ./sdk/
COPY plugins/ ./plugins/
RUN cargo build --profile ${CARGO_PROFILE} -p aegis-server \
    && cp target/${CARGO_PROFILE}/aegis-server /build/aegis-server

# ==========================================
# Stage 3: Runtime Environment (Minimal & Secure)
# ==========================================
FROM debian:bookworm-slim AS runtime
WORKDIR /app

# 最小限のランタイム依存（ca-certificates, curl）
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# セキュアな非root実行用システムユーザーの作成
RUN groupadd -g 10001 aegis && \
    useradd -u 10001 -g aegis -s /bin/false -M aegis

# 必要なディレクトリ群の作成
RUN mkdir -p /app/data /app/config /app/schemas /app/plugins /app/themes /app/admin/dist

# ビルド成果物の配置
COPY --from=rust-builder /build/aegis-server /usr/local/bin/aegis-server
COPY --from=frontend-builder /build/admin/dist /app/admin/dist
COPY config/docker.toml /app/config/docker.toml
COPY config/default.toml /app/config/default.toml
COPY schemas/ /app/schemas/
COPY plugins/ /app/plugins/
COPY themes/ /app/themes/

# ファイル権限の設定
RUN chown -R aegis:aegis /app

# 非rootユーザーに切り替え
USER aegis

# 環境変数・ポート・ボリューム定義
ENV AEGIS_CONFIG=/app/config/docker.toml
EXPOSE 8080
VOLUME ["/app/data"]

# ヘルスチェック
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD curl -f http://127.0.0.1:8080/api/v1/health || exit 1

ENTRYPOINT ["aegis-server"]
CMD ["run"]
