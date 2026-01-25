#[cfg(test)]
pub mod utils {
    use crate::database::{Database, GroupBuy, GroupBuyOrder, GroupBuyStatus};
    use chrono::Utc;
    use rust_decimal::Decimal;

    pub async fn setup_db() -> Database {
        Database::new("sqlite::memory:").await.expect("db init")
    }

    pub fn make_group_buy(id: String, version: i32) -> GroupBuy {
        GroupBuy {
            id,
            creator_id: "creator".to_string(),
            creator_username: "creator".to_string(),
            channel_id: "chan".to_string(),
            post_id: None,
            merchant_name: "shop".to_string(),
            description: None,
            metadata: std::collections::HashMap::new(),
            items: [("apple".to_string(), Decimal::new(1000, 2))]
                .into_iter()
                .collect(),
            status: GroupBuyStatus::Active,
            version,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    pub fn make_order_for(gb_id: String, buyer: &str, registrar: &str) -> GroupBuyOrder {
        GroupBuyOrder {
            id: uuid::Uuid::new_v4().to_string(),
            group_buy_id: gb_id,
            registrar_id: registrar.to_string(),
            registrar_username: registrar.to_string(),
            buyer_id: buyer.to_string(),
            buyer_username: buyer.to_string(),
            item_name: "apple".to_string(),
            quantity: 2,
            original_quantity: None,
            unit_price: Decimal::new(1000, 2),
            created_at: Utc::now(),
        }
    }

    pub async fn insert_group_buy(db: &Database, version: i32) -> GroupBuy {
        let id = uuid::Uuid::new_v4().to_string();
        let gb = make_group_buy(id.clone(), version);
        db.create_group_buy(&gb).await.expect("create gb");
        gb
    }

    pub async fn create_and_insert_order(
        db: &Database,
        gb_id: &str,
        buyer: &str,
        registrar: &str,
        quantity: i32,
    ) -> GroupBuyOrder {
        let mut order = make_order_for(gb_id.to_string(), buyer, registrar);
        order.quantity = quantity;
        db.create_order(&order).await.expect("create order");
        order
    }

    pub async fn close_group_buy(db: &Database, id: &str, expected_version: i32) {
        db.update_status(
            id,
            GroupBuyStatus::Closed,
            expected_version,
            "tester",
            "tester",
        )
        .await
        .expect("close gb");
    }
}
