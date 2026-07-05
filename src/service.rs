use crate::api::AbsClient;
use crate::models::{Library, LibraryItem, InternalUser, ItemType, AppConfig};
use crate::i18n::I18n;
use crate::xml::OpdsBuilder;
use std::sync::Arc;
use std::collections::{HashSet, HashMap};
use unicode_normalization::UnicodeNormalization;
use anyhow::Result;
use rayon::prelude::*;

#[cfg(test)]
#[path = "service_tests.rs"]
mod service_tests;
#[cfg(test)]
#[path = "performance_tests.rs"]
mod performance_tests;

pub struct LibraryService<C: AbsClient + ?Sized> {
    pub client: Arc<C>,
    pub config: AppConfig,
    pub i18n: I18n,
}

impl<C: AbsClient + ?Sized> LibraryService<C> {
    pub fn new(client: Arc<C>, config: AppConfig, i18n: I18n) -> Self {
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

        let results = &items_data.results;
        let filtered_items: Vec<&crate::models::AbsItemResult> = if results.len() > 2000 {
            results.par_iter().filter(|item| self.filter_item(item, query)).collect()
        } else {
            results.iter().filter(|item| self.filter_item(item, query)).collect()
        };

        let total_items = filtered_items.len();
        let page_size = self.config.opds_page_size;
        let start_index = query.page * page_size;

        if start_index < total_items {
             let end_index = std::cmp::min(start_index + page_size, total_items);
             let paginated_refs = &filtered_items[start_index..end_index];
             let mapped_items: Vec<LibraryItem> = paginated_refs.iter().map(|item| {
                 let format = item.media.ebook_format.clone();
                 LibraryItem {
                     id: item.id.clone(),
                     title: item.media.metadata.title.clone(),
                     subtitle: item.media.metadata.subtitle.clone(),
                     description: item.media.metadata.description.clone(),
                     genres: item.media.metadata.genres.clone().unwrap_or_default(),
                     tags: item.media.metadata.tags.clone().unwrap_or_default(),
                     publisher: item.media.metadata.publisher.clone(),
                     isbn: item.media.metadata.isbn.clone(),
                     language: item.media.metadata.language.clone(),
                     published_year: item.media.metadata.published_year.clone(),
                     authors: item.media.metadata.author_name.as_deref().map(|s| {
                         s.split(',').map(|n| crate::models::Author { name: n.trim().to_string() }).collect()
                     }).unwrap_or_default(),
                     narrators: item.media.metadata.narrator_name.as_deref().map(|s| {
                         s.split(',').map(|n| crate::models::Author { name: n.trim().to_string() }).collect()
                     }).unwrap_or_default(),
                     series: item.media.metadata.series_name.as_deref().map(|s| {
                         s.split(',').map(|n| {
                             let cleaned = if let Some(idx) = n.find('#') {
                                 n[..idx].trim()
                             } else {
                                 n.trim()
                             };
                             cleaned.to_string()
                         }).collect()
                     }).unwrap_or_default(),
                     format,
                 }
             }).collect();
             Ok((mapped_items, total_items))
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
         let updated_time = chrono::Utc::now().to_rfc3339();
         let items_data = self.client.get_items(user, library_id).await?;
         let lib_data = self.client.get_library(user, library_id).await?;

         let library = Library {
             id: lib_data.id,
             name: lib_data.name,
             icon: lib_data.icon,
         };

         // Suggestion 6: Sequential fold instead of parallel map-reduce Set merges
         let mut distinct_type = HashSet::new();
         for item in items_data.results {
             match type_ {
                 "authors" => {
                     if let Some(names) = item.media.metadata.author_name {
                         for name in names.split(',') {
                             distinct_type.insert(name.trim().to_string());
                         }
                     }
                 },
                 "narrators" => {
                      if let Some(names) = item.media.metadata.narrator_name {
                         for name in names.split(',') {
                             distinct_type.insert(name.trim().to_string());
                         }
                     }
                 },
                 "genres" => {
                     if let Some(genres) = item.media.metadata.genres {
                         for g in genres {
                             distinct_type.insert(g.trim().to_string());
                         }
                     }
                     if let Some(tags) = item.media.metadata.tags {
                         for t in tags {
                             distinct_type.insert(t.trim().to_string());
                         }
                     }
                 },
                 "series" => {
                      if let Some(series) = item.media.metadata.series_name {
                         for s in series.split(',') {
                             distinct_type.insert(s.trim().to_string());
                         }
                     }
                 },
                 _ => {}
             }
         }

         if query.start.is_none() && self.config.show_char_cards {
                let mut distinct_type_array: Vec<String> = distinct_type.into_iter().collect();
                distinct_type_array.sort_unstable();

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
                           let mut url_buf = String::with_capacity(256);
                           for letter in keys {
                               let count = count_by_start[&letter];
                               let title = format!("{} ({})", letter, count);
                               let link = format!("/opds/libraries/{}/{}?start={}", library_id, type_, letter.to_lowercase());
                               OpdsBuilder::build_custom_card_entry(writer, &title, &link, &updated_time, &mut url_buf)?;
                           }
                           Ok(())
                       },
                       None,
                       None,
                       None,
                       &format!("/opds/libraries/{}/{}", library_id, type_)
                   ).map_err(|e| e.into())
         } else {
             let mut distinct_type_array: Vec<String> = if let Some(start) = &query.start {
                 distinct_type.into_iter()
                     .filter(|item| {
                          let start_char = item.chars().next().unwrap_or(' ').to_lowercase().to_string();
                          let normalized = start_char.nfd().filter(|c| !crate::xml::is_combining_mark(*c)).collect::<String>();
                          normalized == *start
                     })
                     .collect()
             } else {
                 distinct_type.into_iter().collect()
             };
             distinct_type_array.sort_unstable();

             // Suggestion 7: Category pagination
             let total_items = distinct_type_array.len();
             let page_size = self.config.opds_page_size;
             let total_pages = (total_items + page_size - 1) / page_size;
             let start_index = query.page * page_size;

             let (paginated_items, page_info) = if start_index < total_items {
                 let end_index = std::cmp::min(start_index + page_size, total_items);
                 (&distinct_type_array[start_index..end_index], Some((query.page, page_size, total_items, total_pages)))
             } else {
                 (&[][..], Some((query.page, page_size, total_items, total_pages)))
             };

             let mut url_base = format!("/opds/libraries/{}/{}", library_id, type_);
             if let Some(start) = &query.start {
                 url_base.push_str(&format!("?start={}", start));
             }

                 OpdsBuilder::build_opds_skeleton(
                    &format!("urn:uuid:{}", library_id),
                    &library.name,
                    |writer| {
                        let mut url_buf = String::with_capacity(256);
                        for item in paginated_items {
                            OpdsBuilder::build_card_entry(writer, item, &type_, &library_id, &updated_time, &mut url_buf)?;
                        }
                        Ok(())
                    },
                   Some(&library),
                   Some(user),
                   page_info,
                   &url_base
               ).map_err(|e| e.into())
         }
    }

    fn filter_item(&self, item: &crate::models::AbsItemResult, query: &crate::handlers::LibraryQuery) -> bool {
         let format = item.media.ebook_format.as_deref();
         if format.is_none() && !self.config.show_audiobooks {
             return false;
         }

         if query.q.is_some() || query.type_.is_some() {
             let search_term_lower = query.q.as_deref().unwrap_or("").to_lowercase();
             let type_query = query.type_.as_ref();
             let name_query_lower = query.name.as_deref().map(|n| n.to_lowercase());

             let matches = if type_query == Some(&ItemType::Authors) {
                 if let Some(n_lower) = &name_query_lower {
                     author_matches(item.media.metadata.author_name.as_deref(), n_lower)
                 } else {
                     true
                 }
             } else if type_query == Some(&ItemType::Narrators) {
                 if let Some(n_lower) = &name_query_lower {
                     author_matches(item.media.metadata.narrator_name.as_deref(), n_lower)
                 } else {
                     true
                 }
             } else if type_query == Some(&ItemType::Genres) {
                 if let Some(n_lower) = &name_query_lower {
                     let g_match = item.media.metadata.genres.as_ref().map_or(false, |genres| {
                         genres.iter().any(|g| g.to_lowercase().contains(n_lower))
                     });
                     let t_match = item.media.metadata.tags.as_ref().map_or(false, |tags| {
                         tags.iter().any(|t| t.to_lowercase().contains(n_lower))
                     });
                     g_match || t_match
                 } else {
                     true
                 }
             } else if type_query == Some(&ItemType::Series) {
                 if let Some(n_lower) = &name_query_lower {
                     clean_series(item.media.metadata.series_name.as_deref(), n_lower)
                 } else {
                     true
                 }
             } else {
                 if !search_term_lower.is_empty() {
                     matches_search_abs(&item.media.metadata, &search_term_lower)
                 } else {
                     true
                 }
             };

             if !matches {
                 return false;
             }
         }

         if let Some(author) = &query.author {
             let author_lower = author.to_lowercase();
             if !author_matches(item.media.metadata.author_name.as_deref(), &author_lower) {
                 return false;
             }
         }

         if let Some(title) = &query.title {
             let title_lower = title.to_lowercase();
             let title_match = item.media.metadata.title.as_deref().map_or(false, |t| contains_case_insensitive(t, &title_lower)) ||
                 item.media.metadata.subtitle.as_deref().map_or(false, |t| contains_case_insensitive(t, &title_lower));
             if !title_match {
                 return false;
             }
         }

         true
    }
}

fn author_matches(author_name: Option<&str>, term_lower: &str) -> bool {
    author_name.map_or(false, |s| {
        s.split(',').any(|n| contains_case_insensitive(n.trim(), term_lower))
    })
}

fn clean_series(series_name: Option<&str>, term_lower: &str) -> bool {
    series_name.map_or(false, |s| {
        s.split(',').any(|n| {
            let cleaned = if let Some(idx) = n.find('#') {
                n[..idx].trim()
            } else {
                n.trim()
            };
            contains_case_insensitive(cleaned, term_lower)
        })
    })
}

fn matches_search_abs(metadata: &crate::models::AbsMetadata, term_lower: &str) -> bool {
    if term_lower.is_empty() {
        return true;
    }
    metadata.title.as_deref().map_or(false, |s| contains_case_insensitive(s, term_lower)) ||
    metadata.subtitle.as_deref().map_or(false, |s| contains_case_insensitive(s, term_lower)) ||
    metadata.description.as_deref().map_or(false, |s| contains_case_insensitive(s, term_lower)) ||
    metadata.publisher.as_deref().map_or(false, |s| contains_case_insensitive(s, term_lower)) ||
    metadata.isbn.as_deref().map_or(false, |s| contains_case_insensitive(s, term_lower)) ||
    metadata.language.as_deref().map_or(false, |s| contains_case_insensitive(s, term_lower)) ||
    metadata.published_year.as_deref().map_or(false, |s| contains_case_insensitive(s, term_lower)) ||
    metadata.author_name.as_deref().map_or(false, |s| {
        s.split(',').any(|n| contains_case_insensitive(n.trim(), term_lower))
    }) ||
    metadata.genres.as_ref().map_or(false, |genres| {
        genres.iter().any(|g| contains_case_insensitive(g, term_lower))
    }) ||
    metadata.tags.as_ref().map_or(false, |tags| {
        tags.iter().any(|t| contains_case_insensitive(t, term_lower))
    })
}

pub(crate) fn contains_case_insensitive(haystack: &str, needle_lower: &str) -> bool {
    if needle_lower.is_empty() {
        return true;
    }
    if haystack.is_ascii() && needle_lower.is_ascii() {
        haystack.as_bytes().windows(needle_lower.len()).any(|window| {
            window.eq_ignore_ascii_case(needle_lower.as_bytes())
        })
    } else {
        haystack.to_lowercase().contains(needle_lower)
    }
}
