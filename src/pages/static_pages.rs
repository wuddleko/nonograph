use rocket::http::Status;
use rocket::response::content;
use rocket::State;

use crate::config::Config;
use crate::template;
use crate::{parse_frontmatter, render_options};

use super::layout::{article_context, not_found_html};

const STATIC_PAGES: &[&str] = &["about", "legal", "markup", "api"];

fn is_static_page(name: &str) -> bool {
    STATIC_PAGES.contains(&name)
}

#[get("/markup")]
pub fn markup_page(
    config: &State<Config>,
) -> Result<content::RawHtml<String>, (Status, content::RawHtml<String>)> {
    serve_static_page("markup", config, false)
}

#[get("/legal")]
pub fn legal_page(
    config: &State<Config>,
) -> Result<content::RawHtml<String>, (Status, content::RawHtml<String>)> {
    serve_static_page("legal", config, false)
}

#[get("/about")]
pub fn about_page(
    config: &State<Config>,
) -> Result<content::RawHtml<String>, (Status, content::RawHtml<String>)> {
    serve_static_page("about", config, false)
}

#[get("/api")]
pub fn api_page(
    config: &State<Config>,
) -> Result<content::RawHtml<String>, (Status, content::RawHtml<String>)> {
    serve_static_page("api", config, false)
}

#[get("/nojs/<page>")]
pub fn nojs_static_page(
    page: &str,
    config: &State<Config>,
) -> Result<content::RawHtml<String>, (Status, content::RawHtml<String>)> {
    if !is_static_page(page) {
        return Err((Status::NotFound, content::RawHtml(not_found_html())));
    }
    serve_static_page(page, config, true)
}

#[derive(Debug)]
pub(crate) enum BuiltPage {
    Ready(String),
    Invalid(String),
    Missing,
}

fn read_static_page_markdown(page_name: &str) -> Option<String> {
    for dir in ["content", "pages"] {
        if let Ok(file_content) = std::fs::read_to_string(format!("{dir}/{page_name}.md")) {
            return Some(file_content);
        }
    }
    None
}

fn serve_static_page(
    page_name: &str,
    config: &Config,
    nojs: bool,
) -> Result<content::RawHtml<String>, (Status, content::RawHtml<String>)> {
    match build_static_page(page_name, config, nojs) {
        BuiltPage::Ready(html) | BuiltPage::Invalid(html) => Ok(content::RawHtml(html)),
        BuiltPage::Missing => Err((Status::NotFound, content::RawHtml(not_found_html()))),
    }
}

pub(crate) fn build_static_page(page_name: &str, config: &Config, nojs: bool) -> BuiltPage {
    let Some(file_content) = read_static_page_markdown(page_name) else {
        return BuiltPage::Missing;
    };
    let parsed = parse_frontmatter(&file_content);
    let Some((title, author, created_at, raw_content)) = parsed else {
        return BuiltPage::Invalid(format!(
            "<h1>Error</h1><p>Invalid file format for {page_name}</p>"
        ));
    };
    let rendered_content =
        nostorgraph_parser::render_markdown_with_config(&raw_content, &render_options(config));
    let origin = config.normalized_public_origin().unwrap_or_default();
    let mut context = article_context(
        &title,
        &author,
        Some(created_at),
        &format!("/{page_name}"),
        "",
        nojs,
        page_name,
        &origin,
    );
    context.insert("content".to_string(), rendered_content);
    match template::shared().render("post", &context) {
        Ok(html) => BuiltPage::Ready(html),
        Err(error) => BuiltPage::Invalid(format!("Template error: {error}")),
    }
}
