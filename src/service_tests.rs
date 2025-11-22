#[cfg(test)]
mod tests {
    use crate::api::AbsClient;
    use crate::models::{AbsItemsResponse, AbsLibrary, AbsItemResult, AbsMedia, AbsMetadata, InternalUser, AppConfig};
    use crate::service::LibraryService;
    use crate::i18n::I18n;
    use crate::handlers::LibraryQuery;
    use mockall::mock;
    use std::sync::Arc;
    use async_trait::async_trait;

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
            show_char_cards: false,
            opds_no_auth: false,
            abs_noauth_username: "".to_string(),
            abs_noauth_password: "".to_string(),
            opds_page_size: 10,
        }
    }

    fn mock_i18n() -> I18n {
         let languages_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")).join("languages");
         I18n::new(&languages_dir)
    }

    fn mock_items_response(items: Vec<AbsItemResult>) -> AbsItemsResponse {
        AbsItemsResponse { results: items }
    }

    fn create_item(id: &str, title: &str, author: Option<&str>, genre: Option<&str>) -> AbsItemResult {
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
    async fn test_get_filtered_items_search() {
        let mut mock_client = MockAbsClient::new();
        let user = mock_user();

        let items = vec![
            create_item("1", "The Hobbit", Some("J.R.R. Tolkien"), Some("Fantasy")),
            create_item("2", "Harry Potter", Some("J.K. Rowling"), Some("Fantasy")),
            create_item("3", "1984", Some("George Orwell"), Some("Sci-Fi")),
        ];

        mock_client
            .expect_get_items()
            .times(1)
            .returning(move |_, _| Ok(mock_items_response(items.clone())));

        let service = LibraryService::new(Arc::new(mock_client), mock_config(), mock_i18n());

        let query = LibraryQuery {
            q: Some("Harry".to_string()),
            page: 0,
            categories: None,
            author: None,
            title: None,
            name: None,
            type_: None,
            start: None,
        };

        let (filtered, total) = service.get_filtered_items(&user, "lib1", &query).await.unwrap();

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].title, Some("Harry Potter".to_string()));
        assert_eq!(total, 1);
    }

     #[tokio::test]
    async fn test_get_filtered_items_author() {
        let mut mock_client = MockAbsClient::new();
        let user = mock_user();

        let items = vec![
            create_item("1", "The Hobbit", Some("J.R.R. Tolkien"), Some("Fantasy")),
            create_item("2", "LOTR", Some("J.R.R. Tolkien"), Some("Fantasy")),
            create_item("3", "1984", Some("George Orwell"), Some("Sci-Fi")),
        ];

        mock_client
            .expect_get_items()
            .times(1)
            .returning(move |_, _| Ok(mock_items_response(items.clone())));

        let service = LibraryService::new(Arc::new(mock_client), mock_config(), mock_i18n());

        let query = LibraryQuery {
            q: None,
            page: 0,
            categories: None,
            author: Some("Tolkien".to_string()),
            title: None,
            name: None,
            type_: None,
            start: None,
        };

        let (filtered, total) = service.get_filtered_items(&user, "lib1", &query).await.unwrap();

        assert_eq!(filtered.len(), 2);
        assert_eq!(total, 2);
    }

    #[tokio::test]
    async fn test_pagination() {
        let mut mock_client = MockAbsClient::new();
        let user = mock_user();

        let mut items = Vec::new();
        for i in 0..25 {
             items.push(create_item(&format!("{}", i), &format!("Book {}", i), None, None));
        }

        mock_client
            .expect_get_items()
            .times(1)
            .returning(move |_, _| Ok(mock_items_response(items.clone())));

        let mut config = mock_config();
        config.opds_page_size = 10;
        let service = LibraryService::new(Arc::new(mock_client), config, mock_i18n());

        // Page 0
        let query = LibraryQuery {
            q: None,
            page: 0,
            categories: None,
            author: None,
            title: None,
            name: None,
            type_: None,
            start: None,
        };
        let (filtered, total) = service.get_filtered_items(&user, "lib1", &query).await.unwrap();
        assert_eq!(filtered.len(), 10);
        assert_eq!(total, 25);
        assert_eq!(filtered[0].title, Some("Book 0".to_string()));

         // Page 2 (last page, 5 items)
        let _query = LibraryQuery {
            q: None,
            page: 2,
            categories: None,
            author: None,
            title: None,
            name: None,
            type_: None,
            start: None,
        };
        // We need to recreate service or mock because mock expectations are consumed? No, .times(1) consumes.
        // But we can't easily reuse the same service with mockall in this setup without `clone` on client which is Arc.
        // But here we create a new mock expectation.
        // Ideally, we should use .times(2) or separate tests.
        // Let's just test page logic in this function with a new setup or assuming consistent returns if we set .times(2)
    }

     #[tokio::test]
    async fn test_pagination_page_2() {
        let mut mock_client = MockAbsClient::new();
        let user = mock_user();

        let mut items = Vec::new();
        for i in 0..25 {
             items.push(create_item(&format!("{}", i), &format!("Book {}", i), None, None));
        }

        mock_client
            .expect_get_items()
            .times(1)
            .returning(move |_, _| Ok(mock_items_response(items.clone())));

        let mut config = mock_config();
        config.opds_page_size = 10;
        let service = LibraryService::new(Arc::new(mock_client), config, mock_i18n());

         // Page 2 (last page, 5 items)
        let query = LibraryQuery {
            q: None,
            page: 2,
            categories: None,
            author: None,
            title: None,
            name: None,
            type_: None,
            start: None,
        };
        let (filtered, total) = service.get_filtered_items(&user, "lib1", &query).await.unwrap();
        assert_eq!(filtered.len(), 5);
        assert_eq!(total, 25);
        assert_eq!(filtered[0].title, Some("Book 20".to_string()));
    }
}
