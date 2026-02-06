//! 團購功能的整合測試
//! 展示如何使用 MockMattermostService 進行測試

#[cfg(test)]
mod integration_tests {
    use crate::config::{Config, MattermostConfig, StickersConfig, SlashCommandTokens};
    use crate::database::Database;
    use crate::mattermost::DialogElement;
    use crate::sticker::StickerDatabase;
    use crate::test_utils::utils::MockMattermostService;
    use crate::AppState;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    /// 建立測試用的 AppState（使用 MockMattermostService）
    async fn create_test_app_state() -> Arc<RwLock<AppState>> {
        let mock_mattermost = Arc::new(MockMattermostService::new());
        let database = Database::new("sqlite::memory:").await.unwrap();
        let sticker_database = StickerDatabase::new(database.clone());

        let config = Config {
            mattermost: MattermostConfig {
                url: "https://example.com".to_string(),
                bot_token: "test_token".to_string(),
                bot_callback_url: Some("http://localhost:3000".to_string()),
                slash_command_tokens: SlashCommandTokens::default(),
            },
            stickers: StickersConfig {
                categories: vec![],
            },
            admin: vec![],
            database_url: "sqlite::memory:".to_string(),
        };

        Arc::new(RwLock::new(AppState {
            config,
            mattermost_client: mock_mattermost,
            sticker_database,
            database,
            bot_user_id: "bot_user_123".to_string(),
            config_path: PathBuf::from("data/config.yaml"),
        }))
    }

    #[tokio::test]
    async fn test_open_create_dialog_with_mock() {
        use super::super::dialogs::{open_create_dialog, CreateDialogParams};

        let state = create_test_app_state().await;
        let state_guard = state.read().await;

        let params = CreateDialogParams {
            trigger_id: "trigger_123",
            response_url: "https://example.com/hooks/abc",
            channel_id: "channel_123",
            user_id: "user_123",
            user_name: "testuser",
            bot_callback_url: "http://localhost:3000",
        };

        // 呼叫 open_create_dialog
        let result = open_create_dialog(
            state_guard.mattermost_client.as_ref(),
            &params,
        )
        .await;

        // 驗證結果
        assert!(result.is_ok(), "open_create_dialog 應該成功");

        // 驗證 mock 被呼叫
        if let Some(mock) = state_guard
            .mattermost_client
            .as_any()
            .downcast_ref::<MockMattermostService>()
        {
            assert_eq!(
                mock.get_call_count("open_dialog"),
                1,
                "open_dialog 應該被呼叫一次"
            );
        }
    }

    #[tokio::test]
    async fn test_mock_user_interaction() {
        let state = create_test_app_state().await;
        let state_guard = state.read().await;

        // 取得 mock service
        if let Some(mock) = state_guard
            .mattermost_client
            .as_any()
            .downcast_ref::<MockMattermostService>()
        {
            // 新增測試使用者
            mock.add_mock_user("user_123", "testuser");

            // 測試取得使用者
            let user = state_guard
                .mattermost_client
                .get_user("user_123")
                .await
                .unwrap();

            assert_eq!(user.username, "testuser");
            assert_eq!(user.id, "user_123");
            assert_eq!(mock.get_call_count("get_user"), 1);
        } else {
            panic!("應該是 MockMattermostService");
        }
    }

    #[tokio::test]
    async fn test_mock_error_scenario() {
        let state = create_test_app_state().await;
        let state_guard = state.read().await;

        if let Some(mock) = state_guard
            .mattermost_client
            .as_any()
            .downcast_ref::<MockMattermostService>()
        {
            // 設定錯誤模式
            mock.set_error(true);

            // 嘗試取得使用者應該失敗
            let result = state_guard.mattermost_client.get_user("user_123").await;
            assert!(result.is_err(), "應該返回錯誤");
            assert_eq!(mock.get_call_count("get_user"), 1);
        }
    }
}
