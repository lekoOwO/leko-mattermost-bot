mod config;
mod handlers;
mod mattermost;
mod sticker;
mod websocket;

use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};
use warp::Filter;

use config::Config;
use handlers::{handle_action, handle_leko_command, handle_rejection, handle_sticker_command};
use mattermost::MattermostClient;
use sticker::StickerDatabase;
use websocket::start_websocket;

#[derive(Parser, Debug)]
#[command(name = "leko-mattermost-bot")]
#[command(about = "Leko's Mattermost Bot - 通用貼圖機器人", long_about = None)]
struct Args {
    /// 配置檔案路徑
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// HTTP 伺服器監聽位址
    #[arg(short = 'H', long, default_value = "0.0.0.0")]
    host: String,

    /// HTTP 伺服器監聯埠號
    #[arg(short, long, default_value = "3000")]
    port: u16,
}

pub struct AppState {
    pub config: Config,
    pub mattermost_client: MattermostClient,
    pub sticker_database: StickerDatabase,
    pub bot_user_id: String,
    pub config_path: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日誌
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // 解析命令列參數
    let args = Args::parse();

    info!("正在啟動 Leko's Mattermost Bot...");

    // 確定配置文件路徑
    let config_path = args.config
        .or_else(|| std::env::var("CONFIG_YAML").ok().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("config.yaml"));

    // 載入配置
    let config = Config::from_path(&config_path).context("載入配置失敗")?;

    info!("配置載入成功");
    info!("Mattermost URL: {}", config.mattermost.url);

    // 初始化 Mattermost 客戶端
    let mattermost_client = MattermostClient::new(
        config.mattermost.url.clone(),
        config.mattermost.bot_token.clone(),
    )?;

    info!("Mattermost 客戶端初始化成功");

    // 獲取 bot 自己的 user_id
    let bot_user = mattermost_client
        .get_me()
        .await
        .context("無法獲取 bot 使用者資訊")?;
    let bot_user_id = bot_user.id.clone();
    
    info!("Bot 使用者: {} ({})", bot_user.username, bot_user_id);

    // 載入貼圖資料庫
    let sticker_database =
        StickerDatabase::load_from_config(&config.stickers).context("載入貼圖資料庫失敗")?;

    info!("貼圖資料庫載入成功，共 {} 張貼圖", sticker_database.count());

    // 顯示管理員配置
    if !config.admin.is_empty() {
        info!("管理員列表: {:?}", config.admin);
    } else {
        info!("未設定管理員");
    }

    // 建立應用狀態
    let state = Arc::new(RwLock::new(AppState {
        config,
        mattermost_client,
        sticker_database,
        bot_user_id,
        config_path,
    }));

    // 啟動 WebSocket 客戶端（在背景執行）
    let ws_state = state.clone();
    tokio::spawn(async move {
        if let Err(e) = start_websocket(ws_state).await {
            error!("WebSocket 客戶端錯誤: {}", e);
        }
    });

    // 啟動 HTTP 伺服器
    let addr = format!("{}:{}", args.host, args.port);
    info!("正在啟動 HTTP 伺服器於 {}", addr);

    start_server(state, &addr).await?;

    Ok(())
}

async fn start_server(state: Arc<RwLock<AppState>>, addr: &str) -> Result<()> {
    // Slash command 路由
    let sticker_command = warp::post()
        .and(warp::path("sticker"))
        .and(warp::path::end())
        .and(warp::body::form())
        .and(with_state(state.clone()))
        .and_then(handle_sticker_command);

    // /leko slash command 路由
    let leko_command = warp::post()
        .and(warp::path("leko"))
        .and(warp::path::end())
        .and(warp::body::form())
        .and(with_state(state.clone()))
        .and_then(handle_leko_command);

    // Interactive Message Action 處理器
    let action_handler = warp::post()
        .and(warp::path("action"))
        .and(warp::path::end())
        .and(warp::body::json())
        .and(with_state(state.clone()))
        .and_then(handle_action);

    // 健康檢查端點
    let health = warp::get()
        .and(warp::path("health"))
        .and(warp::path::end())
        .map(|| warp::reply::json(&serde_json::json!({"status": "ok"})));

    // 加上請求日誌中間件
    let log = warp::log::custom(|info| {
        info!(
            "{} {} {} - {}",
            info.method(),
            info.path(),
            info.status(),
            info.elapsed().as_millis()
        );
    });

    let routes = health
        .or(action_handler)
        .or(leko_command)
        .or(sticker_command)
        .recover(handle_rejection)
        .with(log);

    warp::serve(routes)
        .run(addr.parse::<std::net::SocketAddr>()?)
        .await;

    Ok(())
}

fn with_state(
    state: Arc<RwLock<AppState>>,
) -> impl warp::Filter<Extract = (Arc<RwLock<AppState>>,), Error = std::convert::Infallible> + Clone
{
    warp::any().map(move || state.clone())
}
