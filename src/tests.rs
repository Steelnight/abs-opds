#[cfg(test)]
mod tests {
    use crate::models::{Library, LibraryItem, Author, InternalUser, AbsLibrary, AbsItemsResponse, AppConfig};
    use crate::xml::OpdsBuilder;
    use quick_xml::Writer;
    use std::io::Cursor;
    use std::sync::Arc;
    use async_trait::async_trait;
    use mockall::mock;

    mock! {
        pub AbsClient {}
        #[async_trait]
        impl crate::api::AbsClient for AbsClient {
            async fn login(&self, username: &str, password: &str) -> anyhow::Result<InternalUser>;
            async fn get_libraries(&self, user: &InternalUser) -> anyhow::Result<Vec<AbsLibrary>>;
            async fn get_library(&self, user: &InternalUser, library_id: &str) -> anyhow::Result<AbsLibrary>;
            async fn get_items(&self, user: &InternalUser, library_id: &str) -> anyhow::Result<AbsItemsResponse>;
        }
    }

    #[test]
    fn test_build_opds_skeleton() {
        let xml = OpdsBuilder::build_opds_skeleton(
            "test_id",
            "Test Title",
            |_| Ok(()),
            None,
            None,
            None,
            "/opds",
            false,
        ).expect("Failed to build XML");

        assert!(xml.contains("<id>test_id</id>"));
        assert!(xml.contains("<title>Test Title</title>"));
        assert!(xml.contains("<feed xmlns=\"http://www.w3.org/2005/Atom\""));
        assert!(xml.contains("<author><name>ABS-OPDS</name></author>"));
        assert!(xml.contains("<link rel=\"self\" type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" href=\"/opds\"/>"));
    }

    #[test]
    fn test_build_library_entry() {
        let lib = Library {
            id: "lib1".to_string(),
            name: "My Library".to_string(),
            icon: None,
        };

        let mut writer = Writer::new(Cursor::new(Vec::new()));
        OpdsBuilder::build_library_entry(&mut writer, &lib, "2026-06-02T12:00:00Z").expect("Failed to build entry");

        let entry = String::from_utf8(writer.into_inner().into_inner()).unwrap();
        assert!(entry.contains("<id>lib1</id>"));
        assert!(entry.contains("<title>My Library</title>"));
        assert!(entry.contains("/opds/libraries/lib1?categories=true"));
    }

    #[test]
    fn test_build_item_entry() {
        let item = LibraryItem {
            id: "item1".to_string(),
            title: Some("Book Title".to_string()),
            subtitle: None,
            description: Some("Description & Details".to_string()),
            genres: vec!["Fantasy".to_string()],
            tags: vec![],
            publisher: Some("Publisher".to_string()),
            isbn: Some("978-3-16-148410-0".to_string()),
            language: Some("en".to_string()),
            published_year: Some("2023".to_string()),
            authors: vec![Author { name: "Author Name".to_string() }],
            narrators: vec![Author { name: "Narrator Name".to_string() }],
            series: vec![],
            format: Some("epub".to_string()),
        };

        let user = InternalUser {
            name: "user".to_string(),
            api_key: "token".to_string(),
            password: None,
        };

        let mut writer = Writer::new(Cursor::new(Vec::new()));
        let mut url_buf = String::new();
        OpdsBuilder::build_item_entry(&mut writer, &item, &user, "http://localhost:3000", "2026-06-02T12:00:00Z", &mut url_buf).expect("Failed to build entry");

        let entry = String::from_utf8(writer.into_inner().into_inner()).unwrap();
        assert!(entry.contains("<id>urn:uuid:item1</id>"));
        assert!(entry.contains("<title>Book Title</title>"));
        assert!(entry.contains("<name>Author Name</name>"));
        assert!(entry.contains("application/epub+zip"));
        assert!(entry.contains("token=token"));
        assert!(entry.contains("<dcterms:publisher>Publisher</dcterms:publisher>"));
        assert!(entry.contains("<dcterms:identifier>urn:isbn:978-3-16-148410-0</dcterms:identifier>"));
        assert!(entry.contains("<dcterms:issued>2023</dcterms:issued>"));
        assert!(entry.contains("<dcterms:language>en</dcterms:language>"));
        assert!(entry.contains("<dcterms:contributor>Narrator Name</dcterms:contributor>"));
        assert!(entry.contains("<content type=\"text\">Description &amp; Details</content>"));
    }

    #[test]
    fn test_xml_description_escaping() {
        let item = LibraryItem {
            id: "item2".to_string(),
            title: Some("Title".to_string()),
            subtitle: None,
            description: Some("Escaping <test> & \"quotes\"".to_string()),
            genres: vec![],
            tags: vec![],
            publisher: None,
            isbn: None,
            language: None,
            published_year: None,
            authors: vec![],
            narrators: vec![],
            series: vec![],
            format: None,
        };

        let user = InternalUser {
            name: "user".to_string(),
            api_key: "token".to_string(),
            password: None,
        };

        let mut writer = Writer::new(Cursor::new(Vec::new()));
        let mut url_buf = String::new();
        OpdsBuilder::build_item_entry(&mut writer, &item, &user, "http://localhost:3000", "2026-06-02T12:00:00Z", &mut url_buf).expect("Failed to build entry");

        let entry = String::from_utf8(writer.into_inner().into_inner()).unwrap();
        assert!(entry.contains("<content type=\"text\">Escaping &lt;test&gt; &amp; &quot;quotes&quot;</content>"));
    }

    #[tokio::test]
    async fn test_routes_content_type_headers() {
        use tower::ServiceExt;
        use axum::http::{Request, StatusCode};
        use crate::build_app_state_with_mock;
        use crate::build_router;

        let mut mock_client = MockAbsClient::new();

        let user_ref = InternalUser {
            name: "test_user".to_string(),
            api_key: "test_token".to_string(),
            password: None,
        };

        mock_client.expect_login()
            .returning(move |_, _| Ok(InternalUser {
                name: "test_user".to_string(),
                api_key: "test_token".to_string(),
                password: Some("pass".to_string()),
            }));

        let libs = vec![
            AbsLibrary { id: "lib1".to_string(), name: "Lib 1".to_string(), icon: None },
            AbsLibrary { id: "lib2".to_string(), name: "Lib 2".to_string(), icon: None },
        ];

        mock_client.expect_get_libraries()
            .returning(move |_| Ok(libs.clone()));

        let lib_detail = AbsLibrary { id: "lib1".to_string(), name: "Lib 1".to_string(), icon: None };
        mock_client.expect_get_library()
            .returning(move |_, _| Ok(lib_detail.clone()));

        mock_client.expect_get_items()
            .returning(move |_, _| Ok(AbsItemsResponse { results: vec![] }));

        let mock_client_arc: Arc<dyn crate::api::AbsClient + Send + Sync> = Arc::new(mock_client);

        let config = AppConfig {
            port: 3010,
            use_proxy: false,
            abs_url: "http://localhost:3000".to_string(),
            opds_users: "test_user:test_token:pass".to_string(),
            internal_users: vec![user_ref.clone()],
            show_audiobooks: false,
            show_char_cards: false,
            opds_no_auth: false,
            abs_noauth_username: "".to_string(),
            abs_noauth_password: "".to_string(),
            opds_page_size: 20,
        };

        let state = build_app_state_with_mock(config, mock_client_arc).await;
        let app = build_router(state);

        let request_and_check = |app: axum::Router, path: String, expected_ct: String| async move {
            let req = Request::builder()
                .uri(&path)
                .header("Authorization", "Basic dGVzdF91c2VyOnBhc3M=")
                .body(axum::body::Body::empty())
                .unwrap();

            let response = app.oneshot(req).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let ct = response.headers().get(axum::http::header::CONTENT_TYPE).unwrap();
            assert_eq!(ct.to_str().unwrap(), &expected_ct);
        };

        request_and_check(app.clone(), "/opds".to_string(), "application/atom+xml;profile=opds-catalog;kind=navigation".to_string()).await;
        request_and_check(app.clone(), "/opds/libraries/lib1".to_string(), "application/atom+xml;profile=opds-catalog;kind=acquisition".to_string()).await;
        request_and_check(app.clone(), "/opds/libraries/lib1?categories=true".to_string(), "application/atom+xml;profile=opds-catalog;kind=navigation".to_string()).await;
        request_and_check(app.clone(), "/opds/libraries/lib1/search-definition".to_string(), "application/opensearchdescription+xml".to_string()).await;
    }

    #[test]
    fn test_xml_escaping() {
        let mut writer = Writer::new(Cursor::new(Vec::new()));
        OpdsBuilder::write_link(&mut writer, "alternate", "text/html", "Dungeons & Dragons", "http://localhost:3000/opds?q=foo&type=epub")
            .expect("Failed to write link");

        let entry = String::from_utf8(writer.into_inner().into_inner()).unwrap();
        assert!(entry.contains("title=\"Dungeons &amp; Dragons\""));
        assert!(entry.contains("href=\"http://localhost:3000/opds?q=foo&amp;type=epub\""));
    }

    #[test]
    fn test_search_definition_escaping() {
        let xml = OpdsBuilder::build_search_definition("lib-123").unwrap();
        assert!(xml.contains("template=\"/opds/libraries/lib-123?q={searchTerms}&amp;author={atom:author}&amp;title={atom:title}\""));
    }

    #[test]
    fn test_password_colon_parsing() {
        let mut config = crate::models::AppConfig {
            port: 3010,
            use_proxy: false,
            abs_url: "http://localhost:3000".to_string(),
            opds_users: "my_user:my_token:my:pass:with:colons".to_string(),
            internal_users: vec![],
            show_audiobooks: false,
            show_char_cards: false,
            opds_no_auth: false,
            abs_noauth_username: "".to_string(),
            abs_noauth_password: "".to_string(),
            opds_page_size: 20,
        };

        config.parse_users().expect("Failed to parse users");
        assert_eq!(config.internal_users.len(), 1);
        assert_eq!(config.internal_users[0].name, "my_user");
        assert_eq!(config.internal_users[0].api_key, "my_token");
        assert_eq!(config.internal_users[0].password.as_deref(), Some("my:pass:with:colons"));
    }

    #[tokio::test]
    async fn test_api_client_login_cache() {
        use wiremock::{MockServer, Mock, ResponseTemplate};
        use wiremock::matchers::{method, path, body_json};

        let mock_server = MockServer::start().await;

        // Mock success only for the correct password
        Mock::given(method("POST"))
            .and(path("/login"))
            .and(body_json(serde_json::json!({
                "username": "test_user",
                "password": "password123"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "user": {
                    "username": "test_user",
                    "accessToken": "test_token"
                }
            })))
            .mount(&mock_server)
            .await;

        // Mock failure for the wrong password
        Mock::given(method("POST"))
            .and(path("/login"))
            .and(body_json(serde_json::json!({
                "username": "test_user",
                "password": "wrong_password"
            })))
            .respond_with(ResponseTemplate::new(401))
            .mount(&mock_server)
            .await;

        let client = crate::api::ApiClient::new(mock_server.uri(), reqwest::Client::new());
        use crate::api::AbsClient;

        // 1. Success login
        let user = client.login("test_user", "password123").await.unwrap();
        assert_eq!(user.api_key, "test_token");

        // 2. Success login again (cached)
        let user_cached = client.login("test_user", "password123").await.unwrap();
        assert_eq!(user_cached.api_key, "test_token");

        // 3. Login with wrong password (should fail because it hits backend and gets 401, instead of using cached token!)
        let err = client.login("test_user", "wrong_password").await;
        assert!(err.is_err());
    }

    #[test]
    fn test_contains_case_insensitive() {
        use crate::service::contains_case_insensitive;
        assert!(contains_case_insensitive("Hello World", "hello"));
        assert!(contains_case_insensitive("Hello World", "world"));
        assert!(contains_case_insensitive("Hello World", ""));
        assert!(!contains_case_insensitive("Hello World", "hi"));
        // Unicode case folding test
        assert!(contains_case_insensitive("Äpfel", "äpfel"));
    }

    #[test]
    fn test_get_token_from_query() {
        use crate::auth::get_token_from_query;
        assert_eq!(get_token_from_query("token=my_secret"), Some("my_secret"));
        assert_eq!(get_token_from_query("foo=bar&token=secret2&baz=qux"), Some("secret2"));
        assert_eq!(get_token_from_query("foo=bar"), None);
    }

    #[test]
    fn test_opds2_serialization_root() {
        use crate::models::Library;
        use crate::opds2::Opds2Builder;

        let libs = vec![
            Library { id: "lib1".to_string(), name: "First Lib".to_string(), icon: None },
            Library { id: "lib2".to_string(), name: "Second Lib".to_string(), icon: None },
        ];

        let json_str = Opds2Builder::build_root(&libs, "2026-06-02T12:00:00Z");
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert_eq!(parsed.get("metadata").unwrap().get("title").unwrap().as_str().unwrap(), "Libraries");
        let navigation = parsed.get("navigation").unwrap().as_array().unwrap();
        assert_eq!(navigation.len(), 2);
        assert_eq!(navigation[0].get("title").unwrap().as_str().unwrap(), "First Lib");
        assert_eq!(navigation[0].get("href").unwrap().as_str().unwrap(), "/opds/libraries/lib1?categories=true");
    }

    #[test]
    fn test_opds2_serialization_categories() {
        use crate::opds2::Opds2Builder;
        use crate::i18n::I18n;

        let i18n = I18n::new();
        let json_str = Opds2Builder::build_categories_root("lib1", &i18n, None, "2026-06-02T12:00:00Z");
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert_eq!(parsed.get("metadata").unwrap().get("title").unwrap().as_str().unwrap(), "Categories");
        let navigation = parsed.get("navigation").unwrap().as_array().unwrap();
        assert_eq!(navigation.len(), 5);
        assert_eq!(navigation[0].get("title").unwrap().as_str().unwrap(), "All books");
        assert_eq!(navigation[0].get("href").unwrap().as_str().unwrap(), "/opds/libraries/lib1");
        assert_eq!(navigation[1].get("title").unwrap().as_str().unwrap(), "Authors");
    }

    #[test]
    fn test_opds2_serialization_publications() {
        use crate::models::{LibraryItem, Author, InternalUser};
        use crate::opds2::Opds2Builder;

        let item = LibraryItem {
            id: "item1".to_string(),
            title: Some("Book Title".to_string()),
            subtitle: Some("Subtitle Details".to_string()),
            description: Some("This is a book description".to_string()),
            genres: vec!["Fantasy".to_string()],
            tags: vec!["SciFi".to_string()],
            publisher: Some("Super Publisher".to_string()),
            isbn: Some("123456789".to_string()),
            language: Some("en".to_string()),
            published_year: Some("2025".to_string()),
            authors: vec![Author { name: "Author Name".to_string() }],
            narrators: vec![Author { name: "Narrator Name".to_string() }],
            series: vec!["Super Series".to_string()],
            format: Some("epub".to_string()),
        };

        let user = InternalUser {
            name: "testuser".to_string(),
            api_key: "my_key".to_string(),
            password: None,
        };

        let json_str = Opds2Builder::build_publications(
            "lib_id",
            "My Library",
            &[item],
            &user,
            "http://localhost:3000",
            "2026-06-02T12:00:00Z",
            Some((0, 10, 1, 1)),
            "/opds/libraries/lib_id",
        );

        let parsed: serde_json::Value = serde_json::from_str(&json_str).expect("Failed to parse JSON");
        
        let metadata = parsed.get("metadata").unwrap();
        assert_eq!(metadata.get("title").unwrap().as_str().unwrap(), "My Library");
        assert_eq!(metadata.get("numberOfItems").unwrap().as_u64().unwrap(), 1);
        assert_eq!(metadata.get("itemsPerPage").unwrap().as_u64().unwrap(), 10);
        assert_eq!(metadata.get("currentPage").unwrap().as_u64().unwrap(), 1);

        let links = parsed.get("links").unwrap().as_array().unwrap();
        let search_link = links.iter().find(|l| l.get("rel").and_then(|r| r.as_str()) == Some("search")).unwrap();
        assert_eq!(search_link.get("href").unwrap().as_str().unwrap(), "/opds/libraries/lib_id?q={query}");
        assert_eq!(search_link.get("templated").unwrap().as_bool().unwrap(), true);

        let publications = parsed.get("publications").unwrap().as_array().unwrap();
        assert_eq!(publications.len(), 1);
        let pub1 = &publications[0];
        let p_meta = pub1.get("metadata").unwrap();
        assert_eq!(p_meta.get("title").unwrap().as_str().unwrap(), "Book Title");
        assert_eq!(p_meta.get("subtitle").unwrap().as_str().unwrap(), "Subtitle Details");
        assert_eq!(p_meta.get("@type").unwrap().as_str().unwrap(), "http://schema.org/Book");
        assert_eq!(p_meta.get("identifier").unwrap().as_str().unwrap(), "urn:uuid:item1");
        assert_eq!(p_meta.get("publisher").unwrap().as_str().unwrap(), "Super Publisher");
        assert_eq!(p_meta.get("published").unwrap().as_str().unwrap(), "2025");
        
        let author = p_meta.get("author").unwrap().as_array().unwrap();
        assert_eq!(author.len(), 1);
        assert_eq!(author[0].get("name").unwrap().as_str().unwrap(), "Author Name");

        let narrator = p_meta.get("narrator").unwrap().as_array().unwrap();
        assert_eq!(narrator.len(), 1);
        assert_eq!(narrator[0].get("name").unwrap().as_str().unwrap(), "Narrator Name");

        let belongs_to = p_meta.get("belongsTo").unwrap();
        let series = belongs_to.get("series").unwrap();
        assert_eq!(series.get("name").unwrap().as_str().unwrap(), "Super Series");

        let categories = p_meta.get("category").unwrap().as_array().unwrap();
        assert!(categories.iter().any(|c| c.as_str() == Some("Fantasy")));
        assert!(categories.iter().any(|c| c.as_str() == Some("SciFi")));

        let p_links = pub1.get("links").unwrap().as_array().unwrap();
        assert_eq!(p_links.len(), 2);
        assert!(p_links.iter().any(|l| l.get("rel").unwrap().as_str() == Some("download") && l.get("type").unwrap().as_str() == Some("application/epub+zip")));

        let p_images = pub1.get("images").unwrap().as_array().unwrap();
        assert_eq!(p_images.len(), 2);
        assert!(p_images.iter().any(|img| img.get("type").unwrap().as_str() == Some("image/webp")));
    }

    #[tokio::test]
    async fn test_routes_content_type_headers_opds2() {
        use tower::ServiceExt;
        use axum::http::{Request, StatusCode};
        use crate::build_app_state_with_mock;
        use crate::build_router;

        let mut mock_client = MockAbsClient::new();

        mock_client.expect_login()
            .returning(move |_, _| Ok(InternalUser {
                name: "test_user".to_string(),
                api_key: "test_token".to_string(),
                password: Some("pass".to_string()),
            }));

        let user_ref = InternalUser {
            name: "test_user".to_string(),
            api_key: "test_token".to_string(),
            password: None,
        };

        let libs = vec![
            AbsLibrary { id: "lib1".to_string(), name: "Lib 1".to_string(), icon: None },
            AbsLibrary { id: "lib2".to_string(), name: "Lib 2".to_string(), icon: None },
        ];

        mock_client.expect_get_libraries()
            .returning(move |_| Ok(libs.clone()));

        let lib_detail = AbsLibrary { id: "lib1".to_string(), name: "Lib 1".to_string(), icon: None };
        mock_client.expect_get_library()
            .returning(move |_, _| Ok(lib_detail.clone()));

        mock_client.expect_get_items()
            .returning(move |_, _| Ok(AbsItemsResponse { results: vec![] }));

        let mock_client_arc: Arc<dyn crate::api::AbsClient + Send + Sync> = Arc::new(mock_client);

        let config = AppConfig {
            port: 3010,
            use_proxy: false,
            abs_url: "http://localhost:3000".to_string(),
            opds_users: "test_user:test_token:pass".to_string(),
            internal_users: vec![user_ref.clone()],
            show_audiobooks: false,
            show_char_cards: false,
            opds_no_auth: false,
            abs_noauth_username: "".to_string(),
            abs_noauth_password: "".to_string(),
            opds_page_size: 20,
        };

        let state = build_app_state_with_mock(config, mock_client_arc).await;
        let app = build_router(state);

        let request_and_check = |app: axum::Router, path: String, accept_header: Option<String>, expected_ct: String| async move {
            let mut req_builder = Request::builder()
                .uri(&path)
                .header("Authorization", "Basic dGVzdF91c2VyOnBhc3M=");
            if let Some(accept) = accept_header {
                req_builder = req_builder.header("Accept", accept);
            }
            let req = req_builder.body(axum::body::Body::empty()).unwrap();

            let response = app.oneshot(req).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let ct = response.headers().get(axum::http::header::CONTENT_TYPE).unwrap();
            assert_eq!(ct.to_str().unwrap(), &expected_ct);

            if expected_ct.contains("application/opds+json") {
                let body_bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
                let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
                let v: serde_json::Value = serde_json::from_str(&body_str).unwrap();
                assert!(v.get("metadata").is_some());
                assert!(v.get("links").is_some());
            }
        };

        request_and_check(app.clone(), "/opds".to_string(), Some("application/opds+json".to_string()), "application/opds+json".to_string()).await;
        request_and_check(app.clone(), "/opds/libraries/lib1".to_string(), Some("application/opds+json".to_string()), "application/opds+json".to_string()).await;
        request_and_check(app.clone(), "/opds/libraries/lib1?categories=true".to_string(), Some("application/opds+json".to_string()), "application/opds+json".to_string()).await;
        
        request_and_check(app.clone(), "/opds".to_string(), None, "application/atom+xml;profile=opds-catalog;kind=navigation".to_string()).await;
        request_and_check(app.clone(), "/opds/libraries/lib1".to_string(), None, "application/atom+xml;profile=opds-catalog;kind=acquisition".to_string()).await;
    }
}
