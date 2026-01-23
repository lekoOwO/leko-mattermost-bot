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
) -> Result<(), warp::Rejection> {
    let app_state = state.read().await;
    if let Some(expected_token) = &app_state.config.mattermost.slash_command_token {
        if let Some(received_token) = form.get("token") {
            if received_token != expected_token {
                error!(
                    "無效的 slash command token: 收到 '{}', 期望 '{}'",
                    &received_token[..8.min(received_token.len())],
                    &expected_token[..8.min(expected_token.len())]
                );
                drop(app_state);
                return Err(warp::reject::custom(UnauthorizedError));
            } else {
                info!("Token 驗證成功");
            }
        } else {
            error!("請求中缺少 token");
            drop(app_state);
            return Err(warp::reject::custom(UnauthorizedError));
        }
    } else {
        info!("未設定 slash_command_token，跳過驗證");
    }
    Ok(())
}
