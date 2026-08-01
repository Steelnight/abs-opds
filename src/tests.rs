#[cfg(test)]
mod suite {
    use crate::models::{
        AbsItemsResponse, AbsLibrary, AppConfig, Author, InternalUser, Library, LibraryItem,
    };
    use crate::xml::OpdsBuilder;
    use async_trait::async_trait;
    use mockall::mock;
    use quick_xml::Writer;
    use std::io::Cursor;
    use std::sync::Arc;

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
            "2026-06-02T12:00:00Z",
        )
        .expect("Failed to build XML");

        assert!(xml.contains("<id>test_id</id>"));
        assert!(xml.contains("<title>Test Title</title>"));
        assert!(xml.contains("<feed xmlns=\"http://www.w3.org/2005/Atom\""));
        assert!(xml.contains("<author><name>ABS-OPDS</name></author>"));
        assert!(xml.contains("<link rel=\"self\" type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" href=\"/opds\"/>"));
    }

    #[test]
    fn test_build_opds_skeleton_uses_supplied_updated_time() {
        // Regression test: build_opds_skeleton previously called Utc::now()
        // internally instead of using the updated_time callers already
        // computed, so every response body -- and every ETag derived from
        // it -- was unique even for back-to-back identical requests.
        let xml = OpdsBuilder::build_opds_skeleton(
            "test_id",
            "Test Title",
            |_| Ok(()),
            None,
            None,
            None,
            "/opds",
            false,
            "2020-01-01T00:00:00Z",
        )
        .expect("Failed to build XML");

        assert!(xml.contains("<updated>2020-01-01T00:00:00Z</updated>"));

        let xml_again = OpdsBuilder::build_opds_skeleton(
            "test_id",
            "Test Title",
            |_| Ok(()),
            None,
            None,
            None,
            "/opds",
            false,
            "2020-01-01T00:00:00Z",
        )
        .expect("Failed to build XML");

        assert_eq!(
            xml, xml_again,
            "identical inputs must produce identical output for ETags to work"
        );
    }

    #[test]
    fn test_build_library_entry() {
        let lib = Library {
            id: "lib1".to_string(),
            name: "My Library".to_string(),
            icon: None,
        };

        let mut writer = Writer::new(Cursor::new(Vec::new()));
        OpdsBuilder::build_library_entry(&mut writer, &lib, "2026-06-02T12:00:00Z")
            .expect("Failed to build entry");

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
            authors: vec![Author {
                name: "Author Name".to_string(),
            }],
            narrators: vec![Author {
                name: "Narrator Name".to_string(),
            }],
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
        OpdsBuilder::build_item_entry(
            &mut writer,
            &item,
            &user,
            "http://localhost:3000",
            "2026-06-02T12:00:00Z",
            &mut url_buf,
        )
        .expect("Failed to build entry");

        let entry = String::from_utf8(writer.into_inner().into_inner()).unwrap();
        assert!(entry.contains("<id>urn:uuid:item1</id>"));
        assert!(entry.contains("<title>Book Title</title>"));
        assert!(entry.contains("<name>Author Name</name>"));
        assert!(entry.contains("application/epub+zip"));
        assert!(entry.contains("token=token"));
        assert!(entry.contains("<dcterms:publisher>Publisher</dcterms:publisher>"));
        assert!(
            entry.contains("<dcterms:identifier>urn:isbn:978-3-16-148410-0</dcterms:identifier>")
        );
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
        OpdsBuilder::build_item_entry(
            &mut writer,
            &item,
            &user,
            "http://localhost:3000",
            "2026-06-02T12:00:00Z",
            &mut url_buf,
        )
        .expect("Failed to build entry");

        let entry = String::from_utf8(writer.into_inner().into_inner()).unwrap();
        assert!(entry.contains(
            "<content type=\"text\">Escaping &lt;test&gt; &amp; &quot;quotes&quot;</content>"
        ));
    }

    #[tokio::test]
    async fn test_routes_content_type_headers() {
        use crate::build_app_state_with_mock;
        use crate::build_router;
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt;

        let mut mock_client = MockAbsClient::new();

        let user_ref = InternalUser {
            name: "test_user".to_string(),
            api_key: "test_token".to_string(),
            password: None,
        };

        mock_client.expect_login().returning(move |_, _| {
            Ok(InternalUser {
                name: "test_user".to_string(),
                api_key: "test_token".to_string(),
                password: Some("pass".to_string()),
            })
        });

        let libs = vec![
            AbsLibrary {
                id: "lib1".to_string(),
                name: "Lib 1".to_string(),
                icon: None,
            },
            AbsLibrary {
                id: "lib2".to_string(),
                name: "Lib 2".to_string(),
                icon: None,
            },
        ];

        mock_client
            .expect_get_libraries()
            .returning(move |_| Ok(libs.clone()));

        let lib_detail = AbsLibrary {
            id: "lib1".to_string(),
            name: "Lib 1".to_string(),
            icon: None,
        };
        mock_client
            .expect_get_library()
            .returning(move |_, _| Ok(lib_detail.clone()));

        mock_client
            .expect_get_items()
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
            let ct = response
                .headers()
                .get(axum::http::header::CONTENT_TYPE)
                .unwrap();
            assert_eq!(ct.to_str().unwrap(), &expected_ct);
        };

        request_and_check(
            app.clone(),
            "/opds".to_string(),
            "application/atom+xml;profile=opds-catalog;kind=navigation".to_string(),
        )
        .await;
        request_and_check(
            app.clone(),
            "/opds/libraries/lib1".to_string(),
            "application/atom+xml;profile=opds-catalog;kind=acquisition".to_string(),
        )
        .await;
        request_and_check(
            app.clone(),
            "/opds/libraries/lib1?categories=true".to_string(),
            "application/atom+xml;profile=opds-catalog;kind=navigation".to_string(),
        )
        .await;
        request_and_check(
            app.clone(),
            "/opds/libraries/lib1/search-definition".to_string(),
            "application/opensearchdescription+xml".to_string(),
        )
        .await;
    }

    #[tokio::test]
    async fn test_conditional_get_returns_304_on_repeat_request() {
        use crate::build_app_state_with_mock;
        use crate::build_router;
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt;

        let mut mock_client = MockAbsClient::new();

        let user_ref = InternalUser {
            name: "test_user".to_string(),
            api_key: "test_token".to_string(),
            password: None,
        };

        mock_client.expect_login().returning(move |_, _| {
            Ok(InternalUser {
                name: "test_user".to_string(),
                api_key: "test_token".to_string(),
                password: Some("pass".to_string()),
            })
        });

        // A single library routes /opds straight to the Categories skeleton,
        // exercising the fixed build_opds_skeleton path directly.
        let libs = vec![AbsLibrary {
            id: "lib1".to_string(),
            name: "Lib 1".to_string(),
            icon: None,
        }];
        mock_client
            .expect_get_libraries()
            .returning(move |_| Ok(libs.clone()));

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

        let get_opds = |app: axum::Router| async move {
            let req = Request::builder()
                .uri("/opds")
                .header("Authorization", "Basic dGVzdF91c2VyOnBhc3M=")
                .body(axum::body::Body::empty())
                .unwrap();
            app.oneshot(req).await.unwrap()
        };

        let first = get_opds(app.clone()).await;
        assert_eq!(first.status(), StatusCode::OK);
        let etag = first
            .headers()
            .get(axum::http::header::ETAG)
            .expect("first response must carry an ETag")
            .to_str()
            .unwrap()
            .to_string();

        let second = get_opds(app.clone()).await;
        assert_eq!(second.status(), StatusCode::OK);
        let etag_again = second
            .headers()
            .get(axum::http::header::ETAG)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(
            etag, etag_again,
            "two back-to-back requests must produce the same ETag"
        );

        let conditional_req = Request::builder()
            .uri("/opds")
            .header("Authorization", "Basic dGVzdF91c2VyOnBhc3M=")
            .header(axum::http::header::IF_NONE_MATCH, etag)
            .body(axum::body::Body::empty())
            .unwrap();
        let conditional_resp = app.clone().oneshot(conditional_req).await.unwrap();
        assert_eq!(conditional_resp.status(), StatusCode::NOT_MODIFIED);
    }

    #[tokio::test]
    async fn test_pagination_self_link_encodes_query_value() {
        // Regression test: the `self`/pagination links handlers.rs builds
        // for /opds/libraries/{id} echo the incoming q/name/author/title
        // query parameters back into a new URL. Unencoded, a value
        // containing '&' opened an unintended second query parameter.
        use crate::build_app_state_with_mock;
        use crate::build_router;
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt;

        let mut mock_client = MockAbsClient::new();

        let lib_detail = AbsLibrary {
            id: "lib1".to_string(),
            name: "Lib 1".to_string(),
            icon: None,
        };
        mock_client
            .expect_get_library()
            .returning(move |_, _| Ok(lib_detail.clone()));
        mock_client
            .expect_get_items()
            .returning(move |_, _| Ok(AbsItemsResponse { results: vec![] }));

        let user_ref = InternalUser {
            name: "test_user".to_string(),
            api_key: "test_token".to_string(),
            password: Some("pass".to_string()),
        };

        let mock_client_arc: Arc<dyn crate::api::AbsClient + Send + Sync> = Arc::new(mock_client);

        let config = AppConfig {
            port: 3010,
            use_proxy: false,
            abs_url: "http://localhost:3000".to_string(),
            opds_users: "test_user:test_token:pass".to_string(),
            internal_users: vec![user_ref],
            show_audiobooks: true,
            show_char_cards: false,
            opds_no_auth: false,
            abs_noauth_username: "".to_string(),
            abs_noauth_password: "".to_string(),
            opds_page_size: 20,
        };

        let state = build_app_state_with_mock(config, mock_client_arc).await;
        let app = build_router(state);

        let req = Request::builder()
            .uri("/opds/libraries/lib1?q=Tom%20%26%20Jerry")
            .header("Authorization", "Basic dGVzdF91c2VyOnBhc3M=")
            .body(axum::body::Body::empty())
            .unwrap();
        let response = app.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let body = String::from_utf8(body_bytes.to_vec()).unwrap();

        assert!(
            body.contains("q=Tom%20%26%20Jerry"),
            "expected the re-encoded query value in the self link, got: {}",
            body
        );
        assert!(
            !body.contains("q=Tom & Jerry") && !body.contains("q=Tom &type"),
            "raw '&' from the query value must not appear unencoded in a generated link"
        );
    }

    #[tokio::test]
    async fn test_handler_panic_returns_500_instead_of_crashing() {
        // AppConfig::validate() now rejects OPDS_PAGE_SIZE=0 at startup, but
        // CatchPanicLayer is defense in depth against *any* handler panic,
        // not just this one. Construct a config that bypasses validate()
        // (as build_app_state_with_mock always does) with page_size=0, the
        // exact value that used to take the whole connection down via a
        // divide-by-zero in total_items.div_ceil(page_size).
        use crate::build_app_state_with_mock;
        use crate::build_router;
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt;

        let mut mock_client = MockAbsClient::new();

        let lib_detail = AbsLibrary {
            id: "lib1".to_string(),
            name: "Lib 1".to_string(),
            icon: None,
        };
        mock_client
            .expect_get_library()
            .returning(move |_, _| Ok(lib_detail.clone()));

        let item = AbsItemsResponse {
            results: vec![crate::models::AbsItemResult {
                id: "item1".to_string(),
                media: crate::models::AbsMedia {
                    ebook_format: Some("epub".to_string()),
                    metadata: crate::models::AbsMetadata {
                        title: Some("Title".to_string()),
                        subtitle: None,
                        description: None,
                        genres: None,
                        tags: None,
                        publisher: None,
                        isbn: None,
                        language: None,
                        published_year: None,
                        author_name: None,
                        narrator_name: None,
                        series_name: None,
                    },
                },
            }],
        };
        mock_client
            .expect_get_items()
            .returning(move |_, _| Ok(item.clone()));

        let user_ref = InternalUser {
            name: "test_user".to_string(),
            api_key: "test_token".to_string(),
            password: Some("pass".to_string()),
        };

        let mock_client_arc: Arc<dyn crate::api::AbsClient + Send + Sync> = Arc::new(mock_client);

        let config = AppConfig {
            port: 3010,
            use_proxy: false,
            abs_url: "http://localhost:3000".to_string(),
            opds_users: "test_user:test_token:pass".to_string(),
            internal_users: vec![user_ref],
            show_audiobooks: true,
            show_char_cards: false,
            opds_no_auth: false,
            abs_noauth_username: "".to_string(),
            abs_noauth_password: "".to_string(),
            opds_page_size: 0,
        };

        let state = build_app_state_with_mock(config, mock_client_arc).await;
        let app = build_router(state);

        let req = Request::builder()
            .uri("/opds/libraries/lib1")
            .header("Authorization", "Basic dGVzdF91c2VyOnBhc3M=")
            .body(axum::body::Body::empty())
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_xml_escaping() {
        let mut writer = Writer::new(Cursor::new(Vec::new()));
        OpdsBuilder::write_link(
            &mut writer,
            "alternate",
            "text/html",
            "Dungeons & Dragons",
            "http://localhost:3000/opds?q=foo&type=epub",
        )
        .expect("Failed to write link");

        let entry = String::from_utf8(writer.into_inner().into_inner()).unwrap();
        assert!(entry.contains("title=\"Dungeons &amp; Dragons\""));
        assert!(entry.contains("href=\"http://localhost:3000/opds?q=foo&amp;type=epub\""));
    }

    #[test]
    fn test_encode_query_value() {
        use crate::xml::encode_query_value;

        assert_eq!(
            encode_query_value("Simon & Schuster"),
            "Simon%20%26%20Schuster"
        );
        assert_eq!(encode_query_value("Foo #2"), "Foo%20%232");
        assert_eq!(
            encode_query_value("plain-text_ok.here~"),
            "plain-text_ok.here~"
        );
    }

    #[test]
    fn test_build_card_entry_encodes_ampersand_and_hash() {
        // Regression test: a name containing '&' or '#' used to be
        // interpolated into the link's query string unencoded. XML-escaping
        // turned '&' into '&amp;' for valid XML, but decoded back to a
        // literal '&' the link still meant something different than
        // intended -- it opened a second query parameter (or, for '#',
        // started a fragment), silently pointing at the wrong filter.
        let mut writer = Writer::new(Cursor::new(Vec::new()));
        let mut url_buf = String::new();
        OpdsBuilder::build_card_entry(
            &mut writer,
            "Simon & Schuster #2",
            "authors",
            "lib1",
            "2026-06-02T12:00:00Z",
            &mut url_buf,
        )
        .expect("Failed to build entry");

        let entry = String::from_utf8(writer.into_inner().into_inner()).unwrap();
        assert!(entry.contains(
            "href=\"/opds/libraries/lib1?name=Simon%20%26%20Schuster%20%232&amp;type=authors\""
        ));
        assert!(!entry.contains("name=Simon &"));
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
        assert_eq!(
            config.internal_users[0].password.as_deref(),
            Some("my:pass:with:colons")
        );
    }

    #[test]
    fn test_page_size_zero_rejected() {
        let config = crate::models::AppConfig {
            port: 3010,
            use_proxy: false,
            abs_url: "http://localhost:3000".to_string(),
            opds_users: "my_user:my_token:pass".to_string(),
            internal_users: vec![InternalUser {
                name: "my_user".to_string(),
                api_key: "my_token".to_string(),
                password: Some("pass".to_string()),
            }],
            show_audiobooks: false,
            show_char_cards: false,
            opds_no_auth: false,
            abs_noauth_username: "".to_string(),
            abs_noauth_password: "".to_string(),
            opds_page_size: 0,
        };

        let err = config.validate().expect_err("page_size=0 must be rejected");
        assert!(err.to_string().contains("OPDS_PAGE_SIZE"));
    }

    #[tokio::test]
    async fn test_api_client_login_cache() {
        use wiremock::matchers::{body_json, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

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
        assert_eq!(
            get_token_from_query("foo=bar&token=secret2&baz=qux"),
            Some("secret2")
        );
        assert_eq!(get_token_from_query("foo=bar"), None);
    }

    #[test]
    fn test_opds2_serialization_root() {
        use crate::models::Library;
        use crate::opds2::Opds2Builder;

        let libs = vec![
            Library {
                id: "lib1".to_string(),
                name: "First Lib".to_string(),
                icon: None,
            },
            Library {
                id: "lib2".to_string(),
                name: "Second Lib".to_string(),
                icon: None,
            },
        ];

        let json_str = Opds2Builder::build_root(&libs, "2026-06-02T12:00:00Z");
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert_eq!(
            parsed
                .get("metadata")
                .unwrap()
                .get("title")
                .unwrap()
                .as_str()
                .unwrap(),
            "Libraries"
        );
        let navigation = parsed.get("navigation").unwrap().as_array().unwrap();
        assert_eq!(navigation.len(), 2);
        assert_eq!(
            navigation[0].get("title").unwrap().as_str().unwrap(),
            "First Lib"
        );
        assert_eq!(
            navigation[0].get("href").unwrap().as_str().unwrap(),
            "/opds/libraries/lib1?categories=true"
        );
    }

    #[test]
    fn test_opds2_serialization_categories() {
        use crate::i18n::I18n;
        use crate::opds2::Opds2Builder;

        let i18n = I18n::new();
        let json_str =
            Opds2Builder::build_categories_root("lib1", &i18n, None, "2026-06-02T12:00:00Z");
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert_eq!(
            parsed
                .get("metadata")
                .unwrap()
                .get("title")
                .unwrap()
                .as_str()
                .unwrap(),
            "Categories"
        );
        let navigation = parsed.get("navigation").unwrap().as_array().unwrap();
        assert_eq!(navigation.len(), 5);
        assert_eq!(
            navigation[0].get("title").unwrap().as_str().unwrap(),
            "All books"
        );
        assert_eq!(
            navigation[0].get("href").unwrap().as_str().unwrap(),
            "/opds/libraries/lib1"
        );
        assert_eq!(
            navigation[1].get("title").unwrap().as_str().unwrap(),
            "Authors"
        );
    }

    #[test]
    fn test_opds2_serialization_publications() {
        use crate::models::{Author, InternalUser, LibraryItem};
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
            authors: vec![Author {
                name: "Author Name".to_string(),
            }],
            narrators: vec![Author {
                name: "Narrator Name".to_string(),
            }],
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

        let parsed: serde_json::Value =
            serde_json::from_str(&json_str).expect("Failed to parse JSON");

        let metadata = parsed.get("metadata").unwrap();
        assert_eq!(
            metadata.get("title").unwrap().as_str().unwrap(),
            "My Library"
        );
        assert_eq!(metadata.get("numberOfItems").unwrap().as_u64().unwrap(), 1);
        assert_eq!(metadata.get("itemsPerPage").unwrap().as_u64().unwrap(), 10);
        assert_eq!(metadata.get("currentPage").unwrap().as_u64().unwrap(), 1);

        let links = parsed.get("links").unwrap().as_array().unwrap();
        let search_link = links
            .iter()
            .find(|l| l.get("rel").and_then(|r| r.as_str()) == Some("search"))
            .unwrap();
        assert_eq!(
            search_link.get("href").unwrap().as_str().unwrap(),
            "/opds/libraries/lib_id?q={query}"
        );
        assert!(search_link.get("templated").unwrap().as_bool().unwrap());

        let publications = parsed.get("publications").unwrap().as_array().unwrap();
        assert_eq!(publications.len(), 1);
        let pub1 = &publications[0];
        let p_meta = pub1.get("metadata").unwrap();
        assert_eq!(p_meta.get("title").unwrap().as_str().unwrap(), "Book Title");
        assert_eq!(
            p_meta.get("subtitle").unwrap().as_str().unwrap(),
            "Subtitle Details"
        );
        assert_eq!(
            p_meta.get("@type").unwrap().as_str().unwrap(),
            "http://schema.org/Book"
        );
        assert_eq!(
            p_meta.get("identifier").unwrap().as_str().unwrap(),
            "urn:uuid:item1"
        );
        assert_eq!(
            p_meta.get("publisher").unwrap().as_str().unwrap(),
            "Super Publisher"
        );
        assert_eq!(p_meta.get("published").unwrap().as_str().unwrap(), "2025");

        let author = p_meta.get("author").unwrap().as_array().unwrap();
        assert_eq!(author.len(), 1);
        assert_eq!(
            author[0].get("name").unwrap().as_str().unwrap(),
            "Author Name"
        );

        let narrator = p_meta.get("narrator").unwrap().as_array().unwrap();
        assert_eq!(narrator.len(), 1);
        assert_eq!(
            narrator[0].get("name").unwrap().as_str().unwrap(),
            "Narrator Name"
        );

        let belongs_to = p_meta.get("belongsTo").unwrap();
        let series = belongs_to.get("series").unwrap();
        assert_eq!(
            series.get("name").unwrap().as_str().unwrap(),
            "Super Series"
        );

        let categories = p_meta.get("category").unwrap().as_array().unwrap();
        assert!(categories.iter().any(|c| c.as_str() == Some("Fantasy")));
        assert!(categories.iter().any(|c| c.as_str() == Some("SciFi")));

        let p_links = pub1.get("links").unwrap().as_array().unwrap();
        assert_eq!(p_links.len(), 2);
        assert!(p_links
            .iter()
            .any(|l| l.get("rel").unwrap().as_str() == Some("download")
                && l.get("type").unwrap().as_str() == Some("application/epub+zip")));

        let p_images = pub1.get("images").unwrap().as_array().unwrap();
        assert_eq!(p_images.len(), 2);
        assert!(p_images
            .iter()
            .any(|img| img.get("type").unwrap().as_str() == Some("image/webp")));
    }

    #[tokio::test]
    async fn test_routes_content_type_headers_opds2() {
        use crate::build_app_state_with_mock;
        use crate::build_router;
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt;

        let mut mock_client = MockAbsClient::new();

        mock_client.expect_login().returning(move |_, _| {
            Ok(InternalUser {
                name: "test_user".to_string(),
                api_key: "test_token".to_string(),
                password: Some("pass".to_string()),
            })
        });

        let user_ref = InternalUser {
            name: "test_user".to_string(),
            api_key: "test_token".to_string(),
            password: None,
        };

        let libs = vec![
            AbsLibrary {
                id: "lib1".to_string(),
                name: "Lib 1".to_string(),
                icon: None,
            },
            AbsLibrary {
                id: "lib2".to_string(),
                name: "Lib 2".to_string(),
                icon: None,
            },
        ];

        mock_client
            .expect_get_libraries()
            .returning(move |_| Ok(libs.clone()));

        let lib_detail = AbsLibrary {
            id: "lib1".to_string(),
            name: "Lib 1".to_string(),
            icon: None,
        };
        mock_client
            .expect_get_library()
            .returning(move |_, _| Ok(lib_detail.clone()));

        mock_client
            .expect_get_items()
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

        let request_and_check = |app: axum::Router,
                                 path: String,
                                 accept_header: Option<String>,
                                 expected_ct: String| async move {
            let mut req_builder = Request::builder()
                .uri(&path)
                .header("Authorization", "Basic dGVzdF91c2VyOnBhc3M=");
            if let Some(accept) = accept_header {
                req_builder = req_builder.header("Accept", accept);
            }
            let req = req_builder.body(axum::body::Body::empty()).unwrap();

            let response = app.oneshot(req).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let ct = response
                .headers()
                .get(axum::http::header::CONTENT_TYPE)
                .unwrap();
            assert_eq!(ct.to_str().unwrap(), &expected_ct);

            if expected_ct.contains("application/opds+json") {
                let body_bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
                    .await
                    .unwrap();
                let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
                let v: serde_json::Value = serde_json::from_str(&body_str).unwrap();
                assert!(v.get("metadata").is_some());
                assert!(v.get("links").is_some());
            }
        };

        request_and_check(
            app.clone(),
            "/opds".to_string(),
            Some("application/opds+json".to_string()),
            "application/opds+json".to_string(),
        )
        .await;
        request_and_check(
            app.clone(),
            "/opds/libraries/lib1".to_string(),
            Some("application/opds+json".to_string()),
            "application/opds+json".to_string(),
        )
        .await;
        request_and_check(
            app.clone(),
            "/opds/libraries/lib1?categories=true".to_string(),
            Some("application/opds+json".to_string()),
            "application/opds+json".to_string(),
        )
        .await;

        request_and_check(
            app.clone(),
            "/opds".to_string(),
            None,
            "application/atom+xml;profile=opds-catalog;kind=navigation".to_string(),
        )
        .await;
        request_and_check(
            app.clone(),
            "/opds/libraries/lib1".to_string(),
            None,
            "application/atom+xml;profile=opds-catalog;kind=acquisition".to_string(),
        )
        .await;
    }
}
