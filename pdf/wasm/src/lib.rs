use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn note_pdf(title: &str, alias: &str, html: &str) -> Result<Vec<u8>, JsError> {
    pdf::note_pdf(title, alias, html).map_err(|error| JsError::new(&error.to_string()))
}
