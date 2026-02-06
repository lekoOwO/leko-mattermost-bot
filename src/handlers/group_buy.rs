use anyhow::Result;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};
use warp::http::StatusCode;
use warp::reply::{Json, WithStatus};

use super::auth::verify_slash_command_token;
use crate::AppState;
use crate::database::{GroupBuy, GroupBuyOrder, GroupBuyStatus};
use crate::mattermost::{DialogElement, DialogElementType, DialogOption};

mod messages;
pub use messages::{
    generate_action_buttons, generate_group_buy_message, generate_group_buy_message_with_orders,
};
mod actions;
mod dialogs;
mod utils;
#[cfg(test)]
mod tests;
pub use actions::handle_group_buy_action;
pub use dialogs::{
    handle_adjust_shortage_dialog, handle_cancel_register_dialog, handle_create_dialog,
    handle_edit_items_dialog, handle_register_dialog,
};

/// Slash command 參數
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct SlashCommandRequest {
    pub token: Option<String>,
    pub team_id: String,
    pub team_domain: String,
    pub channel_id: String,
    pub channel_name: String,
    pub user_id: String,
    pub user_name: String,
    pub command: String,
    pub text: String,
    pub response_url: String,
    pub trigger_id: String,
}

/// Slash command 回應
#[derive(Debug, Serialize)]
pub struct SlashCommandResponse {
    pub response_type: String,
    pub text: String,
}

/// 解析 slash command 表單資料
#[allow(dead_code)]
fn parse_slash_command(form: &HashMap<String, String>) -> SlashCommandRequest {
    SlashCommandRequest {
        token: form.get("token").cloned(),
        team_id: form.get("team_id").cloned().unwrap_or_default(),
        team_domain: form.get("team_domain").cloned().unwrap_or_default(),
        channel_id: form.get("channel_id").cloned().unwrap_or_default(),
        channel_name: form.get("channel_name").cloned().unwrap_or_default(),
        user_id: form.get("user_id").cloned().unwrap_or_default(),
        user_name: form.get("user_name").cloned().unwrap_or_default(),
        command: form.get("command").cloned().unwrap_or_default(),
        text: form.get("text").cloned().unwrap_or_default(),
        response_url: form.get("response_url").cloned().unwrap_or_default(),
        trigger_id: form.get("trigger_id").cloned().unwrap_or_default(),
    }
}

/// Dialog 提交資料
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct DialogSubmission {
    pub r#type: String,
    pub callback_id: String,
    pub state: Option<String>,
    pub user_id: String,
    pub channel_id: String,
    pub team_id: String,
    pub submission: HashMap<String, serde_json::Value>,
    pub cancelled: Option<bool>,
}

/// Dialog 提交回應
#[derive(Debug, Serialize)]
pub struct DialogSubmissionResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// 處理 /group_buy slash command（帶 token 驗證）
pub async fn handle_group_buy_command(
    form: HashMap<String, String>,
    state: Arc<RwLock<AppState>>,
) -> Result<WithStatus<Json>, warp::Rejection> {
    verify_slash_command_token(&form, &state, "group_buy").await?;
    handle_group_buy_command_impl(form, state).await
}

/// 處理團購指令的實際邏輯（可被 /group_buy 和 /leko group_buy 共用）
pub async fn handle_group_buy_command_impl(
    form: HashMap<String, String>,
    state: Arc<RwLock<AppState>>,
) -> Result<WithStatus<Json>, warp::Rejection> {
    let req = parse_slash_command(&form);
    let state_guard = state.read().await;
    let bot_callback_url = utils::bot_callback_url_from_state(&state_guard);
    let create_params = dialogs::CreateDialogParams {
        trigger_id: &req.trigger_id,
        response_url: &req.response_url,
        channel_id: &req.channel_id,
        user_id: &req.user_id,
        user_name: &req.user_name,
        bot_callback_url: &bot_callback_url,
    };

    match dialogs::open_create_dialog(state_guard.mattermost_client.as_ref(), &create_params).await {
        Ok(_) => {
            info!("用戶 {} 開啟建立團購 dialog", req.user_name);
            Ok(warp::reply::with_status(
                warp::reply::json(&SlashCommandResponse {
                    response_type: "ephemeral".to_string(),
                    text: "".to_string(),
                }),
                StatusCode::OK,
            ))
        }
        Err(e) => {
            error!("開啟 dialog 失敗: {}", e);
            Ok(warp::reply::with_status(
                warp::reply::json(&SlashCommandResponse {
                    response_type: "ephemeral".to_string(),
                    text: format!("開啟對話框失敗: {}", e),
                }),
                StatusCode::OK,
            ))
        }
    }
}
