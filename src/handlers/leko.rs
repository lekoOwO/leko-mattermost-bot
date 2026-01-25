//! `/leko` 指令處理

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;
use warp::http::StatusCode;

use super::auth::verify_slash_command_token;
use super::group_buy::handle_group_buy_command;
use super::sticker::handle_sticker_command_impl;
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

    let text = form.get("text").cloned().unwrap_or_default();
    let text_trimmed = text.trim();

    // 解析子指令
    let parts: Vec<&str> = text_trimmed.split_whitespace().collect();
    let subcommand = parts.first().copied().unwrap_or("");

    match subcommand {
        "" => {
            // 無參數，顯示 help
            Ok(warp::reply::with_status(handle_leko_help(), StatusCode::OK))
        }
        "help" => {
            // 顯示 help
            Ok(warp::reply::with_status(handle_leko_help(), StatusCode::OK))
        }
        "group_buy" => {
            // 團購功能
            handle_group_buy_command(form, state).await
        }
        "sticker" => {
            // 取得 sticker 後面的關鍵字
            let keyword = parts.get(1..).map(|s| s.join(" ")).unwrap_or_default();
            // 建立新的 form，將 text 替換成關鍵字
            let mut sticker_form = form.clone();
            sticker_form.insert("text".to_string(), keyword);
            let response = handle_sticker_command_impl(sticker_form, state).await?;
            Ok(warp::reply::with_status(response, StatusCode::OK))
        }
        _ => {
            // 未知的子指令，顯示 help
            Ok(warp::reply::with_status(handle_leko_help(), StatusCode::OK))
        }
    }
}

/// 處理 /leko help - 顯示使用說明
fn handle_leko_help() -> warp::reply::Json {
    info!("顯示 /leko 使用說明");
    warp::reply::json(&serde_json::json!({
        "response_type": "ephemeral",
        "text": "### 📚 `/leko` 指令使用說明\n\n**可用子指令：**\n\n- `/leko help` - 顯示此說明訊息\n- `/leko group_buy` - 開啟建立團購對話框\n- `/leko sticker [關鍵字]` - 搜尋並發送貼圖\n\n**範例：**\n```\n/leko group_buy\n/leko sticker 快樂\n/leko sticker\n```\n\n💡 提示：你也可以直接使用 `/group_buy` 或 `/sticker` 指令。"
    }))
}
