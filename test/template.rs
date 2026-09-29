
use super::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_simple_template_rendering() {
    let dir = tempdir().unwrap();
    let templates_path = dir.path().to_str().unwrap();

    // Create a test template
    let template_content = "<h1>{{title}}</h1><p>{{content}}</p>";
    fs::write(dir.path().join("test.html"), template_content).unwrap();

    let engine = TemplateEngine::new(templates_path);
    let mut context = HashMap::new();
    context.insert("title".to_string(), "Hello World".to_string());
    context.insert("content".to_string(), "This is content".to_string());

    let result = engine.render("test", &context).unwrap();
    assert_eq!(result, "<h1>Hello World</h1><p>This is content</p>");
}

#[test]
fn test_template_with_missing_variables() {
    let dir = tempdir().unwrap();
    let templates_path = dir.path().to_str().unwrap();

    let template_content = "<h1>{{title}}</h1><p>{{missing}}</p>";
    fs::write(dir.path().join("test.html"), template_content).unwrap();

    let engine = TemplateEngine::new(templates_path);
    let mut context = HashMap::new();
    context.insert("title".to_string(), "Hello".to_string());

    let result = engine.render("test", &context).unwrap();
    assert_eq!(result, "<h1>Hello</h1><p>{{missing}}</p>");
}

#[test]
fn test_render_with_defaults() {
    let dir = tempdir().unwrap();
    let templates_path = dir.path().to_str().unwrap();

    let template_content = "<title>{{title}}</title><div>{{content}}</div>";
    fs::write(dir.path().join("page.html"), template_content).unwrap();

    let engine = TemplateEngine::new(templates_path);
    let mut context = HashMap::new();
    context.insert("content".to_string(), "Custom content".to_string());

    let result = engine.render_with_defaults("page", &context).unwrap();
    assert_eq!(result, "<title>Nonograph</title><div>Custom content</div>");
}

#[test]
fn test_html_escaping_not_performed() {
    let dir = tempdir().unwrap();
    let templates_path = dir.path().to_str().unwrap();

    let template_content = "<div>{{content}}</div>";
    fs::write(dir.path().join("test.html"), template_content).unwrap();

    let engine = TemplateEngine::new(templates_path);
    let mut context = HashMap::new();
    context.insert(
        "content".to_string(),
        "<script>alert('xss')</script>".to_string(),
    );

    let result = engine.render("test", &context).unwrap();
    // Note: Our simple template engine doesn't escape HTML - this will be handled by ammonia
    assert_eq!(result, "<div><script>alert('xss')</script></div>");
}

#[test]
fn test_strip_module_exports_and_line_comments() {
    let source = "\
// full line comment
  // indented full line comment
const re = /https:\\/\\//g; // trailing comment stays
const url = \"https://example.com\";
export { Thing };
  export default Thing;
const x = 1;";

    let result = strip_module_exports_and_line_comments(source);

    // Full-line comments (including indented ones) are dropped.
    assert!(!result.contains("full line comment"));
    assert!(!result.contains("indented full line comment"));
    // Top-level exports are dropped.
    assert!(!result.contains("export"));
    // Code with `//` inside a regex or string is preserved verbatim.
    assert!(result.contains("const re = /https:\\/\\//g; // trailing comment stays"));
    assert!(result.contains("const url = \"https://example.com\";"));
    assert!(result.contains("const x = 1;"));
}

#[test]
fn test_values_are_not_scanned_for_placeholders() {
    let dir = tempdir().unwrap();
    let templates_path = dir.path().to_str().unwrap();
    let template_content = "<h1>{{title}}</h1><div>{{content}}</div><script type=\"application/json\">{{raw_post_json}}</script>";
    fs::write(dir.path().join("post.html"), template_content).unwrap();

    let engine = TemplateEngine::new(templates_path);
    let mut context = HashMap::new();
    context.insert("title".to_string(), "Real title".to_string());
    context.insert(
        "content".to_string(),
        "<p>See {{title}} and {{content}}</p>".to_string(),
    );
    context.insert(
        "raw_post_json".to_string(),
        r#""See {{title}} and {{parser_js_path}}""#.to_string(),
    );

    let result = engine.render("post", &context).unwrap();
    assert_eq!(
            result,
            "<h1>Real title</h1><div><p>See {{title}} and {{content}}</p></div><script type=\"application/json\">\"See {{title}} and {{parser_js_path}}\"</script>"
        );
}
