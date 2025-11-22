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
pub struct ApiClient {
    base_url: String,
    client: Client,
    token_cache: Arc<RwLock<HashMap<String, (String, Instant)>>>,
    cache_ttl: Duration,
}

impl ApiClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: Client::new(),
            token_cache: Arc::new(RwLock::new(HashMap::new())),
            cache_ttl: Duration::from_secs(600), // 10 minutes
        }
    }
}

#[async_trait]
impl AbsClient for ApiClient {
    async fn login(&self, username: &str, password: &str) -> anyhow::Result<InternalUser> {
        // Check cache
        {
            let cache = self.token_cache.read().unwrap();
            if let Some((token, expires)) = cache.get(username) {
                if Instant::now() < *expires {
                    return Ok(InternalUser {
                        name: username.to_string(),
                        api_key: token.clone(),
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
                        cache.insert(
                            username.to_string(),
                            (data.user.access_token.clone(), Instant::now() + self.cache_ttl),
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

        Ok(response.json::<AbsLibrary>().await?)
    }

    async fn get_items(&self, user: &InternalUser, library_id: &str) -> anyhow::Result<AbsItemsResponse> {
        let url = format!("{}/api/libraries/{}/items", self.base_url, library_id);
        let response = self
            .client
            .get(&url)
            .bearer_auth(&user.api_key)
            .send()
            .await?;

        Ok(response.json::<AbsItemsResponse>().await?)
    }
}
