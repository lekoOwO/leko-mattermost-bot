//! 認證相關功能

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

use crate::AppState;

/// 自訂錯誤類型：未授權
#[derive(Debug)]
pub struct UnauthorizedError;
impl warp::reject::Reject for UnauthorizedError {}

/// 驗證 slash command token
pub async fn verify_slash_command_token(
    form: &std::collections::HashMap<String, String>,
    state: &Arc<RwLock<AppState>>,
    command: &str,
) -> Result<(), warp::Rejection> {
    let app_state = state.read().await;

    // 根據命令名稱選擇對應的 token
    let expected_token = match command {
        "group_buy" => &app_state.config.mattermost.slash_command_tokens.group_buy,
        "leko" => &app_state.config.mattermost.slash_command_tokens.leko,
        "stickers" => &app_state.config.mattermost.slash_command_tokens.stickers,
        _ => {
            info!("未知命令: {}，跳過驗證", command);
            return Ok(());
        }
    };

    if let Some(expected_token) = expected_token {
        if let Some(received_token) = form.get("token") {
            if received_token != expected_token {
                error!(
                    "無效的 {} slash command token: 收到 '{}', 期望 '{}'",
                    command,
                    &received_token[..8.min(received_token.len())],
                    &expected_token[..8.min(expected_token.len())]
                );
                drop(app_state);
                return Err(warp::reject::custom(UnauthorizedError));
            } else {
                info!("{} Token 驗證成功", command);
            }
        } else {
            error!("請求中缺少 token");
            drop(app_state);
            return Err(warp::reject::custom(UnauthorizedError));
        }
    } else {
        info!("未設定 {} slash_command_token，跳過驗證", command);
    }
    Ok(())
}
