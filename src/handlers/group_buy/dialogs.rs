use super::*;
use chrono::Utc;
use std::collections::HashMap;

/// Parameters for opening the create dialog.
pub struct CreateDialogParams<'a> {
    pub trigger_id: &'a str,
    pub response_url: &'a str,
    pub channel_id: &'a str,
    pub user_id: &'a str,
    pub user_name: &'a str,
    pub bot_callback_url: &'a str,
}

// Open create dialog
pub async fn open_create_dialog(
    client: &MattermostClient,
    params: &CreateDialogParams<'_>,
) -> Result<()> {
    let elements = vec![
        DialogElement {
            display_name: "商家名稱".to_string(),
            name: "merchant_name".to_string(),
            element_type: DialogElementType::Text,
            placeholder: Some("例如：五十嵐".to_string()),
            help_text: None,
            optional: false,
            min_length: Some(1),
            max_length: Some(100),
            data_source: None,
            options: None,
            default: None,
            subtype: None,
        },
        DialogElement {
            display_name: "描述".to_string(),
            name: "description".to_string(),
            element_type: DialogElementType::Textarea,
            placeholder: Some("團購的詳細說明（可選）".to_string()),
            help_text: None,
            optional: true,
            min_length: None,
            max_length: Some(500),
            data_source: None,
            options: None,
            default: None,
            subtype: None,
        },
        DialogElement {
            display_name: "其他資訊".to_string(),
            name: "metadata".to_string(),
            element_type: DialogElementType::Textarea,
            placeholder: Some(
                "YAML 格式，例如：\n截止時間: 2026-01-25 18:00\n取貨地點: 公司大廳".to_string(),
            ),
            help_text: Some("使用 YAML 格式填寫 key-value pairs（可選）".to_string()),
            optional: true,
            min_length: None,
            max_length: Some(1000),
            data_source: None,
            options: None,
            default: None,
            subtype: None,
        },
    ];

    let state = serde_json::json!({
        "response_url": params.response_url,
        "channel_id": params.channel_id,
        "user_id": params.user_id,
        "user_name": params.user_name,
    })
    .to_string();

    let dialog_url = format!(
        "{}/api/v1/group_buy/dialog/create",
        params.bot_callback_url.trim_end_matches('/')
    );

    client
        .open_dialog(
            params.trigger_id,
            &dialog_url,
            "建立團購",
            &elements,
            None,
            None,
            Some(&state),
        )
        .await?;

    Ok(())
}

// Handle create dialog submission
pub async fn handle_create_dialog(
    form: HashMap<String, String>,
    state: Arc<RwLock<AppState>>,
) -> Result<WithStatus<Json>, warp::Rejection> {
    info!("收到建立團購 Dialog 提交");
    info!("Form keys: {:?}", form.keys().collect::<Vec<_>>());

    let submission = match super::utils::parse_dialog_submission_form(&form) {
        Ok(s) => s,
        Err(e) => {
            error!("{}", e);
            return Err(warp::reject::reject());
        }
    };

    info!("成功解析 Dialog submission");

    if submission.cancelled == Some(true) {
        return Ok(warp::reply::with_status(
            warp::reply::json(&DialogSubmissionResponse {
                error: None,
                text: None,
                errors: None,
            }),
            StatusCode::OK,
        ));
    }

    let state_data = serde_json::from_str(submission.state.as_deref().unwrap_or("{}"))
        .unwrap_or_else(|_| serde_json::json!({}));

    let response_url = state_data
        .get("response_url")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let channel_id = state_data
        .get("channel_id")
        .and_then(|v| v.as_str())
        .unwrap_or(&submission.channel_id);

    let user_id = state_data
        .get("user_id")
        .and_then(|v| v.as_str())
        .unwrap_or(&submission.user_id);

    let user_name = state_data
        .get("user_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if response_url.is_empty() {
        error!("response_url 為空");
        return Ok(warp::reply::with_status(
            warp::reply::json(&DialogSubmissionResponse {
                error: Some("內部錯誤：缺少 response_url".to_string()),
                text: None,
                errors: None,
            }),
            StatusCode::OK,
        ));
    }

    let merchant_name = submission
        .submission
        .get("merchant_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let description = submission
        .submission
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let metadata_yaml = submission
        .submission
        .get("metadata")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let metadata: HashMap<String, String> = if let Some(yaml_str) = metadata_yaml {
        if !yaml_str.trim().is_empty() {
            match serde_yaml::from_str(&yaml_str) {
                Ok(data) => data,
                Err(e) => {
                    return Ok(warp::reply::with_status(
                        warp::reply::json(&DialogSubmissionResponse {
                            error: None,
                            text: None,
                            errors: Some(
                                [("metadata".to_string(), format!("YAML 格式錯誤: {}", e))]
                                    .into_iter()
                                    .collect(),
                            ),
                        }),
                        StatusCode::OK,
                    ));
                }
            }
        } else {
            HashMap::new()
        }
    } else {
        HashMap::new()
    };

    let state_guard = state.read().await;

    let group_buy_id = uuid::Uuid::new_v4().to_string();

    let user = match state_guard
        .mattermost_client
        .get_user(&submission.user_id)
        .await
    {
        Ok(u) => u,
        Err(e) => {
            error!("取得用戶資訊失敗: {}", e);
            return Ok(warp::reply::with_status(
                warp::reply::json(&DialogSubmissionResponse {
                    error: Some("無法取得用戶資訊".to_string()),
                    text: None,
                    errors: None,
                }),
                StatusCode::OK,
            ));
        }
    };

    let bot_callback_url = super::utils::bot_callback_url_from_state(&state_guard);

    let message = generate_group_buy_message(
        &merchant_name,
        &description,
        &metadata,
        &GroupBuyStatus::Active,
        &HashMap::new(),
    );
    let attachments =
        generate_action_buttons(&group_buy_id, &GroupBuyStatus::Active, &bot_callback_url);

    let mattermost_url = &state_guard.config.mattermost.url;
    let icon_url = format!("{}/api/v4/users/{}/image", mattermost_url, user_id);

    let response_payload = serde_json::json!({
        "response_type": "in_channel",
        "text": message,
        "attachments": attachments,
        "username": user_name,
        "icon_url": icon_url
    });

    let client = reqwest::Client::new();
    let response = client
        .post(response_url)
        .json(&response_payload)
        .send()
        .await;

    if let Err(e) = response {
        error!("發送到 response_url 失敗: {}", e);
        return Ok(warp::reply::with_status(
            warp::reply::json(&DialogSubmissionResponse {
                error: Some(format!("建立團購訊息失敗: {}", e)),
                text: None,
                errors: None,
            }),
            StatusCode::OK,
        ));
    }

    let response = response.unwrap();
    let status_code = response.status();
    let response_text = response.text().await.unwrap_or_default();

    if !status_code.is_success() {
        error!(
            "發送到 response_url 失敗，狀態碼: {}, 回應: {}",
            status_code, response_text
        );
        return Ok(warp::reply::with_status(
            warp::reply::json(&DialogSubmissionResponse {
                error: Some(format!("建立團購訊息失敗: HTTP {}", status_code)),
                text: None,
                errors: None,
            }),
            StatusCode::OK,
        ));
    }

    let post_id = None;

    let now = Utc::now();
    let group_buy = GroupBuy {
        id: group_buy_id.clone(),
        creator_id: submission.user_id.clone(),
        creator_username: user.username.clone(),
        channel_id: channel_id.to_string(),
        post_id,
        merchant_name: merchant_name.clone(),
        description: description.filter(|s| !s.is_empty()),
        metadata,
        items: HashMap::new(),
        status: GroupBuyStatus::Active,
        version: 1,
        created_at: now,
        updated_at: now,
    };

    if let Err(e) = state_guard.database.create_group_buy(&group_buy).await {
        error!("儲存團購到資料庫失敗: {}", e);
        return Ok(warp::reply::with_status(
            warp::reply::json(&DialogSubmissionResponse {
                error: Some(format!("儲存團購資料失敗: {}", e)),
                text: None,
                errors: None,
            }),
            StatusCode::OK,
        ));
    }

    info!(
        "用戶 {} 建立團購: {} (ID: {})",
        user.username, merchant_name, group_buy_id
    );

    Ok(warp::reply::with_status(
        warp::reply::json(&DialogSubmissionResponse {
            error: None,
            text: None,
            errors: None,
        }),
        StatusCode::OK,
    ))
}

// helpers: items_to_yaml & parse_items_yaml
pub fn items_to_yaml(items: &HashMap<String, Decimal>) -> String {
    if items.len() == 1 && items.contains_key("範例商品") {
        return "# 範例商品: 10\n".to_string();
    }

    let mut yaml = String::new();
    for (name, price) in items {
        yaml.push_str(&format!("{}: {}\n", name, price));
    }
    yaml
}

pub fn parse_items_yaml(yaml: &str) -> Result<HashMap<String, Decimal>> {
    let mut items = HashMap::new();

    for line in yaml.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = line.splitn(2, ':').collect();
        if parts.len() != 2 {
            anyhow::bail!("格式錯誤：{}", line);
        }

        let name = parts[0].trim();
        let price_str = parts[1].trim();

        if name.is_empty() {
            anyhow::bail!("商品名稱不能為空");
        }

        let price = Decimal::from_str(price_str)
            .map_err(|_| anyhow::anyhow!("價格格式錯誤：{}", price_str))?;

        if price.is_sign_negative() {
            anyhow::bail!("價格不能為負數");
        }

        items.insert(name.to_string(), price);
    }

    Ok(items)
}

// Open edit items dialog
pub async fn open_edit_items_dialog(
    client: &MattermostClient,
    params: &EditItemsDialogParams<'_>,
) -> Result<()> {
    let elements = vec![DialogElement {
        display_name: "商品列表 (YAML 格式)".to_string(),
        name: "items".to_string(),
        element_type: DialogElementType::Textarea,
        subtype: None,
        placeholder: Some("商品名稱: 價格\n例：\n珍珠奶茶: 50\n紅茶拿鐵: 45".to_string()),
        help_text: Some("每行一個商品，格式：商品名稱: 價格".to_string()),
        default: Some(params.items_yaml.to_string()),
        optional: false,
        min_length: None,
        max_length: Some(3000),
        data_source: None,
        options: None,
    }];

    let state = serde_json::json!({
        "group_buy_id": params.group_buy_id,
        "version": params.version,
        "post_id": params.post_id,
    })
    .to_string();

    let dialog_url = format!(
        "{}/api/v1/group_buy/dialog/edit_items",
        params.bot_callback_url.trim_end_matches('/')
    );

    client
        .open_dialog(
            params.trigger_id,
            &dialog_url,
            "編輯商品",
            &elements,
            Some("儲存"),
            None,
            Some(&state),
        )
        .await?;

    Ok(())
}

/// Parameters for opening the edit-items dialog.
pub struct EditItemsDialogParams<'a> {
    pub trigger_id: &'a str,
    pub group_buy_id: &'a str,
    pub items_yaml: &'a str,
    pub version: i32,
    pub post_id: Option<&'a str>,
    pub bot_callback_url: &'a str,
}

// Handle edit items submission
pub async fn handle_edit_items_dialog(
    form: HashMap<String, String>,
    state: Arc<RwLock<AppState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    info!("收到編輯商品 Dialog 提交");

    let submission = match super::utils::parse_dialog_submission_form(&form) {
        Ok(s) => s,
        Err(e) => {
            error!("{}", e);
            return Err(warp::reject::reject());
        }
    };

    let state_data = match super::utils::extract_state_value(&submission) {
        Ok(v) => v,
        Err(e) => {
            error!("{}", e);
            return Err(warp::reject::reject());
        }
    };

    let group_buy_id = state_data
        .get("group_buy_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            error!("state 缺少 group_buy_id");
            warp::reject::reject()
        })?
        .to_string();

    let version = state_data
        .get("version")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| {
            error!("state 缺少 version");
            warp::reject::reject()
        })? as i32;

    let post_id = state_data
        .get("post_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let items_yaml = submission
        .submission
        .get("items")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let items = match parse_items_yaml(items_yaml) {
        Ok(items) => items,
        Err(e) => {
            return Ok(warp::reply::with_status(
                warp::reply::json(&DialogSubmissionResponse {
                    error: None,
                    text: None,
                    errors: Some(
                        [("items".to_string(), format!("YAML 格式錯誤: {}", e))]
                            .into_iter()
                            .collect(),
                    ),
                }),
                StatusCode::OK,
            ));
        }
    };

    if items.is_empty() {
        return Ok(warp::reply::with_status(
            warp::reply::json(&DialogSubmissionResponse {
                error: None,
                text: None,
                errors: Some(
                    [("items".to_string(), "至少需要一個商品".to_string())]
                        .into_iter()
                        .collect(),
                ),
            }),
            StatusCode::OK,
        ));
    }

    let state_guard = state.read().await;

    let user = match state_guard
        .mattermost_client
        .get_user(&submission.user_id)
        .await
    {
        Ok(u) => u,
        Err(e) => {
            error!("取得用戶資訊失敗: {}", e);
            return Ok(warp::reply::with_status(
                warp::reply::json(&DialogSubmissionResponse {
                    error: Some("無法取得用戶資訊".to_string()),
                    text: None,
                    errors: None,
                }),
                StatusCode::OK,
            ));
        }
    };

    if let Err(e) = state_guard
        .database
        .update_items(
            &group_buy_id,
            &items,
            version,
            &submission.user_id,
            &user.username,
        )
        .await
    {
        error!("更新商品列表失敗: {}", e);
        return Ok(warp::reply::with_status(
            warp::reply::json(&DialogSubmissionResponse {
                error: Some(format!("更新失敗: {}", e)),
                text: None,
                errors: None,
            }),
            StatusCode::OK,
        ));
    }

    let group_buy = match state_guard.database.get_group_buy(&group_buy_id).await {
        Ok(Some(gb)) => gb,
        Ok(None) => {
            error!("更新後找不到團購資料");
            return Ok(warp::reply::with_status(
                warp::reply::json(&DialogSubmissionResponse {
                    error: Some("內部錯誤：找不到團購資料".to_string()),
                    text: None,
                    errors: None,
                }),
                StatusCode::OK,
            ));
        }
        Err(e) => {
            error!("取得團購資料失敗: {}", e);
            return Ok(warp::reply::with_status(
                warp::reply::json(&DialogSubmissionResponse {
                    error: Some("內部錯誤".to_string()),
                    text: None,
                    errors: None,
                }),
                StatusCode::OK,
            ));
        }
    };

    info!("成功更新團購 {} 的商品列表", group_buy_id);

    let mut items_list = String::new();
    items_list.push_str("### ✅ 商品列表更新成功\n\n");
    items_list.push_str("| 商品 | 價格 |\n");
    items_list.push_str("|------|-----:|\n");

    let mut sorted_items: Vec<_> = group_buy.items.iter().collect();
    sorted_items.sort_by_key(|(name, _)| *name);

    for (name, price) in sorted_items {
        items_list.push_str(&format!("| {} | ${} |\n", name, price));
    }

    let channel_id = submission.channel_id.clone();
    let user_username = user.username.clone();
    let post_id_clone = post_id.clone();
    let client = state_guard.mattermost_client.clone();

    info!("準備發送公開回覆（tag user）:");
    info!("  channel_id: {}", channel_id);
    info!("  user: {}", user_username);
    info!("  post_id (root_id): {:?}", post_id_clone);

    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let message = format!("@{} 編輯商品成功", user_username);
        let post = crate::mattermost::Post {
            id: None,
            channel_id: channel_id.clone(),
            message,
            root_id: post_id_clone.as_deref().map(|s: &str| s.to_string()),
            props: None,
        };

        if let Err(e) = client.create_post(&post).await {
            error!("發送公開回覆失敗: {}", e);
        } else {
            info!("公開回覆已發送");
        }
    });

    Ok(warp::reply::with_status(
        warp::reply::json(&DialogSubmissionResponse {
            error: None,
            text: None,
            errors: None,
        }),
        StatusCode::OK,
    ))
}

// Cancel register: open + handle
#[allow(clippy::too_many_arguments)]
pub async fn open_cancel_register_dialog(
    client: &MattermostClient,
    params: &CancelRegisterDialogParams,
) -> Result<()> {
    let elements = vec![DialogElement {
        display_name: "被登記人 (要取消的人)".to_string(),
        name: "target_buyer".to_string(),
        element_type: DialogElementType::Select,
        placeholder: Some("選擇被登記人".to_string()),
        help_text: Some("將會清除該用戶的所有登記".to_string()),
        optional: false,
        min_length: None,
        max_length: None,
        data_source: None,
        options: Some(params.buyer_options.clone()),
        default: None,
        subtype: None,
    }];

    let state = serde_json::json!({
        "group_buy_id": params.group_buy_id,
        "version": params.version,
        "post_id": params.post_id.as_deref(),
    })
    .to_string();

    let dialog_url = format!(
        "{}/api/v1/group_buy/dialog/cancel_register",
        params.bot_callback_url.trim_end_matches('/')
    );

    client
        .open_dialog(
            params.trigger_id.as_str(),
            &dialog_url,
            "取消登記",
            &elements,
            Some("確認取消"),
            params.introduction_text.as_deref(),
            Some(&state),
        )
        .await?;

    Ok(())
}

/// Parameters for opening the cancel-register dialog.
pub struct CancelRegisterDialogParams {
    pub trigger_id: String,
    pub group_buy_id: String,
    pub buyer_options: Vec<DialogOption>,
    pub version: i32,
    pub post_id: Option<String>,
    pub introduction_text: Option<String>,
    pub bot_callback_url: String,
}

pub async fn handle_cancel_register_dialog(
    form: HashMap<String, String>,
    state: Arc<RwLock<AppState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    info!("收到取消登記 Dialog 提交");

    let submission = match super::utils::parse_dialog_submission_form(&form) {
        Ok(s) => s,
        Err(e) => {
            error!("{}", e);
            return Err(warp::reject::reject());
        }
    };

    let state_data = match super::utils::extract_state_value(&submission) {
        Ok(v) => v,
        Err(e) => {
            error!("{}", e);
            return Err(warp::reject::reject());
        }
    };

    let group_buy_id = state_data
        .get("group_buy_id")
        .and_then(|v| v.as_str())
        .ok_or_else(warp::reject::reject)?
        .to_string();

    let target_buyer = submission
        .submission
        .get("target_buyer")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if target_buyer.is_empty() {
        return Ok(warp::reply::with_status(
            warp::reply::json(&DialogSubmissionResponse {
                error: Some("請選擇要取消的被登記人".to_string()),
                text: None,
                errors: None,
            }),
            StatusCode::OK,
        ));
    }

    let state_guard = state.read().await;

    let actor = match state_guard
        .mattermost_client
        .get_user(&submission.user_id)
        .await
    {
        Ok(u) => u,
        Err(e) => {
            error!("取得操作使用者資訊失敗: {}", e);
            return Ok(warp::reply::with_status(
                warp::reply::json(&DialogSubmissionResponse {
                    error: Some("內部錯誤：無法取得使用者資訊".to_string()),
                    text: None,
                    errors: None,
                }),
                StatusCode::OK,
            ));
        }
    };

    match state_guard
        .database
        .delete_orders_for_buyer(
            &group_buy_id,
            target_buyer,
            &submission.user_id,
            &actor.username,
        )
        .await
    {
        Ok(rows) => {
            info!("已刪除 {} 筆訂單，buyer: {}", rows, target_buyer);
        }
        Err(e) => {
            error!("刪除訂單失敗: {}", e);
            return Ok(warp::reply::with_status(
                warp::reply::json(&DialogSubmissionResponse {
                    error: Some(format!("刪除失敗: {}", e)),
                    text: None,
                    errors: None,
                }),
                StatusCode::OK,
            ));
        }
    }

    Ok(warp::reply::with_status(
        warp::reply::json(&DialogSubmissionResponse {
            error: None,
            text: None,
            errors: None,
        }),
        StatusCode::OK,
    ))
}

// Open register dialog
#[allow(clippy::too_many_arguments)]
pub async fn open_register_dialog(
    client: &MattermostClient,
    params: &RegisterDialogParams<'_>,
) -> Result<()> {
    let item_options: Vec<DialogOption> = params
        .items
        .iter()
        .map(|(name, price)| DialogOption {
            text: format!("{} (NT${})", name, price),
            value: name.clone(),
        })
        .collect();

    let elements = vec![
        DialogElement {
            display_name: "購買人".to_string(),
            name: "buyer".to_string(),
            element_type: DialogElementType::Select,
            placeholder: Some("選擇購買人".to_string()),
            help_text: Some("可以幫其他人登記".to_string()),
            optional: false,
            min_length: None,
            max_length: None,
            data_source: Some("users".to_string()),
            options: None,
            default: None,
            subtype: None,
        },
        DialogElement {
            display_name: "商品".to_string(),
            name: "item".to_string(),
            element_type: DialogElementType::Select,
            placeholder: Some("選擇商品".to_string()),
            help_text: None,
            optional: false,
            min_length: None,
            max_length: None,
            data_source: None,
            options: Some(item_options),
            default: None,
            subtype: None,
        },
        DialogElement {
            display_name: "數量".to_string(),
            name: "quantity".to_string(),
            element_type: DialogElementType::Text,
            placeholder: Some("1".to_string()),
            help_text: None,
            optional: false,
            min_length: Some(1),
            max_length: Some(10),
            data_source: None,
            options: None,
            default: Some("1".to_string()),
            subtype: Some("number".to_string()),
        },
    ];

    let state = serde_json::json!({
        "group_buy_id": params.group_buy_id,
        "version": params.version,
        "post_id": params.post_id,
    })
    .to_string();

    let dialog_url = format!(
        "{}/api/v1/group_buy/dialog/register",
        params.bot_callback_url.trim_end_matches('/')
    );

    client
        .open_dialog(
            params.trigger_id,
            &dialog_url,
            "登記團購",
            &elements,
            Some("確認登記"),
            params.introduction_text,
            Some(&state),
        )
        .await?;

    Ok(())
}

/// Parameters for opening the register dialog.
pub struct RegisterDialogParams<'a> {
    pub trigger_id: &'a str,
    pub group_buy_id: &'a str,
    pub items: &'a HashMap<String, Decimal>,
    pub version: i32,
    pub post_id: Option<&'a str>,
    pub introduction_text: Option<&'a str>,
    pub bot_callback_url: &'a str,
}

// Handle register submission
pub async fn handle_register_dialog(
    form: HashMap<String, String>,
    state: Arc<RwLock<AppState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    info!("收到登記 Dialog 提交");

    let submission = match super::utils::parse_dialog_submission_form(&form) {
        Ok(s) => s,
        Err(e) => {
            error!("{}", e);
            return Err(warp::reject::reject());
        }
    };

    let state_data = match super::utils::extract_state_value(&submission) {
        Ok(v) => v,
        Err(e) => {
            error!("{}", e);
            return Err(warp::reject::reject());
        }
    };

    let group_buy_id = state_data
        .get("group_buy_id")
        .and_then(|v| v.as_str())
        .ok_or_else(warp::reject::reject)?
        .to_string();

    let _post_id = state_data
        .get("post_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let buyer_id = submission
        .submission
        .get("buyer")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let item_name = submission
        .submission
        .get("item")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let quantity_str = submission
        .submission
        .get("quantity")
        .and_then(|v| {
            v.as_str()
                .map(|s| s.to_string())
                .or_else(|| v.as_i64().map(|i| i.to_string()))
        })
        .unwrap_or_else(|| "1".to_string());

    let quantity: i32 = match quantity_str.parse() {
        Ok(q) if q >= 0 => q,
        _ => {
            return Ok(warp::reply::with_status(
                warp::reply::json(&DialogSubmissionResponse {
                    error: None,
                    text: None,
                    errors: Some(
                        [("quantity".to_string(), "數量必須是正整數".to_string())]
                            .into_iter()
                            .collect(),
                    ),
                }),
                StatusCode::OK,
            ));
        }
    };

    let state_guard = state.read().await;

    let buyer = match state_guard.mattermost_client.get_user(buyer_id).await {
        Ok(u) => u,
        Err(e) => {
            error!("取得購買人資訊失敗: {}", e);
            return Ok(warp::reply::with_status(
                warp::reply::json(&DialogSubmissionResponse {
                    error: Some("無法取得購買人資訊".to_string()),
                    text: None,
                    errors: None,
                }),
                StatusCode::OK,
            ));
        }
    };

    let registrar = match state_guard
        .mattermost_client
        .get_user(&submission.user_id)
        .await
    {
        Ok(u) => u,
        Err(e) => {
            error!("取得登記人資訊失敗: {}", e);
            return Ok(warp::reply::with_status(
                warp::reply::json(&DialogSubmissionResponse {
                    error: Some("無法取得登記人資訊".to_string()),
                    text: None,
                    errors: None,
                }),
                StatusCode::OK,
            ));
        }
    };

    let group_buy = match state_guard.database.get_group_buy(&group_buy_id).await {
        Ok(Some(gb)) => gb,
        _ => {
            return Ok(warp::reply::with_status(
                warp::reply::json(&DialogSubmissionResponse {
                    error: Some("找不到該團購".to_string()),
                    text: None,
                    errors: None,
                }),
                StatusCode::OK,
            ));
        }
    };

    let unit_price = match group_buy.items.get(item_name) {
        Some(&price) => price,
        None => {
            return Ok(warp::reply::with_status(
                warp::reply::json(&DialogSubmissionResponse {
                    error: Some("商品不存在".to_string()),
                    text: None,
                    errors: None,
                }),
                StatusCode::OK,
            ));
        }
    };

    if quantity == 0 {
        match state_guard
            .database
            .delete_buyer_item_orders(
                &group_buy_id,
                buyer_id,
                item_name,
                &submission.user_id,
                &registrar.username,
            )
            .await
        {
            Ok(rows) => {
                info!(
                    "刪除了 {} 筆 {} 的登記 (buyer: {})",
                    rows, item_name, buyer_id
                );
            }
            Err(e) => {
                error!("刪除登記失敗: {}", e);
                return Ok(warp::reply::with_status(
                    warp::reply::json(&DialogSubmissionResponse {
                        error: Some(format!("刪除失敗: {}", e)),
                        text: None,
                        errors: None,
                    }),
                    StatusCode::OK,
                ));
            }
        }

        return Ok(warp::reply::with_status(
            warp::reply::json(&DialogSubmissionResponse {
                error: None,
                text: None,
                errors: None,
            }),
            StatusCode::OK,
        ));
    }

    let order = GroupBuyOrder {
        id: uuid::Uuid::new_v4().to_string(),
        group_buy_id: group_buy_id.clone(),
        registrar_id: submission.user_id.clone(),
        registrar_username: registrar.username.clone(),
        buyer_id: buyer_id.to_string(),
        buyer_username: buyer.username.clone(),
        item_name: item_name.to_string(),
        quantity,
        original_quantity: None,
        unit_price,
        created_at: Utc::now(),
    };

    if let Err(e) = state_guard.database.create_order(&order).await {
        error!("建立訂單失敗: {}", e);
        return Ok(warp::reply::with_status(
            warp::reply::json(&DialogSubmissionResponse {
                error: Some(format!("登記失敗: {}", e)),
                text: None,
                errors: None,
            }),
            StatusCode::OK,
        ));
    }

    info!(
        "{} 為 {} 登記：{} x{}",
        registrar.username, buyer.username, item_name, quantity
    );

    Ok(warp::reply::with_status(
        warp::reply::json(&DialogSubmissionResponse {
            error: None,
            text: None,
            errors: None,
        }),
        StatusCode::OK,
    ))
}

// Open adjust shortage dialog
pub async fn open_adjust_shortage_dialog(
    client: &MattermostClient,
    params: &AdjustShortageDialogParams<'_>,
) -> Result<()> {
    let mut yaml = String::new();
    yaml.push_str("# 格式：order_id: 新數量\n");
    yaml.push_str("# 設為 0 表示完全缺貨，維持原數量則不填或保持原值\n\n");

    for order in params.orders {
        yaml.push_str(&format!(
            "# @{} - {} x{}\n{}: {}\n\n",
            order.buyer_username, order.item_name, order.quantity, order.id, order.quantity
        ));
    }

    let elements = vec![DialogElement {
        display_name: "調整數量 (YAML 格式)".to_string(),
        name: "adjustments".to_string(),
        element_type: DialogElementType::Textarea,
        placeholder: Some("order_id: 新數量".to_string()),
        help_text: Some("只需填寫要調整的訂單，格式：order_id: 新數量".to_string()),
        optional: false,
        min_length: None,
        max_length: Some(3000),
        data_source: None,
        options: None,
        default: Some(yaml),
        subtype: None,
    }];

    let state = serde_json::json!({
        "group_buy_id": params.group_buy_id,
        "version": params.version,
    })
    .to_string();

    let dialog_url = format!(
        "{}/api/v1/group_buy/dialog/adjust_shortage",
        params.bot_callback_url.trim_end_matches('/')
    );

    client
        .open_dialog(
            params.trigger_id,
            &dialog_url,
            "調整缺貨",
            &elements,
            Some("確認調整"),
            None,
            Some(&state),
        )
        .await?;

    Ok(())
}

/// Parameters for opening the adjust-shortage dialog.
pub struct AdjustShortageDialogParams<'a> {
    pub trigger_id: &'a str,
    pub group_buy_id: &'a str,
    pub orders: &'a [GroupBuyOrder],
    pub version: i32,
    pub bot_callback_url: &'a str,
}

// parse adjustments yaml
pub fn parse_adjustments_yaml(yaml: &str) -> Result<HashMap<String, i32>> {
    let mut adjustments = HashMap::new();

    for line in yaml.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = line.splitn(2, ':').collect();
        if parts.len() != 2 {
            continue;
        }

        let order_id = parts[0].trim();
        let quantity_str = parts[1].trim();

        if order_id.is_empty() {
            continue;
        }

        let quantity: i32 = quantity_str
            .parse()
            .map_err(|_| anyhow::anyhow!("數量必須是整數：{}", quantity_str))?;

        if quantity < 0 {
            anyhow::bail!("數量不能為負數");
        }

        adjustments.insert(order_id.to_string(), quantity);
    }

    Ok(adjustments)
}

// Handle adjust shortage submission
pub async fn handle_adjust_shortage_dialog(
    form: HashMap<String, String>,
    state: Arc<RwLock<AppState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    info!("收到調整缺貨 Dialog 提交");

    let json_str = if let Some(payload) = form.get("payload") {
        payload.clone()
    } else if form.len() == 1 {
        form.keys().next().unwrap().clone()
    } else {
        error!("Dialog submission 格式不正確");
        return Err(warp::reject::reject());
    };

    let submission: DialogSubmission = serde_json::from_str(&json_str).map_err(|e| {
        error!("解析 Dialog submission 失敗: {}", e);
        warp::reject::reject()
    })?;

    let state_data: serde_json::Value = if let Some(state_str) = &submission.state {
        serde_json::from_str(state_str).map_err(|e| {
            error!("解析 state 失敗: {}", e);
            warp::reject::reject()
        })?
    } else {
        return Err(warp::reject::reject());
    };

    let group_buy_id = state_data
        .get("group_buy_id")
        .and_then(|v| v.as_str())
        .ok_or_else(warp::reject::reject)?
        .to_string();

    let adjustments_yaml = submission
        .submission
        .get("adjustments")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let adjustments = match parse_adjustments_yaml(adjustments_yaml) {
        Ok(adj) => adj,
        Err(e) => {
            return Ok(warp::reply::with_status(
                warp::reply::json(&DialogSubmissionResponse {
                    error: None,
                    text: None,
                    errors: Some(
                        [("adjustments".to_string(), format!("YAML 格式錯誤: {}", e))]
                            .into_iter()
                            .collect(),
                    ),
                }),
                StatusCode::OK,
            ));
        }
    };

    if adjustments.is_empty() {
        return Ok(warp::reply::with_status(
            warp::reply::json(&DialogSubmissionResponse {
                error: Some("沒有需要調整的訂單".to_string()),
                text: None,
                errors: None,
            }),
            StatusCode::OK,
        ));
    }

    let state_guard = state.read().await;

    let user = match state_guard
        .mattermost_client
        .get_user(&submission.user_id)
        .await
    {
        Ok(u) => u,
        Err(e) => {
            error!("取得用戶資訊失敗: {}", e);
            return Ok(warp::reply::with_status(
                warp::reply::json(&DialogSubmissionResponse {
                    error: Some("無法取得用戶資訊".to_string()),
                    text: None,
                    errors: None,
                }),
                StatusCode::OK,
            ));
        }
    };

    let state_guard = state_guard; // keep borrow

    for (order_id, new_quantity) in adjustments {
        if let Err(e) = state_guard
            .database
            .adjust_single_order(&order_id, new_quantity, &submission.user_id, &user.username)
            .await
        {
            error!("調整訂單 {} 數量失敗: {}", order_id, e);
            return Ok(warp::reply::with_status(
                warp::reply::json(&DialogSubmissionResponse {
                    error: Some(format!("調整訂單失敗: {}", e)),
                    text: None,
                    errors: None,
                }),
                StatusCode::OK,
            ));
        }
    }

    let _group_buy = match state_guard.database.get_group_buy(&group_buy_id).await {
        Ok(Some(gb)) => gb,
        _ => {
            return Ok(warp::reply::with_status(
                warp::reply::json(&DialogSubmissionResponse {
                    error: Some("取得團購資料失敗".to_string()),
                    text: None,
                    errors: None,
                }),
                StatusCode::OK,
            ));
        }
    };

    info!("{} 調整了團購 {} 的缺貨", user.username, group_buy_id);

    Ok(warp::reply::with_status(
        warp::reply::json(&DialogSubmissionResponse {
            error: None,
            text: None,
            errors: None,
        }),
        StatusCode::OK,
    ))
}
