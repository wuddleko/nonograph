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
fn template_is_read_once() {
    let dir = tempdir().unwrap();
    let templates_path = dir.path().to_str().unwrap();
    fs::write(dir.path().join("page.html"), "<p>{{title}}</p>").unwrap();

    let engine = TemplateEngine::new(templates_path);
    let mut context = HashMap::new();
    context.insert("title".to_string(), "One".to_string());
    assert_eq!(engine.render("page", &context).unwrap(), "<p>One</p>");

    fs::write(dir.path().join("page.html"), "<p>changed {{title}}</p>").unwrap();
    assert_eq!(engine.render("page", &context).unwrap(), "<p>One</p>");
}

#[test]
fn script_placeholder_is_not_escaped() {
    let dir = tempdir().unwrap();
    let templates_path = dir.path().to_str().unwrap();
    fs::write(dir.path().join("page.html"), "<body>{{scripts}}</body>").unwrap();

    let engine = TemplateEngine::new(templates_path);
    let mut context = HashMap::new();
    context.insert(
        "scripts".to_string(),
        "<script src=\"/home.js\"></script>".to_string(),
    );
    assert_eq!(
        engine.render("page", &context).unwrap(),
        "<body><script src=\"/home.js\"></script></body>"
    );
}

#[test]
fn test_values_are_not_scanned_for_placeholders() {
    let dir = tempdir().unwrap();
    let templates_path = dir.path().to_str().unwrap();
    let template_content = "<h1>{{title}}</h1><div>{{content}}</div>";
    fs::write(dir.path().join("post.html"), template_content).unwrap();

    let engine = TemplateEngine::new(templates_path);
    let mut context = HashMap::new();
    context.insert("title".to_string(), "Real title".to_string());
    context.insert(
        "content".to_string(),
        "<p>See {{title}} and {{content}}</p>".to_string(),
    );

    let result = engine.render("post", &context).unwrap();
    assert_eq!(
        result,
        "<h1>Real title</h1><div><p>See {{title}} and {{content}}</p></div>"
    );
}
