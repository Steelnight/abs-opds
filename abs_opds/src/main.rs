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
mod xml;
#[cfg(test)]
mod tests;

use api::ApiClient;
use i18n::I18n;
use models::{AppConfig, InternalUser};

pub struct AppState {
    pub config: AppConfig,
    pub api_client: ApiClient,
    pub i18n: I18n,
    pub api_client_raw: reqwest::Client, // For proxy
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

    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "3010".to_string())
        .parse::<u16>()
        .unwrap_or(3010);

    let use_proxy = std::env::var("USE_PROXY").unwrap_or_else(|_| "false".to_string()) == "true";
    let abs_url = std::env::var("ABS_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());
    let internal_users_str = std::env::var("OPDS_USERS").unwrap_or_default();
    let show_audiobooks = std::env::var("SHOW_AUDIOBOOKS").unwrap_or_else(|_| "false".to_string()) == "true";
    let show_char_cards = std::env::var("SHOW_CHAR_CARDS").unwrap_or_else(|_| "false".to_string()) == "true";
    let no_auth_mode = std::env::var("OPDS_NO_AUTH").unwrap_or_else(|_| "false".to_string()) == "true";
    let no_auth_username = std::env::var("ABS_NOAUTH_USERNAME").unwrap_or_default();
    let no_auth_password = std::env::var("ABS_NOAUTH_PASSWORD").unwrap_or_default();
    let opds_page_size = std::env::var("OPDS_PAGE_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);

    let internal_users: Vec<InternalUser> = internal_users_str
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|user| {
            let parts: Vec<&str> = user.split(':').collect();
            if parts.len() >= 3 {
                 InternalUser {
                    name: parts[0].to_string(),
                    api_key: parts[1].to_string(),
                    password: Some(parts[2].to_string()),
                }
            } else {
                InternalUser {
                    name: "Invalid".to_string(),
                    api_key: "".to_string(),
                    password: None,
                }
            }
        })
        .filter(|u| !u.api_key.is_empty())
        .collect();

    let languages_dir = std::env::current_dir().unwrap().join("languages");
    let i18n = I18n::new(&languages_dir);

    let config = AppConfig {
        port,
        use_proxy,
        abs_url: abs_url.clone(),
        internal_users,
        show_audiobooks,
        show_char_cards,
        no_auth_mode,
        no_auth_username,
        no_auth_password,
        opds_page_size,
    };

    let api_client = ApiClient::new(abs_url.clone());
    let api_client_raw = reqwest::Client::new();

    let state = Arc::new(AppState {
        config,
        api_client,
        i18n,
        api_client_raw,
    });

    let app = Router::new()
        .route("/opds", get(handlers::get_opds_root))
        .route("/opds/libraries/:library_id", get(handlers::get_library))
        .route("/opds/libraries/:library_id/search-definition", get(handlers::search_definition))
        .route("/opds/libraries/:library_id/:type", get(handlers::get_category))
        .route("/opds/proxy/*any", any(handlers::proxy_handler))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("OPDS server running at http://{}", addr);
    tracing::info!("Server URL: {}", abs_url);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
