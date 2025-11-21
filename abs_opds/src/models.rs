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

impl LibraryItem {
    pub fn matches(&self, re: &regex::Regex) -> bool {
        self.title.as_deref().map_or(false, |s| re.is_match(s)) ||
        self.subtitle.as_deref().map_or(false, |s| re.is_match(s)) ||
        self.description.as_deref().map_or(false, |s| re.is_match(s)) ||
        self.publisher.as_deref().map_or(false, |s| re.is_match(s)) ||
        self.isbn.as_deref().map_or(false, |s| re.is_match(s)) ||
        self.language.as_deref().map_or(false, |s| re.is_match(s)) ||
        self.published_year.as_deref().map_or(false, |s| re.is_match(s)) ||
        self.authors.iter().any(|a| re.is_match(&a.name)) ||
        self.genres.iter().any(|g| re.is_match(g)) ||
        self.tags.iter().any(|t| re.is_match(t))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Author {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemType {
    Authors,
    Narrators,
    Genres,
    Series,
}

impl std::fmt::Display for ItemType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ItemType::Authors => write!(f, "authors"),
            ItemType::Narrators => write!(f, "narrators"),
            ItemType::Genres => write!(f, "genres"),
            ItemType::Series => write!(f, "series"),
        }
    }
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
#[derive(Clone, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_use_proxy")]
    pub use_proxy: bool,
    #[serde(default = "default_abs_url")]
    pub abs_url: String,
    #[serde(default)]
    pub opds_users: String, // Raw string from env
    #[serde(skip)]
    pub internal_users: Vec<InternalUser>,
    #[serde(default = "default_false")]
    pub show_audiobooks: bool,
    #[serde(default = "default_false")]
    pub show_char_cards: bool,
    #[serde(default = "default_false")]
    pub opds_no_auth: bool, // Renamed from no_auth_mode to match env
    #[serde(default)]
    pub abs_noauth_username: String,
    #[serde(default)]
    pub abs_noauth_password: String,
    #[serde(default = "default_page_size")]
    pub opds_page_size: usize,
}

impl AppConfig {
    // Method to parse internal users after deserialization
    pub fn parse_users(&mut self) {
        self.internal_users = self.opds_users
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
    }
}

fn default_port() -> u16 { 3010 }
fn default_use_proxy() -> bool { false }
fn default_abs_url() -> String { "http://localhost:3000".to_string() }
fn default_false() -> bool { false }
fn default_page_size() -> usize { 20 }
