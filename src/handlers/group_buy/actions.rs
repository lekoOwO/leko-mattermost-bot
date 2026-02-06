use super::*;
use crate::handlers::reply_helpers::{empty_json_reply, ephemeral_text_json};
use std::collections::HashMap;

/// 處理團購按鈕 Action（dispatcher）
pub async fn handle_group_buy_action(
    action_req: crate::mattermost::ActionRequest,
    state: Arc<RwLock<AppState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    info!("收到團購 Action: {:?}", action_req);

    // 取得 group_buy_id
    let group_buy_id = action_req
        .context
        .get("group_buy_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            error!("Action context 缺少 group_buy_id");
            warp::reject::reject()
        })?;

    // 檢查並更新 post_id（在獨立的作用域中），使用 utils::fetch_group_buy 以統一錯誤處理
    {
        let state_guard = state.read().await;
        match super::utils::fetch_group_buy(&state_guard, group_buy_id).await {
            Ok(group_buy) => {
                if group_buy.post_id.is_none() {
                    info!(
                        "更新團購 {} 的 post_id: {}",
                        group_buy_id, action_req.post_id
                    );
                    if let Err(e) = state_guard
                        .database
                        .update_post_id(group_buy_id, &action_req.post_id)
                        .await
                    {
                        error!("更新 post_id 失敗: {}", e);
                    }
                }
            }
            Err(msg) => {
                // 原先此處對錯誤不回覆使用者，僅記錄，因此這裡只記錯誤。
                tracing::debug!("fetch_group_buy for post_id update: {}", msg);
            }
        }
    }

    let action = action_req
        .context
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    match action {
        "edit_items" => handle_edit_items_action(action_req, state).await,
        "register" => handle_register_action(action_req, state).await,
        "cancel_register" => handle_cancel_register_action(action_req, state).await,
        "close" => handle_close_action(action_req, state).await,
        "reopen" => handle_reopen_action(action_req, state).await,
        "adjust_shortage" => handle_adjust_shortage_action(action_req, state).await,
        "shopping_list" => handle_shopping_list_action(action_req, state).await,
        "subtotal" => handle_subtotal_action(action_req, state).await,
        _ => {
            error!("未知的 action: {}", action);
            Ok(ephemeral_text_json("未知的操作"))
        }
    }
}

/// 處理「編輯商品」按鈕
async fn handle_edit_items_action(
    action_req: crate::mattermost::ActionRequest,
    state: Arc<RwLock<AppState>>,
) -> Result<warp::reply::Json, warp::Rejection> {
    let group_buy_id = action_req
        .context
        .get("group_buy_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let state_guard = state.read().await;

    // 取得團購資料（使用 utils::fetch_group_buy 簡化錯誤回覆）
    let group_buy = match super::utils::fetch_group_buy(&state_guard, group_buy_id).await {
        Ok(gb) => gb,
        Err(msg) => {
            return Ok(warp::reply::json(
                &serde_json::json!({"ephemeral_text": msg}),
            ));
        }
    };

    // 檢查權限：只有建立者可以編輯
    if let Err(msg) = super::utils::check_creator_permission(&group_buy, &action_req.user_id, "編輯商品") {
        return Ok(ephemeral_text_json(&msg));
    }

    // 檢查狀態：只有 Active 狀態可以編輯
    if let Err(msg) = super::utils::check_active_status(&group_buy, "編輯商品") {
        return Ok(ephemeral_text_json(&msg));
    }

    // 將當前商品轉換為 YAML 格式（helper in dialogs submodule）
    let items_yaml = super::dialogs::items_to_yaml(&group_buy.items);

    // 打開編輯商品的 Dialog
    let trigger_id = action_req.trigger_id.as_ref().ok_or_else(|| {
        error!("Action 缺少 trigger_id");
        warp::reject::reject()
    })?;

    let bot_callback_url = super::utils::bot_callback_url_from_state(&state_guard);

    let edit_params = super::dialogs::EditItemsDialogParams {
        trigger_id: trigger_id.as_str(),
        group_buy_id,
        items_yaml: items_yaml.as_str(),
        version: group_buy.version,
        post_id: group_buy.post_id.as_deref(), // 傳遞 post_id
        bot_callback_url: bot_callback_url.as_str(),
    };

    if let Err(e) =
        super::dialogs::open_edit_items_dialog(state_guard.mattermost_client.as_ref(), &edit_params).await
    {
        error!("打開編輯商品 Dialog 失敗: {}", e);
        return Ok(ephemeral_text_json("打開編輯視窗失敗"));
    }

    Ok(empty_json_reply())
}

async fn handle_register_action(
    action_req: crate::mattermost::ActionRequest,
    state: Arc<RwLock<AppState>>,
) -> Result<warp::reply::Json, warp::Rejection> {
    let group_buy_id = action_req
        .context
        .get("group_buy_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let state_guard = state.read().await;

    // 取得團購資料
    let group_buy = match state_guard.database.get_group_buy(group_buy_id).await {
        Ok(Some(gb)) => gb,
        Ok(None) => {
            return Ok(ephemeral_text_json("找不到該團購"));
        }
        Err(e) => {
            error!("取得團購資料失敗: {}", e);
            return Ok(ephemeral_text_json("取得團購資料失敗"));
        }
    };

    // 檢查狀態
    if group_buy.status != GroupBuyStatus::Active {
        return Ok(ephemeral_text_json("⚠️ 此團購已截止，無法登記"));
    }

    // 檢查是否有商品
    if group_buy.items.is_empty()
        || (group_buy.items.len() == 1 && group_buy.items.contains_key("範例商品"))
    {
        return Ok(ephemeral_text_json("⚠️ 請先編輯商品列表"));
    }

    // 打開登記 Dialog
    let trigger_id = action_req.trigger_id.as_ref().ok_or_else(|| {
        error!("Action 缺少 trigger_id");
        warp::reject::reject()
    })?;

    let bot_callback_url = super::utils::bot_callback_url_from_state(&state_guard);

    // 建立 introduction_text：顯示該使用者目前已登記的商品（表格）
    let intro_text = match state_guard
        .database
        .get_buyer_orders(group_buy_id, &action_req.user_id)
        .await
    {
        Ok(orders) if !orders.is_empty() => {
            let mut s = String::new();
            s.push_str("已購買項目：\n\n| 商品 | 數量 | 小計 |\n|------|----:|-----:|\n");
            use std::collections::HashMap;
            let mut by_item: HashMap<String, (i32, rust_decimal::Decimal)> = HashMap::new();
            for o in orders {
                let entry = by_item
                    .entry(o.item_name.clone())
                    .or_insert((0, o.unit_price));
                entry.0 += o.quantity;
            }
            for (name, (qty, price)) in by_item {
                let subtotal = price * rust_decimal::Decimal::from(qty);
                s.push_str(&format!("| {} | {} | ${} |\n", name, qty, subtotal));
            }
            Some(s)
        }
        _ => None,
    };

    let register_params = super::dialogs::RegisterDialogParams {
        trigger_id: trigger_id.as_str(),
        group_buy_id,
        items: &group_buy.items,
        version: group_buy.version,
        post_id: group_buy.post_id.as_deref(), // 傳遞 post_id
        introduction_text: intro_text.as_deref(),
        bot_callback_url: bot_callback_url.as_str(),
    };

    if let Err(e) =
        super::dialogs::open_register_dialog(state_guard.mattermost_client.as_ref(), &register_params).await
    {
        error!("打開登記 Dialog 失敗: {}", e);
        return Ok(ephemeral_text_json("打開登記視窗失敗"));
    }

    Ok(empty_json_reply())
}

async fn handle_cancel_register_action(
    action_req: crate::mattermost::ActionRequest,
    state: Arc<RwLock<AppState>>,
) -> Result<warp::reply::Json, warp::Rejection> {
    let group_buy_id = action_req
        .context
        .get("group_buy_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let state_guard = state.read().await;

    // 取得團購資料（使用 utils helper）
    let group_buy = match super::utils::fetch_group_buy(&state_guard, group_buy_id).await {
        Ok(gb) => gb,
        Err(msg) => {
            return Ok(warp::reply::json(
                &serde_json::json!({"ephemeral_text": msg}),
            ));
        }
    };

    // 取得所有訂單，用以建構被登記人選項與介紹文字
    let orders = state_guard
        .database
        .get_all_orders(group_buy_id)
        .await
        .unwrap_or_default();

    if orders.is_empty() {
        return Ok(ephemeral_text_json("目前沒有任何登記可供取消"));
    }

    use std::collections::HashMap;
    let mut buyers: HashMap<String, String> = HashMap::new(); // buyer_id -> buyer_username
    for o in &orders {
        buyers.insert(o.buyer_id.clone(), o.buyer_username.clone());
    }

    let mut buyer_options: Vec<DialogOption> = Vec::new();
    for (id, username) in &buyers {
        buyer_options.push(DialogOption {
            text: format!("@{}", username),
            value: id.clone(),
        });
    }

    let mut intro = String::new();
    intro.push_str("目前登記：\n\n| 被登記人 | 商品 | 數量 | 登記人 |\n|---|---|---:|---|\n");
    for o in &orders {
        intro.push_str(&format!(
            "| @{} | {} | {} | @{} |\n",
            o.buyer_username, o.item_name, o.quantity, o.registrar_username
        ));
    }

    let trigger_id = action_req.trigger_id.as_ref().ok_or_else(|| {
        error!("Action 缺少 trigger_id");
        warp::reject::reject()
    })?;

    let bot_callback_url = super::utils::bot_callback_url_from_state(&state_guard);

    let cancel_params = super::dialogs::CancelRegisterDialogParams {
        trigger_id: trigger_id.to_string(),
        group_buy_id: group_buy_id.to_string(),
        buyer_options: buyer_options.clone(),
        version: group_buy.version,
        post_id: group_buy.post_id.clone(),
        introduction_text: Some(intro.clone()),
        bot_callback_url: bot_callback_url.clone(),
    };

    if let Err(e) =
        super::dialogs::open_cancel_register_dialog(state_guard.mattermost_client.as_ref(), &cancel_params)
            .await
    {
        error!("打開取消登記 Dialog 失敗: {}", e);
        return Ok(ephemeral_text_json("打開取消登記視窗失敗"));
    }

    Ok(empty_json_reply())
}

async fn handle_close_action(
    action_req: crate::mattermost::ActionRequest,
    state: Arc<RwLock<AppState>>,
) -> Result<warp::reply::Json, warp::Rejection> {
    let group_buy_id = action_req
        .context
        .get("group_buy_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let state_guard = state.read().await;

    // 取得團購資料（使用 utils helper）
    let group_buy = match super::utils::fetch_group_buy(&state_guard, group_buy_id).await {
        Ok(gb) => gb,
        Err(msg) => {
            return Ok(warp::reply::json(
                &serde_json::json!({"ephemeral_text": msg}),
            ));
        }
    };

    // 檢查權限：只有建立者可以截止
    if let Err(msg) = super::utils::check_creator_permission(&group_buy, &action_req.user_id, "截止") {
        return Ok(ephemeral_text_json(&msg));
    }

    // 檢查狀態
    if let Err(msg) = super::utils::check_active_status(&group_buy, "截止") {
        return Ok(ephemeral_text_json(&msg));
    }

    // 取得用戶資訊
    let user = match state_guard
        .mattermost_client
        .get_user(&action_req.user_id)
        .await
    {
        Ok(u) => u,
        Err(e) => {
            error!("取得用戶資訊失敗: {}", e);
            return Ok(ephemeral_text_json("無法取得用戶資訊"));
        }
    };

    // 更新狀態
    if let Err(e) = state_guard
        .database
        .update_status(
            group_buy_id,
            GroupBuyStatus::Closed,
            group_buy.version,
            &action_req.user_id,
            &user.username,
        )
        .await
    {
        error!("更新狀態失敗: {}", e);
        return Ok(ephemeral_text_json(format!("截止失敗: {}", e)));
    }

    // 重新取得團購資料
    let group_buy = match state_guard.database.get_group_buy(group_buy_id).await {
        Ok(Some(gb)) => gb,
        _ => {
            return Ok(ephemeral_text_json("取得團購資料失敗"));
        }
    };

    // 準備更新後的訊息
    let bot_callback_url = super::utils::bot_callback_url_from_state(&state_guard);

    let orders = state_guard
        .database
        .get_orders_by_group_buy(group_buy_id)
        .await
        .unwrap_or_default();

    let message = generate_group_buy_message_with_orders(
        &group_buy.merchant_name,
        &group_buy.description,
        &group_buy.metadata,
        &group_buy.status,
        &group_buy.items,
        &orders,
    );

    let attachments = generate_action_buttons(group_buy_id, &group_buy.status, &bot_callback_url);

    info!("{} 截止了團購 {}", user.username, group_buy_id);

    Ok(warp::reply::json(&serde_json::json!({
        "update": {
            "message": message,
            "props": {
                "attachments": attachments
            }
        }
    })))
}

async fn handle_reopen_action(
    action_req: crate::mattermost::ActionRequest,
    state: Arc<RwLock<AppState>>,
) -> Result<warp::reply::Json, warp::Rejection> {
    let group_buy_id = action_req
        .context
        .get("group_buy_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let state_guard = state.read().await;

    // 取得團購資料（使用 utils helper）
    let group_buy = match super::utils::fetch_group_buy(&state_guard, group_buy_id).await {
        Ok(gb) => gb,
        Err(msg) => {
            return Ok(warp::reply::json(
                &serde_json::json!({"ephemeral_text": msg}),
            ));
        }
    };

    // 檢查權限：只有建立者可以重新開放
    if let Err(msg) = super::utils::check_creator_permission(&group_buy, &action_req.user_id, "重新開放") {
        return Ok(ephemeral_text_json(&msg));
    }

    // 檢查狀態
    if let Err(msg) = super::utils::check_closed_status(&group_buy) {
        return Ok(ephemeral_text_json(&msg));
    }

    // 取得用戶資訊
    let user = match state_guard
        .mattermost_client
        .get_user(&action_req.user_id)
        .await
    {
        Ok(u) => u,
        Err(e) => {
            error!("取得用戶資訊失敗: {}", e);
            return Ok(ephemeral_text_json("無法取得用戶資訊"));
        }
    };

    // 更新狀態
    if let Err(e) = state_guard
        .database
        .update_status(
            group_buy_id,
            GroupBuyStatus::Active,
            group_buy.version,
            &action_req.user_id,
            &user.username,
        )
        .await
    {
        error!("更新狀態失敗: {}", e);
        return Ok(ephemeral_text_json(format!("重新開放失敗: {}", e)));
    }

    // 重新取得團購資料
    let group_buy = match state_guard.database.get_group_buy(group_buy_id).await {
        Ok(Some(gb)) => gb,
        _ => {
            return Ok(ephemeral_text_json("取得團購資料失敗"));
        }
    };

    // 準備更新後的訊息
    let bot_callback_url = super::utils::bot_callback_url_from_state(&state_guard);

    let orders = state_guard
        .database
        .get_orders_by_group_buy(group_buy_id)
        .await
        .unwrap_or_default();

    let message = generate_group_buy_message_with_orders(
        &group_buy.merchant_name,
        &group_buy.description,
        &group_buy.metadata,
        &group_buy.status,
        &group_buy.items,
        &orders,
    );

    let attachments = generate_action_buttons(group_buy_id, &group_buy.status, &bot_callback_url);

    info!("{} 重新開放了團購 {}", user.username, group_buy_id);

    Ok(warp::reply::json(&serde_json::json!({
        "update": {
            "message": message,
            "props": {
                "attachments": attachments
            }
        }
    })))
}

async fn handle_adjust_shortage_action(
    action_req: crate::mattermost::ActionRequest,
    state: Arc<RwLock<AppState>>,
) -> Result<warp::reply::Json, warp::Rejection> {
    let group_buy_id = action_req
        .context
        .get("group_buy_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let state_guard = state.read().await;

    // 取得團購資料（使用 utils helper）
    let group_buy = match super::utils::fetch_group_buy(&state_guard, group_buy_id).await {
        Ok(gb) => gb,
        Err(msg) => {
            return Ok(warp::reply::json(
                &serde_json::json!({"ephemeral_text": msg}),
            ));
        }
    };

    // 檢查權限：只有建立者可以調整
    if let Err(msg) = super::utils::check_creator_permission(&group_buy, &action_req.user_id, "調整缺貨") {
        return Ok(ephemeral_text_json(&msg));
    }

    // 檢查狀態：只有 Closed 可以調整
    if let Err(msg) = super::utils::check_closed_status(&group_buy) {
        return Ok(ephemeral_text_json(&msg));
    }

    // 取得訂單
    let orders = match state_guard
        .database
        .get_orders_by_group_buy(group_buy_id)
        .await
    {
        Ok(o) => o,
        Err(e) => {
            error!("取得訂單失敗: {}", e);
            return Ok(ephemeral_text_json("取得訂單失敗"));
        }
    };

    if orders.is_empty() {
        return Ok(ephemeral_text_json("尚無登記資料"));
    }

    // 打開調整缺貨 Dialog
    let trigger_id = action_req.trigger_id.as_ref().ok_or_else(|| {
        error!("Action 缺少 trigger_id");
        warp::reject::reject()
    })?;

    let bot_callback_url = super::utils::bot_callback_url_from_state(&state_guard);

    let adjust_params = super::dialogs::AdjustShortageDialogParams {
        trigger_id: trigger_id.as_str(),
        group_buy_id,
        orders: &orders,
        version: group_buy.version,
        bot_callback_url: bot_callback_url.as_str(),
    };

    if let Err(e) =
        super::dialogs::open_adjust_shortage_dialog(state_guard.mattermost_client.as_ref(), &adjust_params)
            .await
    {
        error!("打開調整缺貨 Dialog 失敗: {}", e);
        return Ok(ephemeral_text_json("打開調整視窗失敗"));
    }

    Ok(empty_json_reply())
}

async fn handle_shopping_list_action(
    action_req: crate::mattermost::ActionRequest,
    state: Arc<RwLock<AppState>>,
) -> Result<warp::reply::Json, warp::Rejection> {
    let group_buy_id = action_req
        .context
        .get("group_buy_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let state_guard = state.read().await;

    // 取得團購資料
    let group_buy = match state_guard.database.get_group_buy(group_buy_id).await {
        Ok(Some(gb)) => gb,
        Ok(None) => {
            return Ok(warp::reply::json(&serde_json::json!({
                "ephemeral_text": "找不到該團購"
            })));
        }
        Err(e) => {
            error!("取得團購資料失敗: {}", e);
            return Ok(warp::reply::json(&serde_json::json!({
                "ephemeral_text": "取得團購資料失敗"
            })));
        }
    };

    // 取得訂單
    let orders = match state_guard
        .database
        .get_orders_by_group_buy(group_buy_id)
        .await
    {
        Ok(o) => o,
        Err(e) => {
            error!("取得訂單失敗: {}", e);
            return Ok(warp::reply::json(&serde_json::json!({
                "ephemeral_text": "取得訂單失敗"
            })));
        }
    };

    if orders.is_empty() {
        return Ok(warp::reply::json(&serde_json::json!({
            "ephemeral_text": "尚無登記資料"
        })));
    }

    // 統計每個商品的總數量
    let mut shopping_list: HashMap<String, i32> = HashMap::new();
    for order in &orders {
        *shopping_list.entry(order.item_name.clone()).or_insert(0) += order.quantity;
    }

    // 計算統計資訊
    let num_items = shopping_list.len();
    let num_people: std::collections::HashSet<_> =
        orders.iter().map(|o| o.buyer_id.clone()).collect();

    // 生成採購列表訊息（使用表格）
    let mut msg = "### 🛍️ 採購列表\n\n".to_string();
    msg.push_str(&format!(
        "**商家：{}  •  品項：{}  •  人數：{}**\n\n",
        group_buy.merchant_name,
        num_items,
        num_people.len()
    ));
    msg.push_str("| 商品 | 數量 | 單價 | 小計 |\n");
    msg.push_str("|------|-----:|-----:|-----:|\n");

    // 排序商品名稱
    let mut sorted_items: Vec<_> = shopping_list.iter().collect();
    sorted_items.sort_by_key(|(name, _)| *name);

    for (item_name, total_qty) in sorted_items {
        let price = group_buy
            .items
            .get(item_name)
            .copied()
            .unwrap_or(Decimal::ZERO);
        let subtotal = price * Decimal::from(*total_qty);
        msg.push_str(&format!(
            "| {} | {} | ${} | ${} |\n",
            item_name, total_qty, price, subtotal
        ));
    }

    // 計算總金額（使用 Decimal 進行精確計算）
    let total_amount: Decimal = orders
        .iter()
        .map(|o| o.unit_price * Decimal::from(o.quantity))
        .sum();

    msg.push_str(&format!("\n**💰 總金額：NT${}**", total_amount));

    Ok(ephemeral_text_json(msg))
}

async fn handle_subtotal_action(
    action_req: crate::mattermost::ActionRequest,
    state: Arc<RwLock<AppState>>,
) -> Result<warp::reply::Json, warp::Rejection> {
    let group_buy_id = action_req
        .context
        .get("group_buy_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let state_guard = state.read().await;

    // 取得團購資料
    let group_buy = match state_guard.database.get_group_buy(group_buy_id).await {
        Ok(Some(gb)) => gb,
        Ok(None) => {
            return Ok(warp::reply::json(&serde_json::json!({
                "ephemeral_text": "找不到該團購"
            })));
        }
        Err(e) => {
            error!("取得團購資料失敗: {}", e);
            return Ok(warp::reply::json(&serde_json::json!({
                "ephemeral_text": "取得團購資料失敗"
            })));
        }
    };

    // 取得訂單
    let orders = match state_guard
        .database
        .get_orders_by_group_buy(group_buy_id)
        .await
    {
        Ok(o) => o,
        Err(e) => {
            error!("取得訂單失敗: {}", e);
            return Ok(warp::reply::json(&serde_json::json!({
                "ephemeral_text": "取得訂單失敗"
            })));
        }
    };

    if orders.is_empty() {
        return Ok(warp::reply::json(&serde_json::json!({
            "ephemeral_text": "尚無登記資料"
        })));
    }

    // 按購買人分組統計（使用 Decimal 進行精確計算）
    let mut subtotals: HashMap<String, Decimal> = HashMap::new();
    for order in &orders {
        let item_total = order.unit_price * Decimal::from(order.quantity);
        *subtotals
            .entry(order.buyer_username.clone())
            .or_insert(Decimal::ZERO) += item_total;
    }

    // 排序（按金額由高到低）
    let mut sorted_subtotals: Vec<_> = subtotals.iter().collect();
    sorted_subtotals.sort_by(|a, b| b.1.cmp(a.1));

    // 生成小計訊息（使用表格）
    let num_people = subtotals.len();
    let mut msg = "### 💰 個人小計\n\n".to_string();
    msg.push_str(&format!(
        "**商家：{}  •  人數：{}**\n\n",
        group_buy.merchant_name, num_people
    ));
    msg.push_str("| 訂購人 | 金額 |\n");
    msg.push_str("|--------|-----:|\n");

    for (buyer, amount) in sorted_subtotals {
        msg.push_str(&format!("| @{} | ${} |\n", buyer, amount));
    }

    // 總金額（使用 Decimal 進行精確計算）
    let total_amount: Decimal = orders
        .iter()
        .map(|o| o.unit_price * Decimal::from(o.quantity))
        .sum();

    msg.push_str(&format!("\n**🧮 總計：NT${}**", total_amount));

    Ok(warp::reply::json(&serde_json::json!({
        "ephemeral_text": msg
    })))
}
