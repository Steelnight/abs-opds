use crate::auth::AuthUser;
use crate::models::{Library, LibraryItem, ItemType};
use crate::xml::OpdsBuilder;
use crate::AppState;
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    body::Body,
};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};
use sha1_smol::Sha1;
use unicode_normalization::UnicodeNormalization;

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
    match state.api_client.get_libraries(&user).await {
        Ok(libraries) => {
            let libraries: Vec<Library> = libraries.into_iter().map(|l| Library {
                id: l.id,
                name: l.name,
                icon: l.icon,
            }).collect();

            if libraries.len() == 1 {
                 return axum::response::Redirect::temporary(&format!("/opds/libraries/{}?categories=true", libraries[0].id)).into_response();
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
                "/opds"
            ).unwrap_or_else(|_| String::new());

             ([(axum::http::header::CONTENT_TYPE, "application/xml")], xml).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to fetch libraries: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to fetch libraries").into_response()
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
             &format!("/opds/libraries/{}", library_id)
         ).unwrap_or_else(|_| String::new());
         return ([(axum::http::header::CONTENT_TYPE, "application/xml")], xml).into_response();
    }

    let items_res = state.api_client.get_items(&user, &library_id).await;
    let lib_res = state.api_client.get_library(&user, &library_id).await;

    if let (Ok(items_data), Ok(lib_data)) = (items_res, lib_res) {
         let library = Library {
             id: lib_data.id,
             name: lib_data.name,
             icon: lib_data.icon,
         };

         let mut parsed_items: Vec<LibraryItem> = items_data.results.into_iter().filter_map(|item| {
             let format = item.media.ebook_format;
             if format.is_some() || state.config.show_audiobooks {
                 Some(LibraryItem {
                     id: item.id,
                     title: item.media.metadata.title,
                     subtitle: item.media.metadata.subtitle,
                     description: item.media.metadata.description,
                     genres: item.media.metadata.genres.unwrap_or_default(),
                     tags: item.media.metadata.tags.unwrap_or_default(),
                     publisher: item.media.metadata.publisher,
                     isbn: item.media.metadata.isbn,
                     language: item.media.metadata.language,
                     published_year: item.media.metadata.published_year,
                     authors: item.media.metadata.author_name.map(|s| s.split(',').map(|n| crate::models::Author { name: n.trim().to_string() }).collect()).unwrap_or_default(),
                     narrators: item.media.metadata.narrator_name.map(|s| s.split(',').map(|n| crate::models::Author { name: n.trim().to_string() }).collect()).unwrap_or_default(),
                     series: item.media.metadata.series_name.map(|s| {
                         static SERIES_CLEANUP_RE: OnceLock<regex::Regex> = OnceLock::new();
                         let re = SERIES_CLEANUP_RE.get_or_init(|| regex::Regex::new(r"#.*$").unwrap());
                         s.split(',').map(|n| n.trim().replace(re.as_str(), "").trim().to_string()).collect()
                     }).unwrap_or_default(),
                     format,
                 })
             } else {
                 None
             }
         }).collect();

         if query.q.is_some() || query.type_.is_some() {
             let search_term = query.q.as_deref().unwrap_or("");
             let re = regex::RegexBuilder::new(&regex::escape(search_term))
                .case_insensitive(true)
                .build()
                .unwrap_or_else(|_| regex::Regex::new("").unwrap());

             let type_query = query.type_.as_ref();
             let name_query_re = query.name.as_deref().map(|n| {
                  regex::RegexBuilder::new(&regex::escape(n))
                    .case_insensitive(true)
                    .build()
                    .unwrap_or_else(|_| regex::Regex::new("").unwrap())
             });

             parsed_items.retain(|item| {
                 if type_query == Some(&ItemType::Authors) {
                     if let Some(re) = &name_query_re {
                         return item.authors.iter().any(|a| re.is_match(&a.name));
                     }
                 } else if type_query == Some(&ItemType::Narrators) {
                      if let Some(re) = &name_query_re {
                         return item.narrators.iter().any(|a| re.is_match(&a.name));
                     }
                 } else if type_query == Some(&ItemType::Genres) {
                     if let Some(re) = &name_query_re {
                         return item.genres.iter().any(|g| re.is_match(g)) || item.tags.iter().any(|t| re.is_match(t));
                     }
                 } else if type_query == Some(&ItemType::Series) {
                      if let Some(re) = &name_query_re {
                         return item.series.iter().any(|s| re.is_match(s));
                     }
                 } else {
                      if !search_term.is_empty() {
                         return item.matches(&re);
                      }
                 }
                 true
             });
         }

         if let Some(author) = &query.author {
             let re = regex::RegexBuilder::new(&regex::escape(author)).case_insensitive(true).build().unwrap();
             parsed_items.retain(|item| item.authors.iter().any(|a| re.is_match(&a.name)));
         }

         if let Some(title) = &query.title {
             let re = regex::RegexBuilder::new(&regex::escape(title)).case_insensitive(true).build().unwrap();
             parsed_items.retain(|item| item.title.as_deref().map_or(false, |t| re.is_match(t)) || item.subtitle.as_deref().map_or(false, |t| re.is_match(t)));
         }

         let total_items = parsed_items.len();
         let page_size = state.config.opds_page_size;
         let total_pages = (total_items + page_size - 1) / page_size;
         let start_index = query.page * page_size;

         let paginated_items = if start_index < total_items {
             let end_index = std::cmp::min(start_index + page_size, total_items);
             &parsed_items[start_index..end_index]
         } else {
             &[]
         };

         let link_url = if state.config.use_proxy { "/opds/proxy" } else { &state.config.abs_url };

         let mut url_base = format!("/opds/libraries/{}", library_id);
         let mut params = Vec::new();
         if let Some(q) = &query.q { params.push(format!("q={}", q)); }
         if let Some(t) = &query.type_ { params.push(format!("type={}", t)); }
         if let Some(n) = &query.name { params.push(format!("name={}", n)); }
         if let Some(a) = &query.author { params.push(format!("author={}", a)); }
         if let Some(t) = &query.title { params.push(format!("title={}", t)); }

         if !params.is_empty() {
             url_base.push('?');
             url_base.push_str(&params.join("&"));
         }

         let xml = OpdsBuilder::build_opds_skeleton(
             &format!("urn:uuid:{}", library_id),
             &library.name,
             |writer| {
                 for item in paginated_items {
                     OpdsBuilder::build_item_entry(writer, item, &user, link_url)?;
                 }
                 Ok(())
             },
             Some(&library),
             Some(&user),
             Some((query.page, page_size, total_items, total_pages)),
             &url_base
         ).unwrap_or_else(|_| String::new());

         ([(axum::http::header::CONTENT_TYPE, "application/xml")], xml).into_response()

    } else {
         (StatusCode::INTERNAL_SERVER_ERROR, "Failed to load library or items").into_response()
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

    let items_res = state.api_client.get_items(&user, &library_id).await;
    let lib_res = state.api_client.get_library(&user, &library_id).await;

    if let (Ok(items_data), Ok(lib_data)) = (items_res, lib_res) {
         let library = Library {
             id: lib_data.id,
             name: lib_data.name,
             icon: lib_data.icon,
         };

         let mut distinct_type = HashSet::new();

         for item in items_data.results {
              match type_.as_str() {
                  "authors" => {
                      if let Some(names) = item.media.metadata.author_name {
                          for name in names.split(',') { distinct_type.insert(name.trim().to_string()); }
                      }
                  },
                  "narrators" => {
                       if let Some(names) = item.media.metadata.narrator_name {
                          for name in names.split(',') { distinct_type.insert(name.trim().to_string()); }
                      }
                  },
                  "genres" => {
                      if let Some(genres) = item.media.metadata.genres {
                          for g in genres { distinct_type.insert(g.trim().to_string()); }
                      }
                      if let Some(tags) = item.media.metadata.tags {
                          for t in tags { distinct_type.insert(t.trim().to_string()); }
                      }
                  },
                  "series" => {
                       if let Some(series) = item.media.metadata.series_name {
                          for s in series.split(',') { distinct_type.insert(s.trim().to_string()); }
                      }
                  },
                  _ => {}
              }
         }

         let mut distinct_type_array: Vec<String> = distinct_type.into_iter().collect();
         distinct_type_array.sort();

         if query.start.is_none() && state.config.show_char_cards {
              let mut count_by_start: HashMap<String, usize> = HashMap::new();
              for item in &distinct_type_array {
                  let start_char = item.chars().next().unwrap_or(' ').to_uppercase().to_string();
                  let normalized = start_char.nfd().filter(|c| !crate::xml::is_combining_mark(*c)).collect::<String>();
                  let key = if normalized >= "A".to_string() && normalized <= "Z".to_string() { normalized } else { String::new() };
                  if !key.is_empty() {
                       *count_by_start.entry(key).or_insert(0) += 1;
                  }
              }

              let mut keys: Vec<String> = count_by_start.keys().cloned().collect();
              keys.sort();

              let xml = OpdsBuilder::build_opds_skeleton(
                    &format!("urn:uuid:{}", library_id),
                    &library.name,
                    |writer| {
                        for letter in keys {
                            let count = count_by_start[&letter];
                            let title = format!("{} ({})", letter, count);
                            let link = format!("/opds/libraries/{}/{}?start={}", library_id, type_, letter.to_lowercase());
                            OpdsBuilder::build_custom_card_entry(writer, &title, &link)?;
                        }
                        Ok(())
                    },
                    None,
                    None,
                    None,
                    &format!("/opds/libraries/{}/{}", library_id, type_)
                ).unwrap_or_else(|_| String::new());
                 return ([(axum::http::header::CONTENT_TYPE, "application/xml")], xml).into_response();
         }

         if let Some(start) = &query.start {
             distinct_type_array.retain(|item| {
                  let start_char = item.chars().next().unwrap_or(' ').to_lowercase().to_string();
                   let normalized = start_char.nfd().filter(|c| !crate::xml::is_combining_mark(*c)).collect::<String>();
                   normalized == *start
             });
         }

          let xml = OpdsBuilder::build_opds_skeleton(
             &format!("urn:uuid:{}", library_id),
             &library.name,
             |writer| {
                 for item in distinct_type_array {
                     OpdsBuilder::build_card_entry(writer, &item, &type_, &library_id)?;
                 }
                 Ok(())
             },
             None,
             None,
             None,
             &format!("/opds/libraries/{}/{}", library_id, type_)
         ).unwrap_or_else(|_| String::new());

         ([(axum::http::header::CONTENT_TYPE, "application/xml")], xml).into_response()

    } else {
        (StatusCode::INTERNAL_SERVER_ERROR, "Failed to fetch items").into_response()
    }
}

pub async fn search_definition(
    Path(library_id): Path<String>,
) -> Response {
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
            let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);

            for (k, v) in resp.headers() {
                 if let Ok(h_name) = axum::http::header::HeaderName::from_bytes(k.as_str().as_bytes()) {
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
