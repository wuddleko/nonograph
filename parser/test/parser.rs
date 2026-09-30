use super::*;

#[test]
fn test_basic_formatting() {
    assert_eq!(
        render_markdown("*italic* text").contains("<em>italic</em>"),
        true
    );
    assert_eq!(
        render_markdown("**bold** text").contains("<strong>bold</strong>"),
        true
    );
    assert_eq!(
        render_markdown("_underline_ text").contains("<u>underline</u>"),
        true
    );
    assert_eq!(
        render_markdown("==highlight== text").contains("<mark>highlight</mark>"),
        true
    );
}

#[test]
fn test_highlighting_with_mixed_formatting() {
    // Test highlighting mixed with other formatting
    let mixed = "This has ==highlighted== text with **bold** and *italic* formatting.";
    let result = render_markdown(mixed);
    assert!(result.contains("<mark>highlighted</mark>"));
    assert!(result.contains("<strong>bold</strong>"));
    assert!(result.contains("<em>italic</em>"));

    // Test nested highlighting scenarios
    let complex = "==This is ==nested== highlighting== and normal text.";
    let complex_result = render_markdown(complex);
    assert!(complex_result.contains("<mark>"));
}

#[test]
fn test_unicode_handling() {
    let japanese = "渋い美しさ *bold* text";
    let result = render_markdown(japanese);
    assert!(result.contains("渋い美しさ"));
    assert!(result.contains("<em>bold</em>"));
}

#[test]
fn test_links() {
    let text = "[Google](https://google.com)";
    let result = render_markdown(text);

    assert!(result.contains("<a href=\"https://google.com\""));
    assert!(result.contains("target=\"_blank\""));
    assert!(result.contains("rel=\"noopener noreferrer\""));
    assert!(result.contains(">Google</a>"));
}

#[test]
fn render_options_reach_links_and_images() {
    let config = RenderOptions {
        max_url_length: 12,
        external_link_security: false,
        ..RenderOptions::default()
    };

    let short = render_markdown_with_config("[Go](https://a.co)", &config);
    assert!(short.contains("href=\"https://a.co\""));
    assert!(short.contains(">Go</a>"));
    assert!(!short.contains("target=\"_blank\""));

    let long = render_markdown_with_config("[Go](https://example.com/long)", &config);
    assert!(!long.contains("<a "));
    assert!(long.contains("https://example.com/long"));

    let image = render_markdown_with_config("![alt](https://example.com/long.png)", &config);
    assert!(!image.contains("<img"));
    assert!(image.contains("https://example.com/long.png"));
}

#[test]
fn breaks_before_tables_collapse_in_one_pass() {
    assert_eq!(
        clear_breaks_before_tables("<br><br>keep".to_string()),
        "<br><br>keep"
    );
    assert_eq!(
        clear_breaks_before_tables("<br><br><br><br>x<br><table>".to_string()),
        "<br><br>x<table>"
    );
    assert_eq!(
        clear_breaks_before_tables("<br> <br><table>".to_string()),
        "<br> <table>"
    );
    assert_eq!(
        clear_breaks_before_tables("<br>\n<table>".to_string()),
        "<br>\n<table>"
    );
}

#[test]
fn test_simple_links() {
    let text = "[https://example.com]";
    let result = render_markdown(text);

    assert!(result.contains("<a href=\"https://example.com\""));
    assert!(result.contains("target=\"_blank\""));
    assert!(result.contains("rel=\"noopener noreferrer\""));
    assert!(result.contains(">https://example.com</a>"));
}

#[test]
fn test_urls_with_underscores() {
    let text = "[Article](https://unsigned.io/articles/2022_02_02_surveillance-and-your-soul.html)";
    let result = render_markdown(text);
    assert!(result.contains(
        "href=\"https://unsigned.io/articles/2022_02_02_surveillance-and-your-soul.html\""
    ));
    assert!(!result.contains("<u>"));
    assert!(!result.contains("</u>"));

    let text2 = "[https://example.com/some_path_with_underscores]";
    let result2 = render_markdown(text2);
    assert!(result2.contains("href=\"https://example.com/some_path_with_underscores\""));
    assert!(!result2.contains("<u>"));

    let text3 = "[my_link_text](https://example.com)";
    let result3 = render_markdown(text3);
    assert!(result3.contains(">my_link_text</a>"));
    assert!(!result3.contains("<u>"));
}

#[test]
fn test_secret_text() {
    let text = "#secret message#";
    let result = render_markdown(text);
    assert!(result.contains("<span class=\"secret\">secret message</span>"));
}

#[test]
fn test_code() {
    let text = "`code block`";
    let result = render_markdown(text);
    assert!(result.contains("<code>code block</code>"));
}

#[test]
fn test_code_literal_rendering() {
    // Test that markdown inside code blocks is not processed
    let text = "`(link text)[https://example.com]`";
    let result = render_markdown(text);

    assert!(result.contains("<code>(link text)[https://example.com]</code>"));
    assert!(!result.contains("<a href"));

    // Test with various markdown elements inside code
    let text2 = "`*bold* **italic** _underline_ ~strike~`";
    let result2 = render_markdown(text2);

    assert!(result2.contains("<code>*bold* **italic** _underline_ ~strike~</code>"));
    assert!(!result2.contains("<strong>"));
    assert!(!result2.contains("<em>"));
    assert!(!result2.contains("<u>"));
    assert!(!result2.contains("<del>"));

    // Test code block mixed with regular markdown
    let text3 = "This is *bold* and `this is code with *asterisks*` and more *bold*.";
    let result3 = render_markdown(text3);
    assert!(result3.contains("<em>bold</em>"));
    assert!(result3.contains("<code>this is code with *asterisks*</code>"));
    // The asterisks inside code should NOT become <strong> tags
    assert!(!result3.contains("<code>this is code with <em>asterisks</em></code>"));

    // Test adjacent backticks with content
    let text4 = "Test `first` and `second` code blocks";
    let result4 = render_markdown(text4);
    assert!(result4.contains("<code>first</code>"));
    assert!(result4.contains("<code>second</code>"));
}

#[test]
fn test_inline_code_on_separate_lines_gets_line_breaks() {
    let text = "`int`\n`fuck`\n`whatever`";
    let result = render_markdown(text);

    assert!(result.contains("<code>int</code>"));
    assert!(result.contains("<code>fuck</code>"));
    assert!(result.contains("<code>whatever</code>"));
    assert!(result.contains("<code>int</code><br><code>fuck</code>"));
    assert!(result.contains("<code>fuck</code><br><code>whatever</code>"));
    assert!(!result.contains("<code>int</code><code>fuck</code>"));
}

#[test]
fn test_mixed_text_and_inline_code_lines_get_line_breaks() {
    let text = "B\nC\n`int`\n`fuck`";
    let result = render_markdown(text);

    assert!(result.contains("B<br>C<br><code>int</code><br><code>fuck</code>"));
}

#[test]
fn test_malicious_inline_code_on_separate_lines_is_neutralized() {
    let text = "`<script>alert('xss')</script>`\n`<img src=x onerror=alert(1)>`\n`</code><script>alert('escape')</script><code>`";
    let result = render_markdown(text);

    assert!(result.contains("<code>&lt;script&gt;alert('xss')&lt;/script&gt;</code>"));
    assert!(result.contains("<code>&lt;img src=x onerror=alert(1)&gt;</code>"));
    assert!(result.contains(
        "<code>&lt;/code&gt;&lt;script&gt;alert('escape')&lt;/script&gt;&lt;code&gt;</code>"
    ));

    assert!(!result.contains("<script>"));
    assert!(!result.contains("<img "));

    assert!(result.matches("<br>").count() >= 2);
}

#[test]
fn test_markdown_inside_inline_code_on_lines_stays_literal() {
    let text = "`**not bold**`\n`[not a link](https://example.com)`\n`# not a header`";
    let result = render_markdown(text);

    assert!(result.contains("<code>**not bold**</code>"));
    assert!(result.contains("<code>[not a link](https://example.com)</code>"));
    assert!(result.contains("<code># not a header</code>"));
    assert!(!result.contains("<strong>"));
    assert!(!result.contains("<a href"));
    assert!(!result.contains("<h1"));
}

#[test]
fn test_fenced_code_blocks() {
    // Test basic fenced code block
    let text = "```json\n{\"key\": \"value\"}\n```";
    let result = render_markdown(text);
    assert_code_block_structure(&result, Some("json"), true);
    assert!(result.contains("key"));
    assert!(result.contains("value"));

    // Test Python code block
    let text_py = "```py\nprint('hello world')\n```";
    let result_py = render_markdown(text_py);
    assert_code_block_structure(&result_py, Some("python"), true);
    assert!(result_py.contains("print"));
    assert!(result_py.contains("hello world"));

    // Test JavaScript code block
    let text_js = "```js\nconsole.log('hello');\n```";
    let result_js = render_markdown(text_js);
    assert_code_block_structure(&result_js, Some("javascript"), true);
    assert!(result_js.contains("console"));
    assert!(result_js.contains("log"));
    assert!(result_js.contains("hello"));

    // Test code block without language
    let text_no_lang = "```\nsome code\n```";
    let result_no_lang = render_markdown(text_no_lang);
    assert_code_block_structure(&result_no_lang, None, true);
    assert!(result_no_lang.contains("some code"));
    assert!(!result_no_lang.contains("code-language"));

    // Test multiline code block
    let text_multi = "```rust\nfn main() {\n    println!(\"Hello, world!\");\n}\n```";
    let result_multi = render_markdown(text_multi);
    assert_code_block_structure(&result_multi, Some("rust"), true);
    assert!(result_multi.contains("fn"));
    assert!(result_multi.contains("main"));
    assert!(result_multi.contains("println"));
}

#[test]
fn test_fenced_code_blocks_no_markdown_processing() {
    // Test that markdown inside fenced code blocks is not processed
    let text =
        "```js\nconst message = \"*bold* **italic** _underline_\";\nconsole.log(message);\n```";
    let result = render_markdown(text);

    // Should contain raw markdown characters, not HTML tags
    assert!(result.contains("*bold* **italic** _underline_"));
    assert!(!result.contains("<strong>"));
    assert!(!result.contains("<em>"));
    assert!(!result.contains("<u>"));

    // Test with various markdown elements
    let complex = "```python\n# This is *not* **processed**\ndef test():\n    print(\"[not a link](http://example.com)\")\n    return ~strikethrough~ and ^superscript^\n```";
    let complex_result = render_markdown(complex);

    assert!(complex_result.contains("*not* **processed**"));
    assert!(complex_result.contains("[not a link](http://example.com)"));
    assert!(complex_result.contains("~strikethrough~"));
    assert!(complex_result.contains("^superscript^"));
    assert!(!complex_result.contains("<a href"));
    assert!(!complex_result.contains("<del>"));
    assert!(!complex_result.contains("<sup>"));
}

#[test]
fn test_sanitize_text() {
    let malicious_text = "<script>alert('xss')</script>Hello World";
    let sanitized = sanitize_text(&malicious_text);
    assert_eq!(sanitized, "Hello World");

    let various_tags = "<b>Bold</b><i>Italic</i><script>alert('xss')</script>Clean Text";
    let sanitized_tags = sanitize_text(&various_tags);
    assert_eq!(sanitized_tags, "BoldItalicClean Text");

    let clean_text = "Just normal text";
    let sanitized_clean = sanitize_text(&clean_text);
    assert_eq!(sanitized_clean, "Just normal text");

    // Test long text (no truncation)
    let long_text = "This is a very long text that should be truncated";
    let sanitized = sanitize_text(&long_text);
    assert_eq!(
        sanitized,
        "This is a very long text that should be truncated"
    );

    assert_eq!(sanitize_text("Line1\nLine2"), "Line1 Line2");
    assert_eq!(sanitize_text("Line1\r\nLine2"), "Line1 Line2");
    assert_eq!(sanitize_text("A\n\nB"), "A  B");
    assert_eq!(sanitize_text("No newlines"), "No newlines");

    assert_eq!(sanitize_text("Tom & Jerry"), "Tom & Jerry");
    assert_eq!(sanitize_text("5 > 3 and 2 < 4"), "5 > 3 and 2 < 4");
    assert_eq!(sanitize_text("She said \"hello\""), "She said \"hello\"");
    assert_eq!(sanitize_text("O'Brien"), "O'Brien");
    assert_eq!(sanitize_text("100%"), "100%");

    assert_eq!(sanitize_text("§1.2 Legal"), "§1.2 Legal");
    assert_eq!(sanitize_text("Δx = 5"), "Δx = 5");
    assert_eq!(sanitize_text("∑∏∫√∞≈≠≤≥"), "∑∏∫√∞≈≠≤≥");
    assert_eq!(sanitize_text("£100 €200 ¥300 ₹400"), "£100 €200 ¥300 ₹400");
    assert_eq!(sanitize_text("© 2024 ® ™"), "© 2024 ® ™");
    assert_eq!(sanitize_text("α β γ δ ε ζ η θ"), "α β γ δ ε ζ η θ");
    assert_eq!(sanitize_text("° ± × ÷ µ ¶ · ¿ ¡"), "° ± × ÷ µ ¶ · ¿ ¡");
    assert_eq!(sanitize_text("† ‡ • … — – ‹ › « »"), "† ‡ • … — – ‹ › « »");
    assert_eq!(sanitize_text("π ≈ 3.14159"), "π ≈ 3.14159");
    assert_eq!(sanitize_text("f(x) = x² + 2x + 1"), "f(x) = x² + 2x + 1");
    assert_eq!(sanitize_text("∀x ∈ ℝ: x² ≥ 0"), "∀x ∈ ℝ: x² ≥ 0");
    assert_eq!(sanitize_text("¼ ½ ¾ ⅓ ⅔"), "¼ ½ ¾ ⅓ ⅔");
    assert_eq!(sanitize_text("→ ← ↑ ↓ ↔ ⇒ ⇐"), "→ ← ↑ ↓ ↔ ⇒ ⇐");
    assert_eq!(sanitize_text("♠ ♣ ♥ ♦ ♩ ♪ ♫"), "♠ ♣ ♥ ♦ ♩ ♪ ♫");
    assert_eq!(sanitize_text("☀ ☁ ☂ ★ ☆ ✓ ✗"), "☀ ☁ ☂ ★ ☆ ✓ ✗");
}

#[test]
fn test_sanitize_language() {
    assert_eq!(sanitize_language("javascript"), "javascript");
    assert_eq!(sanitize_language("c++"), "c++");
    assert_eq!(sanitize_language("c#"), "c#");
    assert_eq!(sanitize_language("f#"), "f#");
    assert_eq!(sanitize_language("objective-c"), "objective-c");
    assert_eq!(sanitize_language("emacs-lisp"), "emacs-lisp");

    assert_eq!(
        sanitize_language("<script>alert('xss')</script>"),
        "scriptalertxsss"
    );
    assert_eq!(sanitize_language("python; rm -rf /"), "pythonrm-rf");
    assert_eq!(
        sanitize_language("js</style><script>evil()</script>"),
        "jsstylescriptev"
    );
    assert_eq!(
        sanitize_language("bash && curl evil.com"),
        "bashcurlevilcom"
    );
    assert_eq!(sanitize_language("JAVASCRIPT"), "javascript");

    assert_eq!(sanitize_language(""), "");
    assert_eq!(sanitize_language("   python   "), "python");
    assert_eq!(sanitize_language("python3.9"), "python39");

    // Test truncation
    assert_eq!(
        sanitize_language("verylonglanguagename12345"),
        "verylonglanguag"
    );
    assert_eq!(
        sanitize_language("superlonglanguagename"),
        "superlonglangua"
    );
}

#[test]
fn test_malicious_language_code_blocks() {
    let malicious_code = r#"```<script>alert('xss')</script>
console.log('test');
```"#;
    let result = render_markdown(malicious_code);
    assert_code_block_structure(&result, Some("scriptalertxsss"), true);
    assert!(result.contains("console"));
    assert!(result.contains("log"));
    assert!(result.contains("test"));
    assert!(!result.contains("<script>"));

    let injection_code = r#"```python; rm -rf /
print("hello")
```"#;
    let result2 = render_markdown(injection_code);
    assert_code_block_structure(&result2, Some("pythonrm-rf"), true);
    assert!(result2.contains("print"));
    assert!(result2.contains("hello"));
    assert!(!result2.contains("rm -rf"));

    let html_injection = r#"```js</style><script>evil()
var x = 1;
```"#;
    let result3 = render_markdown(html_injection);
    assert_code_block_structure(&result3, Some("jsstylescriptev"), true);
    assert!(result3.contains("var"));
    assert!(result3.contains("x"));
    assert!(!result3.contains("</style>"));
    assert!(!result3.contains("<script>evil()"));
}

#[test]
fn test_legitimate_special_character_languages() {
    let valid_langs = [
        ("c++", "cpp"),
        ("c#", "csharp"),
        ("f#", "fsharp"),
        ("objective-c", "objective-c"),
        ("emacs-lisp", "emacs-lisp"),
    ];

    for (input, mapped_output) in valid_langs {
        let sanitized = sanitize_language(input);
        assert!(!sanitized.contains("<"));
        assert!(!sanitized.contains(">"));

        let code = format!("```{}\ntest code\n```", input);
        let result = render_markdown(&code);
        assert!(result.contains(&format!("code-language\">{}", mapped_output.to_uppercase())));
    }
}

#[test]
fn test_theme_configuration() {
    // Test with valid theme
    let config = RenderOptions {
        syntax_theme: "Solarized (light)".to_string(),
        ..RenderOptions::default()
    };

    let code = "```rust\nlet x = 5;\n```";
    let result = render_markdown_with_config(code, &config);
    assert_code_block_structure(&result, Some("rust"), true);
    assert!(result.contains("let"));
    assert!(result.contains("x"));
    assert!(result.contains("5"));

    // Test with invalid theme (should fall back to default)
    let invalid_config = RenderOptions {
        syntax_theme: "NonExistentTheme".to_string(),
        ..RenderOptions::default()
    };

    let invalid_result = render_markdown_with_config(code, &invalid_config);
    assert_code_block_structure(&invalid_result, Some("rust"), true);
    assert!(invalid_result.contains("let"));
    assert!(invalid_result.contains("x"));
    assert!(invalid_result.contains("5"));
}

#[test]
fn test_markdown_lists() {
    // Test unordered lists (bulleted)
    let bulleted = "- First item\n- Second item\n- Third item";
    let result = render_markdown(bulleted);
    assert!(result.contains("<ul>"));
    assert!(result.contains("<li>First item</li>"));
    assert!(result.contains("<li>Second item</li>"));
    assert!(result.contains("<li>Third item</li>"));
    assert!(result.contains("</ul>"));

    // Test ordered lists (numbered)
    let numbered = "1. First step\n2. Second step\n3. Third step";
    let numbered_result = render_markdown(numbered);
    assert!(numbered_result.contains("<ol>"));
    assert!(numbered_result.contains("<li>First step</li>"));
    assert!(numbered_result.contains("<li>Second step</li>"));
    assert!(numbered_result.contains("<li>Third step</li>"));
    assert!(numbered_result.contains("</ol>"));

    // Test mixed list markers (*, +, -)
    let mixed = "* Asterisk item\n+ Plus item\n- Dash item";
    let mixed_result = render_markdown(mixed);
    assert!(mixed_result.contains("<ul>"));
    assert!(mixed_result.contains("<li>Asterisk item</li>"));
    assert!(mixed_result.contains("<li>Plus item</li>"));
    assert!(mixed_result.contains("<li>Dash item</li>"));

    // Test that regular text is not affected
    let not_list = "This is just text\nWith multiple lines\nBut no list markers";
    let not_list_result = render_markdown(not_list);
    assert!(!not_list_result.contains("<ul>"));
    assert!(!not_list_result.contains("<ol>"));
    assert!(!not_list_result.contains("<li>"));
}

#[test]
fn test_markdown_task_lists() {
    let tasks = "- [ ] Todo item\n- [x] Done item\n- [X] Also done";
    let result = render_markdown(tasks);
    assert!(result.contains("<ul class=\"contains-task-list\">"));
    assert!(result.contains("<li class=\"task-list-item\">"));
    assert!(result.contains("<input type=\"checkbox\"> Todo item"));
    assert!(result.contains("<input type=\"checkbox\" checked=\"\"> Done item"));
    assert!(result.contains("<input type=\"checkbox\" checked=\"\"> Also done"));

    let plain = "- Regular item";
    let plain_result = render_markdown(plain);
    assert!(plain_result.contains("<ul>"));
    assert!(!plain_result.contains("task-list-item"));
    assert!(!plain_result.contains("checkbox"));

    let bracketed = "- [abc] item";
    let bracketed_result = render_markdown(bracketed);
    assert!(!bracketed_result.contains("<input"));
    assert!(bracketed_result.contains("<li>[abc] item</li>"));
}

#[test]
fn test_task_list_marker_validation() {
    // Valid: `- [ ]` and `- [x]` produce checkboxes.
    for valid in ["- [ ] test", "- [x] test"] {
        let out = render_markdown(valid);
        assert!(out.contains("<input"), "expected checkbox for {valid:?}");
    }

    // Invalid: no space after the dash means it isn't even a list item.
    for not_a_list in ["-[] test", "-[]test"] {
        let out = render_markdown(not_a_list);
        assert!(!out.contains("<li"), "{not_a_list:?} must not be a list");
        assert!(!out.contains("<input"));
    }

    // Invalid: empty brackets / missing space are plain list items, no checkbox.
    for no_checkbox in ["- [] test", "- []test"] {
        let out = render_markdown(no_checkbox);
        assert!(
            out.contains("<li>"),
            "{no_checkbox:?} should be a plain item"
        );
        assert!(
            !out.contains("<input"),
            "{no_checkbox:?} must not have a checkbox"
        );
    }
}

#[test]
fn test_nested_task_lists() {
    // Nested checkboxes work like nested lists: indentation creates a child <ul>.
    let input = "- [ ] Parent\n  - [x] Child done\n  - [ ] Child todo";
    let result = render_markdown(input);

    // Outer and inner lists both carry the task-list class.
    assert_eq!(
        result.matches("<ul class=\"contains-task-list\">").count(),
        2
    );
    assert!(result.contains("<input type=\"checkbox\"> Parent"));
    assert!(result.contains("<input type=\"checkbox\" checked=\"\"> Child done"));
    assert!(result.contains("<input type=\"checkbox\"> Child todo"));

    // A task list can nest a plain bullet list and vice versa.
    let mixed = "- [ ] Task\n  - Plain child";
    let mixed_result = render_markdown(mixed);
    assert!(mixed_result.contains("<input"));
    assert!(mixed_result.contains("<li>Plain child</li>"));
}

#[test]
fn test_nested_ordered_lists() {
    let input = "1. This is neat\n2. Hello world\n  3. nesting is cool\n  4. what else can\n5. one do with nesting?\n  6. Not sure";
    let result = render_markdown(input);

    assert!(result.contains("<li>This is neat</li>"));
    assert!(result.contains("<li>one do with nesting?<ol>"));
    assert!(result.contains("<li>Hello world<ol>"));
    assert!(result.contains("<li>nesting is cool</li>"));
    assert!(result.contains("<li>what else can</li>"));
    assert!(result.contains("<li>Not sure</li>"));

    assert!(result
        .contains("<li>Hello world<ol><li>nesting is cool</li><li>what else can</li></ol></li>"));
    assert!(result.contains("<li>one do with nesting?<ol><li>Not sure</li></ol></li>"));
}

#[test]
fn test_nested_bulleted_lists() {
    let input = "* this is neat\n  * wow you can nest these too!\n  * woah\n* huh nested bullets!\n* whats up with that\n  * yay!\n  * wow";
    let result = render_markdown(input);

    assert!(result.contains(
        "<li>this is neat<ul><li>wow you can nest these too!</li><li>woah</li></ul></li>"
    ));
    assert!(result.contains("<li>huh nested bullets!</li>"));
    assert!(result.contains("<li>whats up with that<ul><li>yay!</li><li>wow</li></ul></li>"));
}

#[test]
fn test_deeply_nested_lists() {
    let input = "- a\n  - b\n    - c\n  - d\n- e";
    let result = render_markdown(input);
    assert!(result.contains("<li>a<ul><li>b<ul><li>c</li></ul></li><li>d</li></ul></li>"));
    assert!(result.contains("<li>e</li>"));
}

#[test]
fn test_html_list_sanitization() {
    // Test that raw HTML lists are escaped/removed
    let raw_html = "<li><strong>Input parsing:</strong> The system breaks down.</li>\n<li><strong>Character matching:</strong> For each syllable.</li>";
    let result = render_markdown(raw_html);

    // Should not contain raw <li> tags (they should be escaped)
    // The content should be treated as plain text
    assert!(result.contains("Input parsing"));
    assert!(result.contains("Character matching"));
    assert!(result.contains("&lt;li&gt;"));
    assert!(result.contains("&lt;/li&gt;"));

    // Should not create HTML lists from raw HTML input
    assert!(!result.contains("<li><strong>Input parsing:</strong>"));
}

#[test]
fn test_bold_formatting_debug() {
    // Test to debug bold formatting issue
    let bold_test = "**test**";
    let result = render_markdown(bold_test);
    println!("Bold input: {}", bold_test);
    println!("Bold output: {}", result);
    assert!(result.contains("<strong>test</strong>"));
}

#[test]
fn test_bold_in_lists() {
    // Test bold formatting inside lists
    let bold_in_list = "- This is **bold** text in a list\n- Another **bold** item";
    let result = render_markdown(bold_in_list);
    println!("Bold in list input: {}", bold_in_list);
    println!("Bold in list result: {}", result);

    assert!(result.contains("<ul>"));
    assert!(result.contains("<li>This is <strong>bold</strong> text in a list</li>"));
    assert!(result.contains("<li>Another <strong>bold</strong> item</li>"));
}

#[test]
fn test_formatting_in_lists() {
    // Test all formatting types inside lists
    let formatted_list = r#"- This is **bold** text
- This is *italic* text
- This is _underlined_ text
- This is ~strikethrough~ text
- This is ^superscript^ text
- This is ==highlighted== text
- This is `inline code` text
- This has [a link](https://example.com)

1. Numbered **bold** item
2. Numbered *italic* item
3. Mixed **bold** and *italic* formatting"#;

    let result = render_markdown(formatted_list);
    println!("Formatted list result: {}", result);

    // Test bulleted list formatting
    assert!(result.contains("<li>This is <strong>bold</strong> text</li>"));
    assert!(result.contains("<li>This is <em>italic</em> text</li>"));
    assert!(result.contains("<li>This is <u>underlined</u> text</li>"));
    assert!(result.contains("<li>This is <del>strikethrough</del> text</li>"));
    assert!(result.contains("<li>This is <sup>superscript</sup> text</li>"));
    assert!(result.contains("<li>This is <mark>highlighted</mark> text</li>"));
    assert!(result.contains("<li>This is <code>inline code</code> text</li>"));
    assert!(result.contains("<li>This has <a href=\"https://example.com\""));

    // Test numbered list formatting
    assert!(result.contains("<li>Numbered <strong>bold</strong> item</li>"));
    assert!(result.contains("<li>Numbered <em>italic</em> item</li>"));
    assert!(result.contains("<li>Mixed <strong>bold</strong> and <em>italic</em> formatting</li>"));
}

#[test]
fn test_list_edge_cases() {
    // Test empty list items
    let empty_items = "- \n- Item with content\n- ";
    let result = render_markdown(empty_items);
    assert!(result.contains("<ul>"));
    assert!(result.contains("<li></li>"));
    assert!(result.contains("<li>Item with content</li>"));

    // Test lists with blank lines
    let with_blanks = "1. First item\n\n2. Second item\n3. Third item";
    let blanks_result = render_markdown(with_blanks);
    assert!(blanks_result.contains("<ol>"));
    assert!(blanks_result.contains("<li>First item</li>"));
    assert!(blanks_result.contains("<li>Second item</li>"));

    // Test that non-list numbered text is not converted
    let not_numbered_list = "I have 1. item here but not 2. a list";
    let not_numbered_result = render_markdown(not_numbered_list);
    assert!(!not_numbered_result.contains("<ol>"));
    assert!(!not_numbered_result.contains("<li>"));
}

#[test]
fn test_comprehensive_language_mapping() {
    // Test popular language mappings
    let mappings = [
        ("py", "python"),
        ("js", "javascript"),
        ("ts", "typescript"),
        ("rs", "rust"),
        ("sh", "bash"),
        ("cpp", "cpp"),
        ("c++", "cpp"),
        ("c#", "csharp"),
        ("cs", "csharp"),
        ("rb", "ruby"),
        ("kt", "kotlin"),
        ("go", "go"),
        ("golang", "go"),
        ("yml", "yaml"),
        ("md", "markdown"),
        ("jsx", "jsx"),
        ("tsx", "tsx"),
        ("vue", "vue"),
        ("scss", "scss"),
        ("sass", "sass"),
        ("dockerfile", "dockerfile"),
        ("docker", "dockerfile"),
        ("makefile", "makefile"),
        ("make", "makefile"),
        ("hs", "haskell"),
        ("clj", "clojure"),
        ("ex", "elixir"),
        ("erl", "erlang"),
        ("ml", "ocaml"),
        ("jl", "julia"),
        ("cr", "crystal"),
        ("sol", "solidity"),
        ("asm", "assembly"),
        ("f90", "fortran"),
        ("cob", "cobol"),
        ("pas", "pascal"),
        ("st", "smalltalk"),
        ("elisp", "emacs-lisp"),
        ("purs", "purescript"),
        ("re", "reasonml"),
        ("ino", "arduino"),
        ("pde", "processing"),
        ("scad", "openscad"),
        ("tex", "latex"),
        ("bib", "bibtex"),
        ("rst", "restructuredtext"),
        ("adoc", "asciidoc"),
        ("txt", "plaintext"),
        ("text", "plaintext"),
        ("vlang", "v"),
    ];

    for (alias, expected) in mappings {
        let text = format!("```{}\ncode here\n```", alias);
        let result = render_markdown(&text);

        assert!(
            result.contains(&format!("code-language\">{}", expected.to_uppercase())),
            "Failed mapping: {} should map to {}",
            alias,
            expected
        );
    }
}

#[test]
fn test_code_block_line_count() {
    // Test single line code block
    let single_line = "```rust\nlet x = 5;\n```";
    let result_single = render_markdown(single_line);

    // Test the server-rendered structure
    assert_code_block_structure(&result_single, Some("rust"), true);
    assert!(result_single.contains("let"));
    assert!(result_single.contains("x"));
    assert!(result_single.contains("5"));

    // Test multi-line code block
    let multi_line = "```python\ndef hello():\n    print(\"world\")\n    return True\n```";
    let result_multi = render_markdown(multi_line);

    assert_code_block_structure(&result_multi, Some("python"), true);
    assert!(result_multi.contains("<span class=\"line-number\">1</span>"));
    assert!(result_multi.contains("<span class=\"line-number\">2</span>"));
    assert!(result_multi.contains("<span class=\"line-number\">3</span>"));
    assert!(result_multi.contains("def"));
    assert!(result_multi.contains("hello"));
    assert!(result_multi.contains("print"));
    assert!(result_multi.contains("world"));
    assert!(result_multi.contains("return"));
    assert!(result_multi.contains("True"));

    // Test code block without language
    let no_lang = "```\nsome code\nmore code\n```";
    let result_no_lang = render_markdown(no_lang);

    assert_code_block_structure(&result_no_lang, None, true);
    assert!(result_no_lang.contains("some code"));
    assert!(!result_no_lang.contains("code-language")); // Should be empty

    // Test that line count is properly generated (check line numbers)
    let large_code = "```rust\n".to_string() + &"println!(\"line\");\n".repeat(10) + "```";
    let result_large = render_markdown(&large_code);

    assert!(result_large.contains("<span class=\"line-number\">10</span>"));
    assert!(!result_large.contains("<span class=\"line-number\">11</span>"));
}

// Helper function for testing server-rendered code block structure
fn assert_code_block_structure(result: &str, language: Option<&str>, has_content: bool) {
    assert!(result.contains("<pre"));
    assert!(result.contains("code-header"));
    assert!(result.contains("code-controls"));
    assert!(result.contains("line-numbers"));
    assert!(result.contains("Copy"));
    assert!(result.contains("Wrap"));
    assert!(result.contains("Collapse"));

    if let Some(lang) = language {
        if !lang.is_empty() {
            assert!(result.contains(&format!("class=\"language-{}", lang)));
        }
    }

    if has_content {
        assert!(result.contains("code-wrapper"));
    }
}

#[test]
fn test_mixed_code_blocks() {
    // Test mixing fenced and inline code blocks
    let text = "Here's some `inline code` and a fenced block:\n```json\n{\"test\": true}\n```\nMore text with `more inline`.";
    let result = render_markdown(text);

    assert!(result.contains("<code>inline code</code>"));
    assert!(result.contains("<code>more inline</code>"));
    assert_code_block_structure(&result, Some("json"), true);
    assert!(result.contains("test"));
    assert!(result.contains("true"));
}

#[test]
fn test_fenced_vs_regular_markdown_processing() {
    // Test that shows the clear difference between processed and unprocessed markdown
    let mixed_content = r#"Regular text with **bold** and *italic* formatting.

```js
// This code has *bold* and **italic** but should NOT be processed
const message = "*not bold* and **not italic**";
console.log("[not a link](http://example.com)");
```

More regular text with _underline_ and ~strikethrough~."#;

    let result = render_markdown(mixed_content);

    // Regular text should be processed
    assert!(result.contains("<strong>bold</strong>"));
    assert!(result.contains("<em>italic</em>"));
    assert!(result.contains("<u>underline</u>"));
    assert!(result.contains("<del>strikethrough</del>"));

    // Code block content should be raw/unprocessed
    assert!(result.contains("*not bold* and **not italic**"));
    assert!(result.contains("[not a link](http://example.com)"));

    // Verify the code block doesn't contain processed HTML
    // With new structure, find the code-wrapper content
    let code_wrapper_start = result.find("code-wrapper").unwrap();
    let code_block_part = &result[code_wrapper_start..];
    let code_end = code_block_part.find("</div>").unwrap();
    let code_content = &code_block_part[..code_end];

    assert!(!code_content.contains("<strong>"));
    assert!(!code_content.contains("<em>"));
    assert!(!code_content.contains("<a href"));
}

#[test]
fn test_code_block_line_breaks() {
    let text = "Here's some text:\n```json\n{\"test\": true}\n```\nMore text after.";
    let result = render_markdown(text);

    assert!(result.contains("<p>Here's some text:</p>"));
    assert_code_block_structure(&result, Some("json"), true);
    assert!(result.contains("test"));
    assert!(result.contains("true"));
    assert!(result.contains("<p>More text after.</p>"));
    assert!(!result.contains("<br>\n<pre"));
    assert!(!result.contains("<br><pre"));
    assert!(!result.contains("</pre><br>\n"));
    assert!(!result.contains("</pre><br>"));

    // Test with multiple code blocks
    let multiple = "First block:\n```js\nconsole.log('test');\n```\nMiddle text.\n```py\nprint('hello')\n```\nEnd text.";
    let multiple_result = render_markdown(multiple);

    // Should not have unwanted <br> tags around any code block
    assert!(!multiple_result.contains("<br>\n<pre>"));
    assert!(!multiple_result.contains("<br><pre>"));
    assert!(!multiple_result.contains("</pre><br>\n"));
    assert!(!multiple_result.contains("</pre><br>"));

    // Test that regular paragraph breaks still work properly
    let with_paragraphs = "First paragraph.\n\nSecond paragraph.\n\n```js\nconsole.log('test');\n```\n\nThird paragraph.";
    let paragraph_result = render_markdown(with_paragraphs);

    // Should have proper paragraph structure
    assert!(paragraph_result.contains("<p>First paragraph.</p>"));
    assert!(paragraph_result.contains("<p>Second paragraph.</p>"));
    assert!(paragraph_result.contains("<p>Third paragraph.</p>"));
}

#[test]
fn test_fenced_code_block_structure() {
    let text = "```json\n{\"test\": true}\n```";
    let result = render_markdown(text);

    // Verify the new structure includes all components
    assert_code_block_structure(&result, Some("json"), true);
    assert!(result.contains("test"));
    assert!(result.contains("true"));
    assert!(result.contains("</code>"));
    assert!(result.contains("</pre>"));

    // Test that the JSON content is properly preserved
    assert!(!result.contains("&quot;")); // Should not be double-encoded
}

#[test]
fn test_media_embedding() {
    // Test CommonMark image syntax
    let image_text = "![Alt text](https://example.com/image.jpg)";
    let image_result = render_markdown(image_text);
    assert!(image_result.contains("<img src=\"https://example.com/image.jpg\" alt=\"Alt text\">"));

    // Test video syntax with caption
    let video_text = "![Video caption](https://example.com/video.mp4)";
    let video_result = render_markdown(video_text);
    assert!(video_result.contains("<div class=\"media-with-caption\">"));
    assert!(video_result.contains("<video controls"));
    assert!(video_result.contains("<source src=\"https://example.com/video.mp4\""));
    assert!(video_result.contains("<div class=\"media-caption\">Video caption</div>"));

    // Test video without caption
    let video_no_caption = "![](https://example.com/video.webm)";
    let video_no_caption_result = render_markdown(video_no_caption);
    assert!(video_no_caption_result.contains("<video controls"));
    assert!(video_no_caption_result.contains("<source src=\"https://example.com/video.webm\""));
    assert!(!video_no_caption_result.contains("<div class=\"media-caption\">"));
    assert!(!video_no_caption_result.contains("<div class=\"media-with-caption\">"));

    // Test image with empty alt text
    let empty_alt = "![](https://example.com/test.png)";
    let empty_alt_result = render_markdown(empty_alt);
    assert!(empty_alt_result.contains("<img src=\"https://example.com/test.png\" alt=\"\">"));
}

#[test]
fn test_commonmark_image_syntax() {
    // Test basic image syntax
    let basic = "![Alt text](https://example.com/image.jpg)";
    let basic_result = render_markdown(basic);
    assert!(basic_result.contains("<img src=\"https://example.com/image.jpg\" alt=\"Alt text\">"));

    // Test image with special characters in alt text
    let special_alt = "![My \"special\" image & test](https://example.com/test.png)";
    let special_result = render_markdown(special_alt);
    assert!(special_result.contains("alt=\"My &quot;special&quot; image &amp; test\""));

    // Test image with special characters in URL (sanitizer handles escaping)
    let special_url = "![Test](https://example.com/test<>&.png)";
    let url_result = render_markdown(special_url);
    assert!(url_result.contains("<img src="));
    assert!(url_result.contains("alt=\"Test\""));

    // Test multiple images in one text
    let multiple = "![First](https://example.com/1.jpg) and ![Second](https://example.com/2.png)";
    let multiple_result = render_markdown(multiple);
    assert!(multiple_result.contains("<img src=\"https://example.com/1.jpg\" alt=\"First\">"));
    assert!(multiple_result.contains("<img src=\"https://example.com/2.png\" alt=\"Second\">"));

    // Test image mixed with text
    let mixed = "Here is an image: ![Cool pic](https://example.com/cool.jpg) - isn't it nice?";
    let mixed_result = render_markdown(mixed);
    assert!(mixed_result.contains("<div class=\"media-with-caption\">"));
    assert!(mixed_result.contains("<img src=\"https://example.com/cool.jpg\" alt=\"Cool pic\">"));
    assert!(mixed_result.contains("<div class=\"media-caption\">Cool pic</div>"));

    // Test that incomplete syntax is not processed
    let incomplete1 = "![Alt text](no-closing-paren";
    let incomplete1_result = render_markdown(incomplete1);
    assert!(!incomplete1_result.contains("<img"));

    let incomplete2 = "![Alt text without url]";
    let incomplete2_result = render_markdown(incomplete2);
    assert!(!incomplete2_result.contains("<img"));

    // Test that images are processed before links (so ![text](url) doesn't become a link)
    let not_link = "![This should be an image](https://example.com/image.jpg)";
    let not_link_result = render_markdown(not_link);
    assert!(not_link_result.contains("<img"));
    assert!(!not_link_result.contains("<a href"));
}

#[test]
fn test_image_captions() {
    // Test image with alt text shows caption
    let with_alt = "![This is a caption](https://example.com/image.jpg)";
    let with_alt_result = render_markdown(with_alt);
    assert!(with_alt_result.contains("<div class=\"media-with-caption\">"));
    assert!(with_alt_result
        .contains("<img src=\"https://example.com/image.jpg\" alt=\"This is a caption\">"));
    assert!(with_alt_result.contains("<div class=\"media-caption\">This is a caption</div>"));

    // Test image without alt text shows no caption
    let no_alt = "![](https://example.com/image.jpg)";
    let no_alt_result = render_markdown(no_alt);
    assert!(no_alt_result.contains("<img src=\"https://example.com/image.jpg\" alt=\"\">"));
    assert!(!no_alt_result.contains("<div class=\"media-caption\">"));
    assert!(!no_alt_result.contains("<div class=\"media-with-caption\">"));

    // Test image with only whitespace alt text shows no caption
    let whitespace_alt = "![   ](https://example.com/image.jpg)";
    let whitespace_result = render_markdown(whitespace_alt);
    assert!(whitespace_result.contains("<img src=\"https://example.com/image.jpg\" alt=\"   \">"));
    assert!(!whitespace_result.contains("<div class=\"media-caption\">"));
    assert!(!whitespace_result.contains("<div class=\"media-with-caption\">"));

    // Test image with special characters in alt text
    let special_alt = "![My \"special\" image & test](https://example.com/image.jpg)";
    let special_result = render_markdown(special_alt);
    assert!(special_result.contains("<div class=\"media-with-caption\">"));
    assert!(special_result.contains("alt=\"My &quot;special&quot; image &amp; test\""));
    assert!(special_result
        .contains("<div class=\"media-caption\">My \"special\" image &amp; test</div>"));

    // Test multiple images with different alt text scenarios
    let multiple = "![First caption](img1.jpg) ![](img2.jpg) ![Third caption](img3.jpg)";
    let multiple_result = render_markdown(multiple);
    assert!(multiple_result.contains("<div class=\"media-caption\">First caption</div>"));
    assert!(multiple_result.contains("<div class=\"media-caption\">Third caption</div>"));
    // Count caption occurrences - should be exactly 2
    assert_eq!(
        multiple_result
            .matches("<div class=\"media-caption\">")
            .count(),
        2
    );
    // Count wrapper occurrences - should be exactly 2 (only images with alt text)
    assert_eq!(
        multiple_result
            .matches("<div class=\"media-with-caption\">")
            .count(),
        2
    );
}

#[test]
fn test_image_captions_demo() {
    // Demonstrate the image caption feature
    let demo_text = r#"
# Image Caption Demo

Here's an image with alt text that becomes a caption:
![A beautiful sunset over the mountains](https://example.com/sunset.jpg)

And here's an image without alt text (no caption):
![](https://example.com/no-caption.jpg)

Multiple images:
![First image](img1.jpg) ![Second image](img2.jpg)
"#;

    let result = render_markdown(demo_text);

    // Should have captions for images with alt text
    assert!(result.contains("<div class=\"media-with-caption\">"));
    assert!(
        result.contains("<div class=\"media-caption\">A beautiful sunset over the mountains</div>")
    );
    assert!(result.contains("<div class=\"media-caption\">First image</div>"));
    assert!(result.contains("<div class=\"media-caption\">Second image</div>"));

    // Should have regular img for image without alt text
    assert!(result.contains("<img src=\"https://example.com/no-caption.jpg\" alt=\"\">"));

    // Verify we have the right number of captions (3 images with alt text)
    assert_eq!(result.matches("<div class=\"media-caption\">").count(), 3);
    assert_eq!(
        result.matches("<div class=\"media-with-caption\">").count(),
        3
    );
}

#[test]
fn test_html_escape_function() {
    // Test the html_escape function directly
    let test_input = r#"<script>alert('xss')</script>"onclick""#;
    let escaped = html_escape(test_input);
    println!("Input: {}", test_input);
    println!("Escaped: {}", escaped);

    assert!(!escaped.contains("<script>"));
    assert!(escaped.contains("&lt;script&gt;"));
    assert!(escaped.contains("&quot;onclick&quot;"));
}

#[test]
fn test_image_alt_text_sanitization() {
    // Test that malicious alt text is properly sanitized in both alt attribute and caption
    let malicious_alt = r#"![<script>alert('xss')</script>](https://example.com/image.jpg)"#;
    let result = render_markdown(malicious_alt);

    // The caption should be properly escaped
    assert!(result.contains("<div class=\"media-with-caption\">"));
    assert!(result
        .contains("<div class=\"media-caption\">&lt;script&gt;alert('xss')&lt;/script&gt;</div>"));

    // Test with various dangerous characters
    let complex_alt = r#"![<img src=x onerror=alert(1)> & "quotes"](test.jpg)"#;
    let complex_result = render_markdown(complex_alt);

    // The caption content should be escaped (main security concern)
    assert!(complex_result.contains("&lt;img"));
    assert!(complex_result.contains("&amp;"));
    assert!(complex_result.contains("&quot;quotes&quot;"));
}

#[test]
fn test_image_captions_final_demo() {
    // Final demonstration of the image caption feature
    let input = r#"
Check out this image with a caption:
![A beautiful landscape with mountains and trees](https://example.com/landscape.jpg)

And this one without alt text (no caption):
![](https://example.com/no-alt.jpg)

Multiple images:
![First image](img1.jpg) and ![Second image](img2.jpg)
"#;

    let result = render_markdown(input);

    // Image with alt text gets a caption
    assert!(result.contains("<div class=\"media-with-caption\">"));
    assert!(result.contains("<img src=\"https://example.com/landscape.jpg\" alt=\"A beautiful landscape with mountains and trees\">"));
    assert!(result.contains(
        "<div class=\"media-caption\">A beautiful landscape with mountains and trees</div>"
    ));

    // Image without alt text gets no caption
    assert!(result.contains("<img src=\"https://example.com/no-alt.jpg\" alt=\"\">"));
    assert!(!result.contains("<div class=\"media-caption\"></div>"));

    // Multiple images with captions
    assert!(result.contains("<div class=\"media-caption\">First image</div>"));
    assert!(result.contains("<div class=\"media-caption\">Second image</div>"));

    // Verify caption count
    assert_eq!(result.matches("<div class=\"media-caption\">").count(), 3);
    assert_eq!(
        result.matches("<div class=\"media-with-caption\">").count(),
        3
    );
}

#[test]
fn test_image_caption_html_structure() {
    // Test the exact HTML structure produced
    let input = "![Test caption](image.jpg)";
    let result = render_markdown(input);

    // Should produce the wrapper div with both image and caption inside
    assert!(result.contains("<div class=\"media-with-caption\">"));
    assert!(result.contains("<img src=\"image.jpg\" alt=\"Test caption\">"));
    assert!(result.contains("<div class=\"media-caption\">Test caption</div>"));
    assert!(result.contains("</div></div>")); // Both closing divs

    // Test image without caption
    let no_caption = "![](image.jpg)";
    let no_caption_result = render_markdown(no_caption);
    assert!(!no_caption_result.contains("media-with-caption"));
    assert!(no_caption_result.contains("<img src=\"image.jpg\" alt=\"\">"));
}

#[test]
fn test_legacy_url_embedding_removed() {
    // Test that raw URLs no longer get auto-converted to media elements
    let raw_image_url = "Check this out: https://example.com/image.jpg";
    let result = render_markdown(raw_image_url);

    // Should NOT contain img tag (legacy behavior removed)
    assert!(!result.contains("<img"));

    // Should contain the raw URL as text
    assert!(result.contains("https://example.com/image.jpg"));

    // Test video URL
    let raw_video_url = "Watch this: https://example.com/video.mp4";
    let video_result = render_markdown(raw_video_url);

    // Should NOT contain video tag (legacy behavior removed)
    assert!(!video_result.contains("<video"));

    // Should contain the raw URL as text
    assert!(video_result.contains("https://example.com/video.mp4"));
}

#[test]
fn test_video_caption_functionality() {
    // Test various video formats with captions
    let formats = ["mp4", "webm", "ogg", "mov"];

    for format in &formats {
        let video_text = format!(
            "![My {} video](https://example.com/video.{})",
            format, format
        );
        let result = render_markdown(&video_text);

        assert!(result.contains("<div class=\"media-with-caption\">"));
        assert!(result.contains("<video controls"));
        assert!(result.contains(&format!("src=\"https://example.com/video.{}", format)));
        assert!(result.contains(&format!(
            "<div class=\"media-caption\">My {} video</div>",
            format
        )));
    }

    // Test video with special characters in caption
    let special_caption = r#"![My "special" video & test](https://example.com/video.mp4)"#;
    let special_result = render_markdown(special_caption);
    assert!(special_result
        .contains("<div class=\"media-caption\">My \"special\" video &amp; test</div>"));
}

#[test]
fn test_mixed_images_and_videos_with_captions() {
    // Test mixing images and videos with various caption scenarios
    let mixed_content = r#"
Here's an image with a caption:
![Beautiful landscape](https://example.com/image.jpg)

And a video with a caption:
![Awesome video](https://example.com/video.mp4)

Image without caption:
![](https://example.com/no-caption.png)

Video without caption:
![](https://example.com/silent.webm)

Multiple media in one paragraph:
![First image](img1.jpg) and ![First video](vid1.mp4)
"#;

    let result = render_markdown(mixed_content);

    // Check image with caption
    assert!(result.contains("<div class=\"media-with-caption\">"));
    assert!(
        result.contains("<img src=\"https://example.com/image.jpg\" alt=\"Beautiful landscape\">")
    );
    assert!(result.contains("<div class=\"media-caption\">Beautiful landscape</div>"));

    // Check video with caption
    assert!(result.contains("<video controls"));
    assert!(result.contains("<source src=\"https://example.com/video.mp4\""));
    assert!(result.contains("<div class=\"media-caption\">Awesome video</div>"));

    // Check image without caption (no wrapper)
    assert!(result.contains("<img src=\"https://example.com/no-caption.png\" alt=\"\">"));

    // Check video without caption (no wrapper)
    assert!(result.contains("<source src=\"https://example.com/silent.webm\""));

    // Verify correct number of wrappers (only for media with captions)
    assert_eq!(
        result.matches("<div class=\"media-with-caption\">").count(),
        4
    );
    assert_eq!(result.matches("<div class=\"media-caption\">").count(), 4);
}

#[test]
fn test_consistent_media_naming() {
    // Test that demonstrates the consistent media-* naming convention
    let mixed_media = r#"
Here's an image with caption:
![Beautiful photo](https://example.com/photo.jpg)

And a video with caption:
![Awesome clip](https://example.com/video.mp4)
"#;

    let result = render_markdown(mixed_media);

    // Both images and videos use consistent naming
    assert_eq!(
        result.matches("<div class=\"media-with-caption\">").count(),
        2
    );
    assert_eq!(result.matches("<div class=\"media-caption\">").count(), 2);

    // Check specific captions
    assert!(result.contains("<div class=\"media-caption\">Beautiful photo</div>"));
    assert!(result.contains("<div class=\"media-caption\">Awesome clip</div>"));

    // Check that both wrapper and caption classes are semantic and consistent
    assert!(result.contains("media-with-caption"));
    assert!(result.contains("media-caption"));
    assert!(!result.contains("image-caption"));
    assert!(!result.contains("img-with-caption"));
}

#[test]
fn test_header_edge_cases() {
    // Test headers followed by other block elements
    let edge_cases = r#"# Header Before Code
```rust
let x = 5;
```

## Header Before List
- Item 1
- Item 2

### Header Before Blockquote
> This is a quote

#### Header Before Table
| Col 1 | Col 2 |
|-------|-------|
| A     | B     |

# Header at End"#;

    let result = render_markdown(edge_cases);

    // Check that headers are processed correctly
    assert!(result.contains(
            "<h1 id=\"h1\">Header Before Code<a href=\"#h1\" class=\"header-anchor\" rel=\"noopener noreferrer\">#</a></h1>"
        ));
    assert!(result.contains(
            "<h2 id=\"h2\">Header Before List<a href=\"#h2\" class=\"header-anchor\" rel=\"noopener noreferrer\">#</a></h2>"
        ));
    assert!(result.contains(
            "<h3 id=\"h3\">Header Before Blockquote<a href=\"#h3\" class=\"header-anchor\" rel=\"noopener noreferrer\">#</a></h3>"
        ));
    assert!(result.contains(
            "<h4 id=\"h4\">Header Before Table<a href=\"#h4\" class=\"header-anchor\" rel=\"noopener noreferrer\">#</a></h4>"
        ));
    assert!(result.contains(
            "<h1 id=\"h5\">Header at End<a href=\"#h5\" class=\"header-anchor\" rel=\"noopener noreferrer\">#</a></h1>"
        ));

    // Check that the following elements are still processed correctly
    assert!(result.contains("<pre"));
    assert!(result.contains("let"));
    assert!(result.contains("x"));
    assert!(result.contains("5"));
    assert!(result.contains("<blockquote"));
    assert!(result.contains("<table"));

    // Verify headers are not inside other elements
    assert!(!result.contains("<p><h"));
    assert!(!result.contains("<blockquote><h"));
    assert!(!result.contains("<code><h"));
}

#[test]
fn test_footnotes() {
    // Test reference footnotes
    let reference_text = "This has a footnote[^1] and another[^2].\n\n[^1]: First footnote text.\n[^2]: Second footnote text.";
    let reference_result = render_markdown(reference_text);

    // Check that footnote references are created (link processor adds attributes)
    assert!(reference_result.contains("<sup><a href=\"#FN1\""));
    assert!(reference_result.contains("<sup><a href=\"#FN2\""));
    assert!(reference_result.contains(">1</a></sup>"));
    assert!(reference_result.contains(">2</a></sup>"));
    assert!(reference_result.contains("<div class=\"footnotes\">"));
    assert!(reference_result.contains("First footnote text."));
    assert!(reference_result.contains("Second footnote text."));
    assert!(reference_result.contains("href=\"#fnref1\""));
    assert!(reference_result.contains("href=\"#fnref2\""));

    // Test inline footnotes
    let inline_text = "This has an inline footnote^[This is inline] and another^[Second inline].";
    let inline_result = render_markdown(inline_text);

    // Check that inline footnotes are created
    assert!(inline_result.contains("<sup><a href=\"#ifn1\""));
    assert!(inline_result.contains("<sup><a href=\"#ifn2\""));
    assert!(inline_result.contains("This is inline"));
    assert!(inline_result.contains("Second inline"));
    assert!(inline_result.contains("href=\"#ifn1ref\""));
    assert!(inline_result.contains("href=\"#ifn2ref\""));

    // Test mixed footnotes
    let mixed_text = "Reference[^1] and inline^[Inline text].\n\n[^1]: Reference text.";
    let mixed_result = render_markdown(mixed_text);

    assert!(mixed_result.contains("href=\"#FN1\""));
    assert!(mixed_result.contains("href=\"#ifn1\""));
    assert!(mixed_result.contains("Reference text."));
    assert!(mixed_result.contains("Inline text"));

    // Test footnote without definition (should not be processed)
    let undefined_text = "This has undefined[^missing] footnote.";
    let undefined_result = render_markdown(undefined_text);

    assert!(!undefined_result.contains("<sup>"));
    assert!(undefined_result.contains("[^missing]"));
}

#[test]
fn test_footnote_integration() {
    // Test comprehensive footnote functionality with mixed content
    let complex_text = r#"# Document with Footnotes

This is a **bold** text with a reference footnote[^1] and some *italic* text.

Here's an inline footnote^[This is inline content with **formatting**] in the middle.

Another paragraph with multiple footnotes[^ref] and inline^[Another inline note].

> This blockquote also has a footnote[^quote].

```rust
// Code blocks should not process footnotes[^code]
let x = 42;
```

[^1]: First reference footnote with *formatting*.
[^ref]: Reference footnote with a [link](https://example.com).
[^quote]: Footnote from blockquote."#;

    let result = render_markdown(complex_text);

    // Check that reference footnotes work
    assert!(result.contains("href=\"#FN1\""));
    assert!(result.contains("href=\"#FN2\""));
    assert!(result.contains("href=\"#FN3\""));

    // Check that inline footnotes work
    assert!(result.contains("href=\"#ifn1\""));
    assert!(result.contains("href=\"#ifn2\""));

    // Check footnotes section exists
    assert!(result.contains("<div class=\"footnotes\">"));
    assert!(result.contains("First reference footnote"));
    assert!(result.contains("Reference footnote with a"));
    assert!(result.contains("Footnote from blockquote"));
    assert!(result.contains("This is inline content"));
    assert!(result.contains("Another inline note"));

    // Check that code blocks don't process footnotes
    assert!(result.contains("footnotes[^code]"));
    assert!(!result.contains("href=\"#FN4\""));

    // Check that other markdown still works (with correct formatting)
    assert!(result.contains("<h1 id=\"h1\">"));
    assert!(result.contains(
        "<a href=\"#h1\" class=\"header-anchor\" rel=\"noopener noreferrer\">#</a></h1>"
    ));
    assert!(result.contains("<strong>bold</strong>")); // ** is bold, * is italic in this parser
    assert!(result.contains("<em>italic</em>"));
    assert!(result.contains("<blockquote>"));
    assert!(result.contains("href=\"https://example.com\""));
}

#[test]
fn test_paragraph_formatting() {
    // Test single paragraph
    let text = "This is a single paragraph.";
    let result = render_markdown(text);
    assert!(result.contains("<p>This is a single paragraph.</p>"));

    // Test multiple paragraphs
    let text_multi = "First paragraph.\n\nSecond paragraph.";
    let result_multi = render_markdown(text_multi);
    assert!(result_multi.contains("<p>First paragraph.</p>"));
    assert!(result_multi.contains("<p>Second paragraph.</p>"));

    // Test paragraph with inline formatting
    let text_formatted = "This has *italic* and **bold** text.";
    let result_formatted = render_markdown(text_formatted);
    assert!(result_formatted
        .contains("<p>This has <em>italic</em> and <strong>bold</strong> text.</p>"));
}

#[test]
fn test_comprehensive_paragraph_structure() {
    let complex_content = r#"This is the first paragraph with **bold** text.

This is the second paragraph with a `code snippet` inline.

```python
def hello():
    print("This is a code block")
```

This paragraph comes after the code block.

Here's an image: ![Test image](https://example.com/image.jpg)

Final paragraph with *emphasis* and _underline_."#;

    let result = render_markdown(complex_content);

    // Verify paragraph structure
    assert!(result.contains("<p>This is the first paragraph with <strong>bold</strong> text.</p>"));
    assert!(result
        .contains("<p>This is the second paragraph with a <code>code snippet</code> inline.</p>"));
    assert!(result.contains("<p>This paragraph comes after the code block.</p>"));
    assert!(result.contains("<p>Final paragraph with <em>emphasis</em> and <u>underline</u>.</p>"));

    // Code block should be standalone
    assert_code_block_structure(&result, Some("python"), true);
    assert!(result.contains("def"));
    assert!(result.contains("hello"));
    assert!(result.contains("print"));
    assert!(result.contains("This is a code block"));

    // Image should be standalone with caption wrapper
    assert!(result.contains("<div class=\"media-with-caption\">"));
    assert!(result.contains("<img src=\"https://example.com/image.jpg\" alt=\"Test image\">"));

    // Should not have any <br> tags (everything should be in proper paragraphs or blocks)
    assert!(!result.contains("<br>"));
}

#[test]
fn test_complex_code_block_scenarios() {
    // Test code block with special characters
    let special_chars = "```json\n{\n  \"message\": \"Hello *world* with **markdown**\",\n  \"tags\": [\"<script>\", \"&amp;\"]\n}\n```";
    let special_result = render_markdown(special_chars);

    assert!(special_result.contains("*world*"));
    assert!(special_result.contains("**markdown**"));
    assert!(special_result.contains("&lt;script&gt;"));
    assert!(special_result.contains("&amp;amp;"));

    // Verify key parts of the JSON structure are preserved (split across spans)
    assert!(special_result.contains("message"));
    assert!(special_result.contains("Hello"));
    assert!(special_result.contains("world"));
    assert!(special_result.contains("markdown"));
    assert!(special_result.contains("tags"));

    // Test code block with empty lines (should preserve structure)
    let empty_lines =
        "```python\ndef test():\n\n    print('with empty line')\n\n    return True\n```";
    let empty_result = render_markdown(empty_lines);
    assert_code_block_structure(&empty_result, Some("python"), true);
    assert!(empty_result.contains("def"));
    assert!(empty_result.contains("test"));
    assert!(empty_result.contains("print"));
    assert!(empty_result.contains("with empty line"));
    assert!(empty_result.contains("return"));
    assert!(empty_result.contains("True"));

    // Test mixed content
    let mixed = "Text before\n\n```js\nconsole.log('test');\n```\n\nText after";
    let mixed_result = render_markdown(mixed);
    assert!(mixed_result.contains("<p>Text before</p>"));
    assert!(mixed_result.contains("<p>Text after</p>"));
    assert_code_block_structure(&mixed_result, Some("javascript"), true);
    assert!(mixed_result.contains("console"));
    assert!(mixed_result.contains("log"));
    assert!(mixed_result.contains("test"));
}

#[test]
fn test_line_break_preservation() {
    // Test that single line breaks are preserved as <br> tags
    let text = "First line\nSecond line\nThird line";
    let result = render_markdown(text);
    assert!(result.contains("<p>First line<br>Second line<br>Third line</p>"));

    // Test line breaks mixed with formatting
    let formatted = "Line with *bold*\nAnother line with **italic**";
    let formatted_result = render_markdown(formatted);
    assert!(formatted_result
        .contains("<p>Line with <em>bold</em><br>Another line with <strong>italic</strong></p>"));

    // Test that double line breaks still create separate paragraphs
    let paragraphs = "First paragraph\n\nSecond paragraph";
    let paragraph_result = render_markdown(paragraphs);
    assert!(paragraph_result.contains("<p>First paragraph</p>"));
    assert!(paragraph_result.contains("<p>Second paragraph</p>"));

    // Test empty lines are ignored within paragraphs
    let with_empty = "Line 1\n\nLine 2\n\nLine 3";
    let empty_result = render_markdown(with_empty);
    assert!(empty_result.contains("<p>Line 1</p>"));
    assert!(empty_result.contains("<p>Line 2</p>"));
    assert!(empty_result.contains("<p>Line 3</p>"));
}

#[test]
fn test_comprehensive_line_break_behavior() {
    // Test mixed content with line breaks
    let mixed_content = r#"This is line 1
This is line 2 with **bold**
This is line 3

New paragraph here
Another line in paragraph

```python
def hello():
    print("world")
```

Third paragraph with *italic* formatting."#;

    let result = render_markdown(mixed_content);

    // First paragraph should have line breaks preserved
    assert!(result.contains(
        "<p>This is line 1<br>This is line 2 with <strong>bold</strong><br>This is line 3</p>"
    ));

    // Second paragraph should have line breaks
    assert!(result.contains("<p>New paragraph here<br>Another line in paragraph</p>"));

    // Code block should be separate
    assert_code_block_structure(&result, Some("python"), true);
    assert!(result.contains("def"));
    assert!(result.contains("hello"));
    assert!(result.contains("print"));
    assert!(result.contains("world"));

    // Third paragraph should contain italic formatting
    assert!(result.contains("<p>Third paragraph with <em>italic</em> formatting.</p>"));
}

#[test]
fn test_markdown_tables() {
    // Basic table
    let table_text = "| Name | Age | City |\n|------|-----|------|\n| John | 30  | NYC  |\n| Jane | 25  | LA   |";
    let result = render_markdown(table_text);

    assert!(result.contains("<table>"));
    assert!(result.contains("<thead>"));
    assert!(result.contains("<tbody>"));
    assert!(result.contains("<th>Name</th>"));
    assert!(result.contains("<td>John</td>"));
    assert!(result.contains("</table>"));

    // Table with alignment
    let aligned_table =
        "| Left | Center | Right |\n|:-----|:------:|------:|\n| L1   |   C1   |    R1 |";
    let aligned_result = render_markdown(aligned_table);

    assert!(aligned_result.contains("text-align: center"));
    assert!(aligned_result.contains("text-align: right"));
    assert!(aligned_result.contains("<td>L1</td>"));
    assert!(aligned_result.contains("<td style=\"text-align: center\">C1</td>"));
    assert!(aligned_result.contains("<td style=\"text-align: right\">R1</td>"));
}

#[test]
fn test_tables_with_formatting() {
    // Table with markdown formatting in cells
    let formatted_table = "| **Bold** | *Italic* | `Code` |\n|----------|----------|--------|\n| *test*   | **bold** | `var`  |";
    let result = render_markdown(formatted_table);

    assert!(result.contains("<th><strong>Bold</strong></th>"));
    assert!(result.contains("<th><em>Italic</em></th>"));
    assert!(result.contains("<td><em>test</em></td>"));
    assert!(result.contains("<td><strong>bold</strong></td>"));
}

#[test]
fn test_table_spacing() {
    let simple_table = "| Name | Age |\n|------|-----|\n| John | 30  |";
    let result = render_markdown(simple_table);

    assert!(result.contains("<table>"));
    assert!(!result.contains("<p></p>"));
    assert!(!result.contains("<br><table>"));

    // Test table in context
    let with_text =
        "Here's a table:\n\n| Name | Age |\n|------|-----|\n| John | 30  |\n\nAfter table.";
    let context_result = render_markdown(with_text);
    assert!(context_result.contains("<p>Here's a table:</p>"));
    assert!(context_result.contains("<p>After table.</p>"));
    assert!(context_result.contains("<table>"));
}

#[test]
fn test_table_abovebelow_blanklines() {
    let text = "testing\n| hi | Column 2 | Column 3 |\n| --- | --- | --- |\n| Cell 1 | Cell 2 | Cell 3 |\nafter text";
    let result = render_markdown(text);

    // Before and after the table
    assert!(result.contains("<p>testing</p>"));
    assert!(result.contains("<p>after text</p>"));

    assert!(result.contains("<table>"));
    assert!(result.contains("<td>Cell 1</td>"));
    assert!(result.contains("</table>"));

    // Cheers to anonymity, love from a human <3
    assert!(!result.contains("<br>"));
    assert!(!result.contains("<p></p>"));
    assert!(!result.contains("<br><table>"));
    assert!(!result.contains("<p><table>"));
    assert!(!result.contains("</table></p>"));
}

#[test]
fn test_headers() {
    // Test headers with blank lines (traditional)
    let text_with_blanks = "# Header 1\n\n## Header 2\n\n### Header 3\n\n#### Header 4";
    let result = render_markdown(text_with_blanks);

    assert!(result
            .contains("<h1 id=\"h1\">Header 1<a href=\"#h1\" class=\"header-anchor\" rel=\"noopener noreferrer\">#</a></h1>"));
    assert!(result
            .contains("<h2 id=\"h2\">Header 2<a href=\"#h2\" class=\"header-anchor\" rel=\"noopener noreferrer\">#</a></h2>"));
    assert!(result
            .contains("<h3 id=\"h3\">Header 3<a href=\"#h3\" class=\"header-anchor\" rel=\"noopener noreferrer\">#</a></h3>"));
    assert!(result
            .contains("<h4 id=\"h4\">Header 4<a href=\"#h4\" class=\"header-anchor\" rel=\"noopener noreferrer\">#</a></h4>"));

    // Test headers without blank lines
    let text_without_blanks = "# Main Header\nThis paragraph follows immediately.\n\n## Sub Header\nAnother paragraph right after.";
    let result2 = render_markdown(text_without_blanks);

    assert!(result2.contains(
            "<h1 id=\"h1\">Main Header<a href=\"#h1\" class=\"header-anchor\" rel=\"noopener noreferrer\">#</a></h1>"
        ));
    assert!(result2.contains(
            "<h2 id=\"h2\">Sub Header<a href=\"#h2\" class=\"header-anchor\" rel=\"noopener noreferrer\">#</a></h2>"
        ));
    assert!(result2.contains("<p>This paragraph follows immediately.</p>"));
    assert!(result2.contains("<p>Another paragraph right after.</p>"));

    // Verify headers are not wrapped in paragraphs
    assert!(!result2.contains("<p><h1>"));
    assert!(!result2.contains("<p><h2>"));
}

#[test]
fn test_blockquotes() {
    let text = "> This is a quote\n> Continued quote\n\nNormal text";
    let result = render_markdown(text);

    assert!(result.contains("<blockquote>This is a quote<br>Continued quote</blockquote>"));
    assert!(result.contains("<p>Normal text</p>"));
}

#[test]
fn test_mixed_headers_and_quotes() {
    let text = "# Title\n\n> A quote\n\n## Subtitle\n\nNormal text";
    let result = render_markdown(text);

    assert!(result
            .contains("<h1 id=\"h1\">Title<a href=\"#h1\" class=\"header-anchor\" rel=\"noopener noreferrer\">#</a></h1>"));
    assert!(result.contains("<blockquote>A quote</blockquote>"));
    assert!(result
            .contains("<h2 id=\"h2\">Subtitle<a href=\"#h2\" class=\"header-anchor\" rel=\"noopener noreferrer\">#</a></h2>"));
    assert!(result.contains("<p>Normal text</p>"));
}

#[test]
fn test_markup_md_specific_case() {
    println!("=== MARKUP.MD SPECIFIC TEST ===");

    // Test the exact pattern from markup.md
    let input = "```\n> This is a quoted text\n```";
    println!("Input: {}", input);

    let result = render_markdown(input);
    println!("Output: {}", result);

    // This should contain the literal text with proper HTML escaping, not formatting
    assert!(result.contains("&gt; This is a quoted text"));
    assert!(!result.contains("**italic**"));

    println!("=== END MARKUP.MD TEST ===");
}

#[test]
fn test_manual_comment_verification() {
    let test_content = r#"# Test File for Comment Functionality

This is a test file to verify that comments work correctly in Nonograph.

// This is a comment that should not appear in HTML output
// But should be visible in the .md version

Here is some **bold text** after a comment.

// Another comment here
*Italic text* should still work normally.

## Section with Comments

// Comment in a section
This paragraph contains normal text.

```javascript
// This is NOT a Nonograph comment, it's JavaScript code
function hello() {
    console.log("Hello world");
}
```

// But this IS a Nonograph comment outside the code block

Final paragraph with normal text."#;

    let html_output = render_markdown(test_content);

    println!("=== MANUAL TEST OUTPUT ===");
    println!("{}", html_output);

    // Verify comments are removed from HTML
    assert!(!html_output.contains("// This is a comment that should not appear"));
    assert!(!html_output.contains("// But should be visible in the .md version"));
    assert!(!html_output.contains("// Another comment here"));
    assert!(!html_output.contains("// Comment in a section"));
    assert!(!html_output.contains("// But this IS a Nonograph comment"));

    // Verify normal formatting still works
    assert!(html_output.contains("<strong>bold text</strong>"));
    assert!(html_output.contains("<em>Italic text</em>"));
    assert!(html_output.contains("<h1 id=\"h1\">Test File for Comment Functionality<a href=\"#h1\" class=\"header-anchor\" rel=\"noopener noreferrer\">#</a></h1>"));
    assert!(html_output.contains(
            "<h2 id=\"h2\">Section with Comments<a href=\"#h2\" class=\"header-anchor\" rel=\"noopener noreferrer\">#</a></h2>"
        ));

    // Verify code block comments are preserved (they're inside code blocks)
    assert!(html_output.contains("// This is NOT a Nonograph comment"));

    println!("✅ All manual verification tests passed!");
}

#[test]
fn test_comments_edge_cases() {
    // Test comment that doesn't start at beginning of line
    let input1 = "Normal text // not a comment\n// This is a comment";
    let result1 = render_markdown(input1);
    assert!(result1.contains("Normal text // not a comment"));
    assert!(!result1.contains("// This is a comment"));

    // Test comment with only "//" (no space)
    let input2 = "//No space comment\n// Space comment";
    let result2 = render_markdown(input2);
    assert!(result2.contains("//No space comment"));
    assert!(!result2.contains("// Space comment"));

    // Test empty comment
    let input3 = "// \n//\nNormal text";
    let result3 = render_markdown(input3);
    assert!(!result3.contains("// "));
    assert!(result3.contains("//"));
    assert!(result3.contains("Normal text"));

    // Test comment with special characters
    let input4 = "// Comment with *bold* and [link](url)\nNormal text";
    let result4 = render_markdown(input4);
    assert!(!result4.contains("// Comment with"));
    assert!(result4.contains("Normal text"));
}

#[test]
fn test_comments() {
    let input = "This is normal text\n// This is a comment\nMore normal text\n// Another comment\nFinal text";
    let result = render_markdown(input);

    // Comments should not appear in HTML output
    assert!(!result.contains("// This is a comment"));
    assert!(!result.contains("// Another comment"));

    // Normal text should still be there
    assert!(result.contains("This is normal text"));
    assert!(result.contains("More normal text"));
    assert!(result.contains("Final text"));
}

#[test]
fn markdown_for_page_omits_prose_comments_and_renders_the_same() {
    let input = "Visible line\r\n// hidden note\r\n\r\n```javascript\r\n// kept in code\r\nlet x = 1;\r\n```\r\n// also hidden\r\n`// not a comment`\r\nDone\n";
    let page = markdown_for_page(input);

    assert!(!page.contains("hidden note"));
    assert!(!page.contains("also hidden"));
    assert!(page.contains("// kept in code"));
    assert!(page.contains("`// not a comment`"));
    assert_eq!(render_markdown(input), render_markdown(&page));
    assert_eq!(markdown_for_page("nothing to hide"), "nothing to hide");
    assert_eq!(render_markdown("// only a comment"), render_markdown(""));
}

#[test]
fn test_comments_with_indentation() {
    let input = "Normal line\n    // Indented comment\n**Bold text**";
    let result = render_markdown(input);

    // Comment should be removed even if indented
    assert!(!result.contains("// Indented comment"));

    // Other formatting should work
    assert!(result.contains("<strong>Bold text</strong>"));
    assert!(result.contains("Normal line"));
}

#[test]
fn test_comments_mixed_with_other_features() {
    let input = "# Header\n\n// This is a comment\n**Bold text**\n\n// Another comment\n> Quote";
    let result = render_markdown(input);

    // Comments should not appear
    assert!(!result.contains("// This is a comment"));
    assert!(!result.contains("// Another comment"));

    // Other features should work normally
    assert!(result
            .contains("<h1 id=\"h1\">Header<a href=\"#h1\" class=\"header-anchor\" rel=\"noopener noreferrer\">#</a></h1>"));
    assert!(result.contains("<strong>Bold text</strong>"));
    assert!(result.contains("<blockquote>Quote</blockquote>"));
}

#[test]
fn test_bold_italic_fix_demonstration() {
    // Demonstrate that the fix works correctly
    let text = "This has *italic* text and **bold** text.";
    let result = render_markdown(text);

    // Should render * as italic and ** as bold (standard Markdown)
    assert!(result.contains("<em>italic</em>"));
    assert!(result.contains("<strong>bold</strong>"));

    // Should not have any leftover asterisks
    assert!(!result.contains("*"));

    // Test that **text** doesn't get processed as two *text* anymore
    let bold_only = "**just bold**";
    let bold_result = render_markdown(bold_only);
    assert!(bold_result.contains("<strong>just bold</strong>"));
    assert!(!bold_result.contains("<em>"));
}

#[test]
fn test_ssrf_protection() {
    // Test that dangerous URLs are blocked
    let dangerous_urls = vec![
        "http://192.168.1.1/admin",
        "http://10.0.0.1/config",
        "http://localhost:8009/admin",
        "http://127.0.0.1/secret",
        "http://169.254.169.254/latest/meta-data/",
        "javascript:alert(1)",
        "data:text/html,<script>alert(1)</script>",
        "file:///etc/passwd",
        "http://2130706433/",
        "http://0x7f000001/",
        "http://ⓛⓞⓒⓐⓛⓗⓞⓢⓣ/",
        "http://[::1]/",
        "http://0/",
        "http://localhost%00/",
        "http://[::ffff:127.0.0.1]/",
        "http://[fc00::1]/",
        "http://[fe80::1]/",
    ];

    for dangerous_url in dangerous_urls {
        let content = format!("![test]({})", dangerous_url);
        let result = render_markdown(&content);

        // Dangerous URLs should not appear in img src attributes
        assert!(
            !result.contains(&format!("src=\"{}\"", dangerous_url)),
            "Dangerous URL {} was not blocked",
            dangerous_url
        );
    }

    // Test that safe URLs are allowed
    let safe_urls = vec![
        "https://example.com/image.jpg",
        "https://cdn.example.com/photo.png",
        "http://example.com/video.mp4",
        "relative-image.jpg",
        "./local/image.png",
        "../parent/image.gif",
    ];

    for safe_url in safe_urls {
        let content = format!("![test]({})", safe_url);
        let result = render_markdown(&content);

        // Safe URLs should appear in img src attributes
        assert!(
            result.contains(&format!("src=\"{}\"", safe_url)),
            "Safe URL {} was incorrectly blocked",
            safe_url
        );
    }
}

#[test]
fn test_header_anchor_functionality() {
    // Test that headers get proper anchor links with sequential numbering
    let content = "# First Header\n\nSome content\n\n## Second Header\n\nMore content\n\n### Third Header\n\n#### Fourth Header";
    let result = render_markdown(content);

    // Check that each header gets the correct sequential ID
    assert!(result.contains(
            "<h1 id=\"h1\">First Header<a href=\"#h1\" class=\"header-anchor\" rel=\"noopener noreferrer\">#</a></h1>"
        ));
    assert!(result.contains(
            "<h2 id=\"h2\">Second Header<a href=\"#h2\" class=\"header-anchor\" rel=\"noopener noreferrer\">#</a></h2>"
        ));
    assert!(result.contains(
            "<h3 id=\"h3\">Third Header<a href=\"#h3\" class=\"header-anchor\" rel=\"noopener noreferrer\">#</a></h3>"
        ));
    assert!(result.contains(
            "<h4 id=\"h4\">Fourth Header<a href=\"#h4\" class=\"header-anchor\" rel=\"noopener noreferrer\">#</a></h4>"
        ));

    // Test that numbering is consistent across multiple renders of same content
    let result2 = render_markdown(content);
    assert_eq!(result, result2);

    // Test mixed header levels maintain correct numbering
    let mixed_content = "## Starting with H2\n\n# Then H1\n\n#### Then H4\n\n### Then H3";
    let mixed_result = render_markdown(mixed_content);

    assert!(mixed_result.contains(
            "<h2 id=\"h1\">Starting with H2<a href=\"#h1\" class=\"header-anchor\" rel=\"noopener noreferrer\">#</a></h2>"
        ));
    assert!(mixed_result.contains(
            "<h1 id=\"h2\">Then H1<a href=\"#h2\" class=\"header-anchor\" rel=\"noopener noreferrer\">#</a></h1>"
        ));
    assert!(mixed_result.contains(
            "<h4 id=\"h3\">Then H4<a href=\"#h3\" class=\"header-anchor\" rel=\"noopener noreferrer\">#</a></h4>"
        ));
    assert!(mixed_result.contains(
            "<h3 id=\"h4\">Then H3<a href=\"#h4\" class=\"header-anchor\" rel=\"noopener noreferrer\">#</a></h3>"
        ));
}

#[test]
fn test_dividers() {
    // Test three stars divider
    let stars_text = "Some text\n***\nMore text";
    let stars_result = render_markdown(stars_text);
    assert!(stars_result.contains("<div class=\"divider-stars\">"));
    assert!(stars_result.contains("<div class=\"asterisk\"><div class=\"center\"></div></div>"));

    // Test single asterisk divider
    let asterisk_text = "Some text\n-*-\nMore text";
    let asterisk_result = render_markdown(asterisk_text);
    assert!(asterisk_result
        .contains("<div class=\"divider-asterisk\"><div class=\"center\"></div></div>"));

    // Test horizontal thin divider
    let thin_text = "Some text\n---\nMore text";
    let thin_result = render_markdown(thin_text);
    assert!(thin_result.contains("<hr class=\"divider-thin\">"));

    // Test horizontal double-line divider
    let double_text = "Some text\n===\nMore text";
    let double_result = render_markdown(double_text);
    assert!(double_result.contains("<hr class=\"divider-double\">"));

    // Test that dividers work with surrounding whitespace
    let whitespace_text = "   ***   ";
    let whitespace_result = render_markdown(whitespace_text);
    assert!(whitespace_result.contains("<div class=\"divider-stars\">"));
    assert!(
        whitespace_result.contains("<div class=\"asterisk\"><div class=\"center\"></div></div>")
    );

    // Test that partial matches don't trigger dividers
    let partial_text = "This has *** in the middle of text";
    let partial_result = render_markdown(partial_text);
    assert!(!partial_result.contains("<div class=\"divider-stars\">"));

    // Test dividers mixed with other content on same line don't trigger
    let mixed_content_tests = vec![
        "Here is some *** text after",
        "Before text --- and after",
        "Some === content here",
        "Text -*-  more text",
        "# Header with *** stars",
        "## Another --- header",
        "**Bold *** text**",
        "*Italic -*- text*",
    ];

    for mixed_text in mixed_content_tests {
        let mixed_result = render_markdown(mixed_text);
        assert!(!mixed_result.contains("<div class=\"divider-stars\">"));
        assert!(!mixed_result.contains("<div class=\"divider-asterisk\">"));
        assert!(!mixed_result.contains("<hr class=\"divider-thin\">"));
        assert!(!mixed_result.contains("<hr class=\"divider-double\">"));
    }

    // Test multiple dividers
    let multiple_text = "Text\n***\nMore text\n---\nEven more\n===\nFinal text";
    let multiple_result = render_markdown(multiple_text);
    assert!(multiple_result.contains("<div class=\"divider-stars\">"));
    assert!(multiple_result.contains("<div class=\"asterisk\"><div class=\"center\"></div></div>"));
    assert!(multiple_result.contains("<hr class=\"divider-thin\">"));
    assert!(multiple_result.contains("<hr class=\"divider-double\">"));
}
