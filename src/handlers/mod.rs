//! HTTP 請求處理器模組

mod actions;
mod auth;
mod group_buy;
mod leko;
mod sticker;

// 重新導出公開的處理器函數
pub use actions::handle_action;
pub use auth::UnauthorizedError;
pub use group_buy::{
    handle_adjust_shortage_dialog, handle_cancel_register_dialog, handle_create_dialog,
    handle_edit_items_dialog, handle_group_buy_action, handle_group_buy_command,
    handle_register_dialog,
};
pub use leko::handle_leko_command;
pub use sticker::handle_sticker_command;

use tracing::error;
use warp::http::StatusCode;

/// 錯誤處理器
pub async fn handle_rejection(
    err: warp::Rejection,
) -> Result<impl warp::Reply, std::convert::Infallible> {
    if err.is_not_found() {
        Ok(warp::reply::with_status(
            warp::reply::json(&serde_json::json!({
                "error": "Not Found"
            })),
            StatusCode::NOT_FOUND,
        ))
    } else if err.find::<UnauthorizedError>().is_some() {
        error!("未授權的請求");
        Ok(warp::reply::with_status(
            warp::reply::json(&serde_json::json!({
                "error": "Unauthorized: Invalid slash command token"
            })),
            StatusCode::UNAUTHORIZED,
        ))
    } else {
        error!("未處理的錯誤: {:?}", err);
        Ok(warp::reply::with_status(
            warp::reply::json(&serde_json::json!({
                "error": "Internal Server Error"
            })),
            StatusCode::INTERNAL_SERVER_ERROR,
        ))
    }
}
