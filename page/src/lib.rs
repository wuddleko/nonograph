use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn seal(
    title: &str,
    author: &str,
    content: &str,
    locator: &str,
    secret: &[u8],
    nonce: &[u8],
) -> Result<String, JsError> {
    let secret = bytes32(secret, "invalid secret").map_err(JsError::new)?;
    let nonce = bytes32(nonce, "invalid nonce").map_err(JsError::new)?;
    nonograph_nip44::seal(title, author, content, locator, &secret, &nonce)
        .map_err(js_error)
}

#[wasm_bindgen]
pub fn open(payload: &str, secret: &[u8], d_tag: &str) -> Result<Note, JsError> {
    let secret = bytes32(secret, "invalid secret").map_err(JsError::new)?;
    let note = nonograph_nip44::open(payload, &secret, d_tag).map_err(js_error)?;
    Ok(Note {
        title: note.title,
        author: note.author,
        content: note.content,
        locator: note.locator,
    })
}

#[wasm_bindgen]
pub struct Note {
    title: String,
    author: String,
    content: String,
    locator: String,
}

#[wasm_bindgen]
impl Note {
    #[wasm_bindgen(getter)]
    pub fn title(&self) -> String {
        self.title.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn author(&self) -> String {
        self.author.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn content(&self) -> String {
        self.content.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn locator(&self) -> String {
        self.locator.clone()
    }
}

fn bytes32(bytes: &[u8], message: &'static str) -> Result<[u8; 32], &'static str> {
    bytes.try_into().map_err(|_| message)
}

fn js_error(error: nonograph_nip44::Error) -> JsError {
    JsError::new(&error.to_string())
}

#[cfg(test)]
mod tests {
    use super::bytes32;

    #[test]
    fn a_short_secret_is_rejected() {
        assert_eq!(bytes32(&[0u8; 16], "invalid secret"), Err("invalid secret"));
    }
}

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
