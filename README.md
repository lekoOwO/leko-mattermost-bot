# Leko's Mattermost Bot

一個用 Rust 開發的 Mattermost 機器人，提供貼圖選擇功能，支援多分類管理。

## 功能

- 🎨 **互動式貼圖選擇**：透過 Interactive Dialog 介面選擇貼圖
- 📁 **分類管理**：支援多分類組織貼圖
- 🔍 **即時搜尋**：可以搜尋貼圖名稱
- 👤 **身份保留**：發送的貼圖顯示為觸發指令者的身份和頭像
- 🔒 **Token 驗證**：支援 Slash Command Token 驗證
- � **管理功能**：透過 Direct Message 管理 bot（限管理員）
- �🚀 **多平台支援**：提供 Linux (x86_64/aarch64)、Windows、macOS 預編譯版本

## 快速開始

### 方法 1：下載預編譯版本

從 [Releases](../../releases) 頁面下載對應平台的執行檔。

### 方法 2：從原始碼編譯

```bash
cargo build --release
```

### 配置

建立 `data/config.yaml`：

```yaml
mattermost:
  url: http://your-mattermost-server:8065
  bot_token: your-bot-token-here
  slash_command_token: your-slash-command-token  # 可選，建議啟用
  bot_callback_url: http://your-bot-server:3000  # Bot 服務位址

stickers:
  categories:
    - name: 海綿寶寶
      csv:
        - data/sb.csv
      json:
        - data/sb.json

admin:                          # 管理員列表（可選）
  - "@username"                 # @開頭代表 username
  - "userid123"                 # 否則為 user_id
```

### 在 Mattermost 設定

1. **建立 Bot Account**：
   - 到 System Console > Integrations > Bot Accounts
   - Create Bot Account
   - 複製 Access Token 到 `config.yaml` 的 `bot_token`

2. **建立 Slash Command**：
   - 到 Integrations > Slash Commands > Add Slash Command
   - Trigger Word: `sticker`（或 `leko`）
   - Request URL: `http://your-bot-server:3000/sticker`（或 `/leko`）
   - Request Method: `POST`
   - 複製 Token 到 `config.yaml` 的 `slash_command_token`

3. **啟用 Interactive Dialogs**：
   - 到 System Console > Integrations > Integration Management
   - 確認 "Enable integrations to override usernames" 已啟用
   - 確認 "Enable integrations to override profile picture icons" 已啟用

> **注意**：Bot 會自動透過 WebSocket 連接到 Mattermost 接收 Direct Message，不需要額外設定 Outgoing Webhook。

### 執行

```bash
./leko-mattermost-bot -c data/config.yaml -H 0.0.0.0 -p 3000
```

### 使用

在 Mattermost 頻道中使用 Slash Command：

```
/sticker              # 顯示所有貼圖
/sticker 關鍵字        # 搜尋貼圖
/leko sticker         # 等同於 /sticker
/leko help            # 顯示 /leko 指令說明
```

在與 bot 的 Direct Message 中（限管理員）：

```
help                  # 顯示管理指令說明
ping                  # 測試連線
status                # 顯示 bot 狀態
```

## 資料格式

### CSV 格式

支援三種 header：

```csv
名稱,圖片
海綿寶寶,https://i.imgur.com/abc123.jpg
派大星,https://i.imgur.com/def456.jpg
```

或

```csv
名稱,圖片網址
海綿寶寶,https://i.imgur.com/abc123.jpg
派大星,https://i.imgur.com/def456.jpg
```

或

```csv
名稱,i.imgur
海綿寶寶,abc123
派大星,def456
```

### JSON 格式

```json
{
  "海螺": "https://i.imgur.com/xyz789.jpg",
  "蟹老闆": "https://i.imgur.com/def456.jpg"
}
```

## Docker 部署

### 使用 GitHub Container Registry

```bash
docker pull ghcr.io/lekoowo/leko-mattermost-bot:main

docker run -d \
  -p 3000:3000 \
  -v $(pwd)/data:/app/data \
  ghcr.io/lekoowo/leko-mattermost-bot:main \
  -c /app/data/config.yaml -H 0.0.0.0 -p 3000
```

### 自行建置

```bash
docker build -t leko-mattermost-bot .

docker run -d \
  -p 3000:3000 \
  -v $(pwd)/data:/app/data \
  leko-mattermost-bot \
  -c /app/data/config.yaml -H 0.0.0.0 -p 3000
```

## 開發

參見 [DEV.md](DEV.md) 和 [AGENTS.md](AGENTS.md)

### 執行測試

```bash
cargo test
```

### 啟用除錯日誌

```bash
RUST_LOG=debug cargo run -- -c data/config.yaml
```

## 專案結構

```
.
├── src/
│   ├── main.rs         # HTTP 伺服器與路由
│   ├── config.rs       # 配置管理
│   ├── mattermost.rs   # Mattermost API 客戶端
│   ├── sticker.rs      # 貼圖資料庫
│   └── app.rs          # Mattermost App 框架類型
├── data/
│   ├── config.yaml     # 配置檔案
│   ├── sb.csv          # CSV 格式貼圖資料
│   └── sb.json         # JSON 格式貼圖資料
└── .github/
    └── workflows/
        └── ci.yml      # CI/CD 自動化

```

## 授權

MIT
