# ==========================================
# Stage 1: Frontend Builder (Admin Studio)
# ==========================================
FROM node:22-alpine AS frontend-builder
WORKDIR /build/admin

# キャッシュ最適化のため package.json を先行コピー
COPY admin/package.json admin/package-lock.json ./
RUN npm ci

# フロントエンドソースコードをコピーしてビルド
COPY admin/ ./
RUN npm run build

# ==========================================
# Stage 2: Rust Builder (Aegis Core Server)
# ==========================================
FROM rust:latest AS rust-builder
WORKDIR /build

# ビルドツール（OpenSSLやビルド補助）の準備
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# ソースコード全体をコピー
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
COPY schemas/ ./schemas/
COPY plugins/ ./plugins/

# 本番用最適化リリースビルド
RUN cargo build --release -p aegis-server

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
RUN mkdir -p /app/data /app/config /app/schemas /app/plugins /app/admin/dist

# ビルド成果物の配置
COPY --from=rust-builder /build/target/release/aegis-server /usr/local/bin/aegis-server
COPY --from=frontend-builder /build/admin/dist /app/admin/dist
COPY config/docker.toml /app/config/docker.toml
COPY config/default.toml /app/config/default.toml
COPY schemas/ /app/schemas/
COPY plugins/ /app/plugins/

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
