//! Mattermost WebSocket 客戶端

use crate::constants::websocket::{RECONNECT_DELAY, AUTH_SEQUENCE, AUTH_ACTION};
use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};

use crate::AppState;
use crate::mattermost::Post;

/// WebSocket 事件類型
#[derive(Debug, Deserialize)]
pub struct WebSocketEvent {
    #[serde(rename = "event")]
    #[serde(default)]
    pub event_type: Option<String>,
    #[serde(default)]
    pub data: serde_json::Value,
    #[serde(default)]
    #[allow(dead_code)]
    pub broadcast: serde_json::Value,
    #[serde(default)]
    #[allow(dead_code)]
    pub seq: u64,
    #[serde(default)]
    #[allow(dead_code)]
    pub status: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub seq_reply: Option<u64>,
}

/// WebSocket 認證請求
#[derive(Debug, Serialize)]
struct AuthChallenge {
    seq: u64,
    action: String,
    data: AuthData,
}

#[derive(Debug, Serialize)]
struct AuthData {
    token: String,
}

/// Posted 事件資料
#[derive(Debug, Deserialize, Clone)]
pub struct PostedEventData {
    #[serde(default)]
    #[allow(dead_code)]
    pub channel_display_name: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub channel_name: Option<String>,
    #[serde(default)]
    pub channel_type: Option<String>,
    #[serde(default)]
    pub post: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub sender_name: Option<String>,
}

/// Post 資料結構
#[derive(Debug, Deserialize, Clone)]
pub struct PostData {
    #[serde(default)]
    #[allow(dead_code)]
    pub id: Option<String>,
    #[serde(default)]
    pub channel_id: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

/// 啟動 WebSocket 客戶端
pub async fn start_websocket(state: Arc<RwLock<AppState>>) -> Result<()> {
    let app_state = state.read().await;
    let base_url = app_state.config.mattermost.url.clone();
    let bot_token = app_state.config.mattermost.bot_token.clone();
    drop(app_state);

    // 將 http/https 轉換為 ws/wss
    let ws_url = base_url
        .replace("https://", "wss://")
        .replace("http://", "ws://");
    let ws_url = format!("{}/api/v4/websocket", ws_url);

    info!("正在連接到 Mattermost WebSocket: {}", ws_url);

    loop {
        match connect_and_handle(&ws_url, &bot_token, state.clone()).await {
            Ok(_) => {
                info!("WebSocket 連接正常關閉");
            }
            Err(e) => {
                error!("WebSocket 錯誤: {}", e);
            }
        }

        // 等待指定時間後重新連接
        info!("{} 秒後重新連接 WebSocket...", RECONNECT_DELAY.as_secs());
        tokio::time::sleep(RECONNECT_DELAY).await;
    }
}

async fn connect_and_handle(
    ws_url: &str,
    bot_token: &str,
    state: Arc<RwLock<AppState>>,
) -> Result<()> {
    let (ws_stream, _) = connect_async(ws_url).await.context("WebSocket 連接失敗")?;

    info!("WebSocket 連接成功");

    let (mut write, mut read) = ws_stream.split();

    // 發送認證請求
    let auth = AuthChallenge {
        seq: AUTH_SEQUENCE,
        action: AUTH_ACTION.to_string(),
        data: AuthData {
            token: bot_token.to_string(),
        },
    };

    let auth_msg = serde_json::to_string(&auth)?;
    write
        .send(Message::Text(auth_msg))
        .await
        .context("發送認證訊息失敗")?;

    info!("已發送 WebSocket 認證請求");

    // 處理接收到的訊息
    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                debug!("收到 WebSocket 訊息: {}", text);
                if let Err(e) = handle_websocket_message(&text, state.clone()).await {
                    // 只在 debug 模式記錄完整錯誤，避免日誌過多
                    debug!("處理 WebSocket 訊息失敗: {} - 原始訊息: {}", e, text);
                }
            }
            Ok(Message::Close(_)) => {
                info!("WebSocket 連接被關閉");
                break;
            }
            Ok(Message::Ping(data)) => {
                if let Err(e) = write.send(Message::Pong(data)).await {
                    error!("發送 Pong 失敗: {}", e);
                    break;
                }
            }
            Ok(_) => {
                // 忽略其他類型的訊息
            }
            Err(e) => {
                error!("WebSocket 訊息錯誤: {}", e);
                break;
            }
        }
    }

    Ok(())
}

/// 解析 WebSocket 事件（純函數，可測試）
pub fn parse_websocket_event(text: &str) -> Result<Option<WebSocketEvent>> {
    match serde_json::from_str(text) {
        Ok(event) => Ok(Some(event)),
        Err(e) => {
            debug!("無法解析 WebSocket 事件: {} - 訊息: {}", e, text);
            Ok(None) // 忽略無法解析的事件
        }
    }
}

/// 判斷事件類型（純函數，可測試）
pub fn should_process_event(event: &WebSocketEvent) -> Option<String> {
    // 處理認證回應
    if let Some(status) = &event.status {
        if status == "OK" {
            info!("WebSocket 認證成功");
            return None;
        }
    }

    // 如果沒有 event_type，忽略
    event.event_type.clone()
}

async fn handle_websocket_message(text: &str, state: Arc<RwLock<AppState>>) -> Result<()> {
    let Some(event) = parse_websocket_event(text)? else {
        return Ok(());
    };

    let Some(event_type) = should_process_event(&event) else {
        return Ok(());
    };

    match event_type.as_str() {
        "hello" => {
            info!("收到 WebSocket hello 事件");
        }
        "posted" => {
            handle_posted_event(&event.data, state).await?;
        }
        "status_change" | "typing" | "user_updated" => {
            // 忽略這些常見事件
        }
        _ => {
            // 記錄未知事件類型
            debug!("收到未知 WebSocket 事件: {}", event_type);
        }
    }

    Ok(())
}

/// 解析 posted 事件資料（純函數，可測試）
pub fn parse_posted_event_data(data: &serde_json::Value) -> Result<Option<PostedEventData>> {
    let event_data: PostedEventData = serde_json::from_value(data.clone())
        .context("解析 posted 事件資料失敗")?;
    
    // 檢查是否為 Direct Message
    let channel_type = event_data.channel_type.as_deref().unwrap_or("");
    if channel_type != "D" {
        return Ok(None);
    }
    
    Ok(Some(event_data))
}

/// 解析 post 資料（純函數，可測試）
pub fn parse_post_data(event_data: &PostedEventData) -> Result<Option<PostData>> {
    let post_json = event_data.post.as_deref().unwrap_or("{}");
    let post: PostData = serde_json::from_str(post_json)
        .context("解析 post 資料失敗")?;
    
    let user_id = post.user_id.as_deref().unwrap_or("");
    let channel_id = post.channel_id.as_deref().unwrap_or("");
    
    if user_id.is_empty() || channel_id.is_empty() {
        return Ok(None);
    }
    
    Ok(Some(post))
}

async fn handle_posted_event(data: &serde_json::Value, state: Arc<RwLock<AppState>>) -> Result<()> {
    // 解析事件資料
    let Some(event_data) = parse_posted_event_data(data)? else {
        return Ok(());
    };

    // 解析 post 資料
    let Some(post) = parse_post_data(&event_data)? else {
        return Ok(());
    };

    let user_id = post.user_id.as_deref().unwrap();
    let channel_id = post.channel_id.as_deref().unwrap();
    let message = post.message.as_deref().unwrap_or("").trim();

    // 獲取 bot 自己的 user_id（避免回應自己的訊息）
    let app_state = state.read().await;

    // 如果是 bot 自己的訊息，忽略
    if user_id == app_state.bot_user_id {
        return Ok(());
    }

    // 獲取使用者資訊
    let user = match app_state.mattermost_client.get_user(user_id).await {
        Ok(u) => u,
        Err(e) => {
            warn!("無法獲取使用者資訊: {}", e);
            return Ok(());
        }
    };

    let username = user.username.clone();

    // 檢查是否為管理員
    if !app_state.config.is_admin(user_id, &username) {
        warn!("非管理員嘗試使用 DM: {} ({})", username, user_id);

        // 發送警告訊息
        let post = Post {
            id: None,
            channel_id: channel_id.to_string(),
            message: "⚠️ 您沒有使用此功能的權限。".to_string(),
            root_id: None,
            props: None,
        };

        if let Err(e) = app_state.mattermost_client.create_post(&post).await {
            error!("發送警告訊息失敗: {}", e);
        }

        return Ok(());
    }

    info!("管理員 {} ({}) 發送 DM: '{}'", username, user_id, message);

    // 解析指令
    let parts: Vec<&str> = message.split_whitespace().collect();
    let command = parts.first().copied().unwrap_or("");

    let response_message = handle_admin_command(command, state.clone()).await;

    // 重新獲取 app_state 來發送回應
    let app_state = state.read().await;

    // 發送回應
    let response_post = Post {
        id: None,
        channel_id: channel_id.to_string(),
        message: response_message,
        root_id: None,
        props: None,
    };

    if let Err(e) = app_state
        .mattermost_client
        .create_post(&response_post)
        .await
    {
        error!("發送回應訊息失敗: {}", e);
    }

    Ok(())
}

/// 處理管理員命令（可在 DM 或 slash command 中使用）
pub async fn handle_admin_command(command: &str, state: Arc<RwLock<AppState>>) -> String {
    match command {
        "" | "help" | "幫助" | "?" => get_help_message(),
        "ping" => "🏓 Pong!".to_string(),
        "status" | "狀態" => {
            let app_state = state.read().await;
            let sticker_db = app_state.sticker_database.clone();
            let admin_count = app_state.config.admin.len();
            drop(app_state);
            let sticker_count = match sticker_db.count().await {
                Ok(c) => c,
                Err(e) => {
                    warn!("無法取得貼圖數量: {}", e);
                    0
                }
            };

            format!(
                "### ℹ️ Bot 狀態\n\n- **貼圖數量**: {} 張\n- **管理員數量**: {} 人\n- **狀態**: 🟢 運行中",
                sticker_count, admin_count
            )
        }
        "reload" => match handle_reload_config(state.clone()).await {
            Ok(msg) => msg,
            Err(e) => {
                error!("重新載入配置失敗: {}", e);
                format!("❌ 重新載入配置失敗: {}", e)
            }
        },
        "sticker" | "stickers" | "貼圖" => handle_sticker_stats(state.clone()).await,
        _ => format!("❓ 未知指令: `{}`\n\n輸入 `help` 查看可用指令。", command),
    }
}

/// 生成 help 訊息
pub fn get_help_message() -> String {
    r#"### 🤖 Bot 管理指令

歡迎使用 Leko's Mattermost Bot 管理功能！

#### 可用指令：

- **`help`** / **`幫助`** / **`?`** - 顯示此說明訊息
- **`ping`** - 測試 bot 連線狀態
- **`status`** / **`狀態`** - 顯示 bot 運行狀態
- **`sticker`** / **`stickers`** / **`貼圖`** - 顯示貼圖庫統計資訊
- **`reload`** - 重新載入配置（貼圖、管理員等）

#### 提示：

- 這些指令只能由管理員使用
- 可在 Direct Message 中使用，或透過 `/leko admin <指令>` 使用
- `reload` 指令會重新讀取配置檔案，但不會影響 Mattermost 連線
- 更多功能正在開發中...

---
💡 如需協助，請聯繫系統管理員。
"#
    .to_string()
}

/// 處理重新載入配置
async fn handle_reload_config(state: Arc<RwLock<AppState>>) -> Result<String> {
    info!("開始重新載入配置...");

    let mut app_state = state.write().await;

    // 讀取配置文件路徑
    let config_path = app_state.config_path.clone();

    // 重新載入配置
    let new_config = crate::config::Config::from_path(&config_path).context("讀取配置檔案失敗")?;

    info!("配置檔案讀取成功");

    // 重新載入貼圖資料庫 into existing SQLite database
    let new_sticker_database = crate::sticker::StickerDatabase::load_from_config(
        &app_state.database,
        &new_config.stickers,
    )
    .await
    .context("載入貼圖資料庫失敗")?;

    let sticker_count = match new_sticker_database.count().await {
        Ok(c) => c,
        Err(e) => {
            warn!("無法取得貼圖數量: {}", e);
            0
        }
    };
    info!("貼圖資料庫重新載入成功，共 {} 張貼圖", sticker_count);

    // 更新 admin 列表
    let admin_count = new_config.admin.len();
    if !new_config.admin.is_empty() {
        info!("管理員列表已更新: {:?}", new_config.admin);
    } else {
        info!("未設定管理員");
    }

    // 更新狀態（保留 mattermost_client 和 bot_user_id）
    app_state.config.stickers = new_config.stickers;
    app_state.config.admin = new_config.admin;
    app_state.sticker_database = new_sticker_database;

    info!("配置重新載入完成");

    Ok(format!(
        "### ✅ 配置重新載入成功\n\n- **貼圖數量**: {} 張\n- **管理員數量**: {} 人\n- **配置檔案**: `{}`",
        sticker_count,
        admin_count,
        config_path.display()
    ))
}

/// 處理貼圖統計資訊
pub async fn handle_sticker_stats(state: Arc<RwLock<AppState>>) -> String {
    let app_state = state.read().await;
    let sticker_db = app_state.sticker_database.clone();
    drop(app_state);

    // 取得統計資訊
    let total_count = match sticker_db.get_total_count().await {
        Ok(c) => c,
        Err(e) => {
            warn!("無法取得貼圖總數: {}", e);
            0
        }
    };

    let category_stats = match sticker_db.get_category_stats().await {
        Ok(m) => m,
        Err(e) => {
            warn!("無法取得分類統計: {}", e);
            std::collections::HashMap::new()
        }
    };

    // 排序分類名稱
    let mut categories: Vec<_> = category_stats.iter().collect();
    categories.sort_by(|a, b| a.0.cmp(b.0));

    // 建立訊息
    let mut message = String::from("### 📊 貼圖庫統計\n\n");
    message.push_str(&format!("**總計**: {} 張貼圖\n\n", total_count));

    if categories.is_empty() {
        message.push_str("⚠️ 目前沒有任何貼圖資料。\n");
    } else {
        message.push_str("#### 各分類貼圖數量：\n\n");
        for (category, count) in categories {
            message.push_str(&format!("- **{}**: {} 張\n", category, count));
        }
    }

    message
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_websocket_event_valid() {
        let json = r#"{"event":"posted","data":{},"seq":1}"#;
        let result = parse_websocket_event(json).unwrap();
        assert!(result.is_some());
        let event = result.unwrap();
        assert_eq!(event.event_type, Some("posted".to_string()));
    }

    #[test]
    fn test_parse_websocket_event_invalid() {
        let json = r#"invalid json"#;
        let result = parse_websocket_event(json).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_should_process_event_auth_ok() {
        let event = WebSocketEvent {
            event_type: None,
            data: json!({}),
            broadcast: json!({}),
            seq: 1,
            status: Some("OK".to_string()),
            seq_reply: None,
        };
        let result = should_process_event(&event);
        assert!(result.is_none()); // 認證成功事件不需處理
    }

    #[test]
    fn test_should_process_event_posted() {
        let event = WebSocketEvent {
            event_type: Some("posted".to_string()),
            data: json!({}),
            broadcast: json!({}),
            seq: 1,
            status: None,
            seq_reply: None,
        };
        let result = should_process_event(&event);
        assert_eq!(result, Some("posted".to_string()));
    }

    #[test]
    fn test_parse_posted_event_data_direct_message() {
        let data = json!({
            "channel_type": "D",
            "post": r#"{"user_id":"user1","channel_id":"ch1","message":"test"}"#
        });
        let result = parse_posted_event_data(&data).unwrap();
        assert!(result.is_some());
        let event_data = result.unwrap();
        assert_eq!(event_data.channel_type, Some("D".to_string()));
    }

    #[test]
    fn test_parse_posted_event_data_not_dm() {
        let data = json!({
            "channel_type": "O",
            "post": r#"{"user_id":"user1","channel_id":"ch1","message":"test"}"#
        });
        let result = parse_posted_event_data(&data).unwrap();
        assert!(result.is_none()); // 不是 DM，應該回傳 None
    }

    #[test]
    fn test_parse_post_data_valid() {
        let event_data = PostedEventData {
            channel_display_name: None,
            channel_name: None,
            channel_type: Some("D".to_string()),
            post: Some(r#"{"user_id":"user1","channel_id":"ch1","message":"test"}"#.to_string()),
            sender_name: None,
        };
        let result = parse_post_data(&event_data).unwrap();
        assert!(result.is_some());
        let post = result.unwrap();
        assert_eq!(post.user_id, Some("user1".to_string()));
        assert_eq!(post.channel_id, Some("ch1".to_string()));
    }

    #[test]
    fn test_parse_post_data_empty_fields() {
        let event_data = PostedEventData {
            channel_display_name: None,
            channel_name: None,
            channel_type: Some("D".to_string()),
            post: Some(r#"{"user_id":"","channel_id":"","message":""}"#.to_string()),
            sender_name: None,
        };
        let result = parse_post_data(&event_data).unwrap();
        assert!(result.is_none()); // user_id 或 channel_id 為空應回傳 None
    }

    #[test]
    fn test_get_help_message() {
        let help = get_help_message();
        assert!(help.contains("Bot 管理指令"));
        assert!(help.contains("help"));
        assert!(help.contains("ping"));
        assert!(help.contains("status"));
    }
}
