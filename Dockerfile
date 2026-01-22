# 1. Planner 階段：分析專案依賴
FROM lukemathwalker/cargo-chef:latest-rust-1.92-alpine3.21 AS planner
WORKDIR /app
COPY . .
RUN cargo chef prepare --recipe-json recipe.json

# 2. Cacher 階段：編譯依賴檔 (這層會被強大快取)
FROM lukemathwalker/cargo-chef:latest-rust-1.92-alpine3.21 AS cacher
WORKDIR /app
COPY --from=planner /app/recipe.json recipe.json
# 安裝必要的系統依賴 (例如 musl-dev)
RUN apk add --no-cache musl-dev
RUN cargo chef cook --release --recipe-json recipe.json

# 3. Builder 階段：編譯實際的程式碼
FROM rust:1.92-alpine AS builder
WORKDIR /app
COPY . .
# 從 cacher 複製已經編譯好的依賴
COPY --from=cacher /app/target target
COPY --from=cacher /usr/local/cargo /usr/local/cargo
# 執行真正的編譯
RUN apk add --no-cache musl-dev
RUN cargo build --release

# 4. Runtime 階段：最小執行環境
FROM alpine:3.21
RUN apk add --no-cache ca-certificates libc6-compat
WORKDIR /app

# 從 builder 複製編譯好的執行檔 (請確認名稱與 Cargo.toml 一致)
COPY --from=builder /app/target/release/leko-mattermost-bot .
RUN mkdir -p /app/data

EXPOSE 3000
CMD ["./leko-mattermost-bot"]
