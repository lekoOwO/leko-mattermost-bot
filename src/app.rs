use serde::{Deserialize, Serialize};

/// Mattermost App 呼叫請求
#[derive(Debug, Deserialize)]
pub struct AppCallRequest {
    pub context: AppContext,
    #[serde(default)]
    pub values: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct AppContext {
    #[allow(dead_code)]
    pub bot_user_id: Option<String>,
    #[allow(dead_code)]
    pub bot_access_token: Option<String>,
    #[allow(dead_code)]
    pub acting_user: ActingUser,
    pub channel: Channel,
    #[allow(dead_code)]
    pub team: Team,
    #[allow(dead_code)]
    pub mattermost_site_url: String,
    #[allow(dead_code)]
    pub app_path: String,
}

#[derive(Debug, Deserialize)]
pub struct ActingUser {
    #[allow(dead_code)]
    pub id: String,
    #[allow(dead_code)]
    pub username: String,
}

#[derive(Debug, Deserialize)]
pub struct Channel {
    pub id: String,
    #[allow(dead_code)]
    pub team_id: String,
}

#[derive(Debug, Deserialize)]
pub struct Team {
    #[allow(dead_code)]
    pub id: String,
}

/// Mattermost App 呼叫回應
#[derive(Debug, Serialize)]
pub struct AppCallResponse {
    #[serde(rename = "type")]
    pub response_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub form: Option<AppForm>,
}

#[derive(Debug, Serialize)]
pub struct AppForm {
    pub title: String,
    pub icon: String,
    pub fields: Vec<AppFormField>,
    pub submit: AppFormSubmit,
}

#[derive(Debug, Serialize)]
pub struct AppFormField {
    pub name: String,
    pub label: String,
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<AppFormOption>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_required: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct AppFormOption {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Serialize)]
pub struct AppFormSubmit {
    pub path: String,
    pub expand: AppExpand,
}

#[derive(Debug, Serialize)]
pub struct AppExpand {
    pub acting_user: String,
    pub acting_user_access_token: String,
}

impl AppCallResponse {
    pub fn ok(text: impl Into<String>) -> Self {
        Self {
            response_type: "ok".to_string(),
            text: Some(text.into()),
            form: None,
        }
    }

    pub fn form(form: AppForm) -> Self {
        Self {
            response_type: "form".to_string(),
            text: None,
            form: Some(form),
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            response_type: "error".to_string(),
            text: Some(text.into()),
            form: None,
        }
    }
}
