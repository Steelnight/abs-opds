use serde::Serialize;
use crate::models::{Library, LibraryItem, InternalUser};
use crate::i18n::I18n;

#[derive(Serialize)]
pub struct Feed {
    pub metadata: FeedMetadata,
    pub links: Vec<Link>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub navigation: Option<Vec<Link>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publications: Option<Vec<Publication>>,
}

#[derive(Serialize)]
pub struct FeedMetadata {
    pub title: String,
    #[serde(rename = "numberOfItems", skip_serializing_if = "Option::is_none")]
    pub number_of_items: Option<usize>,
    #[serde(rename = "itemsPerPage", skip_serializing_if = "Option::is_none")]
    pub items_per_page: Option<usize>,
    #[serde(rename = "currentPage", skip_serializing_if = "Option::is_none")]
    pub current_page: Option<usize>,
}

#[derive(Serialize, Clone)]
pub struct Link {
    pub href: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rel: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub templated: Option<bool>,
}

#[derive(Serialize)]
pub struct Publication {
    pub metadata: PublicationMetadata,
    pub links: Vec<Link>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<Link>>,
}

#[derive(Serialize)]
pub struct Contributor {
    pub name: String,
}

#[derive(Serialize)]
pub struct PublicationMetadata {
    #[serde(rename = "@type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<String>,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<Vec<Contributor>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub narrator: Option<Vec<Contributor>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published: Option<String>,
    #[serde(rename = "belongsTo", skip_serializing_if = "Option::is_none")]
    pub belongs_to: Option<BelongsTo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<Vec<String>>,
}

#[derive(Serialize)]
pub struct BelongsTo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series: Option<SeriesMetadata>,
}

#[derive(Serialize)]
pub struct SeriesMetadata {
    pub name: String,
}

pub struct Opds2Builder;

impl Opds2Builder {
    pub fn build_root(libraries: &[Library], _updated_time: &str) -> String {
        let links = vec![Link {
            href: "/opds".to_string(),
            rel: Some("self".to_string()),
            type_: Some("application/opds+json".to_string()),
            title: None,
            templated: None,
        }];

        let navigation = libraries
            .iter()
            .map(|lib| Link {
                href: format!("/opds/libraries/{}?categories=true", lib.id),
                rel: None,
                type_: Some("application/opds+json".to_string()),
                title: Some(lib.name.clone()),
                templated: None,
            })
            .collect();

        let feed = Feed {
            metadata: FeedMetadata {
                title: "Libraries".to_string(),
                number_of_items: None,
                items_per_page: None,
                current_page: None,
            },
            links,
            navigation: Some(navigation),
            publications: None,
        };

        serde_json::to_string(&feed).unwrap_or_default()
    }

    pub fn build_categories_root(
        library_id: &str,
        i18n: &I18n,
        lang: Option<&str>,
        _updated_time: &str,
    ) -> String {
        let links = vec![Link {
            href: format!("/opds/libraries/{}", library_id),
            rel: Some("self".to_string()),
            type_: Some("application/opds+json".to_string()),
            title: None,
            templated: None,
        }];

        let categories = vec![
            (library_id.to_string(), i18n.localize("category.all", lang)),
            ("authors".to_string(), i18n.localize("category.authors", lang)),
            ("narrators".to_string(), i18n.localize("category.narrators", lang)),
            ("genres".to_string(), i18n.localize("category.genres", lang)),
            ("series".to_string(), i18n.localize("category.series", lang)),
        ];

        let navigation = categories
            .into_iter()
            .map(|(id, title)| {
                let href = if id == library_id {
                    format!("/opds/libraries/{}", library_id)
                } else {
                    format!("/opds/libraries/{}/{}", library_id, id)
                };

                Link {
                    href,
                    rel: None,
                    type_: Some("application/opds+json".to_string()),
                    title: Some(title),
                    templated: None,
                }
            })
            .collect();

        let feed = Feed {
            metadata: FeedMetadata {
                title: "Categories".to_string(),
                number_of_items: None,
                items_per_page: None,
                current_page: None,
            },
            links,
            navigation: Some(navigation),
            publications: None,
        };

        serde_json::to_string(&feed).unwrap_or_default()
    }

    pub fn build_category_letters(
        library_id: &str,
        library_name: &str,
        type_: &str,
        letters: &[(String, usize)],
    ) -> String {
        let links = vec![Link {
            href: format!("/opds/libraries/{}/{}", library_id, type_),
            rel: Some("self".to_string()),
            type_: Some("application/opds+json".to_string()),
            title: None,
            templated: None,
        }];

        let navigation = letters
            .iter()
            .map(|(letter, count)| Link {
                href: format!(
                    "/opds/libraries/{}/{}?start={}",
                    library_id,
                    type_,
                    letter.to_lowercase()
                ),
                rel: None,
                type_: Some("application/opds+json".to_string()),
                title: Some(format!("{} ({})", letter, count)),
                templated: None,
            })
            .collect();

        let feed = Feed {
            metadata: FeedMetadata {
                title: library_name.to_string(),
                number_of_items: None,
                items_per_page: None,
                current_page: None,
            },
            links,
            navigation: Some(navigation),
            publications: None,
        };

        serde_json::to_string(&feed).unwrap_or_default()
    }

    pub fn build_category_items(
        library_id: &str,
        library_name: &str,
        type_: &str,
        items: &[String],
        page_info: Option<(usize, usize, usize, usize)>,
        url_base: &str,
    ) -> String {
        let mut links = vec![Link {
            href: url_base.to_string(),
            rel: Some("self".to_string()),
            type_: Some("application/opds+json".to_string()),
            title: None,
            templated: None,
        }];

        let mut current_page = None;
        let mut items_per_page = None;
        let mut number_of_items = None;

        if let Some((page, page_size, total_items, total_pages)) = page_info {
            current_page = Some(page + 1);
            items_per_page = Some(page_size);
            number_of_items = Some(total_items);

            static PAGE_REGEX: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
            let regex = PAGE_REGEX.get_or_init(|| {
                regex::Regex::new(r"[?&]page=\d+").expect("Failed to compile regex")
            });
            let clean_url = if url_base.contains("?page=") || url_base.contains("&page=") {
                regex.replace(url_base, "").to_string()
            } else {
                url_base.to_string()
            };

            let separator = if clean_url.contains('?') { "&" } else { "?" };

            links.push(Link {
                href: clean_url.clone(),
                rel: Some("first".to_string()),
                type_: Some("application/opds+json".to_string()),
                title: None,
                templated: None,
            });

            if page > 0 {
                let prev_page = page - 1;
                let href = if prev_page > 0 {
                    format!("{}{}{}{}", clean_url, separator, "page=", prev_page)
                } else {
                    clean_url.clone()
                };
                links.push(Link {
                    href,
                    rel: Some("previous".to_string()),
                    type_: Some("application/opds+json".to_string()),
                    title: None,
                    templated: None,
                });
            }

            if page + 1 < total_pages {
                let next_page = page + 1;
                let href = format!("{}{}{}{}", clean_url, separator, "page=", next_page);
                links.push(Link {
                    href,
                    rel: Some("next".to_string()),
                    type_: Some("application/opds+json".to_string()),
                    title: None,
                    templated: None,
                });
            }

            if total_pages > 1 {
                let last_page = total_pages - 1;
                let href = format!("{}{}{}{}", clean_url, separator, "page=", last_page);
                links.push(Link {
                    href,
                    rel: Some("last".to_string()),
                    type_: Some("application/opds+json".to_string()),
                    title: None,
                    templated: None,
                });
            }
        }

        let navigation = items
            .iter()
            .map(|item| {
                let mut url_buf = String::new();
                for c in item.chars() {
                    if c == ' ' {
                        url_buf.push('-');
                    } else {
                        for lc in c.to_lowercase() {
                            url_buf.push(lc);
                        }
                    }
                }

                Link {
                    href: format!(
                        "/opds/libraries/{}?name={}&type={}",
                        library_id, item, type_
                    ),
                    rel: None,
                    type_: Some("application/opds+json".to_string()),
                    title: Some(item.clone()),
                    templated: None,
                }
            })
            .collect();

        let feed = Feed {
            metadata: FeedMetadata {
                title: library_name.to_string(),
                number_of_items,
                items_per_page,
                current_page,
            },
            links,
            navigation: Some(navigation),
            publications: None,
        };

        serde_json::to_string(&feed).unwrap_or_default()
    }

    pub fn build_publications(
        library_id: &str,
        library_name: &str,
        items: &[LibraryItem],
        user: &InternalUser,
        link_url: &str,
        updated_time: &str,
        page_info: Option<(usize, usize, usize, usize)>,
        url_base: &str,
    ) -> String {
        let mut links = vec![Link {
            href: url_base.to_string(),
            rel: Some("self".to_string()),
            type_: Some("application/opds+json".to_string()),
            title: None,
            templated: None,
        }];

        // Add template search link
        links.push(Link {
            href: format!("/opds/libraries/{}?q={{query}}", library_id),
            rel: Some("search".to_string()),
            type_: Some("application/opds+json".to_string()),
            title: Some("Search this library".to_string()),
            templated: Some(true),
        });

        let mut current_page = None;
        let mut items_per_page = None;
        let mut number_of_items = None;

        if let Some((page, page_size, total_items, total_pages)) = page_info {
            current_page = Some(page + 1);
            items_per_page = Some(page_size);
            number_of_items = Some(total_items);

            static PAGE_REGEX: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
            let regex = PAGE_REGEX.get_or_init(|| {
                regex::Regex::new(r"[?&]page=\d+").expect("Failed to compile regex")
            });
            let clean_url = if url_base.contains("?page=") || url_base.contains("&page=") {
                regex.replace(url_base, "").to_string()
            } else {
                url_base.to_string()
            };

            let separator = if clean_url.contains('?') { "&" } else { "?" };

            links.push(Link {
                href: clean_url.clone(),
                rel: Some("first".to_string()),
                type_: Some("application/opds+json".to_string()),
                title: None,
                templated: None,
            });

            if page > 0 {
                let prev_page = page - 1;
                let href = if prev_page > 0 {
                    format!("{}{}{}{}", clean_url, separator, "page=", prev_page)
                } else {
                    clean_url.clone()
                };
                links.push(Link {
                    href,
                    rel: Some("previous".to_string()),
                    type_: Some("application/opds+json".to_string()),
                    title: None,
                    templated: None,
                });
            }

            if page + 1 < total_pages {
                let next_page = page + 1;
                let href = format!("{}{}{}{}", clean_url, separator, "page=", next_page);
                links.push(Link {
                    href,
                    rel: Some("next".to_string()),
                    type_: Some("application/opds+json".to_string()),
                    title: None,
                    templated: None,
                });
            }

            if total_pages > 1 {
                let last_page = total_pages - 1;
                let href = format!("{}{}{}{}", clean_url, separator, "page=", last_page);
                links.push(Link {
                    href,
                    rel: Some("last".to_string()),
                    type_: Some("application/opds+json".to_string()),
                    title: None,
                    templated: None,
                });
            }
        }

        let publications = items
            .iter()
            .map(|item| {
                let format = item.format.as_deref().unwrap_or("");
                let (mime_type, schema_type) = match format {
                    "audiobook" => ("audio/mpeg", "http://schema.org/Audiobook"),
                    "epub" => ("application/epub+zip", "http://schema.org/Book"),
                    "pdf" => ("application/pdf", "http://schema.org/Book"),
                    "mobi" => ("application/x-mobipocket-ebook", "http://schema.org/Book"),
                    _ => ("application/octet-stream", "http://schema.org/Book"),
                };

                let p_links = vec![
                    Link {
                        href: format!(
                            "{}/api/items/{}/download?token={}",
                            link_url, item.id, user.api_key
                        ),
                        rel: Some("download".to_string()),
                        type_: Some("application/octet-stream".to_string()),
                        title: None,
                        templated: None,
                    },
                    Link {
                        href: format!(
                            "{}/api/items/{}/ebook?token={}",
                            link_url, item.id, user.api_key
                        ),
                        rel: Some("download".to_string()),
                        type_: Some(mime_type.to_string()),
                        title: None,
                        templated: None,
                    },
                ];

                let images = vec![
                    Link {
                        href: format!(
                            "{}/api/items/{}/cover?token={}",
                            link_url, item.id, user.api_key
                        ),
                        rel: None,
                        type_: Some("image/webp".to_string()),
                        title: None,
                        templated: None,
                    },
                    Link {
                        href: format!(
                            "{}/api/items/{}/cover?token={}",
                            link_url, item.id, user.api_key
                        ),
                        rel: None,
                        type_: Some("image/png".to_string()),
                        title: None,
                        templated: None,
                    },
                ];

                let authors = if item.authors.is_empty() {
                    None
                } else {
                    Some(
                        item.authors
                            .iter()
                            .map(|a| Contributor { name: a.name.clone() })
                            .collect(),
                    )
                };

                let narrators = if item.narrators.is_empty() {
                    None
                } else {
                    Some(
                        item.narrators
                            .iter()
                            .map(|a| Contributor { name: a.name.clone() })
                            .collect(),
                    )
                };

                let belongs_to = if !item.series.is_empty() {
                    Some(BelongsTo {
                        series: Some(SeriesMetadata {
                            name: item.series[0].clone(),
                        }),
                    })
                } else {
                    None
                };

                let category = if item.genres.is_empty() && item.tags.is_empty() {
                    None
                } else {
                    let mut cats = Vec::new();
                    for tag in item.genres.iter().chain(item.tags.iter()) {
                        cats.push(tag.clone());
                    }
                    Some(cats)
                };

                Publication {
                    metadata: PublicationMetadata {
                        type_: Some(schema_type.to_string()),
                        title: item.title.clone().unwrap_or_default(),
                        subtitle: item.subtitle.clone(),
                        identifier: Some(format!("urn:uuid:{}", item.id)),
                        language: item.language.clone(),
                        modified: Some(updated_time.to_string()),
                        description: item.description.clone(),
                        publisher: item.publisher.clone(),
                        author: authors,
                        narrator: narrators,
                        published: item.published_year.clone(),
                        belongs_to,
                        category,
                    },
                    links: p_links,
                    images: Some(images),
                }
            })
            .collect();

        let feed = Feed {
            metadata: FeedMetadata {
                title: library_name.to_string(),
                number_of_items,
                items_per_page,
                current_page,
            },
            links,
            navigation: None,
            publications: Some(publications),
        };

        serde_json::to_string(&feed).unwrap_or_default()
    }
}
