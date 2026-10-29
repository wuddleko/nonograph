mod encode;
mod font;
mod note;
mod place;
mod read;
mod wrap;
mod write;

pub(crate) use note::{draw, Block, Item, Marker, Rule, Run};
pub(crate) use read::read;

#[cfg(test)]
pub(crate) use font::Face;
#[cfg(test)]
pub(crate) use place::{content_width, MARGIN, PAGE_WIDTH};
#[cfg(test)]
pub(crate) use wrap::{advance, wrap, wrap_code};

#[derive(Debug)]
pub struct Error {
    detail: String,
}

impl Error {
    pub(crate) fn subset(err: impl std::fmt::Display) -> Self {
        Self {
            detail: err.to_string(),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "could not build the PDF: {}", self.detail)
    }
}

impl std::error::Error for Error {}

pub fn note_pdf(title: &str, alias: &str, html: &str) -> Result<Vec<u8>, Error> {
    let mut blocks = Vec::new();
    if !title.is_empty() {
        blocks.push(Block::Title(title.to_uppercase()));
    }
    if !alias.is_empty() {
        blocks.push(Block::Alias(alias.to_string()));
    }
    blocks.extend(read(html));
    draw(&blocks)
}

#[cfg(test)]
#[path = "../test/line.rs"]
mod line_tests;

#[cfg(test)]
#[path = "../test/wrap.rs"]
mod wrap_tests;

#[cfg(test)]
#[path = "../test/place.rs"]
mod place_tests;

#[cfg(test)]
#[path = "../test/note.rs"]
mod note_tests;

#[cfg(test)]
#[path = "../test/read.rs"]
mod read_tests;
