
use super::*;

#[test]
fn test_extract_path_from_url() {
    let archiver = TelegraphArchiver::new();

    let url = "https://telegra.ph/Sample-Page-12-15";
    let path = archiver.extract_path_from_url(url).unwrap();
    assert_eq!(path, "Sample-Page-12-15");

    let invalid_url = "https://example.com/page";
    assert!(archiver.extract_path_from_url(invalid_url).is_err());
}

#[test]
fn test_generate_filename() {
    let archiver = TelegraphArchiver::new();
    let page = TelegraphPage {
        path: "Sample-Page-12-15".to_string(),
        url: "https://telegra.ph/Sample-Page-12-15".to_string(),
        title: "Sample Page".to_string(),
        description: "A sample page".to_string(),
        author_name: None,
        author_url: None,
        image_url: None,
        content: None,
        views: 100,
    };

    let filename = archiver.generate_filename(&page);
    assert_eq!(filename, "Sample-Page-12-15.md");
}
