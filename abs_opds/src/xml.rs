use crate::models::{Library, LibraryItem};
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, Event};
use quick_xml::Writer;
use std::io::Cursor;
use crate::models::InternalUser;

pub struct OpdsBuilder;

pub fn is_combining_mark(c: char) -> bool {
    unicode_normalization::char::is_combining_mark(c)
}

impl OpdsBuilder {
    pub fn build_opds_skeleton(
        id: &str,
        title: &str,
        entries_xml: Vec<String>,
        library: Option<&Library>,
        _user: Option<&InternalUser>,
        page_info: Option<(usize, usize, usize, usize)>,
        url_base: &str,
    ) -> String {
        let mut writer = Writer::new(Cursor::new(Vec::new()));
        writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None))).unwrap();

        let mut feed = BytesStart::new("feed");
        feed.push_attribute(("xmlns", "http://www.w3.org/2005/Atom"));
        feed.push_attribute(("xmlns:opds", "http://opds-spec.org/2010/catalog"));
        feed.push_attribute(("xmlns:dcterms", "http://purl.org/dc/terms/"));
        feed.push_attribute(("xmlns:opensearch", "http://a9.com/-/spec/opensearch/1.1/"));

        writer.write_event(Event::Start(feed)).unwrap();

        Self::write_elem(&mut writer, "id", id);
        Self::write_elem(&mut writer, "title", title);

        writer.write_event(Event::Start(BytesStart::new("authentication"))).unwrap();
        Self::write_elem(&mut writer, "type", "http://opds-spec.org/auth/basic");
        writer.write_event(Event::Start(BytesStart::new("labels"))).unwrap();
        Self::write_elem(&mut writer, "login", "Card");
        Self::write_elem(&mut writer, "password", "PW");
        writer.write_event(Event::End(BytesEnd::new("labels"))).unwrap();
        writer.write_event(Event::End(BytesEnd::new("authentication"))).unwrap();

        Self::write_elem(&mut writer, "updated", &chrono::Utc::now().to_rfc3339());

        if let Some(lib) = library {
            Self::write_link(&mut writer, "alternate", "text/html", "Web Interface", &format!("/library/{}", lib.id));
            Self::write_link(&mut writer, "search", "application/opensearchdescription+xml", "Search this library", &format!("/opds/libraries/{}/search-definition", lib.id));
             Self::write_link(&mut writer, "search", "application/atom+xml", "Search this library", &format!("/opds/libraries/{}?q={{searchTerms}}", lib.id));

             if let Some((page, page_size, total_items, total_pages)) = page_info {
                let start_index = page * page_size + 1;
                Self::write_elem_ns(&mut writer, "opensearch:totalResults", &total_items.to_string());
                Self::write_elem_ns(&mut writer, "opensearch:startIndex", &start_index.to_string());
                Self::write_elem_ns(&mut writer, "opensearch:itemsPerPage", &page_size.to_string());

                 let clean_url = if url_base.contains("?page=") || url_base.contains("&page=") {
                     regex::Regex::new(r"[?&]page=\d+").unwrap().replace(url_base, "").to_string()
                 } else {
                     url_base.to_string()
                 };

                 let separator = if clean_url.contains('?') { "&" } else { "?" };

                Self::write_link(&mut writer, "start", "application/atom+xml;profile=opds-catalog;kind=navigation", "", &clean_url);
                Self::write_link(&mut writer, "first", "application/atom+xml;profile=opds-catalog;kind=acquisition", "", &clean_url);

                if page > 0 {
                     let prev_page = page - 1;
                     let href = if prev_page > 0 { format!("{}{}{}{}", clean_url, separator, "page=", prev_page) } else { clean_url.clone() };
                     Self::write_link(&mut writer, "previous", "application/atom+xml;profile=opds-catalog;kind=acquisition", "", &href);
                }

                if page + 1 < total_pages {
                    let next_page = page + 1;
                     let href = format!("{}{}{}{}", clean_url, separator, "page=", next_page);
                     Self::write_link(&mut writer, "next", "application/atom+xml;profile=opds-catalog;kind=acquisition", "", &href);
                }

                if total_pages > 1 {
                     let last_page = total_pages - 1;
                      let href = format!("{}{}{}{}", clean_url, separator, "page=", last_page);
                      Self::write_link(&mut writer, "last", "application/atom+xml;profile=opds-catalog;kind=acquisition", "", &href);
                }

             }
        }

        let mut result = String::from_utf8(writer.into_inner().into_inner()).unwrap();

        for entry in entries_xml {
            result.push_str(&entry);
        }

        result.push_str("</feed>");
        result
    }

    fn write_elem(writer: &mut Writer<Cursor<Vec<u8>>>, name: &str, value: &str) {
        writer.write_event(Event::Start(BytesStart::new(name))).unwrap();
        writer.write_event(Event::Text(quick_xml::events::BytesText::new(value))).unwrap();
        writer.write_event(Event::End(BytesEnd::new(name))).unwrap();
    }

     fn write_elem_ns(writer: &mut Writer<Cursor<Vec<u8>>>, name: &str, value: &str) {
        writer.write_event(Event::Start(BytesStart::new(name))).unwrap();
        writer.write_event(Event::Text(quick_xml::events::BytesText::new(value))).unwrap();
        writer.write_event(Event::End(BytesEnd::new(name))).unwrap();
    }

    fn write_link(writer: &mut Writer<Cursor<Vec<u8>>>, rel: &str, type_: &str, title: &str, href: &str) {
        let mut link = BytesStart::new("link");
        if !rel.is_empty() { link.push_attribute(("rel", rel)); }
        if !type_.is_empty() { link.push_attribute(("type", type_)); }
        if !title.is_empty() { link.push_attribute(("title", title)); }
        link.push_attribute(("href", href));
        writer.write_event(Event::Empty(link)).unwrap();
    }

    pub fn build_library_entry_list(libraries: &[Library]) -> Vec<String> {
        libraries.iter().map(|lib| Self::build_library_entry(lib)).collect()
    }

    pub fn build_library_entry(library: &Library) -> String {
        let mut writer = Writer::new(Cursor::new(Vec::new()));
        let entry = BytesStart::new("entry");
        writer.write_event(Event::Start(entry)).unwrap();

        Self::write_elem(&mut writer, "id", &library.id);
        Self::write_elem(&mut writer, "title", &library.name);
        Self::write_elem(&mut writer, "updated", &chrono::Utc::now().to_rfc3339());

        Self::write_link(&mut writer, "subsection", "application/atom+xml;profile=opds-catalog", "", &format!("/opds/libraries/{}?categories=true", library.id));

        writer.write_event(Event::End(BytesEnd::new("entry"))).unwrap();
        String::from_utf8(writer.into_inner().into_inner()).unwrap()
    }

    pub fn build_category_entries(library_id: &str, i18n: &crate::i18n::I18n, lang: Option<&str>) -> Vec<String> {
        let categories = vec![
            (library_id.to_string(), i18n.localize("category.all", lang)),
            ("authors".to_string(), i18n.localize("category.authors", lang)),
            ("narrators".to_string(), i18n.localize("category.narrators", lang)),
            ("genres".to_string(), i18n.localize("category.genres", lang)),
            ("series".to_string(), i18n.localize("category.series", lang)),
        ];

        categories.into_iter().map(|(id, title)| {
             let mut writer = Writer::new(Cursor::new(Vec::new()));
            writer.write_event(Event::Start(BytesStart::new("entry"))).unwrap();
            Self::write_elem(&mut writer, "id", &id);
            Self::write_elem(&mut writer, "title", &title);
            Self::write_elem(&mut writer, "updated", &chrono::Utc::now().to_rfc3339());

            let href = if id == library_id {
                 format!("/opds/libraries/{}", library_id)
            } else {
                 format!("/opds/libraries/{}/{}", library_id, id)
            };

            Self::write_link(&mut writer, "subsection", "application/atom+xml;profile=opds-catalog", "", &href);

            writer.write_event(Event::End(BytesEnd::new("entry"))).unwrap();
            String::from_utf8(writer.into_inner().into_inner()).unwrap()
        }).collect()
    }

    pub fn build_card_entry(item: &str, type_: &str, library_id: &str) -> String {
         let mut writer = Writer::new(Cursor::new(Vec::new()));
        writer.write_event(Event::Start(BytesStart::new("entry"))).unwrap();

        let id = item.to_lowercase().replace(" ", "-");
        Self::write_elem(&mut writer, "id", &id);
        Self::write_elem(&mut writer, "title", item);
        Self::write_elem(&mut writer, "updated", &chrono::Utc::now().to_rfc3339());

        let href = format!("/opds/libraries/{}?name={}&type={}", library_id, item, type_);
         Self::write_link(&mut writer, "subsection", "application/atom+xml;profile=opds-catalog", "", &href);

        writer.write_event(Event::End(BytesEnd::new("entry"))).unwrap();
        String::from_utf8(writer.into_inner().into_inner()).unwrap()
    }

    pub fn build_custom_card_entry(item: &str, link: &str) -> String {
         let mut writer = Writer::new(Cursor::new(Vec::new()));
        writer.write_event(Event::Start(BytesStart::new("entry"))).unwrap();

        let id = item.to_lowercase().replace(" ", "-");
        Self::write_elem(&mut writer, "id", &id);
        Self::write_elem(&mut writer, "title", item);
        Self::write_elem(&mut writer, "updated", &chrono::Utc::now().to_rfc3339());

         Self::write_link(&mut writer, "subsection", "application/atom+xml;profile=opds-catalog", "", link);

        writer.write_event(Event::End(BytesEnd::new("entry"))).unwrap();
        String::from_utf8(writer.into_inner().into_inner()).unwrap()
    }

    pub fn build_item_entry(item: &LibraryItem, user: &InternalUser, link_url: &str) -> String {
        let mut writer = Writer::new(Cursor::new(Vec::new()));
        writer.write_event(Event::Start(BytesStart::new("entry"))).unwrap();

        Self::write_elem(&mut writer, "id", &format!("urn:uuid:{}", item.id));
        if let Some(t) = &item.title { Self::write_elem(&mut writer, "title", t); }
        if let Some(s) = &item.subtitle { Self::write_elem(&mut writer, "subtitle", s); }
        Self::write_elem(&mut writer, "updated", &chrono::Utc::now().to_rfc3339());

        if let Some(desc) = &item.description {
             let mut content = BytesStart::new("content");
             content.push_attribute(("type", "text"));
             writer.write_event(Event::Start(content)).unwrap();
             writer.write_event(Event::Text(quick_xml::events::BytesText::new(desc))).unwrap();
             writer.write_event(Event::End(BytesEnd::new("content"))).unwrap();
        }

        if let Some(publ) = &item.publisher { Self::write_elem(&mut writer, "publisher", publ); }
        if let Some(isbn) = &item.isbn { Self::write_elem(&mut writer, "isbn", isbn); }
        if let Some(year) = &item.published_year { Self::write_elem(&mut writer, "published", year); }
        if let Some(lang) = &item.language { Self::write_elem(&mut writer, "language", lang); }

        let format = item.format.as_deref().unwrap_or("");
        let mime_type = match format {
            "audiobook" => "audio/mpeg",
            "epub" => "application/epub+zip",
            "pdf" => "application/pdf",
            "mobi" => "application/x-mobipocket-ebook",
             _ => "application/octet-stream"
        };

        Self::write_link(&mut writer, "http://opds-spec.org/acquisition", "application/octet-stream", "",
            &format!("{}/api/items/{}/download?token={}", link_url, item.id, user.api_key));

        Self::write_link(&mut writer, "http://opds-spec.org/acquisition", mime_type, "",
            &format!("{}/api/items/{}/ebook?token={}", link_url, item.id, user.api_key));

        Self::write_link(&mut writer, "http://opds-spec.org/image", "image/webp", "",
            &format!("{}/api/items/{}/cover?token={}", link_url, item.id, user.api_key));

        Self::write_link(&mut writer, "http://opds-spec.org/image", "image/png", "",
            &format!("{}/api/items/{}/cover?token={}", link_url, item.id, user.api_key));

        for author in &item.authors {
             writer.write_event(Event::Start(BytesStart::new("author"))).unwrap();
             Self::write_elem(&mut writer, "name", &author.name);
             writer.write_event(Event::End(BytesEnd::new("author"))).unwrap();
        }

        for tag in item.genres.iter().chain(item.tags.iter()) {
            let mut cat = BytesStart::new("category");
            cat.push_attribute(("label", tag.as_str()));
            cat.push_attribute(("term", tag.as_str()));
            writer.write_event(Event::Empty(cat)).unwrap();
        }

        writer.write_event(Event::End(BytesEnd::new("entry"))).unwrap();
        String::from_utf8(writer.into_inner().into_inner()).unwrap()
    }

     pub fn build_search_definition(id: &str) -> String {
        let mut writer = Writer::new(Cursor::new(Vec::new()));
        writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None))).unwrap();

        let mut root = BytesStart::new("OpenSearchDescription");
        root.push_attribute(("xmlns", "http://a9.com/-/spec/opensearch/1.1/"));
        root.push_attribute(("xmlns:atom", "http://www.w3.org/2005/Atom"));
        writer.write_event(Event::Start(root)).unwrap();

        Self::write_elem(&mut writer, "ShortName", "ABS");
        Self::write_elem(&mut writer, "LongName", "Audiobookshelf");
        Self::write_elem(&mut writer, "Description", "Search for books in Audiobookshelf");

        let mut url = BytesStart::new("Url");
        url.push_attribute(("type", "application/atom+xml;profile=opds-catalog;kind=acquisition"));

        // Fix formatting of template string attribute
        let template = format!("/opds/libraries/{}?q={{searchTerms}}&amp;author={{atom:author}}&amp;title={{atom:title}}", id);
        url.push_attribute(("template", template.as_str()));

        writer.write_event(Event::Empty(url)).unwrap();

        writer.write_event(Event::End(BytesEnd::new("OpenSearchDescription"))).unwrap();
        String::from_utf8(writer.into_inner().into_inner()).unwrap()
     }
}
