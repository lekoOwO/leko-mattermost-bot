//! Interactive Message 動作處理

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

use crate::AppState;
use crate::mattermost::{Action, ActionOption, ActionRequest, Attachment, Integration};

/// 處理 Interactive Message Action callback
pub async fn handle_action(
    action_req: ActionRequest,
    state: Arc<RwLock<AppState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    info!("收到 Action 請求: {:?}", action_req);
    info!(
        "Context 內容: {}",
        serde_json::to_string_pretty(&action_req.context).unwrap_or_default()
    );

    let original_user_id = action_req
        .context
        .get("user_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if !original_user_id.is_empty() && original_user_id != action_req.user_id {
        info!(
            "權限拒絕：操作者 {} 不是原始使用者 {}",
            action_req.user_id, original_user_id
        );
        return Ok(warp::reply::json(&serde_json::json!({
            "ephemeral_text": "⚠️ 只有發起指令的使用者才能操作此面板"
        })));
    }

    let action_type = action_req
        .context
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    match action_type {
        "cancel" => handle_cancel(),
        "select_sticker" => handle_select_sticker(&action_req, state).await,
        "send_sticker" => handle_send_sticker(&action_req, state).await,
        _ => {
            error!("未知的 action 類型: {}", action_type);
            Ok(warp::reply::json(&serde_json::json!({
                "ephemeral_text": "未知的操作"
            })))
        }
    }
}

fn handle_cancel() -> Result<warp::reply::Json, warp::Rejection> {
    info!("使用者取消了貼圖選擇");
    Ok(warp::reply::json(&serde_json::json!({
        "update": {
            "message": "",
            "props": {}
        }
    })))
}

async fn handle_select_sticker(
    action_req: &ActionRequest,
    state: Arc<RwLock<AppState>>,
) -> Result<warp::reply::Json, warp::Rejection> {
    let selected_value = action_req
        .context
        .get("selected_option")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    info!("選擇的貼圖值: '{}'", selected_value);

    if selected_value.is_empty() {
        error!("selected_option 為空");
        return Ok(warp::reply::json(&serde_json::json!({
            "ephemeral_text": "請選擇一個貼圖"
        })));
    }

    let sticker_index: usize = selected_value.parse().unwrap_or(0);
    let user_id = action_req
        .context
        .get("user_id")
        .and_then(|v| v.as_str())
        .unwrap_or(&action_req.user_id);
    let user_name = action_req
        .context
        .get("user_name")
        .and_then(|v| v.as_str())
        .or(action_req.user_name.as_deref())
        .unwrap_or("Unknown");
    let keyword = action_req
        .context
        .get("keyword")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let app_state = state.read().await;
    let sticker_db = app_state.sticker_database.clone();
    let callback_url = app_state
        .config
        .mattermost
        .bot_callback_url
        .as_ref()
        .map(|url| format!("{}/action", url.trim_end_matches('/')))
        .unwrap_or_else(|| "http://localhost/action".to_string());
    let mattermost_url = app_state.config.mattermost.url.clone();
    drop(app_state);

    let stickers = match sticker_db.search_async(keyword, None).await {
        Ok(v) => v.into_iter().take(25).collect::<Vec<_>>(),
        Err(e) => {
            error!("重新搜尋貼圖失敗: {}", e);
            return Ok(warp::reply::json(&serde_json::json!({
                "ephemeral_text": "搜尋貼圖失敗，請稍後再試"
            })));
        }
    };

    let Some(sticker) = stickers.get(sticker_index) else {
        error!("找不到貼圖索引: {}", sticker_index);
        return Ok(warp::reply::json(&serde_json::json!({
            "ephemeral_text": "找不到指定的貼圖"
        })));
    };

    info!(
        "使用者選擇了貼圖: {} (搜尋結果索引: {})",
        sticker.name, sticker_index
    );

    let sticker_options: Vec<ActionOption> = stickers
        .iter()
        .enumerate()
        .map(|(idx, s)| ActionOption {
            text: s.get_display_name(),
            value: idx.to_string(),
        })
        .collect();

    // 克隆需要的資料
    let sticker_name = sticker.name.clone();
    let sticker_display_name = sticker.get_display_name();
    let sticker_image_url = sticker.image_url.clone();

    // 建立包含預覽的 Interactive Message
    let attachment = Attachment {
        fallback: Some(format!("已選擇: {}", sticker_name)),
        color: Some("#36a64f".to_string()),
        pretext: None,
        text: Some(format!("已選擇: **{}**", sticker_display_name)),
        author_name: Some(user_name.to_string()),
        author_icon: Some(format!("{}/api/v4/users/{}/image", mattermost_url, user_id)),
        title: Some("🎨 貼圖預覽".to_string()),
        image_url: Some(sticker_image_url.clone()),
        thumb_url: None,
        actions: Some(vec![
            Action {
                id: "stickerselect".to_string(),
                name: "選擇貼圖".to_string(),
                action_type: "select".to_string(),
                style: None,
                integration: Some(Integration {
                    url: callback_url.clone(),
                    context: Some(serde_json::json!({
                        "action": "select_sticker",
                        "user_id": user_id,
                        "user_name": user_name,
                        "keyword": keyword,
                    })),
                }),
                options: Some(sticker_options),
            },
            Action {
                id: "send".to_string(),
                name: "✅ 發送".to_string(),
                action_type: "button".to_string(),
                style: Some("primary".to_string()),
                integration: Some(Integration {
                    url: callback_url.clone(),
                    context: Some(serde_json::json!({
                        "action": "send_sticker",
                        "sticker_name": sticker_name,
                        "sticker_image_url": sticker_image_url,
                        "user_id": user_id,
                        "user_name": user_name,
                    })),
                }),
                options: None,
            },
            Action {
                id: "cancel".to_string(),
                name: "❌ 取消".to_string(),
                action_type: "button".to_string(),
                style: Some("danger".to_string()),
                integration: Some(Integration {
                    url: callback_url.clone(),
                    context: Some(serde_json::json!({
                        "action": "cancel",
                        "user_id": user_id,
                    })),
                }),
                options: None,
            },
        ]),
    };

    Ok(warp::reply::json(&serde_json::json!({
        "update": {
            "message": "",
            "props": {
                "attachments": [attachment]
            }
        }
    })))
}

async fn handle_send_sticker(
    action_req: &ActionRequest,
    state: Arc<RwLock<AppState>>,
) -> Result<warp::reply::Json, warp::Rejection> {
    let sticker_name = action_req
        .context
        .get("sticker_name")
        .and_then(|v| v.as_str())
        .unwrap_or("sticker");
    let sticker_image_url = action_req
        .context
        .get("sticker_image_url")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let user_name = action_req
        .context
        .get("user_name")
        .and_then(|v| v.as_str())
        .or(action_req.user_name.as_deref())
        .unwrap_or("Unknown");
    let user_id = action_req
        .context
        .get("user_id")
        .and_then(|v| v.as_str())
        .unwrap_or(&action_req.user_id);

    if sticker_image_url.is_empty() {
        error!("sticker_image_url 為空");
        return Ok(warp::reply::json(&serde_json::json!({
            "ephemeral_text": "找不到指定的貼圖"
        })));
    }

    info!("發送貼圖: {} 由 {}", sticker_name, user_name);

    let app_state = state.read().await;
    let mattermost_url = app_state.config.mattermost.url.clone();
    drop(app_state);

    let sticker_message = format!("![{}]({})", sticker_name, sticker_image_url);

    Ok(warp::reply::json(&serde_json::json!({
        "update": {
            "message": sticker_message,
            "props": {
                "override_username": user_name,
                "override_icon_url": format!("{}/api/v4/users/{}/image", mattermost_url, user_id)
            }
        }
    })))
}
