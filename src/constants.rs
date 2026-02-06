//! 專案常數定義
//!
//! 本模組集中管理專案中使用的常數，避免魔法數字和字串散落在程式碼中。

/// 團購相關常數
pub mod group_buy {
    /// 範例商品的預設名稱
    pub const EXAMPLE_ITEM_NAME: &str = "範例商品";
    
    /// 團購的初始版本號
    pub const DEFAULT_VERSION: i32 = 1;
    
    /// 商家名稱的最大長度
    pub const MAX_MERCHANT_NAME_LENGTH: usize = 100;
    
    /// 描述的最大長度
    pub const MAX_DESCRIPTION_LENGTH: usize = 500;
    
    /// Metadata YAML 的最大長度
    pub const MAX_METADATA_LENGTH: usize = 1000;
    
    /// 調整數量 YAML 的最大長度
    pub const MAX_ADJUSTMENTS_LENGTH: usize = 3000;
}

/// WebSocket 相關常數
pub mod websocket {
    use std::time::Duration;
    
    /// WebSocket 斷線後重新連接的延遲時間（秒）
    pub const RECONNECT_DELAY_SECS: u64 = 5;
    
    /// WebSocket 認證請求的序列號
    pub const AUTH_SEQUENCE: u64 = 1;
    
    /// 認證動作名稱
    pub const AUTH_ACTION: &str = "authentication_challenge";
    
    /// 重新連接延遲時間（Duration 形式）
    pub const RECONNECT_DELAY: Duration = Duration::from_secs(RECONNECT_DELAY_SECS);
}

/// 資料庫相關常數
pub mod database {
    use std::time::Duration;
    
    /// SQLite 連接池的最大連接數
    pub const MAX_CONNECTIONS: u32 = 5;
    
    /// 資料庫忙碌時的超時時間（秒）
    pub const BUSY_TIMEOUT_SECS: u64 = 5;
    
    /// 資料庫忙碌超時時間（Duration 形式）
    pub const BUSY_TIMEOUT: Duration = Duration::from_secs(BUSY_TIMEOUT_SECS);
}

/// HTTP 相關常數
pub mod http {
    /// 預設的 HTTP 伺服器監聽位址
    pub const DEFAULT_HOST: &str = "0.0.0.0";
    
    /// 預設的 HTTP 伺服器監聽埠號
    pub const DEFAULT_PORT: u16 = 3000;
    
    /// 預設的 bot callback URL（開發用）
    pub const DEFAULT_CALLBACK_URL: &str = "http://localhost:3000";
}

/// 日誌相關常數
pub mod logging {
    /// 預設的日誌級別
    pub const DEFAULT_LOG_LEVEL: &str = "info";
}

/// 驗證相關常數
pub mod validation {
    use rust_decimal::Decimal;
    
    /// 價格的最大值
    pub const MAX_PRICE: i64 = 100_000;
    
    /// YAML 內容的最大大小（字元數）
    pub const MAX_YAML_SIZE: usize = 10_000;
    
    /// 取得最大價格的 Decimal 表示
    pub fn max_price_decimal() -> Decimal {
        Decimal::new(MAX_PRICE, 0)
    }
}
