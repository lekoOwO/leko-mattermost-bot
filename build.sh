#!/bin/bash

set -e

echo "==================================="
echo "Building Leko's Mattermost Bot"
echo "==================================="

# 檢查是否安裝了 Rust
if ! command -v cargo &> /dev/null; then
    echo "錯誤: 未找到 cargo 命令"
    echo "請先安裝 Rust: https://rustup.rs/"
    exit 1
fi

echo "清理舊的建置..."
cargo clean

echo "執行測試..."
cargo test

echo "建置 release 版本..."
cargo build --release

echo ""
echo "==================================="
echo "建置完成！"
echo "執行檔位置: ./target/release/leko-mattermost-bot"
echo "==================================="
echo ""
echo "執行方式："
echo "  ./target/release/leko-mattermost-bot"
echo "  或"
echo "  ./target/release/leko-mattermost-bot -c /path/to/config.yaml"
echo ""
