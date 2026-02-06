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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_group_buy_message_basic() {
        let items = HashMap::new();
        let metadata = HashMap::new();
        let status = GroupBuyStatus::Active;

        let msg = generate_group_buy_message(
            "測試店家",
            &None,
            &metadata,
            &status,
            &items,
        );

        assert!(msg.contains("🛒 **【團購】測試店家**"));
        assert!(msg.contains("━━━━━━━━━━━━━━━━━━━━"));
        assert!(!msg.contains("🔒"));
    }

    #[test]
    fn test_generate_group_buy_message_closed() {
        let items = HashMap::new();
        let metadata = HashMap::new();
        let status = GroupBuyStatus::Closed;

        let msg = generate_group_buy_message(
            "測試店家",
            &None,
            &metadata,
            &status,
            &items,
        );

        assert!(msg.contains("🔒 **【已截止】**"));
    }

    #[test]
    fn test_generate_group_buy_message_with_description() {
        let items = HashMap::new();
        let metadata = HashMap::new();
        let status = GroupBuyStatus::Active;
        let description = Some("這是一個測試描述".to_string());

        let msg = generate_group_buy_message(
            "測試店家",
            &description,
            &metadata,
            &status,
            &items,
        );

        assert!(msg.contains("📝 **描述:**"));
        assert!(msg.contains("這是一個測試描述"));
    }

    #[test]
    fn test_generate_group_buy_message_with_metadata() {
        let items = HashMap::new();
        let mut metadata = HashMap::new();
        metadata.insert("截止時間".to_string(), "2026-02-10".to_string());
        metadata.insert("取貨地點".to_string(), "辦公室".to_string());
        let status = GroupBuyStatus::Active;

        let msg = generate_group_buy_message(
            "測試店家",
            &None,
            &metadata,
            &status,
            &items,
        );

        assert!(msg.contains("ℹ️ **其他資訊:**"));
        assert!(msg.contains("截止時間"));
        assert!(msg.contains("2026-02-10"));
        assert!(msg.contains("取貨地點"));
        assert!(msg.contains("辦公室"));
    }

    #[test]
    fn test_generate_group_buy_message_with_items() {
        let mut items = HashMap::new();
        items.insert("炸雞".to_string(), Decimal::new(100, 0));
        items.insert("薯條".to_string(), Decimal::new(50, 0));
        let metadata = HashMap::new();
        let status = GroupBuyStatus::Active;

        let msg = generate_group_buy_message(
            "測試店家",
            &None,
            &metadata,
            &status,
            &items,
        );

        assert!(msg.contains("🍱 **商品列表:**"));
        assert!(msg.contains("炸雞 - NT$100"));
        assert!(msg.contains("薯條 - NT$50"));
    }

    #[test]
    fn test_generate_group_buy_message_skips_example_item() {
        let mut items = HashMap::new();
        items.insert(EXAMPLE_ITEM_NAME.to_string(), Decimal::new(0, 0));
        let metadata = HashMap::new();
        let status = GroupBuyStatus::Active;

        let msg = generate_group_buy_message(
            "測試店家",
            &None,
            &metadata,
            &status,
            &items,
        );

        // 不應該顯示範例商品
        assert!(!msg.contains("🍱 **商品列表:**"));
    }

    #[test]
    fn test_generate_action_buttons_active() {
        let buttons = generate_action_buttons(
            "test-id-123",
            &GroupBuyStatus::Active,
            "http://localhost:3000",
        );

        assert_eq!(buttons.len(), 1);
        let actions = buttons[0]["actions"].as_array().unwrap();
        
        // Active 狀態應該有 7 個按鈕
        assert_eq!(actions.len(), 7);
        
        // 檢查按鈕名稱
        let names: Vec<&str> = actions
            .iter()
            .map(|a| a["name"].as_str().unwrap())
            .collect();
        
        assert!(names.contains(&"編輯商品"));
        assert!(names.contains(&"登記"));
        assert!(names.contains(&"取消登記"));
        assert!(names.contains(&"截止"));
        assert!(names.contains(&"採購列表"));
        assert!(names.contains(&"小計"));
    }

    #[test]
    fn test_generate_action_buttons_closed() {
        let buttons = generate_action_buttons(
            "test-id-456",
            &GroupBuyStatus::Closed,
            "http://localhost:3000",
        );

        assert_eq!(buttons.len(), 1);
        let actions = buttons[0]["actions"].as_array().unwrap();
        
        // Closed 狀態應該有 4 個按鈕
        assert_eq!(actions.len(), 4);
        
        let names: Vec<&str> = actions
            .iter()
            .map(|a| a["name"].as_str().unwrap())
            .collect();
        
        assert!(names.contains(&"重新開放"));
        assert!(names.contains(&"調整缺貨"));
        assert!(names.contains(&"採購列表"));
        assert!(names.contains(&"小計"));
    }

    #[test]
    fn test_generate_group_buy_message_with_orders() {
        use chrono::Utc;
        
        let mut items = HashMap::new();
        items.insert("炸雞".to_string(), Decimal::new(100, 0));
        
        let metadata = HashMap::new();
        let status = GroupBuyStatus::Active;
        
        let orders = vec![
            GroupBuyOrder {
                id: "1".to_string(),
                group_buy_id: "test-id".to_string(),
                item_name: "炸雞".to_string(),
                buyer_id: "user1".to_string(),
                buyer_username: "User1".to_string(),
                registrar_id: "user1".to_string(),
                registrar_username: "User1".to_string(),
                quantity: 2,
                original_quantity: None,
                unit_price: Decimal::new(100, 0),
                created_at: Utc::now(),
            },
            GroupBuyOrder {
                id: "2".to_string(),
                group_buy_id: "test-id".to_string(),
                item_name: "炸雞".to_string(),
                buyer_id: "user2".to_string(),
                buyer_username: "User2".to_string(),
                registrar_id: "user3".to_string(),
                registrar_username: "User3".to_string(),
                quantity: 1,
                original_quantity: None,
                unit_price: Decimal::new(100, 0),
                created_at: Utc::now(),
            },
        ];

        let msg = generate_group_buy_message_with_orders(
            "測試店家",
            &None,
            &metadata,
            &status,
            &items,
            &orders,
        );

        assert!(msg.contains("📋 **登記名單:**"));
        assert!(msg.contains("**炸雞** (共 3 份)"));
        assert!(msg.contains("@User1 x2"));
        assert!(msg.contains("@User2 x1 (由 @User3 登記)"));
    }

    #[test]
    fn test_generate_group_buy_message_with_empty_orders() {
        let items = HashMap::new();
        let metadata = HashMap::new();
        let status = GroupBuyStatus::Active;
        let orders = vec![];

        let msg = generate_group_buy_message_with_orders(
            "測試店家",
            &None,
            &metadata,
            &status,
            &items,
            &orders,
        );

        // 沒有訂單時不應顯示登記名單
        assert!(!msg.contains("📋 **登記名單:**"));
    }
}
