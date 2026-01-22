mod app;
mod config;
mod mattermost;
mod sticker;

use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};
use warp::Filter;

use app::{
    AppCallRequest, AppCallResponse, AppExpand, AppForm, AppFormField, AppFormOption, AppFormSubmit,
};
use config::Config;
use mattermost::{Dialog, DialogDefinition, DialogElement, DialogOption, MattermostClient, Post};
use sticker::StickerDatabase;

// 自訂錯誤類型
#[derive(Debug)]
struct UnauthorizedError;
impl warp::reject::Reject for UnauthorizedError {}

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

    /// HTTP 伺服器監聽埠號
    #[arg(short, long, default_value = "3000")]
    port: u16,
}

struct AppState {
    config: Config,
    mattermost_client: MattermostClient,
    sticker_database: StickerDatabase,
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

    // 載入配置
    let config = Config::load(args.config).context("載入配置失敗")?;

    info!("配置載入成功");
    info!("Mattermost URL: {}", config.mattermost.url);

    // 初始化 Mattermost 客戶端
    let mattermost_client = MattermostClient::new(
        config.mattermost.url.clone(),
        config.mattermost.bot_token.clone(),
    )?;

    info!("Mattermost 客戶端初始化成功");

    // 載入貼圖資料庫
    let sticker_database =
        StickerDatabase::load_from_config(&config.stickers).context("載入貼圖資料庫失敗")?;

    info!("貼圖資料庫載入成功，共 {} 張貼圖", sticker_database.count());

    // 建立應用狀態
    let state = Arc::new(RwLock::new(AppState {
        config,
        mattermost_client,
        sticker_database,
    }));

    // 啟動 HTTP 伺服器
    let addr = format!("{}:{}", args.host, args.port);
    info!("正在啟動 HTTP 伺服器於 {}", addr);

    start_server(state, &addr).await?;

    Ok(())
}

async fn start_server(state: Arc<RwLock<AppState>>, addr: &str) -> Result<()> {
    // Mattermost App API 路由
    let app_manifest = warp::get()
        .and(warp::path("manifest.json"))
        .and(warp::path::end())
        .and_then(serve_manifest);

    let app_sticker_call = warp::post()
        .and(warp::path("api"))
        .and(warp::path("v1"))
        .and(warp::path("sticker"))
        .and(warp::path::end())
        .and(warp::body::json())
        .and(with_state(state.clone()))
        .and_then(handle_app_sticker_call);

    let app_sticker_submit = warp::post()
        .and(warp::path("api"))
        .and(warp::path("v1"))
        .and(warp::path("sticker"))
        .and(warp::path("submit"))
        .and(warp::path::end())
        .and(warp::body::json())
        .and(with_state(state.clone()))
        .and_then(handle_app_sticker_submit);

    // 傳統 slash command 路由（向後相容）
    let sticker_command = warp::post()
        .and(warp::path("sticker"))
        .and(warp::path::end())
        .and(warp::body::form())
        .and(with_state(state.clone()))
        .and_then(handle_sticker_command);

    // 對話框提交處理器
    let dialog_submit = warp::post()
        .and(warp::path("dialog"))
        .and(warp::path("submit"))
        .and(warp::path::end())
        .and(warp::body::json())
        .and(with_state(state.clone()))
        .and_then(handle_dialog_submission);

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

    let routes = app_manifest
        .or(health)
        .or(app_sticker_submit) // 先匹配 /api/v1/sticker/submit
        .or(app_sticker_call) // 再匹配 /api/v1/sticker
        .or(dialog_submit) // /dialog/submit
        .or(sticker_command) // 最後匹配 /sticker（避免被前面搶走）
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

async fn handle_sticker_command(
    form: std::collections::HashMap<String, String>,
    state: Arc<RwLock<AppState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    info!("收到 /sticker 指令");
    info!("請求參數: {:?}", form.keys().collect::<Vec<_>>());

    // 驗證 slash command token
    let app_state = state.read().await;
    if let Some(expected_token) = &app_state.config.mattermost.slash_command_token {
        if let Some(received_token) = form.get("token") {
            if received_token != expected_token {
                error!(
                    "無效的 slash command token: 收到 '{}', 期望 '{}'",
                    &received_token[..8.min(received_token.len())],
                    &expected_token[..8.min(expected_token.len())]
                );
                drop(app_state);
                return Err(warp::reject::custom(UnauthorizedError));
            } else {
                info!("Token 驗證成功");
            }
        } else {
            error!("請求中缺少 token");
            drop(app_state);
            return Err(warp::reject::custom(UnauthorizedError));
        }
    } else {
        info!("未設定 slash_command_token，跳過驗證");
    }
    drop(app_state);

    let trigger_id = form.get("trigger_id").cloned().unwrap_or_default();
    let _channel_id = form.get("channel_id").cloned().unwrap_or_default();
    let text = form.get("text").cloned().unwrap_or_default();
    let user_name = form.get("user_name").cloned().unwrap_or_default();
    let user_id = form.get("user_id").cloned().unwrap_or_default();

    info!("trigger_id: {}", trigger_id);
    info!("搜尋關鍵字: '{}', 使用者: {}", text, user_name);

    let app_state = state.read().await;

    // 搜尋貼圖（不限分類）
    let stickers = app_state
        .sticker_database
        .search(&text, None)
        .into_iter()
        .take(25)
        .collect::<Vec<_>>();

    if stickers.is_empty() {
        // 沒有找到貼圖
        drop(app_state);
        let message = if text.is_empty() {
            "沒有可用的貼圖".to_string()
        } else {
            format!("找不到符合「{}」的貼圖", text)
        };
        return Ok(warp::reply::json(&serde_json::json!({
            "response_type": "ephemeral",
            "text": message
        })));
    }

    // 取得所有分類
    let categories = app_state.sticker_database.get_categories();

    // 建立對話框選項（限制為 15 個，Mattermost 的限制）
    let sticker_options: Vec<DialogOption> = stickers
        .iter()
        .take(15) // Mattermost Dialog 下拉選單最多 15 個選項
        .enumerate()
        .map(|(idx, s)| DialogOption {
            text: s.get_display_name(),
            value: idx.to_string(),
        })
        .collect();

    // 建立分類選項
    let category_options: Vec<DialogOption> = std::iter::once(DialogOption {
        text: "全部".to_string(),
        value: "all".to_string(), // 使用 "all" 而不是空字串
    })
    .chain(categories.iter().map(|cat| DialogOption {
        text: cat.clone(),
        value: cat.clone(),
    }))
    .collect();

    // 建立對話框
    let callback_url = app_state
        .config
        .mattermost
        .bot_callback_url
        .as_ref()
        .map(|url| format!("{}/dialog/submit", url.trim_end_matches('/')))
        .unwrap_or_else(|| "http://localhost/dialog/submit".to_string());

    let category_options_len = category_options.len();
    let sticker_options_len = sticker_options.len();

    // 將使用者資訊編碼到 state 中
    let user_state = serde_json::json!({
        "user_name": user_name,
        "user_id": user_id,
    })
    .to_string();

    let dialog = Dialog {
        trigger_id,
        url: callback_url.clone(),
        state: Some(user_state),
        dialog: DialogDefinition {
            callback_id: "sticker_select".to_string(),
            title: "選擇貼圖".to_string(),
            introduction_text: if text.is_empty() {
                "請選擇一個貼圖".to_string()
            } else {
                format!("搜尋「{}」的結果", text)
            },
            submit_label: "發送".to_string(),
            elements: vec![
                DialogElement {
                    display_name: "分類".to_string(),
                    name: "category".to_string(),
                    element_type: "select".to_string(),
                    placeholder: Some("選擇分類...".to_string()),
                    options: Some(category_options),
                    data_source: None,
                    optional: Some(true),
                    default: Some("all".to_string()),
                },
                DialogElement {
                    display_name: "貼圖".to_string(),
                    name: "sticker_id".to_string(),
                    element_type: "select".to_string(),
                    placeholder: Some("選擇貼圖...".to_string()),
                    options: Some(sticker_options),
                    data_source: None,
                    optional: None,
                    default: None,
                },
            ],
        },
    };

    info!("Dialog callback URL: {}", callback_url);
    info!(
        "Dialog 元素數量: 分類選項={}, 貼圖選項={}",
        category_options_len, sticker_options_len
    );

    // 開啟對話框
    if let Err(e) = app_state.mattermost_client.open_dialog(&dialog).await {
        error!("開啟對話框失敗: {}", e);
        drop(app_state);
        return Ok(warp::reply::json(&serde_json::json!({
            "response_type": "ephemeral",
            "text": "開啟對話框失敗，請稍後再試"
        })));
    }

    drop(app_state);

    // 成功開啟對話框，回傳空回應（HTTP 200）
    Ok(warp::reply::json(&serde_json::json!({})))
}

async fn handle_dialog_submission(
    submission: mattermost::DialogSubmission,
    state: Arc<RwLock<AppState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    info!("收到對話框提交: {:?}", submission.callback_id);

    if submission.callback_id != "sticker_select" {
        return Ok(warp::reply::json(&serde_json::json!({})));
    }

    // 解析使用者資訊
    let (user_name, user_id) = if let Some(state_str) = &submission.state {
        if let Ok(state_json) = serde_json::from_str::<serde_json::Value>(state_str) {
            let user_name = state_json
                .get("user_name")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown")
                .to_string();
            let user_id = state_json
                .get("user_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            (user_name, user_id)
        } else {
            ("Unknown".to_string(), String::new())
        }
    } else {
        ("Unknown".to_string(), String::new())
    };

    let sticker_index = submission
        .submission
        .get("sticker_id")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);

    let app_state = state.read().await;

    // 找到對應的貼圖
    if let Some(sticker) = app_state.sticker_database.get_by_index(sticker_index) {
        // 發送貼圖訊息，使用觸發指令的使用者身份
        let props = serde_json::json!({
            "override_username": user_name,
            "override_icon_url": format!("{}/api/v4/users/{}/image",
                app_state.config.mattermost.url, user_id),
        });

        let post = Post {
            channel_id: submission.channel_id.clone(),
            message: format!("![sticker]({})", sticker.image_url),
            root_id: None,
            props: Some(props),
        };

        if let Err(e) = app_state.mattermost_client.create_post(&post).await {
            error!("發送貼圖失敗: {}", e);
        } else {
            info!("成功發送貼圖: {}", sticker.name);
        }
    } else {
        error!("找不到貼圖索引: {}", sticker_index);
    }

    drop(app_state);

    Ok(warp::reply::json(&serde_json::json!({})))
}

// Mattermost App API 處理函數

async fn serve_manifest() -> Result<impl warp::Reply, warp::Rejection> {
    let manifest = tokio::fs::read_to_string("manifest.json")
        .await
        .unwrap_or_else(|_| "{}".to_string());

    Ok(warp::reply::with_header(
        manifest,
        "Content-Type",
        "application/json",
    ))
}

async fn handle_app_sticker_call(
    _call: AppCallRequest,
    state: Arc<RwLock<AppState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    info!("收到 Mattermost App /sticker 呼叫");

    let app_state = state.read().await;

    // 取得前 25 張貼圖
    let stickers: Vec<_> = app_state
        .sticker_database
        .get_all()
        .iter()
        .take(25)
        .collect();

    if stickers.is_empty() {
        drop(app_state);
        return Ok(warp::reply::json(&AppCallResponse::error("沒有可用的貼圖")));
    }

    // 取得所有分類
    let categories = app_state.sticker_database.get_categories();

    // 建立表單選項
    let sticker_options: Vec<AppFormOption> = stickers
        .iter()
        .enumerate()
        .map(|(idx, s)| AppFormOption {
            label: s.get_display_name(),
            value: idx.to_string(),
        })
        .collect();

    // 建立分類選項
    let category_options: Vec<AppFormOption> = std::iter::once(AppFormOption {
        label: "全部".to_string(),
        value: "".to_string(),
    })
    .chain(categories.iter().map(|cat| AppFormOption {
        label: cat.clone(),
        value: cat.clone(),
    }))
    .collect();

    let form = AppForm {
        title: "選擇貼圖".to_string(),
        icon: "🎨".to_string(),
        fields: vec![
            AppFormField {
                name: "category".to_string(),
                label: "分類".to_string(),
                field_type: "static_select".to_string(),
                options: Some(category_options),
                is_required: Some(false),
            },
            AppFormField {
                name: "sticker_id".to_string(),
                label: "貼圖".to_string(),
                field_type: "static_select".to_string(),
                options: Some(sticker_options),
                is_required: Some(true),
            },
        ],
        submit: AppFormSubmit {
            path: "/api/v1/sticker/submit".to_string(),
            expand: AppExpand {
                acting_user: "all".to_string(),
                acting_user_access_token: "all".to_string(),
            },
        },
    };

    drop(app_state);

    Ok(warp::reply::json(&AppCallResponse::form(form)))
}

async fn handle_app_sticker_submit(
    call: AppCallRequest,
    state: Arc<RwLock<AppState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    info!("收到 Mattermost App 貼圖提交");

    let sticker_index = call
        .values
        .get("sticker_id")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);

    let app_state = state.read().await;

    // 找到對應的貼圖
    if let Some(sticker) = app_state.sticker_database.get_by_index(sticker_index) {
        // 發送貼圖訊息
        let post = Post {
            channel_id: call.context.channel.id.clone(),
            message: format!(
                "**{}**\n![{}]({})",
                sticker.name, sticker.name, sticker.image_url
            ),
            root_id: None,
            props: None,
        };

        if let Err(e) = app_state.mattermost_client.create_post(&post).await {
            error!("發送貼圖失敗: {}", e);
            drop(app_state);
            return Ok(warp::reply::json(&AppCallResponse::error("發送貼圖失敗")));
        } else {
            info!("成功發送貼圖: {}", sticker.name);
        }
    } else {
        error!("找不到貼圖索引: {}", sticker_index);
        drop(app_state);
        return Ok(warp::reply::json(&AppCallResponse::error(
            "找不到指定的貼圖",
        )));
    }

    drop(app_state);

    Ok(warp::reply::json(&AppCallResponse::ok("貼圖已發送！")))
}

/// 錯誤處理器
async fn handle_rejection(
    err: warp::Rejection,
) -> Result<impl warp::Reply, std::convert::Infallible> {
    use warp::http::StatusCode;

    if err.is_not_found() {
        Ok(warp::reply::with_status(
            warp::reply::json(&serde_json::json!({
                "error": "Not Found"
            })),
            StatusCode::NOT_FOUND,
        ))
    } else if err.find::<UnauthorizedError>().is_some() {
        error!("未授權的請求");
        Ok(warp::reply::with_status(
            warp::reply::json(&serde_json::json!({
                "error": "Unauthorized: Invalid slash command token"
            })),
            StatusCode::UNAUTHORIZED,
        ))
    } else {
        error!("未處理的錯誤: {:?}", err);
        Ok(warp::reply::with_status(
            warp::reply::json(&serde_json::json!({
                "error": "Internal Server Error"
            })),
            StatusCode::INTERNAL_SERVER_ERROR,
        ))
    }
}
