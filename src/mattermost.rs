use anyhow::{Context, Result};
use reqwest::{header, Client};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct MattermostClient {
    base_url: String,
    #[allow(dead_code)]
    bot_token: String,
    client: Client,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Post {
    pub channel_id: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub props: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Dialog {
    pub trigger_id: String,
    pub url: String,
    pub dialog: DialogDefinition,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DialogDefinition {
    pub callback_id: String,
    pub title: String,
    pub introduction_text: String,
    pub submit_label: String,
    pub elements: Vec<DialogElement>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DialogElement {
    pub display_name: String,
    pub name: String,
    #[serde(rename = "type")]
    pub element_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<DialogOption>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optional: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogOption {
    pub text: String,
    pub value: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct SlashCommand {
    pub channel_id: String,
    pub team_id: String,
    pub user_id: String,
    pub command: String,
    pub text: String,
    pub trigger_id: String,
}

#[derive(Debug, Deserialize)]
pub struct DialogSubmission {
    pub callback_id: String,
    pub submission: serde_json::Value,
    pub channel_id: String,
    #[allow(dead_code)]
    pub user_id: String,
    #[serde(default)]
    pub state: Option<String>,
}

impl MattermostClient {
    /// 建立新的 Mattermost 客戶端
    pub fn new(base_url: String, bot_token: String) -> Result<Self> {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            header::HeaderValue::from_str(&format!("Bearer {}", bot_token))?,
        );

        let client = Client::builder().default_headers(headers).build()?;

        Ok(Self {
            base_url,
            bot_token,
            client,
        })
    }

    /// 發送訊息到頻道
    pub async fn create_post(&self, post: &Post) -> Result<()> {
        let url = format!("{}/api/v4/posts", self.base_url);

        let response = self
            .client
            .post(&url)
            .json(post)
            .send()
            .await
            .context("發送訊息失敗")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("發送訊息失敗: {} - {}", status, text);
        }

        Ok(())
    }

    /// 開啟互動式對話框
    pub async fn open_dialog(&self, dialog: &Dialog) -> Result<()> {
        let url = format!("{}/api/v4/actions/dialogs/open", self.base_url);

        tracing::info!(
            "正在開啟對話框: URL={}, trigger_id={}",
            url,
            dialog.trigger_id
        );
        tracing::debug!("Dialog 內容: {:?}", dialog);

        let response = self
            .client
            .post(&url)
            .json(dialog)
            .send()
            .await
            .context("開啟對話框失敗")?;

        let status = response.status();
        let response_text = response.text().await.unwrap_or_default();

        if !status.is_success() {
            tracing::error!(
                "開啟對話框失敗: status={}, response={}",
                status,
                response_text
            );
            anyhow::bail!("開啟對話框失敗: {} - {}", status, response_text);
        }

        tracing::info!("對話框開啟成功: {}", response_text);

        Ok(())
    }

    /// 發送臨時訊息（只有使用者看得到）
    #[allow(dead_code)]
    pub async fn send_ephemeral_post(
        &self,
        channel_id: &str,
        user_id: &str,
        message: &str,
    ) -> Result<()> {
        let url = format!("{}/api/v4/posts/ephemeral", self.base_url);

        let payload = serde_json::json!({
            "user_id": user_id,
            "post": {
                "channel_id": channel_id,
                "message": message
            }
        });

        let response = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .context("發送臨時訊息失敗")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("發送臨時訊息失敗: {} - {}", status, text);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_client() {
        let client =
            MattermostClient::new("https://example.com".to_string(), "test_token".to_string());
        assert!(client.is_ok());
    }

    #[test]
    fn test_dialog_serialization() {
        let dialog = Dialog {
            trigger_id: "test_trigger".to_string(),
            url: "https://example.com/callback".to_string(),
            state: None,
            dialog: DialogDefinition {
                callback_id: "sticker_select".to_string(),
                title: "選擇貼圖".to_string(),
                introduction_text: "請選擇一個貼圖".to_string(),
                submit_label: "發送".to_string(),
                elements: vec![DialogElement {
                    display_name: "貼圖".to_string(),
                    name: "sticker".to_string(),
                    element_type: "select".to_string(),
                    placeholder: Some("搜尋貼圖...".to_string()),
                    options: Some(vec![DialogOption {
                        text: "測試貼圖".to_string(),
                        value: "TEST001".to_string(),
                    }]),
                    data_source: None,
                    optional: None,
                    default: None,
                }],
            },
        };

        let json = serde_json::to_string(&dialog).unwrap();
        assert!(json.contains("callback_id"));
        assert!(json.contains("sticker_select"));
    }
}
