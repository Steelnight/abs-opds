use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalUser {
    pub name: String,
    pub api_key: String,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryItem {
    pub id: String,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub publisher: Option<String>,
    pub isbn: Option<String>,
    pub language: Option<String>,
    #[serde(rename = "publishedYear")]
    pub published_year: Option<String>,
    #[serde(default)]
    pub authors: Vec<Author>,
    #[serde(default)]
    pub narrators: Vec<Author>,
    #[serde(default)]
    pub series: Vec<String>,
    pub format: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Author {
    pub name: String,
}

// Structures for deserializing ABS API responses

#[derive(Debug, Deserialize)]
pub struct AbsLibrariesResponse {
    pub libraries: Vec<AbsLibrary>,
}

#[derive(Debug, Deserialize)]
pub struct AbsLibrary {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AbsItemsResponse {
    pub results: Vec<AbsItemResult>,
}

#[derive(Debug, Deserialize)]
pub struct AbsItemResult {
    pub id: String,
    pub media: AbsMedia,
}

#[derive(Debug, Deserialize)]
pub struct AbsMedia {
    pub metadata: AbsMetadata,
    #[serde(rename = "ebookFormat")]
    pub ebook_format: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AbsMetadata {
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    pub genres: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub publisher: Option<String>,
    pub isbn: Option<String>,
    pub language: Option<String>,
    #[serde(rename = "publishedYear")]
    pub published_year: Option<String>,
    #[serde(rename = "authorName")]
    pub author_name: Option<String>,
    #[serde(rename = "narratorName")]
    pub narrator_name: Option<String>,
    #[serde(rename = "seriesName")]
    pub series_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AbsLoginResponse {
    pub user: AbsUserResponse,
}

#[derive(Debug, Deserialize)]
pub struct AbsUserResponse {
    pub username: String,
    #[serde(rename = "accessToken")]
    pub access_token: String,
}

// App Configuration
#[derive(Clone)]
pub struct AppConfig {
    pub port: u16,
    pub use_proxy: bool,
    pub abs_url: String,
    pub internal_users: Vec<InternalUser>,
    pub show_audiobooks: bool,
    pub show_char_cards: bool,
    pub no_auth_mode: bool,
    pub no_auth_username: String,
    pub no_auth_password: String,
    pub opds_page_size: usize,
}
