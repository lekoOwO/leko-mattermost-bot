//! HTTP 回應和表單參數處理的輔助函數

use std::collections::HashMap;
use warp::http::StatusCode;

/// 建立 ephemeral 類型的 JSON 回應（僅使用者可見）
/// 
/// # 參數
/// - `text`: 回應訊息內容
/// - `icon_url`: 可選的頭像 URL
pub fn ephemeral_json_reply(text: impl Into<String>, icon_url: Option<String>) -> warp::reply::Json {
    let mut response = serde_json::json!({
        "response_type": "ephemeral",
        "text": text.into()
    });
    
    if let Some(url) = icon_url {
        response["icon_url"] = serde_json::json!(url);
    }
    
    warp::reply::json(&response)
}

/// 建立 ephemeral 類型的 JSON 回應，帶 HTTP 狀態碼
/// 
/// # 參數
/// - `text`: 回應訊息內容
/// - `icon_url`: 可選的頭像 URL
pub fn ephemeral_json_with_status(
    text: impl Into<String>,
    icon_url: Option<String>,
) -> warp::reply::WithStatus<warp::reply::Json> {
    warp::reply::with_status(ephemeral_json_reply(text, icon_url), StatusCode::OK)
}

/// 建立 in_channel 類型的 JSON 回應（所有人可見）
/// 
/// # 參數
/// - `text`: 回應訊息內容
/// - `icon_url`: 可選的頭像 URL
pub fn in_channel_json_reply(
    text: impl Into<String>,
    icon_url: Option<String>,
) -> warp::reply::Json {
    let mut response = serde_json::json!({
        "response_type": "in_channel",
        "text": text.into()
    });
    
    if let Some(url) = icon_url {
        response["icon_url"] = serde_json::json!(url);
    }
    
    warp::reply::json(&response)
}

/// 建立空的 JSON 回應
pub fn empty_json_reply() -> warp::reply::Json {
    warp::reply::json(&serde_json::json!({}))
}

/// 建立 ephemeral_text 欄位的 JSON 回應（用於 Action 回應）
pub fn ephemeral_text_json(text: impl Into<String>) -> warp::reply::Json {
    warp::reply::json(&serde_json::json!({
        "ephemeral_text": text.into()
    }))
}

/// 建立 Dialog 錯誤回應
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

/// 建立空的 Dialog 回應
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

/// 建立帶欄位錯誤的 DialogSubmissionResponse
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

/// 從表單中安全地提取欄位值，找不到時返回空字串
pub fn get_form_field(form: &HashMap<String, String>, key: &str) -> String {
    form.get(key).cloned().unwrap_or_default()
}

/// Slash Command 常用參數結構體
/// 
/// 注意：目前主要用於測試，實際 handler 中多使用 get_form_field 直接提取
/// 保留此結構以便未來需要時使用
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
