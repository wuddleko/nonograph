use crate::font::{self, Face};
use crate::wrap::advance;

pub(crate) const PAGE_WIDTH_U: i32 = 595_280;
pub(crate) const PAGE_HEIGHT_U: i32 = 841_890;
const MARGIN_U: i32 = 72_000;

#[cfg(test)]
pub(crate) const PAGE_WIDTH: f32 = 595.28;
pub(crate) const MARGIN: f32 = 72.0;

#[cfg(test)]
pub(crate) struct Line {
    pub text: String,
    pub x: f32,
    pub y: f32,
}

#[cfg(test)]
pub(crate) struct Page {
    pub lines: Vec<Line>,
}

pub fn content_width() -> f32 {
    points(PAGE_WIDTH_U - 2 * MARGIN_U)
}

#[cfg(test)]
pub(crate) fn place<S: AsRef<str>>(lines: &[S], face: Face, size: f32, leading: f32) -> Vec<Page> {
    if lines.is_empty() {
        return Vec::new();
    }
    let mut flow = Flow::new();
    for line in lines {
        flow.line(line.as_ref(), face, size, leading);
    }
    flow.finish()
        .into_iter()
        .map(|lines| Page {
            lines: lines
                .into_iter()
                .map(|line| Line {
                    text: line.text,
                    x: points(line.x),
                    y: points(line.y),
                })
                .collect(),
        })
        .collect()
}

pub(crate) struct FlowLine {
    pub text: String,
    pub face: Face,
    pub size: i32,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub underline: bool,
    pub strike: bool,
    pub link: Option<String>,
    pub mark: Option<bool>,
    pub bars: u8,
    pub decor: Option<Decor>,
}

#[derive(Clone)]
pub(crate) struct Span {
    pub text: String,
    pub face: Face,
    pub underline: bool,
    pub strike: bool,
    pub link: Option<String>,
    pub gap: i32,
    pub mark: Option<bool>,
    pub bars: u8,
}

#[derive(Clone, Copy)]
pub(crate) enum Decor {
    Thin,
    Double,
    Asterisk,
    Stars,
}

pub(crate) struct Flow {
    pages: Vec<Vec<FlowLine>>,
    current: Vec<FlowLine>,
    cursor: i32,
    pending_gap: i32,
}

impl Flow {
    pub(crate) fn new() -> Self {
        Self {
            pages: Vec::new(),
            current: Vec::new(),
            cursor: PAGE_HEIGHT_U - MARGIN_U,
            pending_gap: 0,
        }
    }

    pub(crate) fn gap(&mut self, amount: f32) {
        if self.current.is_empty() || amount <= 0.0 {
            return;
        }
        self.pending_gap += thousandths(amount);
    }

    pub(crate) fn line(&mut self, text: &str, face: Face, size: f32, leading: f32) {
        self.spans(
            &[Span {
                text: text.to_string(),
                face,
                underline: false,
                strike: false,
                link: None,
                gap: 0,
                mark: None,
                bars: 0,
            }],
            size,
            leading,
        );
    }

    pub(crate) fn spans(&mut self, spans: &[Span], size: f32, leading: f32) {
        self.spans_from(spans, size, leading, MARGIN_U);
    }

    pub(crate) fn spans_from(&mut self, spans: &[Span], size: f32, leading: f32, origin: i32) {
        let size_u = thousandths(size);
        let bars = spans.iter().map(|span| span.bars).max().unwrap_or(0);
        let blank_face = spans
            .first()
            .map(|span| span.face)
            .unwrap_or(Face::Helvetica);
        let mut ascent = None;
        let mut descent = None;
        for span in spans {
            if span.text.is_empty() && span.mark.is_none() {
                continue;
            }
            let (ascender, descender) = font::vertical(span.face);
            let ascender = scale(size_u, ascender);
            let descender = scale(size_u, descender);
            ascent = Some(ascent.map_or(ascender, |value: i32| value.max(ascender)));
            descent = Some(descent.map_or(descender, |value: i32| value.min(descender)));
        }
        let (ascent, descent) = if let (Some(ascent), Some(descent)) = (ascent, descent) {
            (ascent, descent)
        } else {
            let (ascender, descender) = font::vertical(blank_face);
            (scale(size_u, ascender), scale(size_u, descender))
        };
        let gap = std::mem::take(&mut self.pending_gap);
        let mut baseline = self.cursor - gap - ascent;
        if baseline + descent < MARGIN_U && !self.current.is_empty() {
            self.pages.push(std::mem::take(&mut self.current));
            self.cursor = PAGE_HEIGHT_U - MARGIN_U;
            baseline = self.cursor - ascent;
        }
        let mut x = origin;
        let mut placed = false;
        for span in spans {
            x += span.gap;
            if span.text.is_empty() && span.mark.is_none() {
                continue;
            }
            let width = if span.mark.is_some() {
                size_u * 7 / 12
            } else {
                measure_units(&span.text, span.face, size_u)
            };
            self.current.push(FlowLine {
                text: span.text.clone(),
                face: span.face,
                size: size_u,
                x,
                y: baseline,
                width,
                underline: span.underline,
                strike: span.strike,
                link: span.link.clone(),
                mark: span.mark,
                bars: span.bars,
                decor: None,
            });
            x += width;
            placed = true;
        }
        if !placed {
            self.current.push(FlowLine {
                text: String::new(),
                face: blank_face,
                size: size_u,
                x: origin,
                y: baseline,
                width: 0,
                underline: false,
                strike: false,
                link: None,
                mark: None,
                bars,
                decor: None,
            });
        }
        self.cursor = baseline + ascent - thousandths(leading);
    }

    pub(crate) fn band(&mut self, ascent: i32, descent: i32, decor: Decor) {
        let gap = std::mem::take(&mut self.pending_gap);
        let mut baseline = self.cursor - gap - ascent;
        if baseline + descent < MARGIN_U && !self.current.is_empty() {
            self.pages.push(std::mem::take(&mut self.current));
            self.cursor = PAGE_HEIGHT_U - MARGIN_U;
            baseline = self.cursor - ascent;
        }
        self.current.push(FlowLine {
            text: String::new(),
            face: Face::Helvetica,
            size: 0,
            x: MARGIN_U,
            y: baseline,
            width: PAGE_WIDTH_U - 2 * MARGIN_U,
            underline: false,
            strike: false,
            link: None,
            mark: None,
            bars: 0,
            decor: Some(decor),
        });
        self.cursor = baseline + descent;
    }

    pub(crate) fn finish(mut self) -> Vec<Vec<FlowLine>> {
        if !self.current.is_empty() {
            self.pages.push(std::mem::take(&mut self.current));
        }
        self.pages
    }
}

fn thousandths(value: f32) -> i32 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    (f64::from(value) * 1000.0).round() as i32
}

fn points(thousandths: i32) -> f32 {
    thousandths as f32 / 1000.0
}

pub(crate) fn measure(text: &str, face: Face, size: f32) -> i32 {
    measure_units(text, face, thousandths(size))
}

fn measure_units(text: &str, face: Face, size_u: i32) -> i32 {
    scale(size_u, advance(text, face) as i32)
}

pub(crate) fn scale(size_u: i32, units: i32) -> i32 {
    let product = i64::from(size_u) * i64::from(units);
    let denom = 1000i64;
    if product >= 0 {
        ((product + denom - 1) / denom) as i32
    } else {
        let quot = product / denom;
        if product % denom == 0 {
            quot as i32
        } else {
            (quot - 1) as i32
        }
    }
}
