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
}
