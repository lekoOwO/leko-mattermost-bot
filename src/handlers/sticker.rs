//! 貼圖指令處理

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

use super::auth::verify_slash_command_token;
use super::reply_helpers::{empty_json_reply, ephemeral_json_reply, get_form_field};
use crate::AppState;
use crate::mattermost::{Action, ActionOption, Attachment, Integration};

/// 處理 /sticker slash command
pub async fn handle_sticker_command(
    form: std::collections::HashMap<String, String>,
    state: Arc<RwLock<AppState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    info!("收到 /sticker 指令");
    info!("請求參數: {:?}", form.keys().collect::<Vec<_>>());
    info!("完整表單內容: {:?}", form);

    verify_slash_command_token(&form, &state, "stickers").await?;

    handle_sticker_command_impl(form, state).await
}

/// 處理貼圖指令的實際邏輯（可被 /sticker 和 /leko sticker 共用）
pub async fn handle_sticker_command_impl(
    form: std::collections::HashMap<String, String>,
    state: Arc<RwLock<AppState>>,
) -> Result<warp::reply::Json, warp::Rejection> {
    let text = get_form_field(&form, "text");
    let user_name = get_form_field(&form, "user_name");
    let user_id = get_form_field(&form, "user_id");
    let response_url = get_form_field(&form, "response_url");

    info!("搜尋關鍵字: '{}', 使用者: {}", text, user_name);

    let app_state = state.read().await;
    let sticker_db = app_state.sticker_database.clone();
    let mattermost_url = app_state.config.mattermost.url.clone();
    let callback_url = app_state
        .config
        .mattermost
        .bot_callback_url
        .as_ref()
        .map(|url| format!("{}/action", url.trim_end_matches('/')))
        .unwrap_or_else(|| "http://localhost/action".to_string());
    let default_avatar_url = app_state
        .config
        .get_default_avatar_url()
        .map(|avatar| app_state.config.resolve_avatar_url(&avatar));
    drop(app_state);

    let stickers = match sticker_db.search_async(&text, None).await {
        Ok(v) => v.into_iter().take(25).collect::<Vec<_>>(),
        Err(e) => {
            error!("搜尋貼圖失敗: {}", e);
            return Ok(ephemeral_json_reply("搜尋貼圖失敗，請稍後再試", default_avatar_url));
        }
    };

    if stickers.is_empty() {
        let message = if text.is_empty() {
            "沒有可用的貼圖".to_string()
        } else {
            format!("找不到符合「{}」的貼圖", text)
        };
        return Ok(ephemeral_json_reply(message, default_avatar_url));
    }

    let sticker_options: Vec<ActionOption> = stickers
        .iter()
        .enumerate()
        .map(|(idx, s)| ActionOption {
            text: s.get_display_name(),
            value: idx.to_string(),
        })
        .collect();

    let stickers_count = sticker_options.len();

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
            return Ok(ephemeral_json_reply("發送貼圖選擇器失敗，請稍後再試", default_avatar_url));
        }
        info!(
            "已建立 Interactive Message，共 {} 個貼圖選項",
            stickers_count
        );
        // 回傳空回應
        Ok(empty_json_reply())
    } else {
        error!("response_url 為空");
        Ok(ephemeral_json_reply("無法發送貼圖選擇器", default_avatar_url))
    }
}
