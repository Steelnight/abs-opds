use axum::{
    routing::{any, get},
    Router,
};
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub mod api;
pub mod auth;
pub mod handlers;
pub mod i18n;
pub mod models;
pub mod service;
#[cfg(test)]
pub mod tests;
pub mod utils;
pub mod xml;

use api::AbsClient;
use api::ApiClient;
use i18n::I18n;
use models::AppConfig;
use service::LibraryService;

pub struct AppState {
    pub config: AppConfig,
    pub api_client: Arc<dyn AbsClient + Send + Sync>,
    pub i18n: I18n,
    pub api_client_raw: reqwest::Client,
    pub service: LibraryService<dyn AbsClient + Send + Sync>,
}

pub async fn build_app_state(config: AppConfig) -> Arc<AppState> {
    let languages_dir = std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("languages");
    let i18n = I18n::new(&languages_dir);

    let api_client = Arc::new(ApiClient::new(config.abs_url.clone()));
    let api_client_raw = reqwest::Client::new();
    let client_dyn: Arc<dyn AbsClient + Send + Sync> = api_client;

    let service = LibraryService::new(client_dyn.clone(), config.clone(), i18n.clone());

    Arc::new(AppState {
        config,
        api_client: client_dyn,
        i18n,
        api_client_raw,
        service,
    })
}

pub async fn build_app_state_with_mock(
    config: AppConfig,
    mock_client: Arc<dyn AbsClient + Send + Sync>,
) -> Arc<AppState> {
    let languages_dir = std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("languages");
    let i18n = I18n::new(&languages_dir);
    let api_client_raw = reqwest::Client::new();

    let service = LibraryService::new(mock_client.clone(), config.clone(), i18n.clone());

    Arc::new(AppState {
        config,
        api_client: mock_client,
        i18n,
        api_client_raw,
        service,
    })
}

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/opds", get(handlers::get_opds_root))
        .route("/opds/libraries/{library_id}", get(handlers::get_library))
        .route(
            "/opds/libraries/{library_id}/search-definition",
            get(handlers::search_definition),
        )
        .route(
            "/opds/libraries/{library_id}/{type}",
            get(handlers::get_category),
        )
        .route("/opds/proxy/{*any}", any(handlers::proxy_handler))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

pub async fn run() {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "abs_opds=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let mut config = envy::from_env::<AppConfig>().expect("Failed to load configuration");
    if let Err(e) = config.parse_users() {
        tracing::error!("Configuration error: {}", e);
        std::process::exit(1);
    }
    if let Err(e) = config.validate() {
        tracing::error!("Configuration validation failed: {}", e);
        std::process::exit(1);
    }

    let port = config.port;
    let abs_url = config.abs_url.clone();

    let state = build_app_state(config).await;
    let app = build_router(state);

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("OPDS server running at http://{}", addr);
    tracing::info!("Server URL: {}", abs_url);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
