# 架構文件

## 專案架構概覽

`leko-mattermost-bot` 是一個使用 Rust 開發的 Mattermost 機器人，採用現代化的架構設計。

```
┌─────────────────────────────────────────────────────────┐
│                    HTTP Server (Warp)                   │
│  - Slash Commands                                       │
│  - Interactive Dialogs                                  │
│  - Action Buttons                                       │
└────────────────┬────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────────┐
│                     Handlers Layer                      │
│  - Auth Handler                                         │
│  - Sticker Handler                                      │
│  - Group Buy Handler                                    │
│  - Leko Admin Handler                                   │
└────────────────┬────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────────┐
│                    Service Layer                        │
│  - MattermostService (trait)                           │
│    ├─ MattermostClient (prod)                          │
│    └─ MockMattermostService (test)                     │
│  - StickerDatabase                                      │
│  - Database (SQLite)                                    │
└────────────────┬────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────────┐
│                   External Services                     │
│  - Mattermost API (REST + WebSocket)                   │
│  - SQLite Database                                      │
└─────────────────────────────────────────────────────────┘
```

---

## 核心模組

### 1. main.rs
**責任**: 應用程式入口、HTTP 伺服器、路由配置

**關鍵組件**:
- `AppState`: 應用程式全域狀態
  - `config`: 配置管理
  - `mattermost_client`: Mattermost API 客戶端 (trait object)
  - `database`: SQLite 資料庫連線池
  - `sticker_database`: 貼圖資料庫
  - `bot_user_id`: Bot 使用者 ID

**路由**:
```
POST /command/sticker          - 貼圖搜尋指令
POST /command/leko             - 管理指令
POST /command/group_buy        - 團購指令
POST /action                   - Action 按鈕回調
POST /dialog/sticker           - 貼圖 Dialog 提交
POST /api/v1/group_buy/*       - 團購 API 端點
```

---

### 2. mattermost.rs
**責任**: Mattermost API 封裝

**核心類型**:
- `MattermostService` trait: 定義服務介面（支援 DI）
- `MattermostClient`: 真實 API 客戶端實作
- `Post`, `User`, `Channel`: API 資料結構
- `ActionRequest`, `DialogSubmission`: Interactive 相關結構

**關鍵方法**:
```rust
pub trait MattermostService {
    async fn get_user(&self, user_id: &str) -> Result<User>;
    async fn create_post(&self, post: &Post) -> Result<Post>;
    async fn update_post(&self, post_id: &str, post: &Post) -> Result<Post>;
    async fn open_dialog(&self, ...) -> Result<()>;
    // ... 12 個方法
}
```

---

### 3. database.rs
**責任**: SQLite 資料庫操作

**資料表**:
- `group_buys`: 團購主表
- `group_buy_orders`: 團購訂單
- `shortage_adjustments`: 缺貨調整記錄
- `stickers`: 貼圖資料
- `action_logs`: 操作日誌

**特點**:
- 使用 `sqlx` 編譯時檢查 SQL
- WAL 模式提升並發效能
- 連線池管理 (max 5 connections)
- 版本控制（樂觀鎖定）

---

### 4. sticker.rs
**責任**: 貼圖管理

**功能**:
- 從多種來源載入貼圖 (CSV, JSON, HTTP)
- 分類管理
- 全文搜尋（LIKE-based）
- 統計分析

**資料來源**:
```yaml
stickers:
  categories:
    - name: 分類名稱
      sources:
        - type: file        # 本地檔案
          format: csv|json
          path: data/stickers.csv
        - type: http        # HTTP GET
          format: csv|json
          url: https://...
```

---

### 5. websocket.rs
**責任**: WebSocket 客戶端

**功能**:
- 監聽 Mattermost 事件
- 處理 Direct Message
- 自動重連（5 秒延遲）
- 管理員指令處理

**事件處理**:
```rust
pub fn parse_websocket_event(text: &str) -> Result<Option<WebSocketEvent>>
pub fn should_process_event(&event: &WebSocketEvent) -> Option<String>
pub fn parse_posted_event_data(&data: &Value) -> Result<Option<PostedEventData>>
```

**純函數設計**: 所有解析邏輯都是純函數，易於測試。

---

### 6. validation.rs
**責任**: 輸入驗證

**驗證器**:
- `GroupBuyValidator`: 團購表單驗證
  - 商家名稱
  - 描述
  - Metadata (YAML 格式)
  - Items (YAML 格式)
  - 價格
  - 缺貨調整

**錯誤類型**:
```rust
pub enum ValidationError {
    MerchantNameEmpty,
    MerchantNameTooLong,
    DescriptionTooLong,
    MetadataInvalidYaml,
    ItemsInvalidYaml,
    // ...
}
```

---

### 7. test_utils.rs
**責任**: 測試工具

**核心組件**:
- `MockMattermostService`: Mock Mattermost API
  - 記錄所有呼叫
  - 儲存建立的 posts
  - 提供 mock 資料（users, channels）
  - 模擬錯誤情境

**測試輔助**:
```rust
pub mod utils {
    pub async fn setup_db() -> Database
    pub fn make_group_buy(id: String, version: i64) -> GroupBuy
    pub async fn insert_group_buy(db: &Database, version: i64) -> GroupBuy
    // ...
}
```

---

## Handlers 架構

### handlers/mod.rs
**責任**: Handler 路由和全域錯誤處理

### handlers/reply_helpers.rs
**責任**: 標準化回應格式

**輔助函數**:
```rust
pub fn ephemeral_text_reply(text: &str) -> String
pub fn ephemeral_text_json(text: &str) -> warp::reply::Json
pub fn dialog_error_with_status(error: String) -> warp::reply::WithStatus
pub fn dialog_empty_with_status() -> warp::reply::WithStatus
```

### handlers/group_buy/
**團購功能模組**

```
group_buy/
├── mod.rs          - 模組導出
├── actions.rs      - Action 按鈕處理
├── dialogs.rs      - Dialog 管理
├── messages.rs     - 訊息格式化
├── utils.rs        - 工具函數
└── tests.rs        - 整合測試
```

**關鍵設計**:
- **權限檢查**: `check_creator_permission()`
- **狀態檢查**: `check_active_status()`, `check_closed_status()`
- **Dialog Builders**: `text_element()`, `textarea_element()`, `number_element()`
- **訊息生成**: `format_group_buy_message()`

---

## 設計模式

### 1. Dependency Injection (DI)

使用 trait objects 實現依賴注入：

```rust
pub struct AppState {
    pub mattermost_client: Arc<dyn MattermostService>,
    // ...
}
```

**優點**:
- 測試時可注入 mock
- 解耦具體實作
- 支援 TDD

### 2. Builder Pattern

```rust
ActionButton::builder()
    .name("action_name")
    .text("按鈕文字")
    .style(ButtonStyle::Primary)
    .build()
```

**使用場景**:
- ActionButton 建構
- Dialog 元素建構

### 3. Pure Functions

```rust
// 純函數 - 無副作用，易於測試
pub fn parse_websocket_event(text: &str) -> Result<Option<WebSocketEvent>>
pub fn check_creator_permission(...) -> Result<(), String>
```

**優點**:
- 易於測試
- 易於推理
- 可組合

### 4. Error Handling

```rust
// 使用 anyhow::Context 提供詳細錯誤訊息
fs::read_to_string(path)
    .with_context(|| format!("無法讀取檔案: {}", path))?
```

---

## 資料流

### Slash Command 處理流程

```
User types /sticker <query>
         │
         ▼
POST /command/sticker
         │
         ▼
handlers::sticker::handle_sticker_command()
         │
         ├─→ 驗證 token
         ├─→ 搜尋貼圖 (sticker_db.search_async())
         ├─→ 建構 Interactive Message
         └─→ 回傳 JSON response
         │
         ▼
User sees interactive buttons
         │
         ▼
User clicks button
         │
         ▼
POST /action (ActionRequest)
         │
         ▼
handlers::actions::handle_sticker_action()
         │
         ├─→ 查詢貼圖資料
         ├─→ 建立 Post (as user)
         └─→ 回傳空 response
         │
         ▼
Message posted in channel
```

### Dialog 處理流程

```
User clicks "建立團購"
         │
         ▼
POST /action (trigger_id)
         │
         ▼
handlers::group_buy::handle_group_buy_action()
         │
         └─→ open_create_dialog()
             │
             └─→ mattermost_client.open_dialog()
         │
         ▼
Dialog appears
         │
         ▼
User submits dialog
         │
         ▼
POST /api/v1/group_buy/dialog/create
         │
         ▼
handlers::group_buy::dialogs::handle_create_dialog()
         │
         ├─→ 驗證輸入 (GroupBuyValidator)
         ├─→ 儲存到資料庫 (database.create_group_buy())
         ├─→ 發送 Post 到頻道
         └─→ 回傳空 response
         │
         ▼
Group buy message posted
```

---

## 測試策略

### 測試金字塔

```
      /\
     /  \   整合測試 (5 個)
    /____\
   /      \
  /  單元   \   單元測試 (54 個)
 /  測試    \
/____________\
```

### 測試分類

1. **單元測試** (54 個)
   - validation: 9 個
   - websocket: 9 個
   - dialog_builders: 4 個
   - group_buy/utils: 6 個
   - constants: 9 個
   - sticker: 14 個
   - 其他: 3 個

2. **整合測試** (5 個)
   - group_buy/tests: 3 個
   - handlers/leko: 2 個

### Mock 測試

```rust
#[tokio::test]
async fn test_with_mock() {
    let mock = Arc::new(MockMattermostService::new());
    mock.add_mock_user("user1", "username1");
    
    let state = create_test_app_state(mock.clone()).await;
    
    // 執行測試邏輯
    let result = some_handler(&state).await;
    
    // 驗證
    assert_eq!(mock.create_post_call_count(), 1);
    assert!(result.is_ok());
}
```

---

## 效能考量

### 資料庫

- **連線池**: 最多 5 個並發連線
- **WAL 模式**: 改善讀寫並發
- **Prepared Statements**: sqlx 編譯時準備
- **版本控制**: 樂觀鎖定避免衝突

### HTTP

- **Async/Await**: 全面使用 tokio 異步
- **連線重用**: reqwest 自動管理
- **超時設定**: 30 秒 timeout

### WebSocket

- **自動重連**: 5 秒延遲重試
- **心跳檢測**: Ping/Pong 機制
- **事件過濾**: 只處理必要事件

### 記憶體

- **Arc 共享**: 減少複製
- **String 重用**: 避免不必要的 clone
- **Lazy Loading**: 按需載入貼圖

---

## 安全性

### 認證

- **Token 驗證**: Slash Command token
- **管理員檢查**: DM 指令權限控制
- **權限分離**: 團購建立者權限檢查

### 輸入驗證

- **YAML 解析**: 安全的 serde_yaml
- **SQL Injection**: sqlx 參數化查詢
- **長度限制**: 所有輸入都有長度限制

### 錯誤處理

- **不洩漏敏感資訊**: 錯誤訊息不包含內部細節
- **優雅降級**: 錯誤時回傳友善訊息

---

## 部署架構

```
┌──────────────────┐
│  Mattermost      │
│  Server          │
└────────┬─────────┘
         │ REST API
         │ WebSocket
         ▼
┌──────────────────┐
│  leko-           │
│  mattermost-bot  │
│  (Rust)          │
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│  SQLite          │
│  Database        │
└──────────────────┘
```

### 環境變數

```bash
CONFIG_YAML=/path/to/config.yaml    # 配置檔案路徑
LOG_LEVEL=info                       # 日誌等級
RUST_LOG=leko_mattermost_bot=debug  # Rust 日誌
```

### 配置檔案

```yaml
mattermost:
  url: http://mattermost:8065
  bot_token: xxx
  slash_command_token: yyy
  bot_callback_url: http://bot:3000

admin:
  - user_id_1
  - user_id_2

stickers:
  categories:
    - name: Category
      sources:
        - type: file
          format: csv
          path: data/stickers.csv
```

---

## 開發工作流

### 新增功能

1. **設計階段**
   - 定義 API 介面
   - 設計資料結構
   - 規劃測試策略

2. **實作階段**
   - 寫測試 (TDD)
   - 實作功能
   - 確保測試通過

3. **重構階段**
   - 提取共用邏輯
   - 改善可讀性
   - 更新文件

4. **驗證階段**
   - 執行所有測試
   - 手動測試
   - Code review

### 命令

```bash
# 開發
cargo run

# 測試
cargo test
cargo test -- --nocapture

# 建置
cargo build --release

# 檢查
cargo clippy
cargo fmt

# 文件
cargo doc --open

# 統計
./stats.sh
```

---

## 故障排除

### 常見問題

**Q: WebSocket 連線失敗**
- 檢查 Mattermost URL 是否正確
- 確認 bot_token 有效
- 查看網路連線

**Q: 資料庫鎖定**
- WAL 模式已啟用
- 檢查是否有長時間事務
- 考慮增加 busy_timeout

**Q: 測試失敗**
- 檢查 sqlx offline mode
- 執行 `cargo run --bin sqlx_prepare`
- 確認測試隔離性

---

## 參考資源

- **Mattermost API**: https://api.mattermost.com/
- **sqlx**: https://github.com/launchbadge/sqlx
- **warp**: https://github.com/seanmonstar/warp
- **tokio**: https://tokio.rs/

---

**文件版本**: 1.0  
**最後更新**: 2026-02-06  
**維護者**: leko
