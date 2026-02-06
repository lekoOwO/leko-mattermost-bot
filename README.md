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
  default_avatar: https://example.com/avatar.png # 可選，預設頭像（推薦使用 URL）
  # default_avatar: "#bot_user_id_here"          # 或使用 #user_id 格式
  # default_avatar: "@bot_username"              # 或使用 @username 格式（啟動時自動解析）

stickers:
  categories:
    - name: 海綿寶寶
      sources:
        # 從本地檔案載入
        - type: file
          format: csv
          path: data/sb.csv
        
        - type: file
          format: json
          path: data/sb.json
        
        # 從遠端伺服器透過 HTTP GET 獲取
        - type: http_get
          format: json
          url: https://example.com/stickers.json
          headers:                      # 可選
            Authorization: Bearer your-token-here
            User-Agent: Leko-Bot

admin:                          # 管理員列表（可選）
  - "@username"                 # @開頭代表 username
  - "userid123"                 # 否則為 user_id
```

#### 貼圖來源配置說明

**type: file** - 從本地檔案載入
- `format`: 資料格式，`csv` 或 `json`
- `path`: 檔案路徑

**type: http_get** - 從遠端 HTTP GET 獲取
- `format`: 資料格式，`csv` 或 `json`
- `url`: 遠端 URL（必填）
- `headers`: 自定義 HTTP headers（可選）

#### 預設頭像配置說明

`default_avatar` 是可選的配置項，用於設定在沒有指定特定用戶頭像的訊息中顯示的預設頭像。支援三種格式：

- **一般 URL**（推薦）：`https://example.com/avatar.png`
- **用戶 ID 格式**：`#user_id` - 會自動轉換為該用戶的頭像 API URL
- **用戶名稱格式**：`@username` - 啟動時自動解析為用戶 ID

⚠️ **格式說明**：
- `#user_id`：直接指定 Mattermost 用戶 ID（例如：`#w5qj3cmxfjyu5kqjte55rwhhbh`）
- `@username`：指定 Mattermost 用戶名稱（例如：`@bot`），程式啟動時會自動查詢並解析為用戶 ID

**如何獲取 user_id**：
1. 在 Mattermost 中點擊用戶個人資料
2. 在 URL 中可以看到 user_id，例如：`/messages/@user_id`
3. 或使用 Mattermost API: `GET /api/v4/users/username/{username}` 獲取 user_id

此設定適用於以下情況：
- `/leko help` 顯示說明訊息時
- `/leko admin` 管理指令回應時
- `/sticker` 指令錯誤訊息時
- 其他沒有特定用戶身份的 ephemeral 訊息

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

### SQLx offline 查詢快取

此專案使用 `sqlx` 的編譯時查詢宏以提升安全性與可靠性。請參閱 `DEV.md` 的「SQLx offline 查詢快取 (.sqlx)」段落了解如何使用專案內的 `sqlx_prepare` helper 來生成 `.sqlx`。常見命令：

```bash
# 只為主二進位檔準備（快速）
cargo run --bin sqlx_prepare

# 為所有 target（包含 tests）準備（CI/完整本機準備）
SQLX_PREPARE_ALL=1 cargo run --bin sqlx_prepare
```

將產生的 `.sqlx` 加入版本控制可讓其他開發者和 CI 在不需連線真實 DB 的情況下正確編譯。 

## 開發

### 測試

專案包含完整的測試套件 (59 個測試):

```bash
# 執行所有測試
cargo test

# 執行特定模組測試
cargo test websocket::
cargo test validation::
cargo test handlers::group_buy::

# 查看測試覆蓋
cargo test -- --nocapture
```

### 程式碼品質

本專案已經過系統性重構 (Phase 1-3)，具備：

- ✅ **完整測試覆蓋**: 59 個單元與整合測試
- ✅ **依賴注入**: 使用 trait-based DI 支援 mock 測試
- ✅ **模組化設計**: 清晰的關注點分離
- ✅ **統一錯誤處理**: 一致的錯誤處理模式

詳見 [REFACTOR_SUMMARY.md](./REFACTOR_SUMMARY.md) 了解重構細節。

### 專案統計

```bash
# 查看專案統計資訊
./stats.sh
```

### 程式碼結構

```
src/
├── main.rs                 # 應用程式入口與 HTTP 伺服器
├── config.rs               # 配置管理
├── mattermost.rs           # Mattermost API 客戶端 + Service trait
├── sticker.rs              # 貼圖資料庫
├── database.rs             # SQLite 資料庫層
├── websocket.rs            # WebSocket 客戶端
├── constants.rs            # 常數定義 (NEW)
├── env.rs                  # 環境變數管理 (NEW)
├── validation.rs           # 輸入驗證 (NEW)
├── test_utils.rs           # 測試工具與 Mock (重構)
└── handlers/
    ├── mod.rs              # Handler 路由
    ├── reply_helpers.rs    # 回應輔助函數 (NEW)
    ├── auth.rs             # 認證處理
    ├── sticker.rs          # 貼圖 handler
    ├── leko.rs             # /leko 指令 handler
    └── group_buy/          # 團購功能
        ├── mod.rs
        ├── actions.rs      # 按鈕 action 處理
        ├── dialogs.rs      # Dialog 管理
        ├── messages.rs     # 訊息格式化
        ├── utils.rs        # 工具函數
        └── tests.rs        # 整合測試 (NEW)
```

## 專案結構 (Legacy)

舊版結構說明保留供參考。最新結構請見上方「開發 > 程式碼結構」段落。

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

## 貢獻

歡迎提交 Issue 和 Pull Request！

## 授權

MIT
