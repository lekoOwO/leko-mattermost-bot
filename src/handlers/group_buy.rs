use anyhow::Result;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};
use warp::http::StatusCode;
use warp::reply::{Json, WithStatus};

use super::auth::verify_slash_command_token;
use crate::AppState;
use crate::database::{GroupBuy, GroupBuyOrder, GroupBuyStatus};
use crate::mattermost::{DialogElement, DialogElementType, DialogOption, MattermostClient};

mod messages;
pub use messages::{
    generate_action_buttons, generate_group_buy_message, generate_group_buy_message_with_orders,
};
mod actions;
mod dialogs;
mod utils;
pub use actions::handle_group_buy_action;
pub use dialogs::{
    handle_adjust_shortage_dialog, handle_cancel_register_dialog, handle_create_dialog,
    handle_edit_items_dialog, handle_register_dialog,
};
// Re-export params structs so other modules (examples) can reuse the canonical types
// Note: dialog param types are defined in `dialogs` and are intended to be
// referenced directly (`crate::handlers::group_buy::dialogs::CreateDialogParams`)
// when needed. We intentionally avoid re-exporting them here to prevent
// unused-export warnings; add explicit `pub use` lines only when a consumer
// outside the crate requires them.

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
    pub submission: HashMap<String, serde_json::Value>, // 使用 Value 以支持各種類型
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

/// 處理 /group_buy 或 /leko group_buy 指令
pub async fn handle_group_buy_command(
    form: HashMap<String, String>,
    state: Arc<RwLock<AppState>>,
) -> Result<WithStatus<Json>, warp::Rejection> {
    // 驗證 slash command token
    verify_slash_command_token(&form, &state, "group_buy").await?;

    let req = parse_slash_command(&form);

    let state_guard = state.read().await;

    // 取得 bot_callback_url
    let bot_callback_url = utils::bot_callback_url_from_state(&state_guard);

    // 開啟建立團購的 Dialog
    let create_params = dialogs::CreateDialogParams {
        trigger_id: &req.trigger_id,
        response_url: &req.response_url,
        channel_id: &req.channel_id,
        user_id: &req.user_id,
        user_name: &req.user_name,
        bot_callback_url: &bot_callback_url,
    };

    match dialogs::open_create_dialog(&state_guard.mattermost_client, &create_params).await {
        Ok(_) => {
            info!("用戶 {} 開啟建立團購 dialog", req.user_name);
            // 不返回任何訊息，讓 dialog 提交後的 response_url 可以發送新訊息
            Ok(warp::reply::with_status(
                warp::reply::json(&SlashCommandResponse {
                    response_type: "ephemeral".to_string(),
                    text: "".to_string(), // 空訊息
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

// NOTE: related action handlers were moved to `actions.rs`; helpers and
// duplicated implementations were removed here during the refactor.
