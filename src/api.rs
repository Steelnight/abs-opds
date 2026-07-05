use crate::models::{AbsItemsResponse, AbsLibrariesResponse, AbsLibrary, AbsLoginResponse, InternalUser};
use reqwest::Client;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use async_trait::async_trait;

#[async_trait]
pub trait AbsClient: Send + Sync {
    async fn login(&self, username: &str, password: &str) -> anyhow::Result<InternalUser>;
    async fn get_libraries(&self, user: &InternalUser) -> anyhow::Result<Vec<AbsLibrary>>;
    async fn get_library(&self, user: &InternalUser, library_id: &str) -> anyhow::Result<AbsLibrary>;
    async fn get_items(&self, user: &InternalUser, library_id: &str) -> anyhow::Result<AbsItemsResponse>;
}

#[derive(Clone)]
struct CachedSession {
    token: String,
    password_hash: String,
    expires: Instant,
}

#[derive(Clone)]
struct CachedItems {
    response: AbsItemsResponse,
    expires: Instant,
}

#[derive(Clone)]
pub struct ApiClient {
    base_url: String,
    client: Client,
    token_cache: Arc<RwLock<HashMap<String, CachedSession>>>,
    items_cache: Arc<RwLock<HashMap<String, CachedItems>>>,
    cache_ttl: Duration,
}

impl ApiClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: Client::new(),
            token_cache: Arc::new(RwLock::new(HashMap::new())),
            items_cache: Arc::new(RwLock::new(HashMap::new())),
            cache_ttl: Duration::from_secs(600), // 10 minutes
        }
    }
}

#[async_trait]
impl AbsClient for ApiClient {
    async fn login(&self, username: &str, password: &str) -> anyhow::Result<InternalUser> {
        let incoming_hash = {
            let mut hasher = sha1_smol::Sha1::new();
            hasher.update(password.as_bytes());
            hasher.digest().to_string()
        };

        // Check cache
        {
            let cache = self.token_cache.read().unwrap();
            if let Some(session) = cache.get(username) {
                if Instant::now() < session.expires && session.password_hash == incoming_hash {
                    return Ok(InternalUser {
                        name: username.to_string(),
                        api_key: session.token.clone(),
                        password: None,
                    });
                }
            }
        }

        let url = format!("{}/login", self.base_url);
        let body = HashMap::from([("username", username), ("password", password)]);

        match self.client.post(&url).json(&body).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    let data = response.json::<AbsLoginResponse>().await?;
                    {
                        let mut cache = self.token_cache.write().unwrap();
                        let now = Instant::now();
                        // Suggestion 9: Evict expired sessions to prevent memory leaks
                        cache.retain(|_, session| now < session.expires);
                        cache.insert(
                            username.to_string(),
                            CachedSession {
                                token: data.user.access_token.clone(),
                                password_hash: incoming_hash,
                                expires: now + self.cache_ttl,
                            },
                        );
                    }
                    return Ok(InternalUser {
                        name: data.user.username,
                        api_key: data.user.access_token,
                        password: None,
                    });
                } else {
                    return Err(anyhow::anyhow!("Invalid credentials or server error"));
                }
            }
            Err(e) => return Err(e.into()),
        }
    }

    async fn get_libraries(&self, user: &InternalUser) -> anyhow::Result<Vec<AbsLibrary>> {
        let url = format!("{}/api/libraries", self.base_url);
        let response = self
            .client
            .get(&url)
            .bearer_auth(&user.api_key)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Failed to fetch libraries: status {}", response.status()));
        }

        let data = response.json::<AbsLibrariesResponse>().await?;
        Ok(data.libraries)
    }

    async fn get_library(&self, user: &InternalUser, library_id: &str) -> anyhow::Result<AbsLibrary> {
         let url = format!("{}/api/libraries/{}", self.base_url, library_id);
        let response = self
            .client
            .get(&url)
            .bearer_auth(&user.api_key)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Failed to fetch library details: status {}", response.status()));
        }

        Ok(response.json::<AbsLibrary>().await?)
    }

    async fn get_items(&self, user: &InternalUser, library_id: &str) -> anyhow::Result<AbsItemsResponse> {
        let cache_key = format!("{}:{}", user.api_key, library_id);
        {
            let cache = self.items_cache.read().unwrap();
            if let Some(cached) = cache.get(&cache_key) {
                if Instant::now() < cached.expires {
                    return Ok(cached.response.clone());
                }
            }
        }

        let url = format!("{}/api/libraries/{}/items", self.base_url, library_id);
        let response = self
            .client
            .get(&url)
            .bearer_auth(&user.api_key)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Failed to fetch library items: status {}", response.status()));
        }

        let data = response.json::<AbsItemsResponse>().await?;
        {
            let mut cache = self.items_cache.write().unwrap();
            let now = Instant::now();
            cache.retain(|_, cached| now < cached.expires);
            cache.insert(
                cache_key,
                CachedItems {
                    response: data.clone(),
                    expires: now + Duration::from_secs(60), // Cache for 60 seconds
                },
            );
        }
        Ok(data)
    }
}
