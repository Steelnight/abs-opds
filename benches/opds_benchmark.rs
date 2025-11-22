use abs_opds::api::AbsClient;
use abs_opds::models::{
    AbsItemResult, AbsItemsResponse, AbsLibrary, AbsMedia, AbsMetadata, AppConfig, InternalUser,
};
use abs_opds::service::LibraryService;
use abs_opds::xml::OpdsBuilder;
use abs_opds::handlers::LibraryQuery;
use abs_opds::i18n::I18n;
use abs_opds::build_app_state_with_mock;
use abs_opds::build_router;
use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use mockall::mock;
use std::sync::Arc;
use tokio::runtime::Runtime;
use std::fs::File;
use std::io::Write;
use std::sync::Mutex;
use tower_http::trace::TraceLayer;
use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use tower::ServiceExt;
use async_trait::async_trait;
use std::time::Duration;
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path};

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

fn generate_data(n_items: usize, n_authors: usize, n_genres: usize) -> Vec<AbsItemResult> {
    let mut items = Vec::with_capacity(n_items);
    for i in 0..n_items {
        items.push(create_item(
            &format!("{}", i),
            &format!("Book Title {}", i),
            Some(&format!("Author {}", i % n_authors)),
            Some(&format!("Genre {}", i % n_genres)),
        ));
    }
    items
}

fn mock_user() -> InternalUser {
    InternalUser {
        name: "bench_user".to_string(),
        api_key: "bench_token".to_string(),
        password: None,
    }
}

fn mock_config() -> AppConfig {
    AppConfig {
        port: 3000,
        use_proxy: false,
        abs_url: "http://localhost:3000".to_string(),
        opds_users: "bench_user:bench_token:pass".to_string(),
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
    let languages_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")).join("languages");
    I18n::new(&languages_dir)
}

// --- Reporting ---
struct MarkdownReporter {
    file: Mutex<File>,
}

impl MarkdownReporter {
    fn new(path: &str) -> Self {
        let mut file = File::create(path).expect("Unable to create report file");
        writeln!(file, "# Performance Benchmark Report\n").unwrap();
        writeln!(file, "| Metric | Items | Authors | Genres | Time (ms) | Throughput (items/s) |").unwrap();
        writeln!(file, "|---|---|---|---|---|---|").unwrap();
        Self {
            file: Mutex::new(file),
        }
    }

    fn add_entry(&self, name: &str, items: usize, authors: usize, genres: usize, time_ns: f64) {
        let time_ms = time_ns / 1_000_000.0;
        let throughput = if time_ms > 0.0 { items as f64 / (time_ms / 1000.0) } else { 0.0 };
        let mut file = self.file.lock().unwrap();
        writeln!(
            file,
            "| {} | {} | {} | {} | {:.2} | {:.2} |",
            name, items, authors, genres, time_ms, throughput
        ).unwrap();
    }
}

lazy_static::lazy_static! {
    static ref REPORTER: MarkdownReporter = MarkdownReporter::new("performance_report.md");
}

// --- Benchmarks ---

fn bench_service_layer(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("Service Layer");

    group.measurement_time(Duration::from_millis(500));
    group.warm_up_time(Duration::from_millis(100));
    group.sample_size(10);

    let start_pow = 2.0;
    let end_pow = 2000000f64.log10();
    let step = (end_pow - start_pow) / 19.0;

    let mut sizes = Vec::new();
    for i in 0..20 {
        let n = 10f64.powf(start_pow + step * i as f64) as usize;
        sizes.push(n);
    }
    if let Some(last) = sizes.last_mut() {
        if *last < 2000000 { *last = 2000000; }
    }

    for size in sizes {
        let n_items = size;
        let n_authors = std::cmp::max(1, n_items / 40);
        let n_genres = std::cmp::max(1, n_items / 4000);

        let items = generate_data(n_items, n_authors, n_genres);
        let items_response = AbsItemsResponse { results: items.clone() };

        let mut mock_client = MockAbsClient::new();
        mock_client
            .expect_get_items()
            .returning(move |_, _| Ok(items_response.clone()));
        mock_client
            .expect_get_library()
            .returning(|_, _| Ok(AbsLibrary { id: "lib1".to_string(), name: "Test Lib".to_string(), icon: None }));

        let service = LibraryService::new(Arc::new(mock_client), mock_config(), mock_i18n());
        let user = mock_user();

        group.throughput(Throughput::Elements(n_items as u64));

        group.bench_with_input(BenchmarkId::new("get_filtered_items", n_items), &n_items, |b, &_| {
            b.to_async(&rt).iter(|| async {
                 service.get_filtered_items(&user, "lib1", &LibraryQuery {
                    q: None, page: 0, categories: None, author: None, title: None, name: None, type_: None, start: None
                 }).await.unwrap()
            })
        });

        let start = std::time::Instant::now();
        rt.block_on(async {
             service.get_filtered_items(&user, "lib1", &LibraryQuery {
                q: None, page: 0, categories: None, author: None, title: None, name: None, type_: None, start: None
             }).await.unwrap();
        });
        let duration = start.elapsed().as_nanos() as f64;
        REPORTER.add_entry("get_filtered_items", n_items, n_authors, n_genres, duration);

        group.bench_with_input(BenchmarkId::new("get_categories_authors", n_items), &n_items, |b, &_| {
            b.to_async(&rt).iter(|| async {
                 service.get_categories(&user, "lib1", "authors", &LibraryQuery {
                    q: None, page: 0, categories: None, author: None, title: None, name: None, type_: None, start: None
                 }).await.unwrap()
            })
        });

        let start = std::time::Instant::now();
        rt.block_on(async {
             service.get_categories(&user, "lib1", "authors", &LibraryQuery {
                q: None, page: 0, categories: None, author: None, title: None, name: None, type_: None, start: None
             }).await.unwrap();
        });
        let duration = start.elapsed().as_nanos() as f64;
        REPORTER.add_entry("get_categories_authors", n_items, n_authors, n_genres, duration);
    }
    group.finish();
}

fn bench_handlers(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("Handlers");

    group.measurement_time(Duration::from_millis(500));
    group.warm_up_time(Duration::from_millis(100));
    group.sample_size(10);

    let sizes = vec![1000, 10000];

    for size in sizes {
        let n_items = size;
        let n_authors = std::cmp::max(1, n_items / 40);
        let n_genres = std::cmp::max(1, n_items / 4000);

        let items = generate_data(n_items, n_authors, n_genres);
        let items_response = AbsItemsResponse { results: items.clone() };

        let mut mock_client = MockAbsClient::new();
        mock_client
            .expect_get_items()
            .returning(move |_, _| Ok(items_response.clone()));
        mock_client
            .expect_get_library()
            .returning(|_, _| Ok(AbsLibrary { id: "lib1".to_string(), name: "Test Lib".to_string(), icon: None }));
        mock_client
            .expect_login()
            .returning(|_, _| Ok(mock_user()));

        let mut config = mock_config();
        config.parse_users().unwrap();

        let state = rt.block_on(build_app_state_with_mock(config, Arc::new(mock_client)));
        let app = build_router(state);

        group.throughput(Throughput::Elements(n_items as u64));

        group.bench_with_input(BenchmarkId::new("handler_get_library", n_items), &n_items, |b, &_| {
            b.to_async(&rt).iter(|| async {
                 let req = Request::builder()
                    .uri("/opds/libraries/lib1")
                    .header("Authorization", "Basic YmVuY2hfdXNlcjpwYXNz")
                    .body(Body::empty())
                    .unwrap();

                 let resp = app.clone().oneshot(req).await.unwrap();
                 assert_eq!(resp.status(), StatusCode::OK);
            })
        });

        let start = std::time::Instant::now();
        rt.block_on(async {
             let req = Request::builder()
                .uri("/opds/libraries/lib1")
                .header("Authorization", "Basic YmVuY2hfdXNlcjpwYXNz")
                .body(Body::empty())
                .unwrap();
             let _ = app.clone().oneshot(req).await.unwrap();
        });
        let duration = start.elapsed().as_nanos() as f64;
        REPORTER.add_entry("handler_get_library", n_items, n_authors, n_genres, duration);
    }
    group.finish();
}

fn bench_proxy_handler(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("Proxy Handler");
    group.measurement_time(Duration::from_millis(500));
    group.warm_up_time(Duration::from_millis(100));
    group.sample_size(10);

    let mock_server = rt.block_on(MockServer::start());
    let mock_uri = mock_server.uri();

    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/some/image.jpg"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0u8; 1024]))
            .mount(&mock_server)
            .await;
    });

    let n_items = 1;
    let mut config = mock_config();
    config.parse_users().unwrap();
    config.use_proxy = true;
    config.abs_url = mock_uri;

    let mut mock_client = MockAbsClient::new();
    mock_client.expect_login().returning(|_, _| Ok(mock_user()));

    let state = rt.block_on(build_app_state_with_mock(config, Arc::new(mock_client)));
    let app = build_router(state);

    group.throughput(Throughput::Elements(1));

    group.bench_with_input(BenchmarkId::new("proxy_request", 1), &n_items, |b, &_| {
        b.to_async(&rt).iter(|| async {
                let req = Request::builder()
                .uri("/opds/proxy/some/image.jpg")
                .body(Body::empty())
                .unwrap();

                let resp = app.clone().oneshot(req).await.unwrap();
                assert_eq!(resp.status(), StatusCode::OK);
        })
    });

    let start = std::time::Instant::now();
    rt.block_on(async {
            let req = Request::builder()
            .uri("/opds/proxy/some/image.jpg")
            .body(Body::empty())
            .unwrap();
            let _ = app.clone().oneshot(req).await.unwrap();
    });
    let duration = start.elapsed().as_nanos() as f64;
    REPORTER.add_entry("proxy_request", 1, 0, 0, duration);

    group.finish();
}

fn bench_xml_layer(c: &mut Criterion) {
    let mut group = c.benchmark_group("XML Layer");

    group.measurement_time(Duration::from_millis(500));
    group.warm_up_time(Duration::from_millis(100));
    group.sample_size(10);

    let sizes = vec![1000, 10000];

    for size in sizes {
        let n_items = size;
        let n_authors = std::cmp::max(1, n_items / 40);
        let n_genres = std::cmp::max(1, n_items / 4000);

        let abs_items = generate_data(n_items, n_authors, n_genres);
        let library_items: Vec<abs_opds::models::LibraryItem> = abs_items.into_iter().map(|i| {
             abs_opds::models::LibraryItem {
                 id: i.id,
                 title: i.media.metadata.title,
                 subtitle: i.media.metadata.subtitle,
                 description: i.media.metadata.description,
                 genres: i.media.metadata.genres.unwrap_or_default(),
                 tags: i.media.metadata.tags.unwrap_or_default(),
                 publisher: i.media.metadata.publisher,
                 isbn: i.media.metadata.isbn,
                 language: i.media.metadata.language,
                 published_year: i.media.metadata.published_year,
                 authors: i.media.metadata.author_name.map(|s| s.split(',').map(|n| abs_opds::models::Author { name: n.trim().to_string() }).collect()).unwrap_or_default(),
                 narrators: i.media.metadata.narrator_name.map(|s| s.split(',').map(|n| abs_opds::models::Author { name: n.trim().to_string() }).collect()).unwrap_or_default(),
                 series: i.media.metadata.series_name.map(|s| s.split(',').map(|n| n.trim().to_string()).collect()).unwrap_or_default(),
                 format: i.media.ebook_format,
             }
        }).collect();

        let user = mock_user();
        let lib = abs_opds::models::Library { id: "lib1".to_string(), name: "Lib".to_string(), icon: None };

        group.throughput(Throughput::Elements(n_items as u64));

        group.bench_with_input(BenchmarkId::new("xml_build_entries", n_items), &n_items, |b, &_| {
            b.iter(|| {
                 OpdsBuilder::build_opds_skeleton(
                        "urn:uuid:lib1",
                        "Lib",
                        |writer| {
                            for item in &library_items {
                                OpdsBuilder::build_item_entry(writer, item, &user, "/opds")?;
                            }
                            Ok(())
                        },
                        Some(&lib),
                        Some(&user),
                        Some((0, 100, n_items, n_items/100)),
                        "/opds"
                    ).unwrap()
            })
        });

        let start = std::time::Instant::now();
         OpdsBuilder::build_opds_skeleton(
                "urn:uuid:lib1",
                "Lib",
                |writer| {
                    for item in &library_items {
                        OpdsBuilder::build_item_entry(writer, item, &user, "/opds")?;
                    }
                    Ok(())
                },
                Some(&lib),
                Some(&user),
                Some((0, 100, n_items, n_items/100)),
                "/opds"
            ).unwrap();
        let duration = start.elapsed().as_nanos() as f64;
        REPORTER.add_entry("xml_build_entries", n_items, n_authors, n_genres, duration);
    }
    group.finish();
}

criterion_group!(benches, bench_service_layer, bench_handlers, bench_proxy_handler, bench_xml_layer);
criterion_main!(benches);
