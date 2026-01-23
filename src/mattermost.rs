use anyhow::{Context, Result};
use reqwest::{Client, header};
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub channel_id: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub props: Option<serde_json::Value>,
}

/// Interactive Message Attachment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pretext: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actions: Option<Vec<Action>>,
}

/// Interactive Message Action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub action_type: String,  // "button" or "select"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,  // "default", "primary", "success", "good", "warning", "danger"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integration: Option<Integration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<ActionOption>>,
}

/// Action Integration（指定 callback URL 和 context）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Integration {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<serde_json::Value>,
}

/// Select Action Option
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionOption {
    pub text: String,
    pub value: String,
}

/// Interactive Message Action Callback Request
#[derive(Debug, Deserialize)]
pub struct ActionRequest {
    pub user_id: String,
    #[serde(default)]
    pub user_name: Option<String>,
    #[allow(dead_code)]
    pub channel_id: String,
    #[allow(dead_code)]
    pub post_id: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub trigger_id: Option<String>,
    #[serde(default)]
    pub context: serde_json::Value,
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

    /// 發送訊息到頻道並回傳 Post ID
    #[allow(dead_code)]
    pub async fn create_post_with_response(&self, post: &Post) -> Result<String> {
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

        let post_response: serde_json::Value = response.json().await.context("解析回應失敗")?;
        let post_id = post_response
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("回應中缺少 post id"))?
            .to_string();

        Ok(post_id)
    }

    /// 更新訊息
    #[allow(dead_code)]
    pub async fn update_post(&self, post_id: &str, message: &str, props: Option<serde_json::Value>) -> Result<()> {
        let url = format!("{}/api/v4/posts/{}", self.base_url, post_id);

        let mut payload = serde_json::json!({
            "id": post_id,
            "message": message,
        });

        if let Some(p) = props {
            payload["props"] = p;
        }

        let response = self
            .client
            .put(&url)
            .json(&payload)
            .send()
            .await
            .context("更新訊息失敗")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("更新訊息失敗: {} - {}", status, text);
        }

        Ok(())
    }

    /// 刪除訊息
    #[allow(dead_code)]
    pub async fn delete_post(&self, post_id: &str) -> Result<()> {
        let url = format!("{}/api/v4/posts/{}", self.base_url, post_id);

        let response = self
            .client
            .delete(&url)
            .send()
            .await
            .context("刪除訊息失敗")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("刪除訊息失敗: {} - {}", status, text);
        }

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
    fn test_attachment_serialization() {
        let attachment = Attachment {
            fallback: Some("選擇貼圖".to_string()),
            color: Some("#3AA3E3".to_string()),
            pretext: None,
            text: Some("請選擇一個貼圖".to_string()),
            author_name: None,
            author_icon: None,
            title: Some("貼圖選擇器".to_string()),
            image_url: None,
            thumb_url: None,
            actions: Some(vec![
                Action {
                    id: "sticker_select".to_string(),
                    name: "選擇貼圖".to_string(),
                    action_type: "select".to_string(),
                    style: None,
                    integration: Some(Integration {
                        url: "https://example.com/action".to_string(),
                        context: Some(serde_json::json!({
                            "action": "select_sticker"
                        })),
                    }),
                    options: Some(vec![ActionOption {
                        text: "測試貼圖".to_string(),
                        value: "0".to_string(),
                    }]),
                },
            ]),
        };

        let json = serde_json::to_string(&attachment).unwrap();
        assert!(json.contains("sticker_select"));
        assert!(json.contains("選擇貼圖"));
    }
}
