//! Direct Message 處理

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::AppState;
use crate::mattermost::{Post, WebhookPost};

/// 處理 Direct Message webhook
pub async fn handle_dm_webhook(
    webhook_post: WebhookPost,
    state: Arc<RwLock<AppState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    info!("收到 DM webhook: {:?}", webhook_post);

    // 驗證是否為 Direct Message
    let channel_type = webhook_post.channel_type.as_deref().unwrap_or("");
    if channel_type != "D" {
        info!("非 DM 訊息，忽略");
        return Ok(warp::reply::json(&serde_json::json!({
            "status": "ignored"
        })));
    }

    let user_id = webhook_post.user_id.as_deref().unwrap_or("");
    let user_name = webhook_post.user_name.as_deref().unwrap_or("");
    let channel_id = webhook_post.channel_id.as_deref().unwrap_or("");
    let text = webhook_post.text.as_deref().unwrap_or("").trim();

    if user_id.is_empty() || channel_id.is_empty() {
        error!("webhook 資料不完整");
        return Ok(warp::reply::json(&serde_json::json!({
            "status": "error",
            "message": "Invalid webhook data"
        })));
    }

    // 檢查是否為管理員
    let app_state = state.read().await;
    if !app_state.config.is_admin(user_id, user_name) {
        warn!("非管理員嘗試使用 DM: {} ({})", user_name, user_id);
        
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
        
        drop(app_state);
        return Ok(warp::reply::json(&serde_json::json!({
            "status": "unauthorized"
        })));
    }

    info!("管理員 {} ({}) 發送 DM: '{}'", user_name, user_id, text);

    // 解析指令
    let parts: Vec<&str> = text.split_whitespace().collect();
    let command = parts.first().copied().unwrap_or("");

    let response_message = match command {
        "" => {
            // 空訊息，顯示 help
            get_help_message()
        }
        "help" | "幫助" | "?" => {
            // 顯示 help
            get_help_message()
        }
        "ping" => {
            // 測試連線
            "🏓 Pong!".to_string()
        }
        "status" | "狀態" => {
            // 顯示狀態
            let sticker_count = app_state.sticker_database.count();
            format!(
                "### ℹ️ Bot 狀態\n\n- **貼圖數量**: {} 張\n- **管理員數量**: {} 人\n- **狀態**: 🟢 運行中",
                sticker_count,
                app_state.config.admin.len()
            )
        }
        _ => {
            // 未知指令
            format!(
                "❓ 未知指令: `{}`\n\n輸入 `help` 查看可用指令。",
                command
            )
        }
    };

    // 發送回應
    let post = Post {
        id: None,
        channel_id: channel_id.to_string(),
        message: response_message,
        root_id: None,
        props: None,
    };

    if let Err(e) = app_state.mattermost_client.create_post(&post).await {
        error!("發送回應訊息失敗: {}", e);
        drop(app_state);
        return Ok(warp::reply::json(&serde_json::json!({
            "status": "error",
            "message": "Failed to send response"
        })));
    }

    drop(app_state);

    Ok(warp::reply::json(&serde_json::json!({
        "status": "ok"
    })))
}

/// 生成 help 訊息
fn get_help_message() -> String {
    r#"### 🤖 Bot 管理指令

歡迎使用 Leko's Mattermost Bot 管理功能！

#### 可用指令：

- **`help`** / **`幫助`** / **`?`** - 顯示此說明訊息
- **`ping`** - 測試 bot 連線狀態
- **`status`** / **`狀態`** - 顯示 bot 運行狀態

#### 提示：

- 這些指令只能由管理員在 Direct Message 中使用
- 更多功能正在開發中...

---
💡 如需協助，請聯繫系統管理員。
"#
    .to_string()
}
