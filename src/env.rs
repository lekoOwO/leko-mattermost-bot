//! 環境變數管理模組
//!
//! 統一管理所有環境變數的載入和存取，提供型別安全的介面。

use std::env;

/// 環境變數配置
///
/// 集中管理所有從環境變數讀取的配置項目。
/// 使用 `EnvConfig::load()` 來載入環境變數。
#[derive(Debug, Clone)]
pub struct EnvConfig {
    /// 配置檔案路徑（CONFIG_YAML）
    pub config_yaml: Option<String>,
    
    /// 資料庫 Schema 檔案路徑（DB_SCHEMA_FILE）
    pub db_schema_file: Option<String>,
    
    /// 日誌級別（LOG_LEVEL），預設為 "info"
    pub log_level: String,
    
    /// Rust 日誌過濾器（RUST_LOG）
    pub rust_log: Option<String>,
}

impl EnvConfig {
    /// 載入環境變數配置
    ///
    /// 此函數會嘗試從 .env 檔案載入環境變數（如果存在），
    /// 然後讀取所有支援的環境變數。
    ///
    /// # 範例
    ///
    /// ```no_run
    /// use leko_mattermost_bot::env::EnvConfig;
    ///
    /// let config = EnvConfig::load();
    /// println!("Log level: {}", config.log_level);
    /// ```
    pub fn load() -> Self {
        // 嘗試載入 .env 檔案（如果存在）
        // 忽略錯誤，因為 .env 檔案是可選的
        let _ = dotenvy::dotenv();
        
        Self {
            config_yaml: env::var("CONFIG_YAML").ok(),
            db_schema_file: env::var("DB_SCHEMA_FILE").ok(),
            log_level: env::var("LOG_LEVEL")
                .unwrap_or_else(|_| crate::constants::logging::DEFAULT_LOG_LEVEL.to_string()),
            rust_log: env::var("RUST_LOG").ok(),
        }
    }
    
    /// 取得配置檔案路徑
    ///
    /// 優先順序：
    /// 1. 環境變數 CONFIG_YAML
    /// 2. 命令列參數（由呼叫者提供）
    /// 3. 預設值 "config.yaml"
    pub fn get_config_path(&self, cli_path: Option<std::path::PathBuf>) -> std::path::PathBuf {
        cli_path
            .or_else(|| self.config_yaml.as_ref().map(std::path::PathBuf::from))
            .unwrap_or_else(|| std::path::PathBuf::from("config.yaml"))
    }
    
    /// 檢查是否應該使用外部的 Schema 檔案
    pub fn has_external_schema(&self) -> bool {
        self.db_schema_file.is_some()
    }
    
    /// 取得資料庫 Schema 檔案路徑
    pub fn get_schema_path(&self) -> Option<&str> {
        self.db_schema_file.as_deref()
    }
}

impl Default for EnvConfig {
    fn default() -> Self {
        Self::load()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_env_config_default() {
        let config = EnvConfig::load();
        
        // 預設的日誌級別應該是 "info"
        assert_eq!(config.log_level, "info");
    }
    
    #[test]
    fn test_get_config_path_priority() {
        let config = EnvConfig {
            config_yaml: Some("env.yaml".to_string()),
            db_schema_file: None,
            log_level: "info".to_string(),
            rust_log: None,
        };
        
        // 命令列參數優先
        let cli_path = Some(std::path::PathBuf::from("cli.yaml"));
        assert_eq!(config.get_config_path(cli_path), std::path::PathBuf::from("cli.yaml"));
        
        // 其次是環境變數
        assert_eq!(config.get_config_path(None), std::path::PathBuf::from("env.yaml"));
        
        // 最後是預設值
        let config_no_env = EnvConfig {
            config_yaml: None,
            db_schema_file: None,
            log_level: "info".to_string(),
            rust_log: None,
        };
        assert_eq!(config_no_env.get_config_path(None), std::path::PathBuf::from("config.yaml"));
    }
    
    #[test]
    fn test_has_external_schema() {
        let config_with_schema = EnvConfig {
            config_yaml: None,
            db_schema_file: Some("schema.sql".to_string()),
            log_level: "info".to_string(),
            rust_log: None,
        };
        assert!(config_with_schema.has_external_schema());
        
        let config_without_schema = EnvConfig {
            config_yaml: None,
            db_schema_file: None,
            log_level: "info".to_string(),
            rust_log: None,
        };
        assert!(!config_without_schema.has_external_schema());
    }
}
