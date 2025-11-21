use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

#[derive(Clone)]
pub struct I18n {
    localizations: Arc<HashMap<String, Value>>,
    fallback_language: String,
}

impl I18n {
    pub fn new(languages_dir: &Path) -> Self {
        let mut localizations = HashMap::new();
        if let Ok(entries) = fs::read_dir(languages_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "json") {
                    if let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) {
                        if let Ok(content) = fs::read_to_string(&path) {
                            if let Ok(json) = serde_json::from_str(&content) {
                                localizations.insert(file_stem.to_lowercase(), json);
                            }
                        }
                    }
                }
            }
        }

        I18n {
            localizations: Arc::new(localizations),
            fallback_language: "en".to_string(),
        }
    }

    pub fn localize(&self, key: &str, lang: Option<&str>) -> String {
        let localizations = &self.localizations;
        let language_code = lang
            .and_then(|l| l.split('-').next())
            .map(|l| l.to_lowercase())
            .unwrap_or_else(|| self.fallback_language.clone());

        let language = if localizations.contains_key(&language_code) {
            &language_code
        } else {
            &self.fallback_language
        };

        if let Some(lang_map) = localizations.get(language) {
            if let Some(val) = lang_map.get(key) {
                if let Some(s) = val.as_str() {
                    return s.to_string();
                }
            }
        }

        // Fallback
        if language != &self.fallback_language {
            if let Some(lang_map) = localizations.get(&self.fallback_language) {
                 if let Some(val) = lang_map.get(key) {
                    if let Some(s) = val.as_str() {
                        return s.to_string();
                    }
                }
            }
        }

        key.to_string()
    }
}
