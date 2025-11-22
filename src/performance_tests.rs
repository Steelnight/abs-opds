#[cfg(test)]
mod tests {
    use crate::api::AbsClient;
    use crate::handlers::LibraryQuery;
    use crate::i18n::I18n;
    use crate::models::{
        AbsItemResult, AbsItemsResponse, AbsLibrary, AbsMedia, AbsMetadata, AppConfig, InternalUser,
    };
    use crate::service::LibraryService;
    use async_trait::async_trait;
    use mockall::mock;
    use std::sync::Arc;
    use std::time::Instant;

    mock! {
        pub AbsClient {}
        #[async_trait]
        impl AbsClient for AbsClient {
            async fn login(&self, username: &str, password: &str) -> anyhow::Result<InternalUser>;
            async fn get_libraries(&self, user: &InternalUser) -> anyhow::Result<Vec<AbsLibrary>>;
            async fn get_library(&self, user: &InternalUser, library_id: &str) -> anyhow::Result<AbsLibrary>;
            async fn get_items(&self, user: &InternalUser, library_id: &str) -> anyhow::Result<AbsItemsResponse>;
        }
    }

    fn mock_user() -> InternalUser {
        InternalUser {
            name: "test_user".to_string(),
            api_key: "test_token".to_string(),
            password: None,
        }
    }

    fn mock_config() -> AppConfig {
        AppConfig {
            port: 3000,
            use_proxy: false,
            abs_url: "http://localhost:3000".to_string(),
            opds_users: "user:token:pass".to_string(),
            internal_users: vec![],
            show_audiobooks: true,
            show_char_cards: true,
            opds_no_auth: false,
            abs_noauth_username: "".to_string(),
            abs_noauth_password: "".to_string(),
            opds_page_size: 100,
        }
    }

    fn mock_i18n() -> I18n {
        let languages_dir = std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join("languages");
        I18n::new(&languages_dir)
    }

    fn mock_items_response(items: Vec<AbsItemResult>) -> AbsItemsResponse {
        AbsItemsResponse { results: items }
    }

    fn create_item(
        id: &str,
        title: &str,
        author: Option<&str>,
        genre: Option<&str>,
    ) -> AbsItemResult {
        AbsItemResult {
            id: id.to_string(),
            media: AbsMedia {
                ebook_format: Some("epub".to_string()),
                metadata: AbsMetadata {
                    title: Some(title.to_string()),
                    subtitle: None,
                    description: None,
                    genres: genre.map(|g| vec![g.to_string()]),
                    tags: None,
                    publisher: None,
                    isbn: None,
                    language: Some("en".to_string()),
                    published_year: None,
                    author_name: author.map(|a| a.to_string()),
                    narrator_name: None,
                    series_name: None,
                },
            },
        }
    }

    #[tokio::test]
    async fn test_performance_100000_items() {
        let mut mock_client = MockAbsClient::new();
        let user = mock_user();

        let mut items = Vec::with_capacity(100_000);
        for i in 0..100_000 {
            items.push(create_item(
                &format!("{}", i),
                &format!("Book Title {}", i),
                Some(&format!("Author {}", i % 500)), // 500 distinct authors
                Some(&format!("Genre {}", i % 50)),   // 50 distinct genres
            ));
        }

        mock_client
            .expect_get_items()
            .returning(move |_, _| Ok(mock_items_response(items.clone())));

        mock_client.expect_get_library().returning(|_, _| {
            Ok(AbsLibrary {
                id: "lib1".to_string(),
                name: "Test Library".to_string(),
                icon: None,
            })
        });

        let service = LibraryService::new(Arc::new(mock_client), mock_config(), mock_i18n());

        let query = LibraryQuery {
            q: Some("Book Title 50000".to_string()), // Search for a specific item
            page: 0,
            categories: None,
            author: None,
            title: None,
            name: None,
            type_: None,
            start: None,
        };

        println!("Starting performance test with 100,000 items...");

        // Measure get_filtered_items
        let start = Instant::now();
        let (filtered, total) = service
            .get_filtered_items(&user, "lib1", &query)
            .await
            .unwrap();
        let duration = start.elapsed();
        println!("get_filtered_items took: {:?}", duration);
        assert!(total > 0);
        assert!(!filtered.is_empty());

        // Measure get_categories (Authors)
        let start = Instant::now();
        let _categories = service
            .get_categories(
                &user,
                "lib1",
                "authors",
                &LibraryQuery {
                    q: None,
                    page: 0,
                    categories: None,
                    author: None,
                    title: None,
                    name: None,
                    type_: None,
                    start: None,
                },
            )
            .await
            .unwrap();
        let duration = start.elapsed();
        println!("get_categories (authors) took: {:?}", duration);

        // Measure get_categories (Genres)
        let start = Instant::now();
        let _categories = service
            .get_categories(
                &user,
                "lib1",
                "genres",
                &LibraryQuery {
                    q: None,
                    page: 0,
                    categories: None,
                    author: None,
                    title: None,
                    name: None,
                    type_: None,
                    start: None,
                },
            )
            .await
            .unwrap();
        let duration = start.elapsed();
        println!("get_categories (genres) took: {:?}", duration);
    }
}
