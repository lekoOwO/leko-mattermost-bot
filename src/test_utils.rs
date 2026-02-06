#[cfg(test)]
pub mod utils {
    use crate::database::{Database, GroupBuy, GroupBuyOrder, GroupBuyStatus};
    use crate::mattermost::{
        MattermostService, Post, User, Channel, PostResponse, DialogElement,
    };
    use anyhow::Result;
    use async_trait::async_trait;
    use chrono::Utc;
    use rust_decimal::Decimal;
    use std::sync::{Arc, Mutex};
    use std::collections::HashMap;

    /// Mock Mattermost 服務，用於測試
    #[derive(Clone)]
    pub struct MockMattermostService {
        /// 儲存被呼叫的方法名稱和次數
        pub call_log: Arc<Mutex<Vec<String>>>,
        /// 儲存建立的貼文
        pub posts: Arc<Mutex<Vec<Post>>>,
        /// 預設返回的使用者資料
        pub mock_users: Arc<Mutex<HashMap<String, User>>>,
        /// 預設返回的頻道資料
        pub mock_channels: Arc<Mutex<HashMap<String, Channel>>>,
        /// 模擬的錯誤（當設定時會返回錯誤）
        pub should_error: Arc<Mutex<bool>>,
    }

    impl MockMattermostService {
        pub fn new() -> Self {
            Self {
                call_log: Arc::new(Mutex::new(Vec::new())),
                posts: Arc::new(Mutex::new(Vec::new())),
                mock_users: Arc::new(Mutex::new(HashMap::new())),
                mock_channels: Arc::new(Mutex::new(HashMap::new())),
                should_error: Arc::new(Mutex::new(false)),
            }
        }

        pub fn add_mock_user(&self, id: &str, username: &str) {
            let user = User {
                id: id.to_string(),
                username: username.to_string(),
                email: Some(format!("{}@example.com", username)),
                first_name: None,
                last_name: None,
            };
            self.mock_users.lock().unwrap().insert(id.to_string(), user);
        }

        pub fn add_mock_channel(&self, id: &str, channel_type: &str) {
            let channel = Channel {
                id: id.to_string(),
                channel_type: channel_type.to_string(),
                display_name: Some(format!("Channel {}", id)),
                name: Some(format!("channel-{}", id)),
            };
            self.mock_channels.lock().unwrap().insert(id.to_string(), channel);
        }

        pub fn set_error(&self, should_error: bool) {
            *self.should_error.lock().unwrap() = should_error;
        }

        pub fn get_call_count(&self, method: &str) -> usize {
            self.call_log
                .lock()
                .unwrap()
                .iter()
                .filter(|m| m.as_str() == method)
                .count()
        }

        pub fn get_posts(&self) -> Vec<Post> {
            self.posts.lock().unwrap().clone()
        }

        fn log_call(&self, method: &str) {
            self.call_log.lock().unwrap().push(method.to_string());
        }

        fn check_error(&self) -> Result<()> {
            if *self.should_error.lock().unwrap() {
                anyhow::bail!("Mock error")
            }
            Ok(())
        }
    }

    #[async_trait]
    impl MattermostService for MockMattermostService {
        async fn create_post(&self, post: &Post) -> Result<()> {
            self.log_call("create_post");
            self.check_error()?;
            self.posts.lock().unwrap().push(post.clone());
            Ok(())
        }

        async fn create_post_with_response(&self, post: &Post) -> Result<String> {
            self.log_call("create_post_with_response");
            self.check_error()?;
            self.posts.lock().unwrap().push(post.clone());
            Ok("mock_post_id".to_string())
        }

        async fn update_post(&self, _post_id: &str, _message: &str, _props: Option<serde_json::Value>) -> Result<()> {
            self.log_call("update_post");
            self.check_error()?;
            Ok(())
        }

        async fn delete_post(&self, _post_id: &str) -> Result<()> {
            self.log_call("delete_post");
            self.check_error()?;
            Ok(())
        }

        async fn send_ephemeral_post(&self, _channel_id: &str, _user_id: &str, _message: &str, _root_id: Option<&str>) -> Result<()> {
            self.log_call("send_ephemeral_post");
            self.check_error()?;
            Ok(())
        }

        async fn get_user(&self, user_id: &str) -> Result<User> {
            self.log_call("get_user");
            self.check_error()?;
            
            self.mock_users
                .lock()
                .unwrap()
                .get(user_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("User not found: {}", user_id))
        }

        async fn get_me(&self) -> Result<User> {
            self.log_call("get_me");
            self.check_error()?;
            Ok(User {
                id: "bot_id".to_string(),
                username: "bot".to_string(),
                email: Some("bot@example.com".to_string()),
                first_name: None,
                last_name: None,
            })
        }

        async fn get_channel(&self, channel_id: &str) -> Result<Channel> {
            self.log_call("get_channel");
            self.check_error()?;
            
            self.mock_channels
                .lock()
                .unwrap()
                .get(channel_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Channel not found: {}", channel_id))
        }

        async fn create_direct_channel(&self, _user_id_1: &str, _user_id_2: &str) -> Result<Channel> {
            self.log_call("create_direct_channel");
            self.check_error()?;
            Ok(Channel {
                id: "mock_dm_channel".to_string(),
                channel_type: "D".to_string(),
                display_name: None,
                name: None,
            })
        }

        async fn create_post_simple(&self, channel_id: &str, message: &str, _props: Option<serde_json::Value>) -> Result<PostResponse> {
            self.log_call("create_post_simple");
            self.check_error()?;
            
            let post = Post {
                id: Some("mock_post_id".to_string()),
                channel_id: channel_id.to_string(),
                message: message.to_string(),
                root_id: None,
                props: None,
            };
            self.posts.lock().unwrap().push(post);
            
            Ok(PostResponse {
                id: "mock_post_id".to_string(),
                channel_id: channel_id.to_string(),
            })
        }

        async fn open_dialog(
            &self,
            _trigger_id: &str,
            _url: &str,
            _title: &str,
            _elements: &[DialogElement],
            _submit_label: Option<&str>,
            _introduction_text: Option<&str>,
            _state: Option<&str>,
        ) -> Result<()> {
            self.log_call("open_dialog");
            self.check_error()?;
            Ok(())
        }
    }

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

#[cfg(test)]
mod tests {
    use super::utils::*;
    use crate::mattermost::{MattermostService, Post};

    #[tokio::test]
    async fn test_mock_mattermost_service() {
        let mock = MockMattermostService::new();
        
        // 測試建立貼文
        let post = Post {
            id: None,
            channel_id: "test_channel".to_string(),
            message: "Test message".to_string(),
            root_id: None,
            props: None,
        };
        
        assert!(mock.create_post(&post).await.is_ok());
        assert_eq!(mock.get_call_count("create_post"), 1);
        assert_eq!(mock.get_posts().len(), 1);
        
        // 測試 mock 使用者
        mock.add_mock_user("user1", "testuser");
        let user = mock.get_user("user1").await.unwrap();
        assert_eq!(user.username, "testuser");
        assert_eq!(mock.get_call_count("get_user"), 1);
        
        // 測試錯誤情境
        mock.set_error(true);
        assert!(mock.create_post(&post).await.is_err());
    }

    #[tokio::test]
    async fn test_mock_create_post_with_response() {
        let mock = MockMattermostService::new();
        
        let post = Post {
            id: None,
            channel_id: "channel1".to_string(),
            message: "Hello".to_string(),
            root_id: None,
            props: None,
        };
        
        let post_id = mock.create_post_with_response(&post).await.unwrap();
        assert_eq!(post_id, "mock_post_id");
        assert_eq!(mock.get_call_count("create_post_with_response"), 1);
    }
}