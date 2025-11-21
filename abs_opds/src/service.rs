use crate::api::AbsClient;
use crate::models::{Library, LibraryItem, InternalUser, ItemType, AppConfig};
use crate::i18n::I18n;
use crate::xml::OpdsBuilder;
use std::sync::{Arc, OnceLock};
use std::collections::{HashSet, HashMap};
use unicode_normalization::UnicodeNormalization;
use anyhow::Result;

#[cfg(test)]
#[path = "service_tests.rs"]
mod service_tests;

pub struct LibraryService {
    pub client: Arc<dyn AbsClient>,
    pub config: AppConfig,
    pub i18n: I18n,
}

impl LibraryService {
    pub fn new(client: Arc<dyn AbsClient>, config: AppConfig, i18n: I18n) -> Self {
        Self { client, config, i18n }
    }

    pub async fn get_libraries(&self, user: &InternalUser) -> Result<Vec<Library>> {
        let libraries = self.client.get_libraries(user).await?;
        Ok(libraries.into_iter().map(|l| Library {
            id: l.id,
            name: l.name,
            icon: l.icon,
        }).collect())
    }

    pub async fn get_library(&self, user: &InternalUser, library_id: &str) -> Result<Library> {
        let lib = self.client.get_library(user, library_id).await?;
        Ok(Library {
            id: lib.id,
            name: lib.name,
            icon: lib.icon,
        })
    }

    pub async fn get_filtered_items(
        &self,
        user: &InternalUser,
        library_id: &str,
        query: &crate::handlers::LibraryQuery,
    ) -> Result<(Vec<LibraryItem>, usize)> {
        let items_data = self.client.get_items(user, library_id).await?;

        let mut parsed_items: Vec<LibraryItem> = items_data.results.into_iter().filter_map(|item| {
             let format = item.media.ebook_format;
             if format.is_some() || self.config.show_audiobooks {
                 Some(LibraryItem {
                     id: item.id,
                     title: item.media.metadata.title,
                     subtitle: item.media.metadata.subtitle,
                     description: item.media.metadata.description,
                     genres: item.media.metadata.genres.unwrap_or_default(),
                     tags: item.media.metadata.tags.unwrap_or_default(),
                     publisher: item.media.metadata.publisher,
                     isbn: item.media.metadata.isbn,
                     language: item.media.metadata.language,
                     published_year: item.media.metadata.published_year,
                     authors: item.media.metadata.author_name.map(|s| s.split(',').map(|n| crate::models::Author { name: n.trim().to_string() }).collect()).unwrap_or_default(),
                     narrators: item.media.metadata.narrator_name.map(|s| s.split(',').map(|n| crate::models::Author { name: n.trim().to_string() }).collect()).unwrap_or_default(),
                     series: item.media.metadata.series_name.map(|s| {
                         static SERIES_CLEANUP_RE: OnceLock<regex::Regex> = OnceLock::new();
                         let re = SERIES_CLEANUP_RE.get_or_init(|| regex::Regex::new(r"#.*$").unwrap());
                         s.split(',').map(|n| n.trim().replace(re.as_str(), "").trim().to_string()).collect()
                     }).unwrap_or_default(),
                     format,
                 })
             } else {
                 None
             }
         }).collect();

         if query.q.is_some() || query.type_.is_some() {
             let search_term = query.q.as_deref().unwrap_or("");
             let re = regex::RegexBuilder::new(&regex::escape(search_term))
                .case_insensitive(true)
                .build()
                .unwrap_or_else(|_| regex::Regex::new("").unwrap());

             let type_query = query.type_.as_ref();
             let name_query_re = query.name.as_deref().map(|n| {
                  regex::RegexBuilder::new(&regex::escape(n))
                    .case_insensitive(true)
                    .build()
                    .unwrap_or_else(|_| regex::Regex::new("").unwrap())
             });

             parsed_items.retain(|item| {
                 if type_query == Some(&ItemType::Authors) {
                     if let Some(re) = &name_query_re {
                         return item.authors.iter().any(|a| re.is_match(&a.name));
                     }
                 } else if type_query == Some(&ItemType::Narrators) {
                      if let Some(re) = &name_query_re {
                         return item.narrators.iter().any(|a| re.is_match(&a.name));
                     }
                 } else if type_query == Some(&ItemType::Genres) {
                     if let Some(re) = &name_query_re {
                         return item.genres.iter().any(|g| re.is_match(g)) || item.tags.iter().any(|t| re.is_match(t));
                     }
                 } else if type_query == Some(&ItemType::Series) {
                      if let Some(re) = &name_query_re {
                         return item.series.iter().any(|s| re.is_match(s));
                     }
                 } else {
                      if !search_term.is_empty() {
                         return item.matches(&re);
                      }
                 }
                 true
             });
         }

         if let Some(author) = &query.author {
             let re = regex::RegexBuilder::new(&regex::escape(author)).case_insensitive(true).build()?;
             parsed_items.retain(|item| item.authors.iter().any(|a| re.is_match(&a.name)));
         }

         if let Some(title) = &query.title {
             let re = regex::RegexBuilder::new(&regex::escape(title)).case_insensitive(true).build()?;
             parsed_items.retain(|item| item.title.as_deref().map_or(false, |t| re.is_match(t)) || item.subtitle.as_deref().map_or(false, |t| re.is_match(t)));
         }

         let total_items = parsed_items.len();
         let page_size = self.config.opds_page_size;
         let start_index = query.page * page_size;

         if start_index < total_items {
             let end_index = std::cmp::min(start_index + page_size, total_items);
             Ok((parsed_items[start_index..end_index].to_vec(), total_items))
         } else {
             Ok((vec![], total_items))
         }
    }

    pub async fn get_categories(
        &self,
        user: &InternalUser,
        library_id: &str,
        type_: &str,
        query: &crate::handlers::LibraryQuery,
    ) -> Result<String> {
        // Logic from get_category handler
         let items_data = self.client.get_items(user, library_id).await?;
         let lib_data = self.client.get_library(user, library_id).await?;

         let library = Library {
             id: lib_data.id,
             name: lib_data.name,
             icon: lib_data.icon,
         };

         let mut distinct_type = HashSet::new();

         for item in items_data.results {
              match type_ {
                  "authors" => {
                      if let Some(names) = item.media.metadata.author_name {
                          for name in names.split(',') { distinct_type.insert(name.trim().to_string()); }
                      }
                  },
                  "narrators" => {
                       if let Some(names) = item.media.metadata.narrator_name {
                          for name in names.split(',') { distinct_type.insert(name.trim().to_string()); }
                      }
                  },
                  "genres" => {
                      if let Some(genres) = item.media.metadata.genres {
                          for g in genres { distinct_type.insert(g.trim().to_string()); }
                      }
                      if let Some(tags) = item.media.metadata.tags {
                          for t in tags { distinct_type.insert(t.trim().to_string()); }
                      }
                  },
                  "series" => {
                       if let Some(series) = item.media.metadata.series_name {
                          for s in series.split(',') { distinct_type.insert(s.trim().to_string()); }
                      }
                  },
                  _ => {}
              }
         }

         let mut distinct_type_array: Vec<String> = distinct_type.into_iter().collect();
         distinct_type_array.sort();

         if query.start.is_none() && self.config.show_char_cards {
              let mut count_by_start: HashMap<String, usize> = HashMap::new();
              for item in &distinct_type_array {
                  let start_char = item.chars().next().unwrap_or(' ').to_uppercase().to_string();
                  let normalized = start_char.nfd().filter(|c| !crate::xml::is_combining_mark(*c)).collect::<String>();
                  let key = if normalized >= "A".to_string() && normalized <= "Z".to_string() { normalized } else { String::new() };
                  if !key.is_empty() {
                       *count_by_start.entry(key).or_insert(0) += 1;
                  }
              }

              let mut keys: Vec<String> = count_by_start.keys().cloned().collect();
              keys.sort();

              OpdsBuilder::build_opds_skeleton(
                    &format!("urn:uuid:{}", library_id),
                    &library.name,
                    |writer| {
                        for letter in keys {
                            let count = count_by_start[&letter];
                            let title = format!("{} ({})", letter, count);
                            let link = format!("/opds/libraries/{}/{}?start={}", library_id, type_, letter.to_lowercase());
                            OpdsBuilder::build_custom_card_entry(writer, &title, &link)?;
                        }
                        Ok(())
                    },
                    None,
                    None,
                    None,
                    &format!("/opds/libraries/{}/{}", library_id, type_)
                ).map_err(|e| e.into())
         } else {
             if let Some(start) = &query.start {
                 distinct_type_array.retain(|item| {
                      let start_char = item.chars().next().unwrap_or(' ').to_lowercase().to_string();
                       let normalized = start_char.nfd().filter(|c| !crate::xml::is_combining_mark(*c)).collect::<String>();
                       normalized == *start
                 });
             }

              OpdsBuilder::build_opds_skeleton(
                 &format!("urn:uuid:{}", library_id),
                 &library.name,
                 |writer| {
                     for item in distinct_type_array {
                         OpdsBuilder::build_card_entry(writer, &item, &type_, &library_id)?;
                     }
                     Ok(())
                 },
                 None,
                 None,
                 None,
                 &format!("/opds/libraries/{}/{}", library_id, type_)
             ).map_err(|e| e.into())
         }
    }
}
