//! `/leko` 指令處理

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};
use warp::http::StatusCode;

use super::auth::verify_slash_command_token;
use super::group_buy::handle_group_buy_command_impl;
use super::reply_helpers::{ephemeral_json_with_status, get_form_field, IconConfig};
use super::sticker::handle_sticker_command_impl;
use crate::websocket;
use crate::AppState;

/// 處理 /leko slash command
pub async fn handle_leko_command(
    form: std::collections::HashMap<String, String>,
    state: Arc<RwLock<AppState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    info!("收到 /leko 指令");
    info!("請求參數: {:?}", form.keys().collect::<Vec<_>>());
    info!("完整表單內容: {:?}", form);

    // 驗證 slash command token
    verify_slash_command_token(&form, &state, "leko").await?;

    let text = get_form_field(&form, "text");
    let text_trimmed = text.trim();
    let parts: Vec<&str> = text_trimmed.split_whitespace().collect();
    let subcommand = parts.first().copied().unwrap_or("");

    match subcommand {
        "" | "help" => {
            Ok(warp::reply::with_status(handle_leko_help(state).await, StatusCode::OK))
        }
        "admin" => {
            handle_admin_subcommand(&parts, form, state).await
        }
        "group_buy" => {
            handle_group_buy_command_impl(form, state).await
        }
        "sticker" => {
            let keyword = parts.get(1..).map(|s| s.join(" ")).unwrap_or_default();
            let mut sticker_form = form.clone();
            sticker_form.insert("text".to_string(), keyword);
            let response = handle_sticker_command_impl(sticker_form, state).await?;
            Ok(warp::reply::with_status(response, StatusCode::OK))
        }
        _ => {
            Ok(warp::reply::with_status(handle_leko_help(state).await, StatusCode::OK))
        }
    }
}

/// 顯示使用說明
async fn handle_leko_help(state: Arc<RwLock<AppState>>) -> warp::reply::Json {
    info!("顯示 /leko 使用說明");
    
    let app_state = state.read().await;
    let icon = IconConfig::from_config(&app_state.config);
    drop(app_state);
    
    let mut response = serde_json::json!({
        "response_type": "ephemeral",
        "text": "### 📚 `/leko` 指令使用說明\n\n**可用子指令：**\n\n- `/leko help` - 顯示此說明訊息\n- `/leko admin [指令]` - Bot 管理功能（僅管理員）\n- `/leko group_buy` - 開啟建立團購對話框\n- `/leko sticker [關鍵字]` - 搜尋並發送貼圖\n\n**範例：**\n```\n/leko admin help\n/leko admin status\n/leko group_buy\n/leko sticker 快樂\n/leko sticker\n```\n\n💡 提示：你也可以直接使用 `/group_buy` 或 `/sticker` 指令。"
    });
    
    if let Some(icon_config) = icon {
        match icon_config {
            IconConfig::Url(url) => {
                response["icon_url"] = serde_json::json!(url);
            }
            IconConfig::Emoji(emoji) => {
                response["icon_emoji"] = serde_json::json!(emoji);
            }
        }
    }
    
    warp::reply::json(&response)
}

/// 處理 /leko admin 子指令
async fn handle_admin_subcommand(
    parts: &[&str],
    form: std::collections::HashMap<String, String>,
    state: Arc<RwLock<AppState>>,
) -> Result<warp::reply::WithStatus<warp::reply::Json>, warp::Rejection> {
    let user_id = get_form_field(&form, "user_id");
    let user_name = get_form_field(&form, "user_name");

    if user_id.is_empty() {
        let app_state = state.read().await;
        let icon = IconConfig::from_config(&app_state.config);
        drop(app_state);
        return Ok(ephemeral_json_with_status("❌ 無法取得使用者資訊", icon));
    }

    let app_state = state.read().await;
    let is_admin = app_state.config.is_admin(&user_id, &user_name);
    let icon = IconConfig::from_config(&app_state.config);
    drop(app_state);

    if !is_admin {
        warn!(
            "非管理員嘗試使用 admin 指令: {} ({})",
            user_name, user_id
        );
        return Ok(ephemeral_json_with_status(
            "⚠️ 您沒有使用此功能的權限。",
            icon,
        ));
    }

    info!("管理員 {} ({}) 使用 admin 指令", user_name, user_id);

    let admin_command = parts.get(1).copied().unwrap_or("");
    let response_text = websocket::handle_admin_command(admin_command, state.clone()).await;

    let app_state = state.read().await;
    let icon = IconConfig::from_config(&app_state.config);
    drop(app_state);

    Ok(ephemeral_json_with_status(response_text, icon))
}

#[cfg(test)]
mod tests {
    // 注意：handle_leko_help 現在是 async 函數，需要使用 async runtime 來測試
    // #[tokio::test]
    // async fn test_handle_leko_help() {
    //     // 需要建立一個測試用的 AppState
    // }

    #[test]
    fn test_parse_subcommand() {
        let test_cases = vec![
            ("", ""),
            ("help", "help"),
            ("admin status", "admin"),
            ("group_buy", "group_buy"),
            ("sticker 快樂", "sticker"),
            ("  sticker  ", "sticker"),
        ];

        for (input, expected) in test_cases {
            let parts: Vec<&str> = input.trim().split_whitespace().collect();
            let subcommand = parts.first().copied().unwrap_or("");
            assert_eq!(subcommand, expected, "Failed for input: '{}'", input);
        }
    }
}

