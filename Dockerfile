# 多階段建置，最佳化 Docker 映像檔大小
FROM rust:1.83-alpine AS builder

# 安裝建置依賴
RUN apk add --no-cache musl-dev

WORKDIR /app

# 複製依賴檔案
COPY Cargo.toml ./

# 建立假的 main.rs 來快取依賴
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# 複製實際的原始碼
COPY src ./src

# 建置實際的應用程式
RUN cargo build --release

# 執行階段使用 Alpine Linux
FROM alpine:3.19

# 安裝執行時期依賴
RUN apk add --no-cache ca-certificates

WORKDIR /app

# 從建置階段複製編譯好的執行檔
COPY --from=builder /app/target/release/leko-mattermost-bot .

# 建立資料目錄
RUN mkdir -p /app/data

# 暴露預設埠號
EXPOSE 3000

# 執行應用程式
CMD ["./leko-mattermost-bot"]
