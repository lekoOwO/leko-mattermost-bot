//! Mattermost WebSocket 客戶端

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
struct WebSocketEvent {
    #[serde(rename = "event")]
    #[serde(default)]
    event_type: Option<String>,
    #[serde(default)]
    data: serde_json::Value,
    #[serde(default)]
    #[allow(dead_code)]
    broadcast: serde_json::Value,
    #[serde(default)]
    #[allow(dead_code)]
    seq: u64,
    #[serde(default)]
    #[allow(dead_code)]
    status: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    seq_reply: Option<u64>,
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
#[derive(Debug, Deserialize)]
struct PostedEventData {
    #[serde(default)]
    #[allow(dead_code)]
    channel_display_name: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    channel_name: Option<String>,
    #[serde(default)]
    channel_type: Option<String>,
    #[serde(default)]
    post: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    sender_name: Option<String>,
}

/// Post 資料結構
#[derive(Debug, Deserialize)]
struct PostData {
    #[serde(default)]
    #[allow(dead_code)]
    id: Option<String>,
    #[serde(default)]
    channel_id: Option<String>,
    #[serde(default)]
    user_id: Option<String>,
    #[serde(default)]
    message: Option<String>,
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

        // 等待 5 秒後重新連接
        info!("5 秒後重新連接 WebSocket...");
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
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
        seq: 1,
        action: "authentication_challenge".to_string(),
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

async fn handle_websocket_message(text: &str, state: Arc<RwLock<AppState>>) -> Result<()> {
    let event: WebSocketEvent = match serde_json::from_str(text) {
        Ok(e) => e,
        Err(e) => {
            // 解析失敗時記錄 debug 而非 error，因為可能有未知的事件類型
            debug!("無法解析 WebSocket 事件: {} - 訊息: {}", e, text);
            return Ok(()); // 忽略無法解析的事件
        }
    };

    // 處理認證回應
    if let Some(status) = &event.status {
        if status == "OK" {
            info!("WebSocket 認證成功");
            return Ok(());
        }
    }

    // 如果沒有 event_type，忽略
    let Some(event_type) = &event.event_type else {
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

async fn handle_posted_event(data: &serde_json::Value, state: Arc<RwLock<AppState>>) -> Result<()> {
    // 解析事件資料
    let event_data: PostedEventData =
        serde_json::from_value(data.clone()).context("解析 posted 事件資料失敗")?;

    // 檢查是否為 Direct Message
    let channel_type = event_data.channel_type.as_deref().unwrap_or("");
    if channel_type != "D" {
        return Ok(());
    }

    // 解析 post 資料
    let post_json = event_data.post.as_deref().unwrap_or("{}");
    let post: PostData = serde_json::from_str(post_json).context("解析 post 資料失敗")?;

    let user_id = post.user_id.as_deref().unwrap_or("");
    let channel_id = post.channel_id.as_deref().unwrap_or("");
    let message = post.message.as_deref().unwrap_or("").trim();

    if user_id.is_empty() || channel_id.is_empty() {
        return Ok(());
    }

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

    let response_message = match command {
        "" => {
            // 空訊息，顯示 help
            drop(app_state);
            get_help_message()
        }
        "help" | "幫助" | "?" => {
            // 顯示 help
            drop(app_state);
            get_help_message()
        }
        "ping" => {
            // 測試連線
            drop(app_state);
            "🏓 Pong!".to_string()
        }
        "status" | "狀態" => {
            // 顯示狀態
            let sticker_count = app_state.sticker_database.count();
            let admin_count = app_state.config.admin.len();
            drop(app_state);
            format!(
                "### ℹ️ Bot 狀態\n\n- **貼圖數量**: {} 張\n- **管理員數量**: {} 人\n- **狀態**: 🟢 運行中",
                sticker_count, admin_count
            )
        }
        "reload" => {
            // 重新載入配置
            drop(app_state);
            match handle_reload_config(state.clone()).await {
                Ok(msg) => msg,
                Err(e) => {
                    error!("重新載入配置失敗: {}", e);
                    format!("❌ 重新載入配置失敗: {}", e)
                }
            }
        }
        "sticker" | "stickers" | "貼圖" => {
            // 顯示貼圖統計
            drop(app_state);
            handle_sticker_stats(state.clone()).await
        }
        _ => {
            // 未知指令
            drop(app_state);
            format!("❓ 未知指令: `{}`\n\n輸入 `help` 查看可用指令。", command)
        }
    };

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

/// 生成 help 訊息
fn get_help_message() -> String {
    r#"### 🤖 Bot 管理指令

歡迎使用 Leko's Mattermost Bot 管理功能！

#### 可用指令：

- **`help`** / **`幫助`** / **`?`** - 顯示此說明訊息
- **`ping`** - 測試 bot 連線狀態
- **`status`** / **`狀態`** - 顯示 bot 運行狀態
- **`sticker`** / **`stickers`** / **`貼圖`** - 顯示貼圖庫統計資訊
- **`reload`** - 重新載入配置（貼圖、管理員等）

#### 提示：

- 這些指令只能由管理員在 Direct Message 中使用
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

    // 重新載入貼圖資料庫
    let new_sticker_database =
        crate::sticker::StickerDatabase::load_from_config(&new_config.stickers)
            .await
            .context("載入貼圖資料庫失敗")?;

    let sticker_count = new_sticker_database.count();
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
async fn handle_sticker_stats(state: Arc<RwLock<AppState>>) -> String {
    let app_state = state.read().await;

    // 取得統計資訊
    let total_count = app_state.sticker_database.get_total_count();
    let category_stats = app_state.sticker_database.get_category_stats();

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
