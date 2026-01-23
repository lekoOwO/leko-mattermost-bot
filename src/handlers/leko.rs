//! `/leko` 指令處理

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::AppState;
use super::auth::verify_slash_command_token;
use super::sticker::handle_sticker_command_impl;

/// 處理 /leko slash command
pub async fn handle_leko_command(
    form: std::collections::HashMap<String, String>,
    state: Arc<RwLock<AppState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    info!("收到 /leko 指令");
    info!("請求參數: {:?}", form.keys().collect::<Vec<_>>());
    info!("完整表單內容: {:?}", form);

    // 驗證 slash command token
    verify_slash_command_token(&form, &state).await?;

    let text = form.get("text").cloned().unwrap_or_default();
    let text_trimmed = text.trim();

    // 解析子指令
    let parts: Vec<&str> = text_trimmed.split_whitespace().collect();
    let subcommand = parts.first().copied().unwrap_or("");

    match subcommand {
        "" => {
            // 無參數，顯示 help
            Ok(handle_leko_help())
        }
        "help" => {
            // 顯示 help
            Ok(handle_leko_help())
        }
        "sticker" => {
            // 取得 sticker 後面的關鍵字
            let keyword = parts.get(1..).map(|s| s.join(" ")).unwrap_or_default();
            // 建立新的 form，將 text 替換成關鍵字
            let mut sticker_form = form.clone();
            sticker_form.insert("text".to_string(), keyword);
            handle_sticker_command_impl(sticker_form, state).await
        }
        _ => {
            // 未知的子指令，顯示 help
            Ok(handle_leko_help())
        }
    }
}

/// 處理 /leko help - 顯示使用說明
fn handle_leko_help() -> warp::reply::Json {
    info!("顯示 /leko 使用說明");
    warp::reply::json(&serde_json::json!({
        "response_type": "ephemeral",
        "text": "### 📚 `/leko` 指令使用說明\n\n**可用子指令：**\n\n- `/leko help` - 顯示此說明訊息\n- `/leko sticker [關鍵字]` - 搜尋並發送貼圖\n\n**範例：**\n```\n/leko sticker 快樂\n/leko sticker\n```\n\n💡 提示：你也可以直接使用 `/sticker` 指令來搜尋貼圖。"
    }))
}
