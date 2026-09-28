use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn render_markdown(
    markdown: &str,
    syntax_theme: &str,
    max_url_length: u32,
    external_link_security: bool,
) -> String {
    let options = nonograph_parser::RenderOptions {
        max_url_length: max_url_length as usize,
        external_link_security,
        syntax_theme: syntax_theme.to_string(),
    };
    nonograph_parser::render_markdown_with_config(markdown, &options)
}
