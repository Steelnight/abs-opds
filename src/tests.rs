#[cfg(test)]
mod tests {
    use crate::models::{Library, LibraryItem, Author, InternalUser};
    use crate::xml::OpdsBuilder;
    use quick_xml::Writer;
    use std::io::Cursor;

    #[test]
    fn test_build_opds_skeleton() {
        let xml = OpdsBuilder::build_opds_skeleton(
            "test_id",
            "Test Title",
            |_| Ok(()),
            None,
            None,
            None,
            "/opds"
        ).expect("Failed to build XML");

        assert!(xml.contains("<id>test_id</id>"));
        assert!(xml.contains("<title>Test Title</title>"));
        assert!(xml.contains("<feed xmlns=\"http://www.w3.org/2005/Atom\""));
    }

    #[test]
    fn test_build_library_entry() {
        let lib = Library {
            id: "lib1".to_string(),
            name: "My Library".to_string(),
            icon: None,
        };

        let mut writer = Writer::new(Cursor::new(Vec::new()));
        OpdsBuilder::build_library_entry(&mut writer, &lib).expect("Failed to build entry");

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
            description: Some("Description".to_string()),
            genres: vec!["Fantasy".to_string()],
            tags: vec![],
            publisher: Some("Publisher".to_string()),
            isbn: None,
            language: Some("en".to_string()),
            published_year: Some("2023".to_string()),
            authors: vec![Author { name: "Author Name".to_string() }],
            narrators: vec![],
            series: vec![],
            format: Some("epub".to_string()),
        };

        let user = InternalUser {
            name: "user".to_string(),
            api_key: "token".to_string(),
            password: None,
        };

        let mut writer = Writer::new(Cursor::new(Vec::new()));
        OpdsBuilder::build_item_entry(&mut writer, &item, &user, "http://localhost:3000").expect("Failed to build entry");

        let entry = String::from_utf8(writer.into_inner().into_inner()).unwrap();
        assert!(entry.contains("<id>urn:uuid:item1</id>"));
        assert!(entry.contains("<title>Book Title</title>"));
        assert!(entry.contains("<name>Author Name</name>"));
        assert!(entry.contains("application/epub+zip"));
        assert!(entry.contains("token=token"));
    }
}
