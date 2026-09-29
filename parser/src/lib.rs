use std::net::IpAddr;
use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::html::{styled_line_to_highlighted_html, IncludeBackground};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;
use url::Url;

fn process_images(text: &str) -> String {
    process_images_with_config(text, &RenderOptions::default())
}

fn is_ip_blocked(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            // Loopback: 127.0.0.0/8
            if o[0] == 127 {
                return true;
            }
            // Unspecified / any: 0.0.0.0/8
            if o[0] == 0 {
                return true;
            }
            // Private: 10.0.0.0/8
            if o[0] == 10 {
                return true;
            }
            // Private: 172.16.0.0/12
            if o[0] == 172 && o[1] >= 16 && o[1] <= 31 {
                return true;
            }
            // Private: 192.168.0.0/16
            if o[0] == 192 && o[1] == 168 {
                return true;
            }
            // Link-local: 169.254.0.0/16
            if o[0] == 169 && o[1] == 254 {
                return true;
            }
            // Broadcast / limited broadcast
            if o == [255, 255, 255, 255] {
                return true;
            }
            false
        }
        IpAddr::V6(v6) => {
            // Loopback: ::1
            if v6.is_loopback() {
                return true;
            }
            // Unspecified: ::
            if v6.is_unspecified() {
                return true;
            }
            // IPv4-mapped IPv6 (::ffff:0:0/96) — check the embedded v4 address
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_ip_blocked(IpAddr::V4(v4));
            }
            // IPv4-compatible (deprecated but still parseable)
            if let Some(v4) = v6.to_ipv4() {
                return is_ip_blocked(IpAddr::V4(v4));
            }
            // Unique local: fc00::/7
            let first = v6.segments()[0];
            if first & 0xfe00 == 0xfc00 {
                return true;
            }
            // Link-local: fe80::/10
            if first & 0xffc0 == 0xfe80 {
                return true;
            }
            false
        }
    }
}

fn is_safe_url(url: &str) -> bool {
    if !url.contains("://") {
        return true;
    }

    let parsed = match Url::parse(url) {
        Ok(u) => u,
        Err(_) => return false,
    };

    // Only allow http and https
    match parsed.scheme() {
        "http" | "https" => {}
        _ => return false,
    }

    let host = match parsed.host() {
        Some(h) => h,
        None => return false,
    };

    match host {
        url::Host::Ipv4(v4) => {
            if is_ip_blocked(IpAddr::V4(v4)) {
                return false;
            }
        }
        url::Host::Ipv6(v6) => {
            if is_ip_blocked(IpAddr::V6(v6)) {
                return false;
            }
        }
        url::Host::Domain(domain) => {
            let ascii_domain = deunicode::deunicode(domain).to_lowercase();

            if ascii_domain == "localhost" {
                return false;
            }

            let lookup = ascii_domain.trim_end_matches('.');
            if let Ok(ip) = lookup.parse::<IpAddr>() {
                if is_ip_blocked(ip) {
                    return false;
                }
            }
        }
    }

    true
}

pub struct RenderOptions {
    pub max_url_length: usize,
    pub external_link_security: bool,
    pub syntax_theme: String,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            max_url_length: 4096,
            external_link_security: true,
            syntax_theme: "base16-ocean.dark".to_string(),
        }
    }
}

#[allow(dead_code)]
pub fn render_markdown(content: &str) -> String {
    render_markdown_with_config(content, &RenderOptions::default())
}

pub fn render_markdown_with_config(content: &str, config: &RenderOptions) -> String {
    let cleaned_content = remove_standalone_list_tags(content);

    let (protected_content, fenced_blocks) = extract_fenced_code_blocks(&cleaned_content);
    let (mut working_content, code_blocks) = extract_code_blocks(&protected_content);

    // Process comments before other formatting to remove them from HTML output
    working_content = process_comments(&working_content);

    let (working_content_no_media, media_blocks) = extract_media_syntax(&working_content);
    working_content = working_content_no_media;

    let (working_content_no_links, link_blocks) = extract_link_syntax(&working_content);
    working_content = working_content_no_links;

    // Process footnotes before text formatting to avoid conflicts with ^ and []
    working_content = process_footnotes(&working_content);

    working_content = safe_replace(&working_content, "**", "**", "<strong>", "</strong>");
    working_content = safe_replace(&working_content, "*", "*", "<em>", "</em>");
    working_content = safe_replace(&working_content, "_", "_", "<u>", "</u>");
    working_content = safe_replace(&working_content, "~", "~", "<del>", "</del>");
    working_content = safe_replace(&working_content, "^", "^", "<sup>", "</sup>");
    working_content = safe_replace(&working_content, "==", "==", "<mark>", "</mark>");
    working_content = safe_replace(
        &working_content,
        "#",
        "#",
        "<span class=\"secret\">",
        "</span>",
    );

    working_content = restore_media_syntax(&working_content, &media_blocks);
    working_content = restore_link_syntax(&working_content, &link_blocks);
    working_content = process_images(&working_content);
    working_content = process_links(&working_content);
    working_content = process_tables(&working_content);
    working_content = process_lists(&working_content);
    working_content = process_dividers(&working_content);
    working_content = format_paragraphs_with_headers(&working_content);
    working_content =
        restore_fenced_code_blocks_with_config(&working_content, &fenced_blocks, config);
    working_content = restore_code_blocks(&working_content, &code_blocks);
    working_content = restore_footnotes(&working_content);

    sanitize_html(working_content)
}

fn process_images_with_config(text: &str, config: &RenderOptions) -> String {
    let mut result = String::with_capacity(text.len() + 1024);
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars.len() >= 2 && i < chars.len() - 1 && chars[i] == '!' && chars[i + 1] == '[' {
            // Find closing bracket
            let mut bracket_end = None;
            let mut j = i + 2;
            while j < chars.len() && chars[j] != '\n' {
                if chars[j] == ']' {
                    bracket_end = Some(j);
                    break;
                }
                j += 1;
            }

            if let Some(bracket_end_idx) = bracket_end {
                // Check for ![alt](url) pattern
                if bracket_end_idx + 1 < chars.len() && chars[bracket_end_idx + 1] == '(' {
                    let mut paren_end = None;
                    let mut k = bracket_end_idx + 2;
                    while k < chars.len() && chars[k] != '\n' {
                        if chars[k] == ')' {
                            paren_end = Some(k);
                            break;
                        }
                        k += 1;
                    }

                    if let Some(paren_end_idx) = paren_end {
                        let alt_text: String = chars[(i + 2)..bracket_end_idx].iter().collect();
                        let image_url: String =
                            chars[(bracket_end_idx + 2)..paren_end_idx].iter().collect();

                        if !image_url.is_empty()
                            && image_url.len() <= config.max_url_length
                            && is_safe_url(&image_url)
                        {
                            let is_video = is_video_url(&image_url);

                            // Check if alt text is present for caption
                            if !alt_text.trim().is_empty() {
                                result.push_str("<div class=\"media-with-caption\">");
                                if is_video {
                                    result.push_str("<video controls style=\"width: 100%;\">");
                                    result.push_str("<source src=\"");
                                    result.push_str(&html_escape(&image_url));
                                    result.push_str("\" type=\"");
                                    result.push_str(&get_video_mime_type(&image_url));
                                    result.push_str("\">");
                                    result.push_str("Your browser does not support the video tag.");
                                    result.push_str("</video>");
                                } else {
                                    result.push_str("<img src=\"");
                                    result.push_str(&html_escape(&image_url));
                                    result.push_str("\" alt=\"");
                                    result.push_str(&html_escape(&alt_text));
                                    result.push_str("\">");
                                }
                                result.push_str("<div class=\"media-caption\">");
                                result.push_str(&html_escape(&alt_text));
                                result.push_str("</div>");
                                result.push_str("</div>");
                            } else {
                                if is_video {
                                    result.push_str("<video controls style=\"width: 100%;\">");
                                    result.push_str("<source src=\"");
                                    result.push_str(&html_escape(&image_url));
                                    result.push_str("\" type=\"");
                                    result.push_str(&get_video_mime_type(&image_url));
                                    result.push_str("\">");
                                    result.push_str("Your browser does not support the video tag.");
                                    result.push_str("</video>");
                                } else {
                                    result.push_str("<img src=\"");
                                    result.push_str(&html_escape(&image_url));
                                    result.push_str("\" alt=\"");
                                    result.push_str(&html_escape(&alt_text));
                                    result.push_str("\">");
                                }
                            }

                            i = paren_end_idx + 1;
                            continue;
                        }
                    }
                }
            }
        }

        // No pattern matched, add current character
        result.push(chars[i]);
        i += 1;
    }

    result
}

fn is_video_url(url: &str) -> bool {
    let video_extensions = ["mp4", "webm", "ogg", "mov", "avi", "mkv"];
    let lower_url = url.to_lowercase();
    video_extensions
        .iter()
        .any(|ext| lower_url.ends_with(&format!(".{}", ext)))
}

fn get_video_mime_type(url: &str) -> &'static str {
    let lower_url = url.to_lowercase();
    if lower_url.ends_with(".mp4") {
        "video/mp4"
    } else if lower_url.ends_with(".webm") {
        "video/webm"
    } else if lower_url.ends_with(".ogg") {
        "video/ogg"
    } else if lower_url.ends_with(".mov") {
        "video/quicktime"
    } else if lower_url.ends_with(".avi") {
        "video/x-msvideo"
    } else if lower_url.ends_with(".mkv") {
        "video/x-matroska"
    } else {
        "video/mp4" // fallback
    }
}

fn process_links(text: &str) -> String {
    process_links_with_config(text, &RenderOptions::default())
}

fn safe_replace(
    text: &str,
    start_pattern: &str,
    end_pattern: &str,
    open_tag: &str,
    close_tag: &str,
) -> String {
    let mut result = String::with_capacity(text.len() + 1024);
    let mut remaining = text;

    while let Some(start_pos) = remaining.find(start_pattern) {
        result.push_str(&remaining[..start_pos]);

        let after_start = &remaining[start_pos + start_pattern.len()..];
        if let Some(end_pos) = after_start.find(end_pattern) {
            let content = &after_start[..end_pos];
            if !content.is_empty() && !content.contains('\n') {
                result.push_str(open_tag);
                result.push_str(content);
                result.push_str(close_tag);
                remaining = &after_start[end_pos + end_pattern.len()..];
            } else {
                result.push_str(start_pattern);
                remaining = &remaining[start_pos + start_pattern.len()..];
            }
        } else {
            result.push_str(start_pattern);
            remaining = &remaining[start_pos + start_pattern.len()..];
        }
    }

    result.push_str(remaining);
    result
}

fn sanitize_html(html: String) -> String {
    let mut builder = ammonia::Builder::default();
    builder
        .add_tags(&[
            "video",
            "source",
            "pre",
            "p",
            "table",
            "thead",
            "tbody",
            "tr",
            "th",
            "td",
            "em",
            "strong",
            "u",
            "del",
            "sup",
            "mark",
            "span",
            "code",
            "a",
            "img",
            "br",
            "hr",
            "h1",
            "h2",
            "h3",
            "h4",
            "blockquote",
            "div",
            "ol",
            "ul",
            "li",
            "input",
            "button",
            "svg",
            "polyline",
            "line",
            "rect",
            "path",
        ])
        .add_tag_attributes("video", &["controls", "style"])
        .add_tag_attributes("source", &["src", "type"])
        .add_tag_attributes("img", &["src", "alt", "style"])
        .add_tag_attributes("code", &["class", "data-line-count", "style"])
        .add_tag_attributes("span", &["class", "style"])
        .add_tag_attributes("th", &["style"])
        .add_tag_attributes("td", &["style"])
        .add_tag_attributes("a", &["href", "target", "id", "class"])
        .add_tag_attributes("div", &["class"])
        .add_tag_attributes("hr", &["class"])
        .add_tag_attributes("ul", &["class"])
        .add_tag_attributes("li", &["id", "class"])
        .add_tag_attributes("input", &["type", "checked", "disabled"])
        .add_tag_attributes("sup", &["id"])
        .add_tag_attributes("h1", &["id"])
        .add_tag_attributes("h2", &["id"])
        .add_tag_attributes("h3", &["id"])
        .add_tag_attributes("h4", &["id"])
        .add_tag_attributes("button", &["class", "data-icon-expand", "data-icon-check"])
        .add_tag_attributes(
            "svg",
            &[
                "class",
                "viewBox",
                "fill",
                "stroke",
                "stroke-width",
                "stroke-linecap",
                "stroke-linejoin",
                "aria-hidden",
            ],
        )
        .add_tag_attributes("polyline", &["points"])
        .add_tag_attributes("line", &["x1", "y1", "x2", "y2"])
        .add_tag_attributes("rect", &["x", "y", "width", "height", "rx"])
        .add_tag_attributes("path", &["d", "fill-rule"])
        .add_tag_attributes("pre", &["class"])
        .link_rel(Some("noopener noreferrer"));

    builder.clean(&html).to_string()
}

pub fn sanitize_text(text: &str) -> String {
    let builder = ammonia::Builder::empty();
    let sanitized = builder.clean(text).to_string();
    sanitized
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace('\n', " ")
        .replace('\r', "")
}

// Thanks for the code. You know who you are.
pub fn html_attr_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
        .replace('\n', " ") // Replace newlines with space, not entity
        .replace('\r', "") // Remove carriage returns
}

fn sanitize_language(lang: &str) -> String {
    let sanitized = lang
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '+' || *c == '#')
        .collect::<String>()
        .to_lowercase();
    if sanitized.chars().count() > 15 {
        sanitized.chars().take(15).collect()
    } else {
        sanitized
    }
}

fn process_single_header(text: &str, header_count: &mut usize) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.starts_with("#### ") {
        let header_text = &trimmed[5..];
        *header_count += 1;
        Some(format!(
            "<h4 id=\"h{}\">{}<a href=\"#h{}\" class=\"header-anchor\">#</a></h4>",
            *header_count, header_text, *header_count
        ))
    } else if trimmed.starts_with("### ") {
        let header_text = &trimmed[4..];
        *header_count += 1;
        Some(format!(
            "<h3 id=\"h{}\">{}<a href=\"#h{}\" class=\"header-anchor\">#</a></h3>",
            *header_count, header_text, *header_count
        ))
    } else if trimmed.starts_with("## ") {
        let header_text = &trimmed[3..];
        *header_count += 1;
        Some(format!(
            "<h2 id=\"h{}\">{}<a href=\"#h{}\" class=\"header-anchor\">#</a></h2>",
            *header_count, header_text, *header_count
        ))
    } else if trimmed.starts_with("# ") {
        let header_text = &trimmed[2..];
        *header_count += 1;
        Some(format!(
            "<h1 id=\"h{}\">{}<a href=\"#h{}\" class=\"header-anchor\">#</a></h1>",
            *header_count, header_text, *header_count
        ))
    } else {
        None
    }
}

fn process_single_blockquote(text: &str) -> String {
    let mut blockquote_content = String::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("> ") {
            let quote_text = &trimmed[2..];
            if !blockquote_content.is_empty() {
                blockquote_content.push_str("<br>");
            }
            blockquote_content.push_str(quote_text);
        }
    }

    format!("<blockquote>{}</blockquote>", blockquote_content)
}

fn extract_fenced_code_blocks(text: &str) -> (String, Vec<(String, String, u32)>) {
    let mut result = String::new();
    let mut fenced_blocks = Vec::new();

    // If there are no fenced code blocks, return original text
    if !text.contains("```") {
        return (text.to_string(), fenced_blocks);
    }

    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // Check if this line starts a fenced code block
        if let Some(end) = fenced_block_end(&lines, i) {
            let fence_length = line.chars().take_while(|&c| c == '`').count();
            let language = sanitize_language(line[fence_length..].trim());
            let code_content = lines[(i + 1)..end].join("\n");
            // Count lines in code content (minimum 1, maximum 999)
            let line_count = if code_content.is_empty() {
                1
            } else {
                let count = code_content.lines().count() as u32;
                count.clamp(1, 999)
            };
            let placeholder = format!("{{{{FENCEDBLOCK{}}}}}", fenced_blocks.len());
            fenced_blocks.push((language, code_content, line_count));
            result.push_str(&placeholder);
            if i < lines.len() - 1 || text.ends_with('\n') {
                result.push('\n');
            }
            i = end + 1;
        } else {
            // Regular line
            result.push_str(line);
            if i < lines.len() - 1 {
                result.push('\n');
            }
            i += 1;
        }
    }

    (result, fenced_blocks)
}

fn map_language_for_css(lang: &str) -> &str {
    match lang.to_lowercase().as_str() {
        // Primary languages
        "javascript" | "js" => "javascript",
        "python" | "py" => "python",
        "java" => "java",
        "typescript" | "ts" => "typescript",
        "html" => "html",
        "css" => "css",
        "bash" | "sh" | "shell" => "bash",
        "sql" => "sql",
        "c" => "c",
        "cpp" | "c++" => "cpp",
        "csharp" | "c#" | "cs" => "csharp",
        "php" => "php",
        "ruby" | "rb" => "ruby",
        "go" | "golang" => "go",
        "rust" | "rs" => "rust",
        "swift" => "swift",
        "kotlin" | "kt" => "kotlin",
        "r" => "r",
        "matlab" => "matlab",
        "scala" => "scala",
        "perl" => "perl",
        "powershell" | "ps1" => "powershell",
        "json" => "json",
        "xml" => "xml",
        "yaml" | "yml" => "yaml",
        "markdown" | "md" => "markdown",
        "toml" => "toml",
        "ini" => "ini",
        "properties" => "properties",
        "jsx" => "jsx",
        "tsx" => "tsx",
        "vue" => "vue",
        "scss" => "scss",
        "sass" => "sass",
        "less" => "less",
        "graphql" | "gql" => "graphql",
        "svelte" => "svelte",
        "handlebars" | "hbs" => "handlebars",
        "pug" | "jade" => "pug",
        "ejs" => "ejs",
        "nunjucks" | "njk" => "nunjucks",
        "dockerfile" | "docker" => "dockerfile",
        "makefile" | "make" => "makefile",
        "cmake" => "cmake",
        "nginx" => "nginx",
        "apache" => "apache",
        "lua" => "lua",
        "dart" => "dart",
        "elixir" | "ex" => "elixir",
        "haskell" | "hs" => "haskell",
        "clojure" | "clj" => "clojure",
        "objective-c" | "objc" => "objective-c",
        "coffeescript" | "coffee" => "coffeescript",
        "groovy" => "groovy",
        "racket" | "rkt" => "racket",
        "scheme" | "scm" => "scheme",
        "lisp" => "lisp",
        "erlang" | "erl" => "erlang",
        "fsharp" | "f#" | "fs" => "fsharp",
        "ocaml" | "ml" => "ocaml",
        "julia" | "jl" => "julia",
        "nim" => "nim",
        "crystal" | "cr" => "crystal",
        "d" => "d",
        "zig" => "zig",
        "vlang" => "v",
        "solidity" | "sol" => "solidity",
        "vhdl" => "vhdl",
        "verilog" => "verilog",
        "assembly" | "asm" => "assembly",
        "fortran" | "f90" | "f95" => "fortran",
        "cobol" | "cob" => "cobol",
        "pascal" | "pas" => "pascal",
        "ada" => "ada",
        "prolog" | "pl" => "prolog",
        "smalltalk" | "st" => "smalltalk",
        "tcl" => "tcl",
        "awk" => "awk",
        "sed" => "sed",
        "vim" | "vimscript" => "vim",
        "emacs-lisp" | "elisp" => "emacs-lisp",
        "elm" => "elm",
        "purescript" | "purs" => "purescript",
        "reasonml" | "reason" | "re" => "reasonml",
        "apex" => "apex",
        "arduino" | "ino" => "arduino",
        "processing" | "pde" => "processing",
        "openscad" | "scad" => "openscad",
        "latex" | "tex" => "latex",
        "bibtex" | "bib" => "bibtex",
        "rmarkdown" | "rmd" => "rmarkdown",
        "restructuredtext" | "rst" => "restructuredtext",
        "asciidoc" | "adoc" => "asciidoc",
        "textile" => "textile",
        "org" => "org",
        "diff" => "diff",
        "patch" => "patch",
        "plaintext" | "text" | "txt" => "plaintext",
        _ => lang,
    }
}

fn map_language_for_syntect(lang: &str) -> &str {
    match lang.to_lowercase().as_str() {
        // Primary languages
        "javascript" | "js" => "JavaScript",
        "python" | "py" => "Python",
        "java" => "Java",
        "typescript" | "ts" => "TypeScript",
        "html" => "HTML",
        "css" => "CSS",
        "bash" | "sh" | "shell" => "Bash",
        "sql" => "SQL",
        "c" => "C",
        "cpp" | "c++" => "C++",
        "csharp" | "c#" | "cs" => "C#",
        "php" => "PHP",
        "ruby" | "rb" => "Ruby",
        "go" | "golang" => "Go",
        "rust" | "rs" => "Rust",
        "swift" => "Swift",
        "kotlin" | "kt" => "Kotlin",
        "r" => "R",
        "matlab" => "MATLAB",
        "scala" => "Scala",
        "perl" => "Perl",
        "powershell" | "ps1" => "PowerShell",

        // Data formats
        "json" => "JSON",
        "xml" => "XML",
        "yaml" | "yml" => "YAML",
        "markdown" | "md" => "Markdown",
        "toml" => "TOML",
        "ini" => "INI",
        "properties" => "Java Properties",

        // Web technologies
        "jsx" => "JavaScript (JSX)",
        "tsx" => "TypeScript (TSX)",
        "vue" => "Vue",
        "scss" => "SCSS",
        "sass" => "Sass",
        "less" => "Less",
        "graphql" | "gql" => "GraphQL",
        "svelte" => "Svelte",
        "handlebars" | "hbs" => "Handlebars",
        "pug" | "jade" => "Pug",
        "ejs" => "EJS",
        "nunjucks" | "njk" => "Nunjucks",

        // Systems and config
        "dockerfile" | "docker" => "Dockerfile",
        "makefile" | "make" => "Makefile",
        "cmake" => "CMake",
        "nginx" => "nginx",
        "apache" => "ApacheConf",

        // Functional and other languages
        "lua" => "Lua",
        "dart" => "Dart",
        "elixir" | "ex" => "Elixir",
        "haskell" | "hs" => "Haskell",
        "clojure" | "clj" => "Clojure",
        "objective-c" | "objc" => "Objective-C",
        "coffeescript" | "coffee" => "CoffeeScript",
        "groovy" => "Groovy",
        "racket" | "rkt" => "Racket",
        "scheme" | "scm" => "Scheme",
        "lisp" => "Lisp",
        "erlang" | "erl" => "Erlang",
        "fsharp" | "f#" | "fs" => "F#",
        "ocaml" | "ml" => "OCaml",
        "julia" | "jl" => "Julia",
        "nim" => "Nim",
        "crystal" | "cr" => "Crystal",
        "d" => "D",
        "zig" => "Zig",
        "vlang" => "V",
        "solidity" | "sol" => "Solidity",

        // Hardware description
        "vhdl" => "VHDL",
        "verilog" => "Verilog",
        "assembly" | "asm" => "Assembly",

        // Legacy and specialized
        "fortran" | "f90" | "f95" => "Fortran",
        "cobol" | "cob" => "COBOL",
        "pascal" | "pas" => "Pascal",
        "ada" => "Ada",
        "prolog" | "pl" => "Prolog",
        "smalltalk" | "st" => "Smalltalk",
        "tcl" => "Tcl",
        "awk" => "AWK",
        "sed" => "sed",

        // Editors
        "vim" | "vimscript" => "VimL",
        "emacs-lisp" | "elisp" => "Emacs Lisp",

        // Alternative languages
        "elm" => "Elm",
        "purescript" | "purs" => "PureScript",
        "reasonml" | "reason" | "re" => "Reason",
        "apex" => "Apex",
        "arduino" | "ino" => "Arduino",
        "processing" | "pde" => "Processing",
        "openscad" | "scad" => "OpenSCAD",

        // Document formats
        "latex" | "tex" => "LaTeX",
        "bibtex" | "bib" => "BibTeX",
        "rmarkdown" | "rmd" => "R Markdown",
        "restructuredtext" | "rst" => "reStructuredText",
        "asciidoc" | "adoc" => "AsciiDoc",
        "textile" => "Textile",
        "org" => "Org",

        // Version control and patches
        "diff" => "Diff",
        "patch" => "Diff",

        // Generic
        "plaintext" | "text" | "txt" => "Plain Text",

        // Return original if no mapping found
        _ => lang,
    }
}

#[allow(dead_code)]
fn restore_fenced_code_blocks(text: &str, fenced_blocks: &[(String, String, u32)]) -> String {
    restore_fenced_code_blocks_with_config(text, fenced_blocks, &RenderOptions::default())
}

fn process_lists(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut result = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // Check if this line starts a list
        if is_list_item(line) {
            let (list_html, processed_lines) = process_list_block(&lines, i);
            result.push(list_html);
            i += processed_lines;

            // Ensure proper separation after list for subsequent processing
            if i < lines.len() && !lines[i].trim().is_empty() {
                result.push(String::new()); // Add empty line to maintain structure
            }
        } else {
            result.push(lines[i].to_string());
            i += 1;
        }
    }

    result.join("\n")
}

fn remove_standalone_list_tags(text: &str) -> String {
    // Only escape HTML list tags if they appear to be raw HTML input
    let lines: Vec<&str> = text.lines().collect();
    let mut result = Vec::new();

    for line in lines {
        let trimmed = line.trim();

        if (trimmed.contains("<li>")
            || trimmed.contains("</li>")
            || trimmed.contains("<ul>")
            || trimmed.contains("</ul>")
            || trimmed.contains("<ol>")
            || trimmed.contains("</ol>"))
            && !is_list_item(trimmed)
        {
            let escaped = line
                .replace("<li>", "&lt;li&gt;")
                .replace("</li>", "&lt;/li&gt;")
                .replace("<ul>", "&lt;ul&gt;")
                .replace("</ul>", "&lt;/ul&gt;")
                .replace("<ol>", "&lt;ol&gt;")
                .replace("</ol>", "&lt;/ol&gt;");
            result.push(escaped);
        } else {
            result.push(line.to_string());
        }
    }

    result.join("\n")
}

fn is_list_item(line: &str) -> bool {
    let trimmed = line.trim();

    if trimmed == "-"
        || trimmed == "*"
        || trimmed == "+"
        || trimmed.starts_with("- ")
        || trimmed.starts_with("* ")
        || trimmed.starts_with("+ ")
    {
        return true;
    }

    // Numbered lists: 1. 2. etc
    if let Some(pos) = trimmed.find(". ") {
        let prefix = &trimmed[..pos];
        if prefix.chars().all(|c| c.is_ascii_digit()) && !prefix.is_empty() {
            return true;
        }
    }

    false
}

/// Number of leading whitespace columns (tabs count as the configured width).
fn list_indent(line: &str) -> usize {
    let mut indent = 0;
    for c in line.chars() {
        match c {
            ' ' => indent += 1,
            '\t' => indent += LIST_TAB_WIDTH,
            _ => break,
        }
    }
    indent
}

/// A single parsed list item, together with its indentation and any children.
struct ListItem {
    indent: usize,
    ordered: bool,
    task: Option<bool>,
    content: String,
    children: Vec<ListItem>,
}

fn parse_task_marker(content: &str) -> Option<(bool, String)> {
    let mut chars = content.chars();
    if chars.next()? != '[' {
        return None;
    }
    let state = chars.next()?;
    if chars.next()? != ']' {
        return None;
    }
    let checked = match state {
        ' ' => false,
        'x' | 'X' => true,
        _ => return None,
    };
    let rest = &content[3..];
    match rest.chars().next() {
        None => Some((checked, String::new())),
        Some(c) if c.is_whitespace() => Some((checked, rest.trim_start().to_string())),
        _ => None,
    }
}

const LIST_TAB_WIDTH: usize = 2;

fn process_list_block(lines: &[&str], start_idx: usize) -> (String, usize) {
    let mut i = start_idx;

    let mut flat: Vec<ListItem> = Vec::new();
    while i < lines.len() {
        let raw = lines[i];
        let trimmed = raw.trim();

        if trimmed.is_empty() {
            i += 1;
            continue;
        }

        if !is_list_item(raw) {
            break;
        }

        let indent = list_indent(raw);
        let ordered = trimmed.chars().next().unwrap_or(' ').is_ascii_digit();
        let content = if trimmed == "-" || trimmed == "*" || trimmed == "+" {
            "".to_string()
        } else if trimmed.starts_with("- ")
            || trimmed.starts_with("* ")
            || trimmed.starts_with("+ ")
        {
            trimmed[2..].to_string()
        } else if let Some(pos) = trimmed.find(". ") {
            trimmed[pos + 2..].to_string()
        } else {
            trimmed.to_string()
        };

        let (task, content) = if ordered {
            (None, content)
        } else {
            match parse_task_marker(&content) {
                Some((checked, rest)) => (Some(checked), rest),
                None => (None, content),
            }
        };

        flat.push(ListItem {
            indent,
            ordered,
            task,
            content,
            children: Vec::new(),
        });
        i += 1;
    }

    let mut idx = 0;
    let tree = build_list_tree(&flat, &mut idx, flat.first().map_or(0, |item| item.indent));
    let list_html = render_list_tree(&tree);

    (list_html, i - start_idx)
}

fn build_list_tree(flat: &[ListItem], idx: &mut usize, level: usize) -> Vec<ListItem> {
    let mut siblings: Vec<ListItem> = Vec::new();

    while *idx < flat.len() {
        let item = &flat[*idx];

        if item.indent < level {
            // Belongs to an outer list; stop here.
            break;
        }

        if item.indent > level {
            let child_level = item.indent;
            let children = build_list_tree(flat, idx, child_level);
            if let Some(last) = siblings.last_mut() {
                last.children.extend(children);
            } else {
                siblings.extend(children);
            }
            continue;
        }

        *idx += 1;
        let mut node = ListItem {
            indent: item.indent,
            ordered: item.ordered,
            task: item.task,
            content: item.content.clone(),
            children: Vec::new(),
        };

        if *idx < flat.len() && flat[*idx].indent > level {
            let child_level = flat[*idx].indent;
            node.children = build_list_tree(flat, idx, child_level);
        }

        siblings.push(node);
    }

    siblings
}

fn render_list_tree(items: &[ListItem]) -> String {
    if items.is_empty() {
        return String::new();
    }

    let ordered = items[0].ordered;
    let has_task = items.iter().any(|item| item.task.is_some());
    let mut inner = String::new();

    for item in items {
        match item.task {
            Some(checked) => {
                inner.push_str("<li class=\"task-list-item\">");
                inner.push_str(if checked {
                    "<input type=\"checkbox\" checked> "
                } else {
                    "<input type=\"checkbox\"> "
                });
            }
            None => inner.push_str("<li>"),
        }
        inner.push_str(&item.content);
        if !item.children.is_empty() {
            inner.push_str(&render_list_tree(&item.children));
        }
        inner.push_str("</li>");
    }

    if ordered {
        format!("<ol>{}</ol>", inner)
    } else if has_task {
        format!("<ul class=\"contains-task-list\">{}</ul>", inner)
    } else {
        format!("<ul>{}</ul>", inner)
    }
}

fn restore_fenced_code_blocks_with_config(
    text: &str,
    fenced_blocks: &[(String, String, u32)],
    config: &RenderOptions,
) -> String {
    let mut result = text.to_string();

    let (ps, ts) = syntax_and_themes();

    let theme = ts.themes.get(&config.syntax_theme).unwrap_or_else(|| {
        eprintln!(
            "Nonograph: Warning: Theme '{}' not found, falling back to 'base16-ocean.dark'",
            config.syntax_theme
        );
        &ts.themes["base16-ocean.dark"]
    });

    for (index, (language, code_content, line_count)) in fenced_blocks.iter().enumerate() {
        let placeholder = format!("{{{{FENCEDBLOCK{}}}}}", index);
        let syntect_lang = map_language_for_syntect(language);

        // Generate the complete HTML structure
        let replacement = render_code_block(
            &ps,
            theme,
            language,
            &syntect_lang,
            code_content,
            *line_count,
        );

        result = result.replace(&placeholder, &replacement);
    }

    result
}

fn syntax_and_themes() -> (&'static SyntaxSet, &'static ThemeSet) {
    static SYNTAX: OnceLock<SyntaxSet> = OnceLock::new();
    static THEMES: OnceLock<ThemeSet> = OnceLock::new();
    (
        SYNTAX.get_or_init(SyntaxSet::load_defaults_newlines),
        THEMES.get_or_init(ThemeSet::load_defaults),
    )
}

fn render_code_block(
    syntax_set: &SyntaxSet,
    theme: &syntect::highlighting::Theme,
    original_language: &str,
    syntect_language: &str,
    code_content: &str,
    line_count: u32,
) -> String {
    // Find syntax for language - syntect_language is already mapped to syntect names
    let (syntax, auto_detected) = if let Some(s) = syntax_set.find_syntax_by_name(syntect_language)
    {
        (s, false)
    } else if original_language.is_empty() {
        // No language specified — try to auto-detect from first line
        if let Some(s) = syntax_set.find_syntax_by_first_line(code_content) {
            (s, true)
        } else {
            (syntax_set.find_syntax_plain_text(), false)
        }
    } else {
        // Language specified but not recognised — still try first-line as fallback
        (
            syntax_set
                .find_syntax_by_first_line(code_content)
                .unwrap_or_else(|| syntax_set.find_syntax_plain_text()),
            false,
        )
    };

    // Generate syntax-highlighted code
    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut highlighted_code = String::new();

    for line in LinesWithEndings::from(code_content) {
        let line_html = match highlighter.highlight_line(line, syntax_set) {
            Ok(ranges) => {
                match styled_line_to_highlighted_html(&ranges[..], IncludeBackground::No) {
                    Ok(html) => html,
                    Err(_) => html_escape(line),
                }
            }
            Err(_) => html_escape(line),
        };
        highlighted_code.push_str(&format!("<span class=\"code-line\">{}</span>", line_html));
    }

    // Generate line numbers
    let line_numbers = (1..=line_count)
        .map(|i| format!("<span class=\"line-number\">{}</span>", i))
        .collect::<Vec<_>>()
        .join("");

    // Create the complete HTML structure
    let css_lang = map_language_for_css(original_language);
    let lang_display = if auto_detected && syntax.name != "Plain Text" {
        format!(
            "<span class=\"code-language code-language-detected\" title=\"auto-detected\">{}</span>",
            syntax.name.to_uppercase()
        )
    } else if css_lang.is_empty() {
        String::new()
    } else {
        format!(
            "<span class=\"code-language\">{}</span>",
            css_lang.to_uppercase()
        )
    };

    let class_attr = if css_lang.is_empty() {
        String::new()
    } else {
        format!(" class=\"language-{}\"", css_lang)
    };

    format!(
        r#"<pre{}><div class="code-header">{}<div class="code-controls"><button class="wrap-button"><svg class="btn-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="2" y1="4" x2="14" y2="4"/><line x1="2" y1="8" x2="11" y2="8"/><line x1="2" y1="12" x2="9" y2="12"/></svg><span class="btn-label">Wrap</span></button><button class="collapse-button"><svg class="btn-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="14,11 8,5 2,11"/></svg><svg class="btn-icon btn-icon-alt" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="2,5 8,11 14,5"/></svg><span class="btn-label">Collapse</span></button><button class="copy-button"><svg class="btn-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="2" width="10" height="13" rx="1"/><rect x="6" y="1" width="4" height="3" rx="0.5"/></svg><svg class="btn-icon btn-icon-alt" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="2,8 6,12 14,4"/></svg><span class="btn-label">Copy</span></button></div></div><div class="line-numbers">{}</div><div class="code-wrapper"><code>{}</code></div></pre>"#,
        class_attr, lang_display, line_numbers, highlighted_code
    )
}

fn format_paragraphs_with_headers(text: &str) -> String {
    let mut result = String::with_capacity(text.len() + (text.len() / 10));
    let mut header_count = 0;

    // Preprocess text to ensure headers are properly separated
    let preprocessed = preprocess_headers_for_paragraphs(text);
    let parts: Vec<&str> = preprocessed.split("\n\n").collect();

    for (i, part) in parts.iter().enumerate() {
        let trimmed = part.trim();

        if trimmed.is_empty() {
            continue;
        }

        // Check for headers first
        if let Some(header) = process_single_header(trimmed, &mut header_count) {
            result.push_str(&header);
        }
        // Check for blockquotes
        else if trimmed.lines().any(|line| line.trim().starts_with("> ")) {
            result.push_str(&process_single_blockquote(trimmed));
        } else if (trimmed.contains("{{FENCEDBLOCK") || trimmed.contains("<table>"))
            && !trimmed.starts_with("{{FENCEDBLOCK")
            && !trimmed.starts_with("<table>")
        {
            let lines: Vec<&str> = part.lines().collect();
            let mut current_paragraph = String::new();

            for line in lines {
                let line_trimmed = line.trim();

                if line_trimmed.starts_with("{{FENCEDBLOCK") || line_trimmed.starts_with("<table>")
                {
                    if !current_paragraph.is_empty() {
                        result.push_str(&format!("<p>{}</p>\n", current_paragraph.trim()));
                        current_paragraph.clear();
                    }
                    result.push_str(line_trimmed);
                    result.push('\n');
                } else if !line_trimmed.is_empty() {
                    if !current_paragraph.is_empty() {
                        current_paragraph.push_str("<br>");
                    }
                    current_paragraph.push_str(line_trimmed);
                }
            }

            if !current_paragraph.is_empty() {
                result.push_str(&format!("<p>{}</p>", current_paragraph.trim()));
            }
        } else if trimmed.starts_with("{{FENCEDBLOCK")
            || trimmed.starts_with("<img ")
            || trimmed.starts_with("<video ")
            || trimmed.starts_with("<table>")
        {
            result.push_str(trimmed);
        } else {
            let lines: Vec<&str> = part.lines().collect();
            let mut paragraph_content = String::new();

            for (j, line) in lines.iter().enumerate() {
                let trimmed_line = line.trim();
                if !trimmed_line.is_empty() {
                    paragraph_content.push_str(trimmed_line);
                    if j < lines.len() - 1
                        && lines
                            .get(j + 1)
                            .map_or(false, |next| !next.trim().is_empty())
                    {
                        paragraph_content.push_str("<br>");
                    }
                }
            }

            if !paragraph_content.is_empty() {
                result.push_str(&format!("<p>{}</p>", paragraph_content));
            }
        }

        if i < parts.len() - 1 && !result.is_empty() && !result.ends_with('\n') {
            result.push('\n');
        }
    }

    // Clean up empty paragraphs and excessive spacing
    result = result.replace("<p></p>", "");
    result = result.replace("\n\n\n", "\n\n");

    // Remove excessive br tags before tables - more aggressive cleanup
    let mut iterations = 0;
    while result.contains("<br><table>") && iterations < 50 {
        result = result.replace("<br><br>", "<br>");
        result = result.replace("<br><table>", "<table>");
        result = result.replace("<br>\n<table>", "\n<table>");
        iterations += 1;
    }

    result
}

fn process_dividers(content: &str) -> String {
    let mut result = String::new();

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed == "***" && line.chars().all(|c| c == '*' || c.is_whitespace()) {
            // Three stars divider - centered asterisks
            result.push_str("<div class=\"divider-stars\"><div class=\"asterisk\"><div class=\"center\"></div></div><div class=\"asterisk\"><div class=\"center\"></div></div><div class=\"asterisk\"><div class=\"center\"></div></div></div>");
        } else if trimmed == "-*-"
            && line
                .chars()
                .all(|c| c == '-' || c == '*' || c.is_whitespace())
        {
            // Single asterisk divider - centered single asterisk
            result.push_str("<div class=\"divider-asterisk\"><div class=\"center\"></div></div>");
        } else if trimmed == "---" && line.chars().all(|c| c == '-' || c.is_whitespace()) {
            // Horizontal thin divider
            result.push_str("<hr class=\"divider-thin\">");
        } else if trimmed == "===" && line.chars().all(|c| c == '=' || c.is_whitespace()) {
            // Horizontal double-line divider
            result.push_str("<hr class=\"divider-double\">");
        } else {
            result.push_str(line);
        }
        result.push('\n');
    }

    result
}

fn preprocess_headers_for_paragraphs(text: &str) -> String {
    let mut result = String::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // Check if this line is a header
        if trimmed.starts_with("# ")
            || trimmed.starts_with("## ")
            || trimmed.starts_with("### ")
            || trimmed.starts_with("#### ")
        {
            // Add the header line
            result.push_str(line);
            result.push('\n');

            // Always add an extra newline after headers to ensure proper separation
            // This forces headers to be in their own paragraph blocks
            result.push('\n');
        } else {
            result.push_str(line);
            result.push('\n');
        }

        i += 1;
    }

    result
}

fn process_tables(text: &str) -> String {
    let mut result = String::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();

        // Check if this line looks like a table header (contains |)
        if line.contains('|') && line.len() > 0 {
            // Look for separator line
            if i + 1 < lines.len() {
                let separator = lines[i + 1].trim();
                if is_table_separator(separator) {
                    if !result.is_empty() && !result.ends_with("\n\n") {
                        while !result.ends_with('\n') {
                            result.push('\n');
                        }
                        result.push('\n');
                    }

                    let (table_html, lines_consumed) = parse_table(&lines[i..]);
                    result.push_str(&table_html);
                    i += lines_consumed;

                    // Guarantee a blank line after the table as well.
                    if !result.ends_with('\n') {
                        result.push('\n');
                    }
                    result.push('\n');
                    continue;
                }
            }
        }

        // Not a table line, add as is
        result.push_str(lines[i]);
        if i < lines.len() - 1 {
            result.push('\n');
        }
        i += 1;
    }

    result
}

fn is_table_separator(line: &str) -> bool {
    let line = line.trim();
    if line.is_empty() || !line.contains('|') {
        return false;
    }

    // Check if line contains only |, -, :, and spaces
    line.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '))
}

fn parse_table(lines: &[&str]) -> (String, usize) {
    if lines.len() < 2 {
        return (String::new(), 0);
    }

    let header_line = lines[0].trim();
    let separator_line = lines[1].trim();

    // Parse header
    let headers = parse_table_row(header_line);
    let alignments = parse_table_alignments(separator_line);

    let mut table_html = String::from("<table>\n<thead>\n<tr>");
    for (i, header) in headers.iter().enumerate() {
        let style = match alignments.get(i).unwrap_or(&TableAlignment::Left) {
            TableAlignment::Left => "",
            TableAlignment::Center => " style=\"text-align: center\"",
            TableAlignment::Right => " style=\"text-align: right\"",
        };
        table_html.push_str(&format!("<th{}>{}</th>", style, header.trim()));
    }
    table_html.push_str("</tr>\n</thead>\n<tbody>\n");

    // Parse body rows
    let mut rows_processed = 2; // header + separator
    for line_idx in 2..lines.len() {
        let line = lines[line_idx].trim();
        if line.is_empty() || !line.contains('|') {
            break;
        }

        let cells = parse_table_row(line);
        table_html.push_str("<tr>");
        for (i, cell) in cells.iter().enumerate() {
            let style = match alignments.get(i).unwrap_or(&TableAlignment::Left) {
                TableAlignment::Left => "",
                TableAlignment::Center => " style=\"text-align: center\"",
                TableAlignment::Right => " style=\"text-align: right\"",
            };
            table_html.push_str(&format!("<td{}>{}</td>", style, cell.trim()));
        }
        table_html.push_str("</tr>\n");
        rows_processed += 1;
    }

    table_html.push_str("</tbody>\n</table>\n");
    (table_html, rows_processed)
}

fn parse_table_row(line: &str) -> Vec<String> {
    let line = line.trim();
    let line = if line.starts_with('|') {
        &line[1..]
    } else {
        line
    };
    let line = if line.ends_with('|') {
        &line[..line.len() - 1]
    } else {
        line
    };

    line.split('|').map(|s| s.trim().to_string()).collect()
}

#[derive(Debug, Clone)]
enum TableAlignment {
    Left,
    Center,
    Right,
}

fn parse_table_alignments(separator: &str) -> Vec<TableAlignment> {
    let cells = parse_table_row(separator);
    cells
        .iter()
        .map(|cell| {
            let cell = cell.trim();
            if cell.starts_with(':') && cell.ends_with(':') {
                TableAlignment::Center
            } else if cell.ends_with(':') {
                TableAlignment::Right
            } else {
                TableAlignment::Left
            }
        })
        .collect()
}

pub fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

fn extract_media_syntax(text: &str) -> (String, Vec<String>) {
    let mut result = String::new();
    let mut media_blocks = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if i + 1 < chars.len() && chars[i] == '!' && chars[i + 1] == '[' {
            // Find closing bracket
            let mut bracket_end = None;
            let mut j = i + 2;
            while j < chars.len() && chars[j] != '\n' {
                if chars[j] == ']' {
                    bracket_end = Some(j);
                    break;
                }
                j += 1;
            }
            if let Some(b_end) = bracket_end {
                if b_end + 1 < chars.len() && chars[b_end + 1] == '(' {
                    let mut paren_end = None;
                    let mut k = b_end + 2;
                    while k < chars.len() && chars[k] != '\n' {
                        if chars[k] == ')' {
                            paren_end = Some(k);
                            break;
                        }
                        k += 1;
                    }
                    if let Some(p_end) = paren_end {
                        let raw: String = chars[i..=p_end].iter().collect();
                        let placeholder = format!("{{{{MEDIASYNTAX{}}}}}", media_blocks.len());
                        media_blocks.push(raw);
                        result.push_str(&placeholder);
                        i = p_end + 1;
                        continue;
                    }
                }
            }
        }
        result.push(chars[i]);
        i += 1;
    }

    (result, media_blocks)
}

fn restore_media_syntax(text: &str, media_blocks: &[String]) -> String {
    let mut result = text.to_string();
    for (index, raw) in media_blocks.iter().enumerate() {
        let placeholder = format!("{{{{MEDIASYNTAX{}}}}}", index);
        result = result.replace(&placeholder, raw);
    }
    result
}

fn extract_link_syntax(text: &str) -> (String, Vec<String>) {
    let mut result = String::new();
    let mut link_blocks = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '[' && !(i > 0 && chars[i - 1] == '!') {
            let mut bracket_end = None;
            let mut j = i + 1;
            while j < chars.len() && chars[j] != '\n' {
                if chars[j] == ']' {
                    bracket_end = Some(j);
                    break;
                }
                j += 1;
            }

            if let Some(b_end) = bracket_end {
                if b_end + 1 < chars.len() && chars[b_end + 1] == '(' {
                    let mut paren_end = None;
                    let mut k = b_end + 2;
                    while k < chars.len() && chars[k] != '\n' {
                        if chars[k] == ')' {
                            paren_end = Some(k);
                            break;
                        }
                        k += 1;
                    }
                    if let Some(p_end) = paren_end {
                        let raw: String = chars[i..=p_end].iter().collect();
                        let placeholder = format!("{{{{LINKSYNTAX{}}}}}", link_blocks.len());
                        link_blocks.push(raw);
                        result.push_str(&placeholder);
                        i = p_end + 1;
                        continue;
                    }
                }

                let inner: String = chars[(i + 1)..b_end].iter().collect();
                if inner.starts_with("http") {
                    let raw: String = chars[i..=b_end].iter().collect();
                    let placeholder = format!("{{{{LINKSYNTAX{}}}}}", link_blocks.len());
                    link_blocks.push(raw);
                    result.push_str(&placeholder);
                    i = b_end + 1;
                    continue;
                }
            }
        }

        result.push(chars[i]);
        i += 1;
    }

    (result, link_blocks)
}

fn restore_link_syntax(text: &str, link_blocks: &[String]) -> String {
    let mut result = text.to_string();
    for (index, raw) in link_blocks.iter().enumerate() {
        let placeholder = format!("{{{{LINKSYNTAX{}}}}}", index);
        result = result.replace(&placeholder, raw);
    }
    result
}

fn extract_code_blocks(text: &str) -> (String, Vec<String>) {
    let mut result = String::new();
    let mut code_blocks = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '`' {
            // Look for closing backtick
            let start = i + 1;
            let mut end = None;

            for j in start..chars.len() {
                if chars[j] == '`' {
                    end = Some(j);
                    break;
                }
            }

            if let Some(end_pos) = end {
                let code_content: String = chars[start..end_pos].iter().collect();

                if !code_content.is_empty() && !code_content.contains('\n') {
                    let placeholder = format!("{{{{CODEBLOCK{}}}}}", code_blocks.len());
                    code_blocks.push(code_content);
                    result.push_str(&placeholder);
                    i = end_pos + 1;
                    continue;
                }
            }
        }

        result.push(chars[i]);
        i += 1;
    }

    (result, code_blocks)
}

fn restore_code_blocks(text: &str, code_blocks: &[String]) -> String {
    let mut result = text.to_string();

    for (index, code_content) in code_blocks.iter().enumerate() {
        let placeholder = format!("{{{{CODEBLOCK{}}}}}", index);
        let escaped_content = html_escape(code_content);
        let replacement = format!("<code>{}</code>", escaped_content);
        result = result.replace(&placeholder, &replacement);
    }

    result
}

fn is_comment_line(line: &str) -> bool {
    line.trim_start().starts_with("// ")
}

fn process_comments(content: &str) -> String {
    content
        .lines()
        .filter(|line| !is_comment_line(line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Markdown safe to embed in a page.
///
/// Prose comments (`// ` at the start of a line) are omitted, including indented
/// ones. Comments inside fenced code blocks stay, matching [`process_comments`],
/// which runs only after fences have been extracted. The result renders the same
/// HTML as `content`. When there is nothing to omit, `content` is returned unchanged.
pub fn markdown_for_page(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut kept = Vec::with_capacity(lines.len());
    let mut removed = false;
    let mut i = 0;

    while i < lines.len() {
        if let Some(end) = fenced_block_end(&lines, i) {
            kept.extend_from_slice(&lines[i..=end]);
            i = end + 1;
            continue;
        }

        if is_comment_line(lines[i]) {
            removed = true;
            i += 1;
            continue;
        }

        kept.push(lines[i]);
        i += 1;
    }

    if !removed {
        content.to_string()
    } else {
        kept.join("\n")
    }
}

fn fenced_block_end(lines: &[&str], index: usize) -> Option<usize> {
    let line = lines[index];
    if !line.starts_with("```") {
        return None;
    }

    let fence_length = line.chars().take_while(|&c| c == '`').count();
    for end in (index + 1)..lines.len() {
        let closing = lines[end];
        if !closing.starts_with("```") {
            continue;
        }
        let closing_length = closing.chars().take_while(|&c| c == '`').count();
        if closing_length >= fence_length {
            return Some(end);
        }
    }

    None
}

fn process_footnotes(content: &str) -> String {
    let mut result = String::new();
    let mut footnote_definitions = std::collections::HashMap::new();
    let mut footnote_counter = 0u32;
    let mut inline_footnote_counter = 0u32;

    // First pass: extract footnote definitions [^id]: text
    let lines: Vec<&str> = content.lines().collect();
    let mut content_lines = Vec::new();

    for line in &lines {
        let trimmed = line.trim();
        if trimmed.starts_with("[^") && trimmed.contains("]:") {
            if let Some(colon_pos) = trimmed.find("]:") {
                let id_part = &trimmed[2..colon_pos];
                let definition = trimmed[colon_pos + 2..].trim();
                footnote_definitions.insert(id_part.to_string(), definition.to_string());
            }
        } else {
            content_lines.push(*line);
        }
    }

    let content_text = content_lines.join("\n");
    let chars: Vec<char> = content_text.chars().collect();
    let mut i = 0;
    let mut footnote_references = Vec::new();
    let mut inline_footnotes = Vec::new();

    // Second pass: process footnote references and inline footnotes
    while i < chars.len() {
        if chars.len() >= 3 && i < chars.len() - 2 && chars[i] == '^' && chars[i + 1] == '[' {
            // Inline footnote: ^[text]
            let mut bracket_end = None;
            let mut j = i + 2;
            let mut bracket_depth = 1;

            while j < chars.len() && bracket_depth > 0 {
                if chars[j] == '[' {
                    bracket_depth += 1;
                } else if chars[j] == ']' {
                    bracket_depth -= 1;
                    if bracket_depth == 0 {
                        bracket_end = Some(j);
                        break;
                    }
                }
                j += 1;
            }

            if let Some(end_pos) = bracket_end {
                inline_footnote_counter += 1;
                let footnote_text: String = chars[(i + 2)..end_pos].iter().collect();
                let footnote_id = format!("ifn{}", inline_footnote_counter);

                inline_footnotes.push((footnote_id.clone(), footnote_text));

                // Use placeholder to avoid processing by other markdown processors
                result.push_str(&format!("XFOOTNOTEINLINEX{}XENDX", inline_footnote_counter));

                i = end_pos + 1;
                continue;
            }
        } else if chars.len() >= 4 && i < chars.len() - 3 && chars[i] == '[' && chars[i + 1] == '^'
        {
            // Reference footnote: [^id]
            let mut bracket_end = None;
            let mut j = i + 2;

            while j < chars.len() && chars[j] != '\n' {
                if chars[j] == ']' {
                    bracket_end = Some(j);
                    break;
                }
                j += 1;
            }

            if let Some(end_pos) = bracket_end {
                let footnote_id: String = chars[(i + 2)..end_pos].iter().collect();

                if footnote_definitions.contains_key(&footnote_id) {
                    footnote_counter += 1;
                    footnote_references.push((footnote_id.clone(), footnote_counter));

                    // Use placeholder to avoid processing by other markdown processors
                    result.push_str(&format!("XFOOTNOTEREFX{}XENDX", footnote_counter));

                    i = end_pos + 1;
                    continue;
                }
            }
        }

        result.push(chars[i]);
        i += 1;
    }

    // Replace placeholders with actual HTML
    for i in 1..=footnote_counter {
        let placeholder = format!("XFOOTNOTEREFX{}XENDX", i);
        let replacement = format!(
            "<sup><a href=\"XHASHXFN{}\" id=\"fnref{}\">{}</a></sup>",
            i, i, i
        );
        result = result.replace(&placeholder, &replacement);
    }

    for i in 1..=inline_footnote_counter {
        let placeholder = format!("XFOOTNOTEINLINEX{}XENDX", i);
        let replacement = format!(
            "<sup><a href=\"XHASHXifn{}\" id=\"ifn{}ref\">{}</a></sup>",
            i, i, i
        );
        result = result.replace(&placeholder, &replacement);
    }

    // Add footnotes section at the end if there are any footnotes
    if !footnote_references.is_empty() || !inline_footnotes.is_empty() {
        result.push_str("\n\nXFOOTNOTESECTIONSTARTX");

        // Add reference footnotes
        for (footnote_id, number) in footnote_references {
            if let Some(definition) = footnote_definitions.get(&footnote_id) {
                result.push_str(&format!(
                    "<li id=\"fn{}\">{} <a href=\"XHASHXfnref{}\" class=\"footnote-backref\">↩</a></li>",
                    number, definition, number
                ));
            }
        }

        // Add inline footnotes
        for (footnote_id, footnote_text) in inline_footnotes.iter() {
            result.push_str(&format!(
                "<li id=\"{}\">{} <a href=\"XHASHX{}ref\" class=\"footnote-backref\">↩</a></li>",
                footnote_id, footnote_text, footnote_id
            ));
        }

        result.push_str("XFOOTNOTESECTIONENDX");
    }

    result
}

fn restore_footnotes(text: &str) -> String {
    let mut result = text.to_string();

    // Replace footnote section placeholders
    result = result.replace(
        "XFOOTNOTESECTIONSTARTX",
        "<div class=\"footnotes\">\n<ol>\n",
    );
    result = result.replace("XFOOTNOTESECTIONENDX", "</ol>\n</div>");

    // Replace hash placeholders
    result = result.replace("XHASHX", "#");

    result
}

fn process_links_with_config(text: &str, config: &RenderOptions) -> String {
    let mut result = String::with_capacity(text.len() + 1024);
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '[' {
            // Find closing bracket
            let mut bracket_end = None;
            let mut j = i + 1;
            while j < chars.len() && chars[j] != '\n' {
                if chars[j] == ']' {
                    bracket_end = Some(j);
                    break;
                }
                j += 1;
            }

            if let Some(bracket_end_idx) = bracket_end {
                // Check for [text](url) pattern
                if bracket_end_idx + 1 < chars.len() && chars[bracket_end_idx + 1] == '(' {
                    let mut paren_end = None;
                    let mut k = bracket_end_idx + 2;
                    while k < chars.len() && chars[k] != '\n' {
                        if chars[k] == ')' {
                            paren_end = Some(k);
                            break;
                        }
                        k += 1;
                    }

                    if let Some(paren_end_idx) = paren_end {
                        let link_text: String = chars[(i + 1)..bracket_end_idx].iter().collect();
                        let link_url: String =
                            chars[(bracket_end_idx + 2)..paren_end_idx].iter().collect();

                        if !link_text.is_empty()
                            && !link_url.is_empty()
                            && link_url.len() <= config.max_url_length
                        {
                            result.push_str("<a href=\"");
                            result.push_str(&link_url);
                            if config.external_link_security {
                                result.push_str("\" target=\"_blank\">");
                            } else {
                                result.push_str("\">");
                            }
                            result.push_str(&link_text);
                            result.push_str("</a>");
                            i = paren_end_idx + 1;
                            continue;
                        }
                    }
                }

                // Check for [url] pattern (bare URL in brackets)
                let link_url: String = chars[(i + 1)..bracket_end_idx].iter().collect();
                if link_url.len() <= config.max_url_length && link_url.starts_with("http") {
                    result.push_str("<a href=\"");
                    result.push_str(&link_url);
                    if config.external_link_security {
                        result.push_str("\" target=\"_blank\">");
                    } else {
                        result.push_str("\">");
                    }
                    result.push_str(&link_url);
                    result.push_str("</a>");
                    i = bracket_end_idx + 1;
                    continue;
                }
            }
        }

        // No pattern matched, add current character
        result.push(chars[i]);
        i += 1;
    }

    result
}

#[cfg(test)]
#[path = "../test/parser.rs"]
mod tests;
