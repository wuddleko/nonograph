use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::html::{styled_line_to_highlighted_html, IncludeBackground};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;
use url::Url;

const MAX_BLOCKS: usize = 512;
const MAX_HTML_BYTES: usize = 4 * 1024 * 1024;
const ID_PREFIX: &str = "ng-";
const CODE_CONTROLS: &str = concat!(
    "<button class=\"wrap-button\"><svg class=\"btn-icon\" viewBox=\"0 0 16 16\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\">",
    "<line x1=\"2\" y1=\"4\" x2=\"14\" y2=\"4\"></line><line x1=\"2\" y1=\"8\" x2=\"11\" y2=\"8\"></line><line x1=\"2\" y1=\"12\" x2=\"9\" y2=\"12\"></line>",
    "</svg><span class=\"btn-label\">Wrap</span></button>",
    "<button class=\"collapse-button\"><svg class=\"btn-icon\" viewBox=\"0 0 16 16\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\">",
    "<polyline points=\"14,11 8,5 2,11\"></polyline></svg>",
    "<svg class=\"btn-icon btn-icon-alt\" viewBox=\"0 0 16 16\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\">",
    "<polyline points=\"2,5 8,11 14,5\"></polyline></svg><span class=\"btn-label\">Collapse</span></button>",
    "<button class=\"copy-button\"><svg class=\"btn-icon\" viewBox=\"0 0 16 16\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\">",
    "<rect x=\"3\" y=\"2\" width=\"10\" height=\"13\" rx=\"1\"></rect><rect x=\"6\" y=\"1\" width=\"4\" height=\"3\" rx=\"0.5\"></rect></svg>",
    "<svg class=\"btn-icon btn-icon-alt\" viewBox=\"0 0 16 16\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\">",
    "<polyline points=\"2,8 6,12 14,4\"></polyline></svg><span class=\"btn-label\">Copy</span></button>",
);
const TASK_OPEN: &str = "<input type=\"checkbox\"> ";
const TASK_DONE: &str = "<input type=\"checkbox\" checked=\"\"> ";

struct Marks {
    stamp: String,
}

impl Marks {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let salt = &NEXT as *const AtomicU64 as usize;
        Self {
            stamp: format!("ngm{n:x}{salt:x}q"),
        }
    }

    fn slot(&self, kind: char, index: usize) -> String {
        format!("{}{kind}{index}z", self.stamp)
    }

    fn prefix(&self, kind: char) -> String {
        format!("{}{kind}", self.stamp)
    }

    fn marker(&self, kind: char) -> String {
        format!("{}{kind}z", self.stamp)
    }
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
            let lookup = ascii_domain.trim_end_matches('.');

            if lookup == "localhost" {
                return false;
            }

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
    let marks = Marks::new();
    let cleaned_content = remove_standalone_list_tags(content);

    let (protected_content, fenced_blocks) = extract_fenced_code_blocks(&cleaned_content, &marks);
    let (mut working_content, code_blocks) = extract_code_blocks(&protected_content, &marks);

    // Process comments before other formatting to remove them from HTML output
    working_content = process_comments(&working_content);

    let (working_content_no_media, media_blocks) = extract_media_syntax(&working_content, &marks);
    working_content = working_content_no_media;

    let (working_content_no_links, link_blocks) = extract_link_syntax(&working_content, &marks);
    working_content = working_content_no_links;

    // Process footnotes before text formatting to avoid conflicts with ^ and []
    working_content = process_footnotes(&working_content, &marks);

    working_content = apply_emphasis(&working_content);

    working_content =
        restore_media_and_links(&working_content, &media_blocks, &link_blocks, &marks);
    working_content = process_images_and_links(&working_content, config);
    working_content = process_tables(&working_content);
    working_content = process_lists(&working_content, &marks);
    working_content = process_dividers(&working_content);
    working_content = format_paragraphs_with_headers(&working_content, &marks);
    working_content = restore_rendered_blocks(
        &working_content,
        &fenced_blocks,
        &code_blocks,
        config,
        &marks,
    );
    working_content = restore_footnotes(&working_content, &marks);

    finish_html(content, working_content, &marks)
}

pub fn render_inline(content: &str) -> String {
    render_inline_with_config(content, &RenderOptions::default())
}

pub fn render_inline_with_config(content: &str, config: &RenderOptions) -> String {
    let marks = Marks::new();
    let cleaned_content = remove_standalone_list_tags(content);
    let (working_content, code_blocks) = extract_code_blocks(&cleaned_content, &marks);
    let (working_content_no_media, media_blocks) = extract_media_syntax(&working_content, &marks);
    let (working_content_no_links, link_blocks) =
        extract_link_syntax(&working_content_no_media, &marks);
    let mut working_content = apply_emphasis(&working_content_no_links);
    working_content =
        restore_media_and_links(&working_content, &media_blocks, &link_blocks, &marks);
    working_content = process_images_and_links(&working_content, config);
    working_content = restore_code_blocks(&working_content, &code_blocks, &marks);
    finish_html(content, working_content, &marks)
}

fn apply_emphasis(text: &str) -> String {
    let mut working_content = safe_replace(text, "**", "**", "<strong>", "</strong>");
    working_content = safe_replace(&working_content, "*", "*", "<em>", "</em>");
    working_content = safe_replace(&working_content, "_", "_", "<u>", "</u>");
    working_content = safe_replace(&working_content, "~", "~", "<del>", "</del>");
    working_content = safe_replace(&working_content, "^", "^", "<sup>", "</sup>");
    working_content = safe_replace(&working_content, "==", "==", "<mark>", "</mark>");
    safe_replace(
        &working_content,
        "#",
        "#",
        "<span class=\"secret\">",
        "</span>",
    )
}

fn process_images_and_links(text: &str, config: &RenderOptions) -> String {
    let bytes = text.as_bytes();
    let mut result = String::with_capacity(text.len() + 1024);
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'!' && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            if let Some(next) = try_consume_image(&mut result, text, i, config) {
                i = next;
                continue;
            }
            result.push('!');
            i += 1;
            continue;
        }
        if bytes[i] == b'[' {
            if let Some(next) = try_consume_link(&mut result, text, i, config) {
                i = next;
                continue;
            }
            result.push('[');
            i += 1;
            continue;
        }
        let next = next_ascii_marker(bytes, i, b'!', b'[');
        result.push_str(&text[i..next]);
        i = next;
    }

    result
}

fn try_consume_image(
    result: &mut String,
    text: &str,
    i: usize,
    config: &RenderOptions,
) -> Option<usize> {
    let bytes = text.as_bytes();
    let bracket_end = find_ascii_until_newline(bytes, i + 2, b']')?;
    if bracket_end + 1 >= bytes.len() || bytes[bracket_end + 1] != b'(' {
        return None;
    }
    let paren_end = find_ascii_until_newline(bytes, bracket_end + 2, b')')?;
    let alt_text = &text[i + 2..bracket_end];
    let image_url = &text[bracket_end + 2..paren_end];
    if image_url.is_empty() || image_url.len() > config.max_url_length || !is_safe_url(image_url) {
        return None;
    }
    push_image_html(result, alt_text, image_url);
    Some(paren_end + 1)
}

fn push_image_html(result: &mut String, alt_text: &str, image_url: &str) {
    let is_video = is_video_url(image_url);
    if !alt_text.trim().is_empty() {
        result.push_str("<div class=\"media-with-caption\">");
        push_media_tag(result, image_url, alt_text, is_video);
        result.push_str("<div class=\"media-caption\">");
        push_html_escape(result, alt_text, false);
        result.push_str("</div></div>");
    } else {
        push_media_tag(result, image_url, alt_text, is_video);
    }
}

fn push_media_tag(result: &mut String, image_url: &str, alt_text: &str, is_video: bool) {
    if is_video {
        result.push_str("<video controls style=\"width: 100%;\"><source src=\"");
        push_html_escape(result, image_url, false);
        result.push_str("\" type=\"");
        result.push_str(get_video_mime_type(image_url));
        result.push_str("\">Your browser does not support the video tag.</video>");
    } else {
        result.push_str("<img src=\"");
        push_html_escape(result, image_url, false);
        result.push_str("\" alt=\"");
        push_html_escape(result, alt_text, false);
        result.push_str("\">");
    }
}

fn try_consume_link(
    result: &mut String,
    text: &str,
    i: usize,
    config: &RenderOptions,
) -> Option<usize> {
    let bytes = text.as_bytes();
    let bracket_end = find_ascii_until_newline(bytes, i + 1, b']')?;
    if bracket_end + 1 < bytes.len() && bytes[bracket_end + 1] == b'(' {
        if let Some(paren_end) = find_ascii_until_newline(bytes, bracket_end + 2, b')') {
            let link_text = &text[i + 1..bracket_end];
            let link_url = &text[bracket_end + 2..paren_end];
            if !is_safe_url(link_url) {
                return None;
            }
            if !link_text.is_empty()
                && !link_url.is_empty()
                && link_url.len() <= config.max_url_length
            {
                push_anchor(result, link_url, link_text, config.external_link_security);
                return Some(paren_end + 1);
            }
        }
    }
    let link_url = &text[i + 1..bracket_end];
    if link_url.len() <= config.max_url_length
        && link_url.starts_with("http")
        && is_safe_url(link_url)
    {
        push_anchor(result, link_url, link_url, config.external_link_security);
        return Some(bracket_end + 1);
    }
    None
}

fn push_anchor(result: &mut String, href: &str, text: &str, external: bool) {
    result.push_str("<a href=\"");
    result.push_str(href);
    if external {
        result.push_str("\" target=\"_blank\">");
    } else {
        result.push_str("\">");
    }
    result.push_str(text);
    result.push_str("</a>");
}

fn find_ascii_until_newline(bytes: &[u8], start: usize, target: u8) -> Option<usize> {
    let mut j = start;
    while j < bytes.len() && bytes[j] != b'\n' {
        if bytes[j] == target {
            return Some(j);
        }
        j += 1;
    }
    None
}

fn next_byte(bytes: &[u8], start: usize, marker: u8) -> usize {
    let mut i = start + 1;
    while i < bytes.len() && bytes[i] != marker {
        i += 1;
    }
    i
}

fn next_ascii_marker(bytes: &[u8], start: usize, first: u8, second: u8) -> usize {
    let mut i = start + 1;
    while i < bytes.len() && bytes[i] != first && bytes[i] != second {
        i += 1;
    }
    i
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

fn html_cleaner() -> &'static ammonia::Builder<'static> {
    static CLEANER: OnceLock<ammonia::Builder<'static>> = OnceLock::new();
    CLEANER.get_or_init(|| {
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
            ])
            .add_tag_attributes("video", &["controls"])
            .add_tag_attributes("source", &["src", "type"])
            .add_tag_attributes("img", &["src", "alt"])
            .add_tag_attributes("code", &["class", "data-line-count"])
            .add_tag_attributes("span", &["class"])
            .add_tag_attributes("a", &["href", "target", "id", "class"])
            .add_tag_attributes("div", &["class"])
            .add_tag_attributes("hr", &["class"])
            .add_tag_attributes("ul", &["class"])
            .add_tag_attributes("li", &["id", "class"])
            .add_tag_attributes("sup", &["id"])
            .add_tag_attributes("h1", &["id"])
            .add_tag_attributes("h2", &["id"])
            .add_tag_attributes("h3", &["id"])
            .add_tag_attributes("h4", &["id"])
            .add_tag_attributes("pre", &["class"])
            .attribute_filter(keep_generated_ids)
            .link_rel(Some("noopener noreferrer"));
        builder
    })
}

fn keep_generated_ids<'a>(
    _element: &str,
    attribute: &str,
    value: &'a str,
) -> Option<std::borrow::Cow<'a, str>> {
    if attribute == "id" && !generated_id(value) {
        None
    } else {
        Some(std::borrow::Cow::Borrowed(value))
    }
}

fn generated_id(value: &str) -> bool {
    let Some(rest) = value.strip_prefix(ID_PREFIX) else {
        return false;
    };
    let rest = if let Some(number) = rest.strip_prefix("fnref") {
        number
    } else if let Some(number) = rest.strip_prefix("fn") {
        number
    } else if let Some(rest) = rest.strip_prefix("ifn") {
        rest.strip_suffix("ref").unwrap_or(rest)
    } else if let Some(number) = rest.strip_prefix('h') {
        number
    } else {
        return false;
    };
    !rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_digit())
}

fn sanitize_html(html: String) -> String {
    html_cleaner().clean(&html).to_string()
}

fn text_cleaner() -> &'static ammonia::Builder<'static> {
    static CLEANER: OnceLock<ammonia::Builder<'static>> = OnceLock::new();
    CLEANER.get_or_init(ammonia::Builder::empty)
}

pub fn strip_tags(text: &str) -> String {
    let sanitized = text_cleaner().clean(text).to_string();
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
    let mut out = String::with_capacity(text.len());
    push_html_escape(&mut out, text, true);
    out
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
            "<h4 id=\"ng-h{}\">{}<a href=\"#ng-h{}\" class=\"header-anchor\">#</a></h4>",
            *header_count, header_text, *header_count
        ))
    } else if trimmed.starts_with("### ") {
        let header_text = &trimmed[4..];
        *header_count += 1;
        Some(format!(
            "<h3 id=\"ng-h{}\">{}<a href=\"#ng-h{}\" class=\"header-anchor\">#</a></h3>",
            *header_count, header_text, *header_count
        ))
    } else if trimmed.starts_with("## ") {
        let header_text = &trimmed[3..];
        *header_count += 1;
        Some(format!(
            "<h2 id=\"ng-h{}\">{}<a href=\"#ng-h{}\" class=\"header-anchor\">#</a></h2>",
            *header_count, header_text, *header_count
        ))
    } else if trimmed.starts_with("# ") {
        let header_text = &trimmed[2..];
        *header_count += 1;
        Some(format!(
            "<h1 id=\"ng-h{}\">{}<a href=\"#ng-h{}\" class=\"header-anchor\">#</a></h1>",
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

fn extract_fenced_code_blocks(text: &str, marks: &Marks) -> (String, Vec<(String, String, u32)>) {
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
        if fenced_blocks.len() < MAX_BLOCKS {
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
                let placeholder = marks.slot('F', fenced_blocks.len());
                fenced_blocks.push((language, code_content, line_count));
                result.push_str(&placeholder);
                if i < lines.len() - 1 || text.ends_with('\n') {
                    result.push('\n');
                }
                i = end + 1;
                continue;
            }
        }
        result.push_str(line);
        if i < lines.len() - 1 {
            result.push('\n');
        }
        i += 1;
    }

    (result, fenced_blocks)
}

const LANGUAGES: &[(&[&str], &str, &str)] = &[
    (&["javascript", "js"], "javascript", "JavaScript"),
    (&["python", "py"], "python", "Python"),
    (&["java"], "java", "Java"),
    (&["typescript", "ts"], "typescript", "TypeScript"),
    (&["html"], "html", "HTML"),
    (&["css"], "css", "CSS"),
    (&["bash", "sh", "shell"], "bash", "Bash"),
    (&["sql"], "sql", "SQL"),
    (&["c"], "c", "C"),
    (&["cpp", "c++"], "cpp", "C++"),
    (&["csharp", "c#", "cs"], "csharp", "C#"),
    (&["php"], "php", "PHP"),
    (&["ruby", "rb"], "ruby", "Ruby"),
    (&["go", "golang"], "go", "Go"),
    (&["rust", "rs"], "rust", "Rust"),
    (&["swift"], "swift", "Swift"),
    (&["kotlin", "kt"], "kotlin", "Kotlin"),
    (&["r"], "r", "R"),
    (&["matlab"], "matlab", "MATLAB"),
    (&["scala"], "scala", "Scala"),
    (&["perl"], "perl", "Perl"),
    (&["powershell", "ps1"], "powershell", "PowerShell"),
    (&["json"], "json", "JSON"),
    (&["xml"], "xml", "XML"),
    (&["yaml", "yml"], "yaml", "YAML"),
    (&["markdown", "md"], "markdown", "Markdown"),
    (&["toml"], "toml", "TOML"),
    (&["ini"], "ini", "INI"),
    (&["properties"], "properties", "Java Properties"),
    (&["jsx"], "jsx", "JavaScript (JSX)"),
    (&["tsx"], "tsx", "TypeScript (TSX)"),
    (&["vue"], "vue", "Vue"),
    (&["scss"], "scss", "SCSS"),
    (&["sass"], "sass", "Sass"),
    (&["less"], "less", "Less"),
    (&["graphql", "gql"], "graphql", "GraphQL"),
    (&["svelte"], "svelte", "Svelte"),
    (&["handlebars", "hbs"], "handlebars", "Handlebars"),
    (&["pug", "jade"], "pug", "Pug"),
    (&["ejs"], "ejs", "EJS"),
    (&["nunjucks", "njk"], "nunjucks", "Nunjucks"),
    (&["dockerfile", "docker"], "dockerfile", "Dockerfile"),
    (&["makefile", "make"], "makefile", "Makefile"),
    (&["cmake"], "cmake", "CMake"),
    (&["nginx"], "nginx", "nginx"),
    (&["apache"], "apache", "ApacheConf"),
    (&["lua"], "lua", "Lua"),
    (&["dart"], "dart", "Dart"),
    (&["elixir", "ex"], "elixir", "Elixir"),
    (&["haskell", "hs"], "haskell", "Haskell"),
    (&["clojure", "clj"], "clojure", "Clojure"),
    (&["objective-c", "objc"], "objective-c", "Objective-C"),
    (&["coffeescript", "coffee"], "coffeescript", "CoffeeScript"),
    (&["groovy"], "groovy", "Groovy"),
    (&["racket", "rkt"], "racket", "Racket"),
    (&["scheme", "scm"], "scheme", "Scheme"),
    (&["lisp"], "lisp", "Lisp"),
    (&["erlang", "erl"], "erlang", "Erlang"),
    (&["fsharp", "f#", "fs"], "fsharp", "F#"),
    (&["ocaml", "ml"], "ocaml", "OCaml"),
    (&["julia", "jl"], "julia", "Julia"),
    (&["nim"], "nim", "Nim"),
    (&["crystal", "cr"], "crystal", "Crystal"),
    (&["d"], "d", "D"),
    (&["zig"], "zig", "Zig"),
    (&["vlang"], "v", "V"),
    (&["solidity", "sol"], "solidity", "Solidity"),
    (&["vhdl"], "vhdl", "VHDL"),
    (&["verilog"], "verilog", "Verilog"),
    (&["assembly", "asm"], "assembly", "Assembly"),
    (&["fortran", "f90", "f95"], "fortran", "Fortran"),
    (&["cobol", "cob"], "cobol", "COBOL"),
    (&["pascal", "pas"], "pascal", "Pascal"),
    (&["ada"], "ada", "Ada"),
    (&["prolog", "pl"], "prolog", "Prolog"),
    (&["smalltalk", "st"], "smalltalk", "Smalltalk"),
    (&["tcl"], "tcl", "Tcl"),
    (&["awk"], "awk", "AWK"),
    (&["sed"], "sed", "sed"),
    (&["vim", "vimscript"], "vim", "VimL"),
    (&["emacs-lisp", "elisp"], "emacs-lisp", "Emacs Lisp"),
    (&["elm"], "elm", "Elm"),
    (&["purescript", "purs"], "purescript", "PureScript"),
    (&["reasonml", "reason", "re"], "reasonml", "Reason"),
    (&["apex"], "apex", "Apex"),
    (&["arduino", "ino"], "arduino", "Arduino"),
    (&["processing", "pde"], "processing", "Processing"),
    (&["openscad", "scad"], "openscad", "OpenSCAD"),
    (&["latex", "tex"], "latex", "LaTeX"),
    (&["bibtex", "bib"], "bibtex", "BibTeX"),
    (&["rmarkdown", "rmd"], "rmarkdown", "R Markdown"),
    (
        &["restructuredtext", "rst"],
        "restructuredtext",
        "reStructuredText",
    ),
    (&["asciidoc", "adoc"], "asciidoc", "AsciiDoc"),
    (&["textile"], "textile", "Textile"),
    (&["org"], "org", "Org"),
    (&["diff"], "diff", "Diff"),
    (&["patch"], "patch", "Diff"),
    (&["plaintext", "text", "txt"], "plaintext", "Plain Text"),
];

fn language_names(lang: &str) -> (&str, &str) {
    let key = lang.to_lowercase();
    for &(aliases, css, syntect) in LANGUAGES {
        if aliases.iter().any(|alias| *alias == key) {
            return (css, syntect);
        }
    }
    (lang, lang)
}

fn process_lists(text: &str, marks: &Marks) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut result = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // Check if this line starts a list
        if is_list_item(line) {
            let (list_html, processed_lines) = process_list_block(&lines, i, marks);
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

fn process_list_block(lines: &[&str], start_idx: usize, marks: &Marks) -> (String, usize) {
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
    let list_html = render_list_tree(&tree, marks);

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

fn render_list_tree(items: &[ListItem], marks: &Marks) -> String {
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
                inner.push_str(&marks.marker(if checked { 'D' } else { 'O' }));
            }
            None => inner.push_str("<li>"),
        }
        inner.push_str(&item.content);
        if !item.children.is_empty() {
            inner.push_str(&render_list_tree(&item.children, marks));
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

fn restore_rendered_blocks(
    text: &str,
    fenced_blocks: &[(String, String, u32)],
    code_blocks: &[String],
    config: &RenderOptions,
    marks: &Marks,
) -> String {
    let (syntax_set, themes) = syntax_and_themes();
    let theme = theme_for(themes, &config.syntax_theme);
    let fence = marks.prefix('F');
    let code = marks.prefix('C');
    expand_indexed(text, &[(&fence, "z"), (&code, "z")], |prefix, index| {
        if prefix == fence {
            fenced_blocks
                .get(index)
                .map(|(language, code, line_count)| {
                    render_code_block(syntax_set, theme, language, code, *line_count, marks)
                })
        } else if prefix == code {
            code_blocks
                .get(index)
                .map(|code| format!("<code>{}</code>", html_escape(code)))
        } else {
            None
        }
    })
}

fn finish_html(source: &str, html: String, marks: &Marks) -> String {
    let cleaned = if html.len() <= MAX_HTML_BYTES {
        sanitize_html(html)
    } else {
        let end = source
            .char_indices()
            .nth(16_000)
            .map(|(index, _)| index)
            .unwrap_or(source.len());
        sanitize_html(format!("<p>{}</p>", html_escape(&source[..end])))
    };
    install_generated_html(&cleaned, marks)
}

fn install_generated_html(html: &str, marks: &Marks) -> String {
    let controls = marks.marker('K');
    let done = marks.marker('D');
    let open = marks.marker('O');
    replace_literals(
        html,
        &[
            (&controls, CODE_CONTROLS),
            (&done, TASK_DONE),
            (&open, TASK_OPEN),
        ],
    )
}

fn expand_indexed(
    text: &str,
    markers: &[(&str, &str)],
    mut replace: impl FnMut(&str, usize) -> Option<String>,
) -> String {
    let mut result = String::with_capacity(text.len());
    let mut index = 0;
    while index < text.len() {
        if result.len() >= MAX_HTML_BYTES {
            result.push_str(&text[index..]);
            break;
        }
        let mut found: Option<(usize, &str, &str)> = None;
        for (prefix, suffix) in markers {
            if let Some(relative) = text[index..].find(prefix) {
                let at = index + relative;
                if found.map(|(pos, _, _)| at < pos).unwrap_or(true) {
                    found = Some((at, prefix, suffix));
                }
            }
        }
        let Some((at, prefix, suffix)) = found else {
            result.push_str(&text[index..]);
            break;
        };
        result.push_str(&text[index..at]);
        let after = at + prefix.len();
        if let Some((number, consumed)) = numbered_suffix(&text[after..], suffix) {
            if let Some(replacement) = replace(prefix, number) {
                result.push_str(&replacement);
                index = after + consumed;
                continue;
            }
            result.push_str(&text[at..after + consumed]);
            index = after + consumed;
            continue;
        }
        result.push_str(prefix);
        index = after;
    }
    result
}

fn numbered_suffix(text: &str, suffix: &str) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    if !bytes.first().is_some_and(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let mut number = 0usize;
    let mut length = 0usize;
    for byte in bytes {
        if !byte.is_ascii_digit() {
            break;
        }
        if length == 6 {
            return None;
        }
        number = number
            .checked_mul(10)?
            .checked_add((byte - b'0') as usize)?;
        length += 1;
    }
    text[length..]
        .starts_with(suffix)
        .then_some((number, length + suffix.len()))
}

fn replace_literals(text: &str, pairs: &[(&str, &str)]) -> String {
    let mut result = String::with_capacity(text.len());
    let mut index = 0;
    while index < text.len() {
        let mut found: Option<(usize, usize, &str)> = None;
        for (key, replacement) in pairs {
            if let Some(relative) = text[index..].find(key) {
                let at = index + relative;
                if found.map(|(pos, _, _)| at < pos).unwrap_or(true) {
                    found = Some((at, key.len(), replacement));
                }
            }
        }
        let Some((at, key_len, replacement)) = found else {
            result.push_str(&text[index..]);
            break;
        };
        result.push_str(&text[index..at]);
        result.push_str(replacement);
        index = at + key_len;
    }
    result
}

pub fn warm() {
    let _ = syntax_and_themes();
    let _ = html_cleaner();
    let _ = text_cleaner();
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
    code_content: &str,
    line_count: u32,
    marks: &Marks,
) -> String {
    let (syntax, auto_detected, css_lang) =
        code_syntax(syntax_set, original_language, code_content);
    let highlighted_code = highlight_with_syntax(syntax_set, theme, syntax, code_content)
        .into_iter()
        .map(|line| format!("<span class=\"code-line\">{line}</span>"))
        .collect::<String>();

    // Generate line numbers
    let line_numbers = (1..=line_count)
        .map(|i| format!("<span class=\"line-number\">{}</span>", i))
        .collect::<Vec<_>>()
        .join("");

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
        r#"<pre{}><div class="code-header">{}<div class="code-controls">{}</div></div><div class="line-numbers">{}</div><div class="code-wrapper"><code>{}</code></div></pre>"#,
        class_attr,
        lang_display,
        marks.marker('K'),
        line_numbers,
        highlighted_code
    )
}

fn format_paragraphs_with_headers(text: &str, marks: &Marks) -> String {
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
        } else if (trimmed.contains(&marks.prefix('F')) || trimmed.contains("<table>"))
            && !trimmed.starts_with(&marks.prefix('F'))
            && !trimmed.starts_with("<table>")
        {
            let lines: Vec<&str> = part.lines().collect();
            let mut current_paragraph = String::new();

            for line in lines {
                let line_trimmed = line.trim();

                if line_trimmed.starts_with(&marks.prefix('F'))
                    || line_trimmed.starts_with("<table>")
                {
                    if !current_paragraph.is_empty() {
                        result.push_str(&format!("<p>{}</p>\n", current_paragraph.trim_start()));
                        current_paragraph.clear();
                    }
                    result.push_str(line_trimmed);
                    result.push('\n');
                } else if let Some(kept) = paragraph_line(line) {
                    if !current_paragraph.is_empty() {
                        current_paragraph.push_str("<br>");
                    }
                    current_paragraph.push_str(kept);
                }
            }

            if !current_paragraph.is_empty() {
                result.push_str(&format!("<p>{}</p>", current_paragraph.trim_start()));
            }
        } else if trimmed.starts_with(&marks.prefix('F'))
            || trimmed.starts_with("<img ")
            || trimmed.starts_with("<video ")
            || trimmed.starts_with("<table>")
        {
            result.push_str(trimmed);
        } else {
            let lines: Vec<&str> = part.lines().collect();
            let mut paragraph_content = String::new();

            for (j, line) in lines.iter().enumerate() {
                let Some(kept) = paragraph_line(line) else {
                    continue;
                };
                paragraph_content.push_str(kept);
                if j < lines.len() - 1
                    && lines
                        .get(j + 1)
                        .map_or(false, |next| !next.trim().is_empty())
                {
                    paragraph_content.push_str("<br>");
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

    clear_breaks_before_tables(result)
}

/// Highlight one line of code the same way [`render_markdown`](render_markdown_with_config)
/// highlights a fence line: the styled spans only, without the language label,
/// line numbers, or the Wrap, Collapse, and Copy controls.
pub fn highlight_code_line(language: &str, line: &str, syntax_theme: &str) -> String {
    highlight_code_lines(language, line, syntax_theme)
        .into_iter()
        .next()
        .unwrap_or_default()
}

/// Highlight every line of `code` with one highlighter, matching the spans inside
/// each `code-line` of a rendered fence. Line endings stay in the HTML so the
/// result can be compared with that fence.
pub fn highlight_code_lines(language: &str, code: &str, syntax_theme: &str) -> Vec<String> {
    let (syntax_set, themes) = syntax_and_themes();
    let theme = theme_for(themes, syntax_theme);
    let (syntax, _, _) = code_syntax(syntax_set, language, code);
    highlight_with_syntax(syntax_set, theme, syntax, code)
}

fn theme_for<'a>(themes: &'a ThemeSet, name: &str) -> &'a syntect::highlighting::Theme {
    themes.themes.get(name).unwrap_or_else(|| {
        eprintln!(
            "Nostorgraph: Warning: Theme '{}' not found, falling back to 'base16-ocean.dark'",
            name
        );
        &themes.themes["base16-ocean.dark"]
    })
}

fn code_syntax<'a>(
    syntax_set: &'a SyntaxSet,
    original_language: &str,
    code_content: &str,
) -> (&'a syntect::parsing::SyntaxReference, bool, String) {
    let (css_lang, syntect_language) = language_names(original_language);
    let (syntax, auto_detected) = if let Some(s) = syntax_set.find_syntax_by_name(syntect_language)
    {
        (s, false)
    } else if original_language.is_empty() {
        if let Some(s) = syntax_set.find_syntax_by_first_line(code_content) {
            (s, true)
        } else {
            (syntax_set.find_syntax_plain_text(), false)
        }
    } else {
        (
            syntax_set
                .find_syntax_by_first_line(code_content)
                .unwrap_or_else(|| syntax_set.find_syntax_plain_text()),
            false,
        )
    };
    (syntax, auto_detected, css_lang.to_string())
}

fn highlight_with_syntax(
    syntax_set: &SyntaxSet,
    theme: &syntect::highlighting::Theme,
    syntax: &syntect::parsing::SyntaxReference,
    code_content: &str,
) -> Vec<String> {
    let mut highlighter = HighlightLines::new(syntax, theme);
    LinesWithEndings::from(code_content)
        .map(|line| highlight_source_line(&mut highlighter, syntax_set, line))
        .collect()
}

fn highlight_source_line(
    highlighter: &mut HighlightLines,
    syntax_set: &SyntaxSet,
    line: &str,
) -> String {
    match highlighter.highlight_line(line, syntax_set) {
        Ok(ranges) => styled_line_to_highlighted_html(&ranges[..], IncludeBackground::No)
            .unwrap_or_else(|_| html_escape(line)),
        Err(_) => html_escape(line),
    }
}

fn paragraph_line(line: &str) -> Option<&str> {
    if line.trim().is_empty() {
        None
    } else {
        Some(line.trim_start())
    }
}

fn clear_breaks_before_tables(mut input: String) -> String {
    let mut iterations = 0;
    while input.contains("<br><table>") && iterations < 50 {
        input = input.replace("<br><br>", "<br>");
        input = input.replace("<br><table>", "<table>");
        input = input.replace("<br>\n<table>", "\n<table>");
        iterations += 1;
    }
    input
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
    let mut out = String::with_capacity(text.len());
    push_html_escape(&mut out, text, false);
    out
}

fn push_html_escape(out: &mut String, text: &str, for_attr: bool) {
    let bytes = text.as_bytes();
    let mut start = 0;
    for (index, &byte) in bytes.iter().enumerate() {
        let replacement = match byte {
            b'&' => Some("&amp;"),
            b'<' => Some("&lt;"),
            b'>' => Some("&gt;"),
            b'"' => Some("&quot;"),
            b'\'' => Some("&#x27;"),
            b'\n' if for_attr => Some(" "),
            b'\r' if for_attr => Some(""),
            _ => None,
        };
        if let Some(replacement) = replacement {
            out.push_str(&text[start..index]);
            out.push_str(replacement);
            start = index + 1;
        }
    }
    out.push_str(&text[start..]);
}

fn extract_media_syntax(text: &str, marks: &Marks) -> (String, Vec<String>) {
    let bytes = text.as_bytes();
    let mut result = String::with_capacity(text.len());
    let mut media_blocks = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        if media_blocks.len() < MAX_BLOCKS
            && bytes[i] == b'!'
            && i + 1 < bytes.len()
            && bytes[i + 1] == b'['
        {
            if let Some(end) = delimited_span(bytes, i + 2) {
                let placeholder = marks.slot('M', media_blocks.len());
                media_blocks.push(text[i..end].to_string());
                result.push_str(&placeholder);
                i = end;
                continue;
            }
            result.push('!');
            i += 1;
            continue;
        }
        let next = next_byte(bytes, i, b'!');
        result.push_str(&text[i..next]);
        i = next;
    }

    (result, media_blocks)
}

fn restore_media_and_links(
    text: &str,
    media_blocks: &[String],
    link_blocks: &[String],
    marks: &Marks,
) -> String {
    let media = marks.prefix('M');
    let link = marks.prefix('L');
    expand_indexed(text, &[(&media, "z"), (&link, "z")], |prefix, index| {
        if prefix == media {
            media_blocks.get(index).cloned()
        } else if prefix == link {
            link_blocks.get(index).cloned()
        } else {
            None
        }
    })
}

fn extract_link_syntax(text: &str, marks: &Marks) -> (String, Vec<String>) {
    let bytes = text.as_bytes();
    let mut result = String::with_capacity(text.len());
    let mut link_blocks = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        if link_blocks.len() < MAX_BLOCKS && bytes[i] == b'[' && !(i > 0 && bytes[i - 1] == b'!') {
            if let Some(end) = link_syntax_end(text, i) {
                let placeholder = marks.slot('L', link_blocks.len());
                link_blocks.push(text[i..end].to_string());
                result.push_str(&placeholder);
                i = end;
                continue;
            }
            result.push('[');
            i += 1;
            continue;
        }
        let next = next_byte(bytes, i, b'[');
        result.push_str(&text[i..next]);
        i = next;
    }

    (result, link_blocks)
}

fn delimited_span(bytes: &[u8], bracket_start: usize) -> Option<usize> {
    let bracket_end = find_ascii_until_newline(bytes, bracket_start, b']')?;
    if bracket_end + 1 >= bytes.len() || bytes[bracket_end + 1] != b'(' {
        return None;
    }
    let paren_end = find_ascii_until_newline(bytes, bracket_end + 2, b')')?;
    Some(paren_end + 1)
}

fn link_syntax_end(text: &str, i: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let bracket_end = find_ascii_until_newline(bytes, i + 1, b']')?;
    if let Some(end) = delimited_span(bytes, i + 1) {
        return Some(end);
    }
    if text[i + 1..bracket_end].starts_with("http") {
        return Some(bracket_end + 1);
    }
    None
}

fn extract_code_blocks(text: &str, marks: &Marks) -> (String, Vec<String>) {
    let bytes = text.as_bytes();
    let mut result = String::with_capacity(text.len());
    let mut code_blocks = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        if code_blocks.len() < MAX_BLOCKS && bytes[i] == b'`' {
            if let Some(offset) = bytes[i + 1..].iter().position(|&byte| byte == b'`') {
                let end = i + 1 + offset;
                let code_content = &text[i + 1..end];
                if !code_content.is_empty() && !code_content.contains('\n') {
                    let placeholder = marks.slot('C', code_blocks.len());
                    code_blocks.push(code_content.to_string());
                    result.push_str(&placeholder);
                    i = end + 1;
                    continue;
                }
            }
            result.push('`');
            i += 1;
            continue;
        }
        let next = next_byte(bytes, i, b'`');
        result.push_str(&text[i..next]);
        i = next;
    }

    (result, code_blocks)
}

fn restore_code_blocks(text: &str, code_blocks: &[String], marks: &Marks) -> String {
    let code = marks.prefix('C');
    expand_indexed(text, &[(&code, "z")], |_, index| {
        code_blocks
            .get(index)
            .map(|code| format!("<code>{}</code>", html_escape(code)))
    })
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

fn process_footnotes(content: &str, marks: &Marks) -> String {
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
    let bytes = content_text.as_bytes();
    let mut i = 0;
    let mut footnote_references = Vec::new();
    let mut inline_footnotes = Vec::new();

    // Second pass: process footnote references and inline footnotes
    while i < bytes.len() {
        if (inline_footnote_counter as usize) < MAX_BLOCKS
            && i + 2 < bytes.len()
            && bytes[i] == b'^'
            && bytes[i + 1] == b'['
        {
            if let Some(end_pos) = matching_bracket(bytes, i + 1) {
                inline_footnote_counter += 1;
                let footnote_text = content_text[i + 2..end_pos].to_string();
                let footnote_id = format!("ifn{}", inline_footnote_counter);
                inline_footnotes.push((footnote_id, footnote_text));
                result.push_str(&format!(
                    "<sup><a href=\"{}ng-ifn{}\" id=\"ng-ifn{}ref\">{}</a></sup>",
                    marks.marker('H'),
                    inline_footnote_counter,
                    inline_footnote_counter,
                    inline_footnote_counter
                ));
                i = end_pos + 1;
                continue;
            }
        } else if (footnote_counter as usize) < MAX_BLOCKS
            && i + 3 < bytes.len()
            && bytes[i] == b'['
            && bytes[i + 1] == b'^'
        {
            if let Some(end_pos) = find_ascii_until_newline(bytes, i + 2, b']') {
                let footnote_id = &content_text[i + 2..end_pos];
                if footnote_definitions.contains_key(footnote_id) {
                    footnote_counter += 1;
                    footnote_references.push((footnote_id.to_string(), footnote_counter));
                    result.push_str(&format!(
                        "<sup><a href=\"{}ng-FN{}\" id=\"ng-fnref{}\">{}</a></sup>",
                        marks.marker('H'),
                        footnote_counter,
                        footnote_counter,
                        footnote_counter
                    ));
                    i = end_pos + 1;
                    continue;
                }
            }
        }

        if bytes[i] == b'^' || bytes[i] == b'[' {
            result.push(bytes[i] as char);
            i += 1;
        } else {
            let next = next_ascii_marker(bytes, i, b'^', b'[');
            result.push_str(&content_text[i..next]);
            i = next;
        }
    }

    // Add footnotes section at the end if there are any footnotes
    if !footnote_references.is_empty() || !inline_footnotes.is_empty() {
        result.push_str("\n\n");
        result.push_str(&marks.marker('S'));

        // Add reference footnotes
        for (footnote_id, number) in footnote_references {
            if let Some(definition) = footnote_definitions.get(&footnote_id) {
                result.push_str(&format!(
                    "<li id=\"ng-fn{}\">{} <a href=\"{}ng-fnref{}\" class=\"footnote-backref\">↩</a></li>",
                    number,
                    definition,
                    marks.marker('H'),
                    number
                ));
            }
        }

        // Add inline footnotes
        for (footnote_id, footnote_text) in inline_footnotes.iter() {
            result.push_str(&format!(
                "<li id=\"ng-{}\">{} <a href=\"{}ng-{}ref\" class=\"footnote-backref\">↩</a></li>",
                footnote_id,
                footnote_text,
                marks.marker('H'),
                footnote_id
            ));
        }

        result.push_str(&marks.marker('E'));
    }

    result
}

fn restore_footnotes(text: &str, marks: &Marks) -> String {
    let hash = marks.marker('H');
    let start = marks.marker('S');
    let end = marks.marker('E');
    replace_literals(
        text,
        &[
            (&start, "<div class=\"footnotes\">\n<ol>\n"),
            (&end, "</ol>\n</div>"),
            (&hash, "#"),
        ],
    )
}

fn matching_bracket(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 1;
    let mut j = open + 1;
    while j < bytes.len() && depth > 0 {
        if bytes[j] == b'[' {
            depth += 1;
        } else if bytes[j] == b']' {
            depth -= 1;
            if depth == 0 {
                return Some(j);
            }
        }
        j += 1;
    }
    None
}

#[cfg(test)]
#[path = "../test/parser.rs"]
mod tests;
