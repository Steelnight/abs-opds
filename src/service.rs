use crate::api::AbsClient;
use crate::models::{Library, LibraryItem, InternalUser, ItemType, AppConfig};
use crate::i18n::I18n;
use crate::xml::OpdsBuilder;
use std::sync::{Arc, OnceLock};
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

        let config = self.config.clone();
        let query = query.clone();

        let (parsed_items, total_items) = tokio::task::spawn_blocking(move || {
             // Pre-compile Regexes
             let search_term = query.q.as_deref().unwrap_or("");
             let search_re = if !search_term.is_empty() {
                 regex::RegexBuilder::new(&regex::escape(search_term))
                    .case_insensitive(true)
                    .build()
                    .ok()
             } else {
                 None
             };

             let type_query = query.type_.as_ref();
             let name_query_re = query.name.as_deref().and_then(|n| {
                  regex::RegexBuilder::new(&regex::escape(n))
                    .case_insensitive(true)
                    .build()
                    .ok()
             });

             let author_re = query.author.as_deref().and_then(|a| {
                  regex::RegexBuilder::new(&regex::escape(a))
                    .case_insensitive(true)
                    .build()
                    .ok()
             });

             let title_re = query.title.as_deref().and_then(|t| {
                  regex::RegexBuilder::new(&regex::escape(t))
                    .case_insensitive(true)
                    .build()
                    .ok()
             });

             // 1. Filter and collect references to AbsItemResult
             let filtered_refs: Vec<&crate::models::AbsItemResult> = items_data.results.par_iter().filter(|item| {
                 // 1. Format Check
                 let format = item.media.ebook_format.as_deref();
                 if format.is_none() && !config.show_audiobooks {
                     return false;
                 }

                 // 2. Filter on raw metadata (AbsItemResult)
                 // Search Query & Type/Name Query
                 if query.q.is_some() || query.type_.is_some() {
                     let mut matches = true;
                     let metadata = &item.media.metadata;

                     if let Some(t_query) = type_query {
                         match t_query {
                             ItemType::Authors => {
                                 if let Some(re) = &name_query_re {
                                     matches = metadata.author_name.as_deref().map_or(false, |s| re.is_match(s));
                                 }
                             },
                             ItemType::Narrators => {
                                 if let Some(re) = &name_query_re {
                                     matches = metadata.narrator_name.as_deref().map_or(false, |s| re.is_match(s));
                                 }
                             },
                             ItemType::Genres => {
                                 if let Some(re) = &name_query_re {
                                     let genres_match = metadata.genres.as_ref().map_or(false, |v| v.iter().any(|g| re.is_match(g)));
                                     let tags_match = metadata.tags.as_ref().map_or(false, |v| v.iter().any(|t| re.is_match(t)));
                                     matches = genres_match || tags_match;
                                 }
                             },
                             ItemType::Series => {
                                 if let Some(re) = &name_query_re {
                                     matches = metadata.series_name.as_deref().map_or(false, |s| re.is_match(s));
                                 }
                             }
                         }
                     } else {
                         if let Some(re) = &search_re {
                             // Replicate LibraryItem::matches logic but on raw data
                             matches = metadata.title.as_deref().map_or(false, |s| re.is_match(s)) ||
                                       metadata.subtitle.as_deref().map_or(false, |s| re.is_match(s)) ||
                                       metadata.description.as_deref().map_or(false, |s| re.is_match(s)) ||
                                       metadata.publisher.as_deref().map_or(false, |s| re.is_match(s)) ||
                                       metadata.isbn.as_deref().map_or(false, |s| re.is_match(s)) ||
                                       metadata.language.as_deref().map_or(false, |s| re.is_match(s)) ||
                                       metadata.published_year.as_deref().map_or(false, |s| re.is_match(s)) ||
                                       metadata.author_name.as_deref().map_or(false, |s| re.is_match(s)) || // Check raw author string
                                       metadata.genres.as_ref().map_or(false, |v| v.iter().any(|g| re.is_match(g))) ||
                                       metadata.tags.as_ref().map_or(false, |v| v.iter().any(|t| re.is_match(t)));
                         }
                     }
                     if !matches { return false; }
                 }

                 // Author Filter
                 if let Some(re) = &author_re {
                     if !item.media.metadata.author_name.as_deref().map_or(false, |s| re.is_match(s)) {
                         return false;
                     }
                 }

                 // Title Filter
                 if let Some(re) = &title_re {
                     let matches = item.media.metadata.title.as_deref().map_or(false, |t| re.is_match(t)) ||
                                   item.media.metadata.subtitle.as_deref().map_or(false, |t| re.is_match(t));
                     if !matches { return false; }
                 }

                 true
             }).collect();

             let total = filtered_refs.len();
             let page_size = config.opds_page_size;
             let start_index = query.page * page_size;

             let parsed_items = if start_index < total {
                 let end_index = std::cmp::min(start_index + page_size, total);
                 let page_slice = &filtered_refs[start_index..end_index];

                 // 3. Parse ONLY the items for the current page
                 page_slice.par_iter().map(|item| {
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
                         authors: item.media.metadata.author_name.as_deref().map(|s| s.split(',').map(|n| crate::models::Author { name: n.trim().to_string() }).collect()).unwrap_or_default(),
                         narrators: item.media.metadata.narrator_name.as_deref().map(|s| s.split(',').map(|n| crate::models::Author { name: n.trim().to_string() }).collect()).unwrap_or_default(),
                         series: item.media.metadata.series_name.as_deref().map(|s| {
                             static SERIES_CLEANUP_RE: OnceLock<regex::Regex> = OnceLock::new();
                             let re = SERIES_CLEANUP_RE.get_or_init(|| regex::Regex::new(r"#.*$").unwrap());
                             s.split(',').map(|n| n.trim().replace(re.as_str(), "").trim().to_string()).collect()
                         }).unwrap_or_default(),
                         format: item.media.ebook_format.clone(),
                     }
                 }).collect()
             } else {
                 Vec::new()
             };

             (parsed_items, total)
        }).await?;

        Ok((parsed_items, total_items))
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

         let config = self.config.clone();
         let query = query.clone();
         let type_string = type_.to_string();
         let library_id = library_id.to_string();

         let xml = tokio::task::spawn_blocking(move || {
             let distinct_type: HashSet<&str> = items_data.results.par_iter()
                 .fold(HashSet::new, |mut acc, item| {
                     match type_string.as_str() {
                         "authors" => {
                             if let Some(names) = &item.media.metadata.author_name {
                                 for name in names.split(',') { acc.insert(name.trim()); }
                             }
                         },
                         "narrators" => {
                              if let Some(names) = &item.media.metadata.narrator_name {
                                 for name in names.split(',') { acc.insert(name.trim()); }
                             }
                         },
                         "genres" => {
                             if let Some(genres) = &item.media.metadata.genres {
                                 for g in genres { acc.insert(g.trim()); }
                             }
                             if let Some(tags) = &item.media.metadata.tags {
                                 for t in tags { acc.insert(t.trim()); }
                             }
                         },
                         "series" => {
                              if let Some(series) = &item.media.metadata.series_name {
                                 for s in series.split(',') { acc.insert(s.trim()); }
                             }
                         },
                         _ => {}
                     }
                     acc
                 })
                 .reduce(HashSet::new, |mut a, b| {
                     for item in b {
                         a.insert(item);
                     }
                     a
                 });

             let mut distinct_type_array: Vec<String> = distinct_type.into_iter().map(String::from).collect();
             distinct_type_array.sort();

             if query.start.is_none() && config.show_char_cards {
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
                                let link = format!("/opds/libraries/{}/{}?start={}", library_id, type_string, letter.to_lowercase());
                                OpdsBuilder::build_custom_card_entry(writer, &title, &link)?;
                            }
                            Ok(())
                        },
                        None,
                        None,
                        None,
                        &format!("/opds/libraries/{}/{}", library_id, type_string)
                    ).map_err(anyhow::Error::from)
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
                             OpdsBuilder::build_card_entry(writer, &item, &type_string, &library_id)?;
                         }
                         Ok(())
                     },
                     None,
                     None,
                     None,
                     &format!("/opds/libraries/{}/{}", library_id, type_string)
                 ).map_err(anyhow::Error::from)
             }
        }).await??;

        Ok(xml)
    }
}
