use super::*;
use anyhow::Result;
use std::collections::HashMap;

/// 解析 dialog submission 的表單資料並回傳 `DialogSubmission`。
/// 失敗時回傳 anyhow::Error，呼叫端可轉為 warp::Rejection。
pub fn parse_dialog_submission_form(form: &HashMap<String, String>) -> Result<DialogSubmission> {
    let json_str = if let Some(payload) = form.get("payload") {
        payload.clone()
    } else if form.len() == 1 {
        form.keys().next().unwrap().clone()
    } else {
        anyhow::bail!("Dialog submission 格式不正確");
    };

    let submission: DialogSubmission = serde_json::from_str(&json_str)
        .map_err(|e| anyhow::anyhow!("解析 Dialog submission 失敗: {}", e))?;

    Ok(submission)
}

/// 從 `DialogSubmission` 的 `state` 欄位解析成 serde_json::Value
pub fn extract_state_value(submission: &DialogSubmission) -> Result<serde_json::Value> {
    if let Some(state_str) = &submission.state {
        let v = serde_json::from_str(state_str)
            .map_err(|e| anyhow::anyhow!("解析 state 失敗: {}", e))?;
        Ok(v)
    } else {
        anyhow::bail!("state 為空");
    }
}

/// 建立一個針對單一欄位錯誤的 DialogSubmissionResponse
#[allow(dead_code)]
pub fn make_field_error_response(field: &str, message: &str) -> DialogSubmissionResponse {
    let mut errors = HashMap::new();
    errors.insert(field.to_string(), message.to_string());
    DialogSubmissionResponse {
        error: None,
        text: None,
        errors: Some(errors),
    }
}

/// 取得 bot callback url（trim trailing slash），回傳 Owned String
pub fn bot_callback_url_from_state(state_guard: &AppState) -> String {
    state_guard
        .config
        .mattermost
        .bot_callback_url
        .as_ref()
        .map(|url| url.trim_end_matches('/').to_string())
        .unwrap_or_else(|| "http://localhost:3000".to_string())
}

/// 取得 group buy，如果不存在或 DB 發生錯誤，回傳 Err(String) 代表要回覆給使用者的 ephemeral 訊息
pub async fn fetch_group_buy(
    state_guard: &AppState,
    group_buy_id: &str,
) -> Result<GroupBuy, String> {
    match state_guard.database.get_group_buy(group_buy_id).await {
        Ok(Some(gb)) => Ok(gb),
        Ok(None) => Err("找不到該團購".to_string()),
        Err(e) => {
            tracing::error!("取得團購資料失敗: {}", e);
            Err("取得團購資料失敗".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_parse_dialog_submission_form_payload() {
        // minimal DialogSubmission JSON
        let json = serde_json::json!({
            "type": "dialog_submission",
            "callback_id": "cb",
            "state": "{\"k\":\"v\"}",
            "user_id": "u",
            "channel_id": "c",
            "team_id": "t",
            "submission": {},
        })
        .to_string();

        let mut form = HashMap::new();
        form.insert("payload".to_string(), json);

        let submission = parse_dialog_submission_form(&form).expect("parse should succeed");
        assert_eq!(submission.callback_id, "cb");
        assert_eq!(submission.user_id, "u");
        assert!(submission.state.is_some());
    }

    #[test]
    fn test_extract_state_value() {
        let submission = DialogSubmission {
            r#type: "dialog_submission".to_string(),
            callback_id: "cb".to_string(),
            state: Some("{\"hello\": \"world\"}".to_string()),
            user_id: "u".to_string(),
            channel_id: "c".to_string(),
            team_id: "t".to_string(),
            submission: HashMap::new(),
            cancelled: None,
        };

        let v = extract_state_value(&submission).expect("extract should succeed");
        assert_eq!(v.get("hello").and_then(|x| x.as_str()), Some("world"));
    }
}
