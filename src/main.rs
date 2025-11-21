use axum::{
    routing::{get, any},
    Router,
};
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod api;
mod auth;
mod handlers;
mod i18n;
mod models;
mod service;
mod xml;
#[cfg(test)]
mod tests;

use api::ApiClient;
use i18n::I18n;
use models::AppConfig;
use service::LibraryService;

pub struct AppState {
    pub config: AppConfig,
    pub api_client: Arc<ApiClient>,
    pub i18n: I18n,
    pub api_client_raw: reqwest::Client, // For proxy
    pub service: LibraryService<ApiClient>,
}

#[tokio::main]
async fn main() {
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

    let languages_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")).join("languages");
    let i18n = I18n::new(&languages_dir);

    let api_client = Arc::new(ApiClient::new(config.abs_url.clone()));
    let api_client_raw = reqwest::Client::new();
    let service = LibraryService::new(api_client.clone(), config.clone(), i18n.clone());

    let port = config.port;
    let abs_url = config.abs_url.clone();

    let state = Arc::new(AppState {
        config,
        api_client,
        i18n,
        api_client_raw,
        service,
    });

    let app = Router::new()
        .route("/opds", get(handlers::get_opds_root))
        .route("/opds/libraries/{library_id}", get(handlers::get_library))
        .route("/opds/libraries/{library_id}/search-definition", get(handlers::search_definition))
        .route("/opds/libraries/{library_id}/{type}", get(handlers::get_category))
        .route("/opds/proxy/{*any}", any(handlers::proxy_handler))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("OPDS server running at http://{}", addr);
    tracing::info!("Server URL: {}", abs_url);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
