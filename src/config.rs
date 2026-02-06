use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub mattermost: MattermostConfig,
    pub stickers: StickersConfig,
    #[serde(default)]
    pub admin: Vec<String>,
    #[serde(default = "default_database_url")]
    pub database_url: String,
}

fn default_database_url() -> String {
    "sqlite::memory:".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MattermostConfig {
    pub url: String,
    pub bot_token: String,
    #[serde(default)]
    pub slash_command_tokens: SlashCommandTokens,
    #[serde(default)]
    pub bot_callback_url: Option<String>, // Bot 服務器的公開 URL，用於 dialog callback
    #[serde(default)]
    pub default_avatar: Option<String>, // 默認頭像，可以是 URL、#user_id 或 @username 格式
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SlashCommandTokens {
    #[serde(default)]
    pub group_buy: Option<String>,
    #[serde(default)]
    pub leko: Option<String>,
    #[serde(default)]
    pub stickers: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StickersConfig {
    pub categories: Vec<CategoryConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryConfig {
    pub name: String,
    pub sources: Vec<SourceConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SourceConfig {
    File {
        format: FileFormat,
        path: String,
    },
    HttpGet {
        format: FileFormat,
        url: String,
        #[serde(default)]
        headers: std::collections::HashMap<String, String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileFormat {
    Csv,
    Json,
}

impl Config {
    /// 從指定路徑載入配置檔案
    pub fn from_path(path: &PathBuf) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("無法讀取配置檔案: {}", path.display()))?;

        let config: Config = serde_yaml::from_str(&content)
            .with_context(|| format!("無法解析配置檔案: {}", path.display()))?;

        Ok(config)
    }

    /// 從命令列參數、環境變數或預設位置載入配置
    pub fn load(config_path: Option<PathBuf>) -> Result<Self> {
        let path = config_path
            .or_else(|| env::var("CONFIG_YAML").ok().map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("config.yaml"));

        Self::from_path(&path)
    }

    /// 檢查使用者是否為管理員
    /// 支援 username (@開頭) 或 user_id
    pub fn is_admin(&self, user_id: &str, username: &str) -> bool {
        self.admin.iter().any(|admin| {
            if let Some(admin_username) = admin.strip_prefix('@') {
                // @開頭的比對 username
                admin_username == username
            } else {
                // 否則比對 user_id
                admin == user_id
            }
        })
    }

    /// 獲取預設頭像 URL（已解析）
    /// 
    /// 如果配置了 default_avatar:
    /// - 若為普通 URL，直接返回
    /// - 若為 #user_id 格式，會轉換為 Mattermost 用戶頭像 API URL
    /// 
    /// # 注意
    /// - 使用 #user_id 格式可直接指定用戶 ID
    /// - @username 格式應在啟動時解析為 #user_id
    /// 
    /// # 返回值
    /// - `Some(url)` - 解析後的完整 URL
    /// - `None` - 未配置預設頭像
    pub fn default_avatar_url(&self) -> Option<String> {
        self.mattermost.default_avatar.as_ref().map(|avatar| {
            if let Some(user_id) = avatar.strip_prefix('#') {
                // #user_id 格式，轉換為 Mattermost API URL
                format!("{}/api/v4/users/{}/image", self.mattermost.url, user_id)
            } else {
                // 普通 URL（@username 應該在啟動時就已經被解析了）
                avatar.clone()
            }
        })
    }

    /// 檢查是否需要解析 username（@username 格式）
    pub fn needs_avatar_resolution(&self) -> bool {
        self.mattermost.default_avatar
            .as_ref()
            .map(|s| s.starts_with('@'))
            .unwrap_or(false)
    }

    /// 取得需要解析的 username（不含 @ 前綴）
    pub fn get_avatar_username(&self) -> Option<String> {
        self.mattermost.default_avatar
            .as_ref()
            .and_then(|s| s.strip_prefix('@').map(|u| u.to_string()))
    }

    /// 設定已解析的頭像 user_id（替換為 #user_id 格式）
    pub fn set_resolved_avatar(&mut self, user_id: String) {
        self.mattermost.default_avatar = Some(format!("#{}", user_id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_load_config_from_file() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test_config.yaml");

        let yaml_content = r#"
mattermost:
  url: https://example.com
  bot_token: test_token
stickers:
  categories:
    - name: 測試分類
      sources:
        - type: file
          format: csv
          path: data/test.csv
        - type: file
          format: json
          path: data/test.json
admin:
  - "@testuser"
  - "userid123"
"#;

        fs::write(&config_path, yaml_content).unwrap();

        let config = Config::from_path(&config_path).unwrap();

        assert_eq!(config.mattermost.url, "https://example.com");
        assert_eq!(config.mattermost.bot_token, "test_token");
        assert_eq!(config.stickers.categories.len(), 1);
        assert_eq!(config.stickers.categories[0].name, "測試分類");
        assert_eq!(config.stickers.categories[0].sources.len(), 2);
        assert_eq!(config.admin.len(), 2);

        // 測試管理員驗證
        assert!(config.is_admin("userid123", "otheruser"));
        assert!(config.is_admin("anyid", "testuser"));
        assert!(!config.is_admin("otherid", "otheruser"));
    }

    #[test]
    fn test_default_avatar_url() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("avatar_config.yaml");

        // 測試 URL 格式
        let yaml_content = r#"
mattermost:
  url: https://mattermost.example.com
  bot_token: test_token
  default_avatar: https://example.com/avatar.png
stickers:
  categories: []
"#;
        fs::write(&config_path, yaml_content).unwrap();
        let config = Config::from_path(&config_path).unwrap();
        assert_eq!(
            config.default_avatar_url(),
            Some("https://example.com/avatar.png".to_string())
        );
        assert!(!config.needs_avatar_resolution());

        // 測試 #user_id 格式
        let yaml_content = r##"
mattermost:
  url: https://mattermost.example.com
  bot_token: test_token
  default_avatar: "#w5qj3cmxfjyu5kqjte55rwhhbh"
stickers:
  categories: []
"##;
        fs::write(&config_path, yaml_content).unwrap();
        let config = Config::from_path(&config_path).unwrap();
        assert_eq!(
            config.default_avatar_url(),
            Some("https://mattermost.example.com/api/v4/users/w5qj3cmxfjyu5kqjte55rwhhbh/image".to_string())
        );
        assert!(!config.needs_avatar_resolution());

        // 測試 @username 格式
        let yaml_content = r#"
mattermost:
  url: https://mattermost.example.com
  bot_token: test_token
  default_avatar: "@bot"
stickers:
  categories: []
"#;
        fs::write(&config_path, yaml_content).unwrap();
        let mut config = Config::from_path(&config_path).unwrap();
        assert!(config.needs_avatar_resolution());
        assert_eq!(config.get_avatar_username(), Some("bot".to_string()));
        
        // 測試解析後的結果
        config.set_resolved_avatar("w5qj3cmxfjyu5kqjte55rwhhbh".to_string());
        assert_eq!(
            config.default_avatar_url(),
            Some("https://mattermost.example.com/api/v4/users/w5qj3cmxfjyu5kqjte55rwhhbh/image".to_string())
        );
        assert!(!config.needs_avatar_resolution());
    }

    #[test]
    fn test_load_config_with_env_var() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("env_config.yaml");

        let yaml_content = r#"
mattermost:
  url: https://env-example.com
  bot_token: env_token
stickers:
  categories: []
"#;

        fs::write(&config_path, yaml_content).unwrap();

        // SAFETY: This test runs in isolation and does not rely on environment variable
        // consistency across threads.
        unsafe {
            env::set_var("CONFIG_YAML", config_path.to_str().unwrap());
        }

        let config = Config::load(None).unwrap();

        assert_eq!(config.mattermost.url, "https://env-example.com");

        // SAFETY: This test runs in isolation and does not rely on environment variable
        // consistency across threads.
        unsafe {
            env::remove_var("CONFIG_YAML");
        }
    }
}
