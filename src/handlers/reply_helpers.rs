//! HTTP 回應和表單參數處理的輔助函數

use std::collections::HashMap;
use warp::http::StatusCode;

#[derive(Debug, Clone)]
pub enum IconConfig {
    Url(String),
    Emoji(String),
}

impl IconConfig {
    pub fn from_config(config: &crate::config::Config) -> Option<Self> {
        if let Some(emoji) = config.default_avatar_emoji() {
            Some(IconConfig::Emoji(emoji))
        } else if let Some(url) = config.default_avatar_url() {
            Some(IconConfig::Url(url))
        } else {
            None
        }
    }

    fn apply_to_json(&self, response: &mut serde_json::Value) {
        match self {
            IconConfig::Url(url) => {
                response["icon_url"] = serde_json::json!(url);
            }
            IconConfig::Emoji(emoji) => {
                response["icon_emoji"] = serde_json::json!(emoji);
            }
        }
    }
}

pub fn ephemeral_json_reply(text: impl Into<String>, icon: Option<IconConfig>) -> warp::reply::Json {
    let mut response = serde_json::json!({
        "response_type": "ephemeral",
        "text": text.into()
    });
    
    if let Some(icon_config) = icon {
        icon_config.apply_to_json(&mut response);
    }
    
    warp::reply::json(&response)
}

pub fn ephemeral_json_with_status(
    text: impl Into<String>,
    icon: Option<IconConfig>,
) -> warp::reply::WithStatus<warp::reply::Json> {
    warp::reply::with_status(ephemeral_json_reply(text, icon), StatusCode::OK)
}

pub fn in_channel_json_reply(
    text: impl Into<String>,
    icon: Option<IconConfig>,
) -> warp::reply::Json {
    let mut response = serde_json::json!({
        "response_type": "in_channel",
        "text": text.into()
    });
    
    if let Some(icon_config) = icon {
        icon_config.apply_to_json(&mut response);
    }
    
    warp::reply::json(&response)
}

pub fn empty_json_reply() -> warp::reply::Json {
    warp::reply::json(&serde_json::json!({}))
}

pub fn ephemeral_text_json(text: impl Into<String>) -> warp::reply::Json {
    warp::reply::json(&serde_json::json!({
        "ephemeral_text": text.into()
    }))
}

pub fn dialog_error(error_message: impl Into<String>) -> warp::reply::WithStatus<warp::reply::Json> {
    use crate::handlers::group_buy::DialogSubmissionResponse;
    warp::reply::with_status(
        warp::reply::json(&DialogSubmissionResponse {
            error: Some(error_message.into()),
            text: None,
            errors: None,
        }),
        warp::http::StatusCode::OK,
    )
}

pub fn dialog_empty() -> warp::reply::WithStatus<warp::reply::Json> {
    use crate::handlers::group_buy::DialogSubmissionResponse;
    warp::reply::with_status(
        warp::reply::json(&DialogSubmissionResponse {
            error: None,
            text: None,
            errors: None,
        }),
        warp::http::StatusCode::OK,
    )
}

pub fn dialog_field_error(
    field_name: impl Into<String>,
    error_message: impl Into<String>,
) -> warp::reply::WithStatus<warp::reply::Json> {
    use crate::handlers::group_buy::DialogSubmissionResponse;
    let mut errors = std::collections::HashMap::new();
    errors.insert(field_name.into(), error_message.into());
    warp::reply::with_status(
        warp::reply::json(&DialogSubmissionResponse {
            error: None,
            text: None,
            errors: Some(errors),
        }),
        warp::http::StatusCode::OK,
    )
}

pub fn get_form_field(form: &HashMap<String, String>, key: &str) -> String {
    form.get(key).cloned().unwrap_or_default()
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct SlashCommandParams {
    pub text: String,
    pub user_id: String,
    pub user_name: String,
    pub channel_id: String,
    pub response_url: String,
    pub trigger_id: String,
    pub team_id: String,
    pub channel_name: String,
}

impl SlashCommandParams {
    /// 從表單中建立 SlashCommandParams
    pub fn from_form(form: &HashMap<String, String>) -> Self {
        Self {
            text: get_form_field(form, "text"),
            user_id: get_form_field(form, "user_id"),
            user_name: get_form_field(form, "user_name"),
            channel_id: get_form_field(form, "channel_id"),
            response_url: get_form_field(form, "response_url"),
            trigger_id: get_form_field(form, "trigger_id"),
            team_id: get_form_field(form, "team_id"),
            channel_name: get_form_field(form, "channel_name"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_form_field() {
        let mut form = HashMap::new();
        form.insert("key1".to_string(), "value1".to_string());

        assert_eq!(get_form_field(&form, "key1"), "value1");
        assert_eq!(get_form_field(&form, "key2"), "");
    }

    #[test]
    fn test_slash_command_params_from_form() {
        let mut form = HashMap::new();
        form.insert("text".to_string(), "test text".to_string());
        form.insert("user_id".to_string(), "user123".to_string());
        form.insert("user_name".to_string(), "testuser".to_string());

        let params = SlashCommandParams::from_form(&form);

        assert_eq!(params.text, "test text");
        assert_eq!(params.user_id, "user123");
        assert_eq!(params.user_name, "testuser");
        assert_eq!(params.channel_id, "");
    }
}
