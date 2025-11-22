pub fn create_regex(pattern: &str) -> anyhow::Result<regex::Regex> {
    regex::RegexBuilder::new(&regex::escape(pattern))
        .case_insensitive(true)
        .build()
        .map_err(|e| e.into())
}

pub fn create_regex_or_empty(pattern: &str) -> regex::Regex {
    create_regex(pattern).unwrap_or_else(|_| regex::Regex::new("").unwrap())
}
