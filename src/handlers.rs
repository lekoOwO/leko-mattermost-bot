//! HTTP 請求處理器

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

use crate::AppState;
use crate::mattermost::{Action, ActionOption, ActionRequest, Attachment, Integration};

// 自訂錯誤類型
#[derive(Debug)]
pub struct UnauthorizedError;
impl warp::reject::Reject for UnauthorizedError {}

/// 處理 /sticker slash command
pub async fn handle_sticker_command(
    form: std::collections::HashMap<String, String>,
    state: Arc<RwLock<AppState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    info!("收到 /sticker 指令");
    info!("請求參數: {:?}", form.keys().collect::<Vec<_>>());
    info!("完整表單內容: {:?}", form);

    // 驗證 slash command token
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

    let text = form.get("text").cloned().unwrap_or_default();
    let user_name = form.get("user_name").cloned().unwrap_or_default();
    let user_id = form.get("user_id").cloned().unwrap_or_default();
    let response_url = form.get("response_url").cloned().unwrap_or_default();

    info!("搜尋關鍵字: '{}', 使用者: {}", text, user_name);

    // 搜尋貼圖（不限分類）
    let stickers = app_state
        .sticker_database
        .search(&text, None)
        .into_iter()
        .take(25)
        .collect::<Vec<_>>();

    if stickers.is_empty() {
        // 沒有找到貼圖
        drop(app_state);
        let message = if text.is_empty() {
            "沒有可用的貼圖".to_string()
        } else {
            format!("找不到符合「{}」的貼圖", text)
        };
        return Ok(warp::reply::json(&serde_json::json!({
            "response_type": "ephemeral",
            "text": message
        })));
    }

    // 建立貼圖選項
    let sticker_options: Vec<ActionOption> = stickers
        .iter()
        .enumerate()
        .map(|(idx, s)| ActionOption {
            text: s.get_display_name(),
            value: idx.to_string(),
        })
        .collect();

    let stickers_count = sticker_options.len();

    // 取得 callback URL
    let callback_url = app_state
        .config
        .mattermost
        .bot_callback_url
        .as_ref()
        .map(|url| format!("{}/action", url.trim_end_matches('/')))
        .unwrap_or_else(|| "http://localhost/action".to_string());

    // 建立 Interactive Message
    let attachment = Attachment {
        fallback: Some("選擇貼圖".to_string()),
        color: Some("#3AA3E3".to_string()),
        pretext: None,
        text: Some(if text.is_empty() {
            format!("共 {} 張貼圖，請從下拉選單選擇：", stickers_count)
        } else {
            format!("搜尋「{}」找到 {} 張貼圖，請選擇：", text, stickers_count)
        }),
        author_name: None,
        author_icon: None,
        title: Some("🎨 貼圖選擇器".to_string()),
        image_url: None,
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
                        "keyword": text,
                    })),
                }),
                options: Some(sticker_options),
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

    // 取得 Mattermost URL 用於生成 icon_url
    let mattermost_url = app_state.config.mattermost.url.clone();
    drop(app_state);

    // 透過 response_url 發送 Interactive Message
    let response_payload = serde_json::json!({
        "response_type": "in_channel",
        "username": user_name,
        "icon_url": format!("{}/api/v4/users/{}/image", mattermost_url, user_id),
        "attachments": [attachment]
    });

    if !response_url.is_empty() {
        info!(
            "透過 response_url 發送 Interactive Message: {}",
            response_url
        );
        if let Err(e) = reqwest::Client::new()
            .post(&response_url)
            .json(&response_payload)
            .send()
            .await
        {
            error!("透過 response_url 發送失敗: {}", e);
            return Ok(warp::reply::json(&serde_json::json!({
                "response_type": "ephemeral",
                "text": "發送貼圖選擇器失敗，請稍後再試"
            })));
        }
        info!(
            "已建立 Interactive Message，共 {} 個貼圖選項",
            stickers_count
        );
        // 回傳空回應
        Ok(warp::reply::json(&serde_json::json!({})))
    } else {
        error!("response_url 為空");
        Ok(warp::reply::json(&serde_json::json!({
            "response_type": "ephemeral",
            "text": "無法發送貼圖選擇器"
        })))
    }
}

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

    // 權限檢查：只有觸發指令的使用者才能操作
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

/// 取消：清空訊息
fn handle_cancel() -> Result<warp::reply::Json, warp::Rejection> {
    info!("使用者取消了貼圖選擇");
    Ok(warp::reply::json(&serde_json::json!({
        "update": {
            "message": "",
            "props": {}
        }
    })))
}

/// 選擇貼圖：顯示預覽和發送/取消按鈕
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

    // 重新搜尋貼圖以取得選項列表（索引是搜尋結果中的索引）
    let stickers = app_state
        .sticker_database
        .search(keyword, None)
        .into_iter()
        .take(25)
        .collect::<Vec<_>>();

    let Some(sticker) = stickers.get(sticker_index) else {
        error!("找不到貼圖索引: {}", sticker_index);
        drop(app_state);
        return Ok(warp::reply::json(&serde_json::json!({
            "ephemeral_text": "找不到指定的貼圖"
        })));
    };

    info!(
        "使用者選擇了貼圖: {} (搜尋結果索引: {})",
        sticker.name, sticker_index
    );

    // 取得 callback URL
    let callback_url = app_state
        .config
        .mattermost
        .bot_callback_url
        .as_ref()
        .map(|url| format!("{}/action", url.trim_end_matches('/')))
        .unwrap_or_else(|| "http://localhost/action".to_string());

    // 取得 Mattermost URL 以生成 icon_url
    let mattermost_url = app_state.config.mattermost.url.clone();

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

    drop(app_state);

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

/// 發送貼圖：將訊息替換成貼圖
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

    // 替換訊息為貼圖，並設定 override_username 和 override_icon_url
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

/// 錯誤處理器
pub async fn handle_rejection(
    err: warp::Rejection,
) -> Result<impl warp::Reply, std::convert::Infallible> {
    use warp::http::StatusCode;

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
