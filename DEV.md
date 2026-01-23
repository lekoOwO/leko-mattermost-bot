# Leko's Mattermost Bot - 開發文檔

## 技術棧

- **語言**: Rust 2024 Edition
- **HTTP 框架**: warp
- **非同步運行時**: tokio
- **HTTP 客戶端**: reqwest (rustls)
- **序列化**: serde, serde_json, serde_yaml
- **CSV 解析**: csv
- **CLI**: clap
- **日誌**: tracing + tracing-subscriber

## 專案架構

```
src/
├── main.rs              # HTTP 伺服器與路由處理
├── config.rs            # YAML 配置管理
├── mattermost.rs        # Mattermost API 客戶端與資料結構
├── sticker.rs           # 貼圖資料庫（支援搜尋、分類）
└── handlers/            # HTTP 請求處理器模組
    ├── mod.rs           # 模組入口與錯誤處理
    ├── auth.rs          # 認證與 token 驗證
    ├── leko.rs          # /leko 指令處理
    ├── sticker.rs       # /sticker 指令處理
    └── actions.rs       # Interactive Message 動作處理
```

### handlers 模組說明

- **mod.rs**: 模組入口，重新導出公開 API 並處理統一的錯誤處理
- **auth.rs**: 負責 slash command token 驗證，防止未授權的請求
- **leko.rs**: 處理 `/leko` 指令及其子指令（help, sticker）
- **sticker.rs**: 處理 `/sticker` 指令，搜尋並顯示貼圖選擇器
- **actions.rs**: 處理 Interactive Message 的回調動作（選擇貼圖、發送、取消）


## 配置系統

配置檔案位於 `data/config.yaml`，可透過以下方式指定：

1. CLI 參數：`-c` 或 `--config`
2. 環境變數：`CONFIG_YAML`
3. 預設路徑：當前目錄的 `config.yaml`

### 配置欄位說明

```yaml
mattermost:
  url: http://mattermost:8065              # Mattermost 伺服器位址
  bot_token: xxxxx                             # Bot Access Token (必填)
  slash_command_token: yyyyy                   # Slash Command Token (選填，建議啟用)
  bot_callback_url: http://bot:3000  # Bot 服務位址 (必填)

stickers:
  categories:
    - name: 海綿寶寶        # 分類名稱
      csv:
        - data/sb.csv      # CSV 檔案路徑
      json:
        - data/sb.json     # JSON 檔案路徑
```

## 資料格式

### CSV 格式

```csv
名稱,圖片
海綿寶寶,https://i.imgur.com/abc123.jpg
```

### JSON 格式

```json
{
  "海螺": "https://i.imgur.com/xyz789.jpg",
  "蟹老闆": "https://i.imgur.com/def456.jpg"
}
```

## 核心功能

### 1. Slash Command 處理

- 路由：`POST /sticker`
- 驗證：檢查 `slash_command_token`（如果配置）
- 功能：搜尋貼圖並開啟 Interactive Dialog

### 2. Interactive Dialog

- 限制：最多顯示 15 個貼圖選項（Mattermost 限制）
- 分類：支援「全部」選項（optional field，預設值 "all"）
- 狀態傳遞：透過 `state` 欄位傳遞使用者資訊

### 3. 身份覆蓋

發送貼圖時使用 `props` 覆蓋身份：

```rust
props: Some(serde_json::json!({
    "override_username": user_name,
    "override_icon_url": format!("{}/api/v4/users/{}/image", url, user_id)
}))
```

需要在 Mattermost System Console 啟用：
- Enable integrations to override usernames
- Enable integrations to override profile picture icons

### 4. 貼圖顯示格式

- Dialog 選項：`[分類] 名稱 (hash前8碼)`
- 發送訊息：`![sticker](圖片URL)`（不顯示名稱）

## 開發指令

### 編譯

```bash
cargo build                 # Debug 版本
cargo build --release       # Release 版本
```

### 測試

```bash
cargo test                  # 執行所有測試
cargo test -- --nocapture   # 顯示測試輸出
```

### 執行

```bash
# 本地開發
RUST_LOG=debug cargo run -- -c data/test.yaml -H 0.0.0.0 -p 3000

# Release 版本
cargo run --release -- -c data/config.yaml -H 0.0.0.0 -p 3000
```

### 格式化

```bash
cargo fmt                   # 格式化程式碼
cargo fmt --check           # 檢查格式（CI 使用）
```

### Linting

```bash
cargo clippy                # 程式碼檢查
```

## CI/CD

GitHub Actions 自動化流程（`.github/workflows/ci.yml`）：

### Check Job
- 格式檢查 (`cargo fmt`)
- 測試執行 (`cargo test`)

### Build Job
多平台編譯：
- Linux x86_64 musl
- Linux aarch64 musl
- Windows x86_64
- macOS x86_64
- macOS aarch64 (Apple Silicon)

### Release Job
- **Nightly Release**: 每次 push 到 main 自動建立
- **Stable Release**: 推送 `v*` tag 觸發

### Docker Job
- 自動建置並推送到 GitHub Container Registry
- 支援平台：linux/amd64, linux/arm64
- 標籤策略：semver, sha, branch, pr

## 測試策略

### 單元測試

- `config.rs`: 配置載入測試
- `sticker.rs`: 貼圖搜尋、分類測試
- `mattermost.rs`: 暫無（API 客戶端通常用整合測試）

### 手動測試

```bash
# 測試 Slash Command
curl -X POST http://localhost:3000/sticker \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "token=your-token&text=關鍵字&user_name=test&user_id=123&channel_id=abc&trigger_id=xyz"
```

## 常見問題

### Dialog 不顯示

檢查：
1. `bot_callback_url` 是否正確指向 bot 服務
2. Mattermost 能否連接到 bot 服務（防火牆、網路）
3. 查看 bot 日誌：`RUST_LOG=debug cargo run`

### 身份覆蓋無效

檢查 Mattermost System Console：
- Integrations > Integration Management
- 啟用 "Enable integrations to override usernames"
- 啟用 "Enable integrations to override profile picture icons"

### Token 驗證失敗

確認 `slash_command_token` 與 Mattermost Slash Command 設定的 Token 一致。

## 參考資源

- [Mattermost API Documentation](https://api.mattermost.com/)
- [Mattermost Interactive Dialogs](https://developers.mattermost.com/integrate/plugins/interactive-dialogs/)
- [Rust Book](https://doc.rust-lang.org/book/)
- [Tokio Documentation](https://tokio.rs/)
- [Warp Documentation](https://docs.rs/warp/)