
use super::*;

#[test]
fn test_strip_simple_script() {
    let html = r#"<html><head><script>alert('test');</script></head><body>Content</body></html>"#;
    let result = strip_javascript(html);
    assert!(!result.contains("<script"));
    assert!(!result.contains("alert"));
    assert!(result.contains("Content"));
}

#[test]
fn test_strip_script_with_attributes() {
    let html = r#"<div><script type="text/javascript" src="file.js">console.log("hi");</script><p>Keep this</p></div>"#;
    let result = strip_javascript(html);
    assert!(!result.contains("<script"));
    assert!(!result.contains("console.log"));
    assert!(result.contains("Keep this"));
}

#[test]
fn test_strip_multiple_scripts() {
    let html = r#"<html>
        <head><script>var x = 1;</script></head>
        <body>
            <p>Content</p>
            <script>alert('hello');</script>
            <div>More content</div>
        </body>
        </html>"#;
    let result = strip_javascript(html);
    assert!(!result.contains("<script"));
    assert!(!result.contains("var x"));
    assert!(!result.contains("alert"));
    assert!(result.contains("Content"));
    assert!(result.contains("More content"));
}

#[test]
fn test_case_insensitive_script_tags() {
    let html = r#"<SCRIPT>alert('test');</SCRIPT><Script>console.log();</Script>"#;
    let result = strip_javascript(html);
    assert!(!result.contains("SCRIPT"));
    assert!(!result.contains("Script"));
    assert!(!result.contains("alert"));
    assert!(!result.contains("console.log"));
}

#[test]
fn test_no_script_tags() {
    let html =
        r#"<html><head><title>Test</title></head><body><p>No scripts here</p></body></html>"#;
    let result = strip_javascript(html);
    assert_eq!(html, result);
}

#[test]
fn test_script_in_text_content() {
    let html =
        r#"<p>This mentions script tags but isn't one</p><script>alert('remove me');</script>"#;
    let result = strip_javascript(html);
    assert!(result.contains("This mentions script tags"));
    assert!(!result.contains("alert"));
    assert!(!result.contains("<script"));
}

#[test]
fn test_malformed_script_tags() {
    let html = r#"<script>unclosed script<div>content</div>"#;
    let result = strip_javascript(html);
    // Should remove everything after <script> since there's no proper closing
    assert!(!result.contains("<script"));
    assert!(!result.contains("unclosed script"));
    assert!(!result.contains("content"));
}
