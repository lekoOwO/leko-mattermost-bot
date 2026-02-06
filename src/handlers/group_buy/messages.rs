use crate::database::{GroupBuyOrder, GroupBuyStatus};
use crate::constants::group_buy::EXAMPLE_ITEM_NAME;
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
    if !(items.is_empty() || (items.len() == 1 && items.contains_key(EXAMPLE_ITEM_NAME))) {
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

/// 建構器模式，用於生成團購操作按鈕
struct ActionButtonBuilder {
    group_buy_id: String,
    clean_id: String,
    bot_callback_url: String,
}

impl ActionButtonBuilder {
    fn new(group_buy_id: &str, bot_callback_url: &str) -> Self {
        Self {
            group_buy_id: group_buy_id.to_string(),
            clean_id: group_buy_id.replace("-", ""),
            bot_callback_url: bot_callback_url.trim_end_matches('/').to_string(),
        }
    }
    
    /// 建立一個操作按鈕
    fn build(&self, action: &str, name: &str) -> serde_json::Value {
        json!({
            "id": format!("{}{}", action.replace("_", ""), self.clean_id),
            "name": name,
            "type": "button",
            "integration": {
                "url": format!("{}/api/v1/group_buy/action/{}", self.bot_callback_url, action),
                "context": {
                    "action": action,
                    "group_buy_id": &self.group_buy_id,
                }
            }
        })
    }
}

/// 生成操作按鈕
pub fn generate_action_buttons(
    group_buy_id: &str,
    status: &GroupBuyStatus,
    bot_callback_url: &str,
) -> Vec<serde_json::Value> {
    let builder = ActionButtonBuilder::new(group_buy_id, bot_callback_url);
    let mut actions = Vec::new();

    match status {
        GroupBuyStatus::Active => {
            actions.push(builder.build("edit_items", "編輯商品"));
            actions.push(builder.build("register", "登記"));
            actions.push(builder.build("cancel_register", "取消登記"));
            actions.push(builder.build("close", "截止"));
            actions.push(builder.build("close", "截止"));
        }
        GroupBuyStatus::Closed => {
            actions.push(builder.build("reopen", "重新開放"));
            actions.push(builder.build("adjust_shortage", "調整缺貨"));
        }
    }

    // 這些按鈕在任何狀態都顯示
    actions.push(builder.build("shopping_list", "採購列表"));
    actions.push(builder.build("subtotal", "小計"));

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
