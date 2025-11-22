use crate::auth::AuthUser;
use crate::models::ItemType;
use crate::xml::OpdsBuilder;
use crate::AppState;
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use sha1_smol::Sha1;
use std::sync::Arc;

#[derive(serde::Deserialize)]
pub struct LibraryQuery {
    pub categories: Option<String>,
    #[serde(default)]
    pub page: usize,
    pub q: Option<String>,
    pub author: Option<String>,
    pub title: Option<String>,
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub type_: Option<ItemType>,
    pub start: Option<String>,
}

pub async fn get_opds_root(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    _headers: HeaderMap,
) -> Response {
    match state.service.get_libraries(&user).await {
        Ok(libraries) => {
            if libraries.len() == 1 {
                return axum::response::Redirect::temporary(&format!(
                    "/opds/libraries/{}?categories=true",
                    libraries[0].id
                ))
                .into_response();
            }

            let mut hasher = Sha1::new();
            hasher.update(user.name.as_bytes());
            let user_hash = hasher.digest().to_string();

            let xml = OpdsBuilder::build_opds_skeleton(
                &user_hash,
                &format!("{}'s Libraries", user.name),
                OpdsBuilder::build_library_entry_list(&libraries),
                None,
                Some(&user),
                None,
                "/opds",
            )
            .unwrap_or_else(|_| String::new());

            ([(axum::http::header::CONTENT_TYPE, "application/xml")], xml).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to fetch libraries: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to fetch libraries",
            )
                .into_response()
        }
    }
}

pub async fn get_library(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(library_id): Path<String>,
    Query(query): Query<LibraryQuery>,
    headers: HeaderMap,
) -> Response {
    let lang = headers.get("accept-language").and_then(|h| h.to_str().ok());

    if query.categories.is_some() {
        let xml = OpdsBuilder::build_opds_skeleton(
            &format!("urn:uuid:{}", library_id),
            "Categories",
            OpdsBuilder::build_category_entries(&library_id, &state.i18n, lang),
            None,
            None,
            None,
            &format!("/opds/libraries/{}", library_id),
        )
        .unwrap_or_else(|_| String::new());
        return ([(axum::http::header::CONTENT_TYPE, "application/xml")], xml).into_response();
    }

    match state.service.get_library(&user, &library_id).await {
        Ok(library) => {
            match state
                .service
                .get_filtered_items(&user, &library_id, &query)
                .await
            {
                Ok((paginated_items, total_items)) => {
                    let page_size = state.config.opds_page_size;
                    let total_pages = total_items.div_ceil(page_size);

                    let link_url = if state.config.use_proxy {
                        "/opds/proxy"
                    } else {
                        &state.config.abs_url
                    };

                    let mut url_base = format!("/opds/libraries/{}", library_id);
                    let mut params = Vec::new();
                    if let Some(q) = &query.q {
                        params.push(format!("q={}", q));
                    }
                    if let Some(t) = &query.type_ {
                        params.push(format!("type={}", t));
                    }
                    if let Some(n) = &query.name {
                        params.push(format!("name={}", n));
                    }
                    if let Some(a) = &query.author {
                        params.push(format!("author={}", a));
                    }
                    if let Some(t) = &query.title {
                        params.push(format!("title={}", t));
                    }

                    if !params.is_empty() {
                        url_base.push('?');
                        url_base.push_str(&params.join("&"));
                    }

                    let xml = OpdsBuilder::build_opds_skeleton(
                        &format!("urn:uuid:{}", library_id),
                        &library.name,
                        |writer| {
                            for item in paginated_items {
                                OpdsBuilder::build_item_entry(writer, &item, &user, link_url)?;
                            }
                            Ok(())
                        },
                        Some(&library),
                        Some(&user),
                        Some((query.page, page_size, total_items, total_pages)),
                        &url_base,
                    )
                    .unwrap_or_else(|_| String::new());

                    ([(axum::http::header::CONTENT_TYPE, "application/xml")], xml).into_response()
                }
                Err(e) => {
                    tracing::error!("Failed to filter items: {}", e);
                    (StatusCode::INTERNAL_SERVER_ERROR, "Failed to process items").into_response()
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to fetch library: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to fetch library").into_response()
        }
    }
}

pub async fn get_category(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path((library_id, type_)): Path<(String, String)>,
    Query(query): Query<LibraryQuery>,
) -> Response {
    let item_type_str = type_.as_str();
    if !["authors", "narrators", "genres", "series"].contains(&item_type_str) {
        return (StatusCode::BAD_REQUEST, "Invalid type").into_response();
    }

    match state
        .service
        .get_categories(&user, &library_id, &type_, &query)
        .await
    {
        Ok(xml) => ([(axum::http::header::CONTENT_TYPE, "application/xml")], xml).into_response(),
        Err(e) => {
            tracing::error!("Failed to fetch category items: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to fetch category items",
            )
                .into_response()
        }
    }
}

pub async fn search_definition(Path(library_id): Path<String>) -> Response {
    let xml = OpdsBuilder::build_search_definition(&library_id);
    ([(axum::http::header::CONTENT_TYPE, "application/xml")], xml).into_response()
}

pub async fn proxy_handler(
    State(state): State<Arc<AppState>>,
    req: axum::extract::Request,
) -> Response {
    if !state.config.use_proxy {
        return (StatusCode::FORBIDDEN, "Forbidden").into_response();
    }

    if req.method() != axum::http::Method::GET {
        return (StatusCode::METHOD_NOT_ALLOWED, "Method Not Allowed").into_response();
    }

    let path = req.uri().path();
    let target_path = path.trim_start_matches("/opds/proxy");
    let target_url = format!("{}{}", state.config.abs_url, target_path);

    let full_target_url = if let Some(query) = req.uri().query() {
        format!("{}?{}", target_url, query)
    } else {
        target_url
    };

    match state.api_client_raw.get(&full_target_url).send().await {
        Ok(resp) => {
            let mut headers = HeaderMap::new();
            // Convert reqwest status to axum status
            let status =
                StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);

            for (k, v) in resp.headers() {
                if let Ok(h_name) =
                    axum::http::header::HeaderName::from_bytes(k.as_str().as_bytes())
                {
                    if let Ok(h_val) = axum::http::header::HeaderValue::from_bytes(v.as_bytes()) {
                        headers.insert(h_name, h_val);
                    }
                }
            }

            let stream = resp.bytes_stream();
            let body = Body::from_stream(stream);

            (status, headers, body).into_response()
        }
        Err(e) => {
            tracing::error!("Proxy error: {}", e);
            (StatusCode::BAD_GATEWAY, "Bad Gateway").into_response()
        }
    }
}
