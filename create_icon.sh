#!/bin/bash

# 創建一個簡單的 PNG 圖標（使用 ImageMagick）
# 如果沒有 ImageMagick，可以手動創建一個 icon.png

if command -v convert &> /dev/null; then
    # 創建一個 128x128 的藍色方形圖標，中間有 "🧽" emoji
    convert -size 128x128 xc:#FDD835 \
        -gravity center \
        -pointsize 72 \
        -annotate +0+0 "🧽" \
        icon.png
    echo "✅ icon.png 已創建"
else
    echo "⚠️  未安裝 ImageMagick"
    echo "請手動創建 icon.png (128x128 像素)"
    echo "或從網路下載貼圖機器人圖標並命名為 icon.png"
fi
