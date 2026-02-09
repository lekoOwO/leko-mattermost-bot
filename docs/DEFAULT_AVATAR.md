# 預設頭像配置說明

## 概述

`default_avatar` 是一個可選的配置項，允許您為機器人發送的訊息設定預設頭像。當訊息不需要使用特定用戶頭像時，會使用此預設頭像。

## 支援的格式

### 1. 完整 URL（推薦）

直接指定頭像圖片的 URL：

```yaml
mattermost:
  default_avatar: "https://example.com/avatar.png"
```

**優點**：
- 簡單直接
- 不依賴 Mattermost 用戶或 emoji
- 可使用任意圖片來源

**適用場景**：
- 使用自訂的機器人頭像
- 頭像託管在外部服務

### 2. 用戶 ID 格式（`#user_id`）

使用 `#` 前綴加上 Mattermost 用戶 ID：

```yaml
mattermost:
  default_avatar: "#w5qj3cmxfjyu5kqjte55rwhhbh"
```

程式會自動將其轉換為：
```
https://your-mattermost-url/api/v4/users/w5qj3cmxfjyu5kqjte55rwhhbh/image
```

**優點**：
- 直接指定用戶 ID，快速且穩定
- 不需要在啟動時進行額外的 API 查詢

**適用場景**：
- 已知確切的用戶 ID
- 需要最佳啟動效能

**如何獲取 user_id**：
1. 在 Mattermost 中點擊用戶個人資料
2. 在瀏覽器 URL 中可以找到 user_id
3. 或透過 API: `GET /api/v4/users/username/{username}`

### 3. 用戶名稱格式（`@username`）

使用 `@` 前綴加上 Mattermost 用戶名稱：

```yaml
mattermost:
  default_avatar: "@bot"
```

程式啟動時會自動查詢該用戶名稱對應的 user_id，並將配置解析為 `#user_id` 格式。

**優點**：
- 更易讀易記（使用用戶名稱而非 ID）
- 適合在配置檔案中使用

**缺點**：
- 啟動時需要額外的 API 查詢
- 如果用戶名稱不存在，會記錄錯誤但不中斷啟動

**適用場景**：
- 配置檔案的可讀性比效能更重要
- 用戶 ID 不易記憶或取得

### 4. Emoji 格式（`:emoji:`）

使用 `:emoji:` 格式指定 Mattermost 自訂 emoji 名稱：

```yaml
mattermost:
  default_avatar: ":troll:"
```

**程式會直接使用 `icon_emoji` 參數，無需在啟動時解析。**

Mattermost 會自動將 emoji 名稱轉換為對應的圖示顯示。

**優點**：
- ✅ **無需啟動時 API 查詢** - 直接使用 `icon_emoji` 參數，啟動速度快
- 使用 Mattermost 內建或自訂的 emoji
- 易於維護和更換（只需修改 emoji 名稱）
- emoji 與 Mattermost 主題一致
- 如果 emoji 不存在，Mattermost 會自動 fallback 到預設顯示

**適用場景**：
- 想使用 Mattermost 自訂 emoji 作為頭像
- 團隊有統一的 emoji 風格
- 需要經常更換頭像（修改 emoji 名稱即可）
- **推薦**：相較於其他需要解析的格式，emoji 格式最簡單高效

## 範例配置

```yaml
# 範例 1: 使用完整 URL
mattermost:
  url: https://mattermost.example.com
  bot_token: your-token
  default_avatar: https://example.com/bot-avatar.png

# 範例 2: 使用用戶 ID 格式
mattermost:
  url: https://mattermost.example.com
  bot_token: your-token
  default_avatar: "#w5qj3cmxfjyu5kqjte55rwhhbh"

# 範例 3: 使用用戶名稱格式（啟動時自動解析）
mattermost:
  url: https://mattermost.example.com
  bot_token: your-token
  default_avatar: "@bot"

# 範例 4: 使用 emoji 格式（啟動時自動解析）
mattermost:
  url: https://mattermost.example.com
  bot_token: your-token
  default_avatar: ":troll:"
```

## 技術實現

### 配置解析流程

1. **載入配置檔案** - `Config::from_path()` 讀取 YAML
2. **檢查是否需要解析 username** - `config.needs_avatar_resolution()` 檢查是否為 `@username` 格式
3. **解析用戶名稱**（若需要）- 呼叫 `mattermost_client.get_user_by_username(username)` 查詢 user_id
4. **更新配置為 #user_id** - `config.set_resolved_avatar(user_id)` 將 `@username` 替換為 `#user_id`
5. **Emoji 格式直接使用** - `:emoji:` 格式無需解析，直接通過 `config.default_avatar_emoji()` 取得並使用 `icon_emoji` 參數
6. **使用時轉換** - `config.default_avatar_url()` 將 `#user_id` 轉換為完整的頭像 API URL，emoji 格式則返回 `None`

### 圖示參數使用

程式使用 `IconConfig` enum 來統一處理不同類型的頭像：

```rust
pub enum IconConfig {
    Url(String),    // 使用 icon_url 參數
    Emoji(String),  // 使用 icon_emoji 參數
}
```

- **URL 格式**（包括完整 URL、`#user_id`）→ 使用 `icon_url` 參數
- **Emoji 格式**（`:emoji:`）→ 使用 `icon_emoji` 參數

### 相關程式碼位置

- `src/config.rs` - `MattermostConfig::default_avatar` 欄位及相關方法
  - `default_avatar_url()` - 返回 URL 格式的頭像（emoji 格式返回 None）
  - `default_avatar_emoji()` - 返回 emoji 格式的頭像
- `src/handlers/reply_helpers.rs` - `IconConfig` enum 和相關 helper 函數
- `src/main.rs` - 啟動時的用戶名稱解析邏輯（emoji 無需解析）
- `src/mattermost.rs` - `get_user_by_username()` API 方法

## 使用建議

1. **生產環境**：
   - **首選**：`:emoji:` 格式 - 簡單高效，無需啟動時查詢
   - **次選**：完整 URL 或 `#user_id` 格式 - 適合需要使用特定圖片的場景
2. **開發/測試環境**：可使用 `@username` 或 `:emoji:` 格式，提高配置的可讀性
3. **配置管理**：如果使用版本控制管理配置，`@username` 或 `:emoji:` 格式更容易審查和理解
4. **Emoji 頭像**：如果想使用與團隊風格一致的頭像，`:emoji:` 是**最佳選擇**

## 錯誤處理

### @username 解析失敗
- 程式會記錄錯誤訊息到日誌
- 不會中斷程式啟動
- 頭像配置會保持為未解析的 `@username`，可能導致頭像顯示異常

### :emoji: 不存在
- Mattermost 會自動處理不存在的 emoji
- 顯示為 emoji 名稱文字（如 `:unknown_emoji:`）
- 不會影響程式運行

建議在生產環境檢查日誌確保解析成功（僅 `@username` 格式需要）。

## 相關訊息

預設頭像會應用於以下情況：
- `/leko` 指令的回覆訊息
- `/sticker` 指令的互動式訊息
- 團購功能的相關訊息

如需針對特定訊息使用不同頭像，可在程式碼中直接指定 `icon_url` 參數。
