use crate::database::{GroupBuyOrder, GroupBuyStatus};
use rust_decimal::Decimal;
use serde_json::json;
use std::collections::HashMap;

/// 生成團購訊息內容
pub fn generate_group_buy_message(
    merchant_name: &str,
    description: &Option<String>,
    metadata: &HashMap<String, String>,
    status: &GroupBuyStatus,
    items: &HashMap<String, Decimal>,
) -> String {
    let mut msg = String::new();

    // 狀態標記
    if *status == GroupBuyStatus::Closed {
        msg.push_str("🔒 **【已截止】** ");
    }

    msg.push_str(&format!("🛒 **【團購】{}**\n\n", merchant_name));

    // 描述
    if let Some(desc) = description
        && !desc.is_empty()
    {
        msg.push_str(&format!("📝 **描述:**\n{}\n\n", desc));
    }

    // 其他資訊
    if !metadata.is_empty() {
        msg.push_str("ℹ️ **其他資訊:**\n");
        for (key, value) in metadata {
            msg.push_str(&format!("• {}: {}\n", key, value));
        }
        msg.push('\n');
    }

    // 商品列表（如果有且不只是範例）
    if !(items.is_empty() || (items.len() == 1 && items.contains_key("範例商品"))) {
        msg.push_str("🍱 **商品列表:**\n");
        for (item, price) in items {
            // 格式化價格，移除不必要的尾部零
            msg.push_str(&format!("• {} - NT${}\n", item, price));
        }
        msg.push('\n');
    }

    msg.push_str("━━━━━━━━━━━━━━━━━━━━\n");

    msg
}

/// 生成操作按鈕
pub fn generate_action_buttons(
    group_buy_id: &str,
    status: &GroupBuyStatus,
    bot_callback_url: &str,
) -> Vec<serde_json::Value> {
    let mut actions = Vec::new();

    // 移除 group_buy_id 中的 hyphen，使其成為有效的 action id
    let clean_id = group_buy_id.replace("-", "");

    match status {
        GroupBuyStatus::Active => {
            // 編輯商品
            actions.push(json!({
                "id": format!("edititems{}", clean_id),
                "name": "編輯商品",
                "type": "button",
                "integration": {
                    "url": format!("{}/api/v1/group_buy/action/edit_items", bot_callback_url.trim_end_matches('/')),
                    "context": {
                        "action": "edit_items",
                        "group_buy_id": group_buy_id,
                    }
                }
            }));

            // 登記
            actions.push(json!({
                "id": format!("register{}", clean_id),
                "name": "登記",
                "type": "button",
                "integration": {
                    "url": format!("{}/api/v1/group_buy/action/register", bot_callback_url.trim_end_matches('/')),
                    "context": {
                        "action": "register",
                        "group_buy_id": group_buy_id,
                    }
                }
            }));

            // 取消登記（清除某一被登記人的所有登記）
            actions.push(json!({
                "id": format!("cancelregister{}", clean_id),
                "name": "取消登記",
                "type": "button",
                "integration": {
                    "url": format!("{}/api/v1/group_buy/action/cancel_register", bot_callback_url.trim_end_matches('/')),
                    "context": {
                        "action": "cancel_register",
                        "group_buy_id": group_buy_id,
                    }
                }
            }));

            // 截止
            actions.push(json!({
                "id": format!("close{}", clean_id),
                "name": "截止",
                "type": "button",
                "integration": {
                    "url": format!("{}/api/v1/group_buy/action/close", bot_callback_url.trim_end_matches('/')),
                    "context": {
                        "action": "close",
                        "group_buy_id": group_buy_id,
                    }
                }
            }));
        }
        GroupBuyStatus::Closed => {
            // 重新開放
            actions.push(json!({
                "id": format!("reopen{}", clean_id),
                "name": "重新開放",
                "type": "button",
                "integration": {
                    "url": format!("{}/api/v1/group_buy/action/reopen", bot_callback_url.trim_end_matches('/')),
                    "context": {
                        "action": "reopen",
                        "group_buy_id": group_buy_id,
                    }
                }
            }));

            // 調整缺貨
            actions.push(json!({
                "id": format!("adjustshortage{}", clean_id),
                "name": "調整缺貨",
                "type": "button",
                "integration": {
                    "url": format!("{}/api/v1/group_buy/action/adjust_shortage", bot_callback_url.trim_end_matches('/')),
                    "context": {
                        "action": "adjust_shortage",
                        "group_buy_id": group_buy_id,
                    }
                }
            }));
        }
    }

    // 這些按鈕在任何狀態都顯示
    actions.push(json!({
        "id": format!("shoppinglist{}", clean_id),
        "name": "採購列表",
        "type": "button",
        "integration": {
            "url": format!("{}/api/v1/group_buy/action/shopping_list", bot_callback_url.trim_end_matches('/')),
            "context": {
                "action": "shopping_list",
                "group_buy_id": group_buy_id,
            }
        }
    }));

    actions.push(json!({
        "id": format!("subtotal{}", clean_id),
        "name": "小計",
        "type": "button",
        "integration": {
            "url": format!("{}/api/v1/group_buy/action/subtotal", bot_callback_url.trim_end_matches('/')),
            "context": {
                "action": "subtotal",
                "group_buy_id": group_buy_id,
            }
        }
    }));

    vec![json!({
        "actions": actions
    })]
}

/// 生成包含訂單的團購訊息
pub fn generate_group_buy_message_with_orders(
    merchant_name: &str,
    description: &Option<String>,
    metadata: &HashMap<String, String>,
    status: &GroupBuyStatus,
    items: &HashMap<String, Decimal>,
    orders: &[GroupBuyOrder],
) -> String {
    let mut msg = generate_group_buy_message(merchant_name, description, metadata, status, items);

    if !orders.is_empty() {
        msg.push_str("\n📋 **登記名單:**\n");

        // 按商品分組
        let mut orders_by_item: HashMap<String, Vec<&GroupBuyOrder>> = HashMap::new();
        for order in orders {
            orders_by_item
                .entry(order.item_name.clone())
                .or_default()
                .push(order);
        }

        for (item_name, item_orders) in orders_by_item {
            let total_qty: i32 = item_orders.iter().map(|o| o.quantity).sum();
            msg.push_str(&format!("\n**{}** (共 {} 份):\n", item_name, total_qty));

            for order in item_orders {
                let registrar_note = if order.registrar_id != order.buyer_id {
                    format!(" (由 @{} 登記)", order.registrar_username)
                } else {
                    String::new()
                };
                msg.push_str(&format!(
                    "• @{} x{}{}\n",
                    order.buyer_username, order.quantity, registrar_note
                ));
            }
        }
        msg.push('\n');
    }

    msg
}
