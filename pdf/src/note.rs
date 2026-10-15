use std::collections::BTreeSet;

use crate::encode;
use crate::font::{self, Face};
use crate::place::{self, measure, scale, Decor, Flow, FlowLine, Span};
use crate::wrap::{wrap, wrap_code, wrap_glyphs, Glyph};
use crate::write::{format_thousandths, media_box, write_pdf};

pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub code: bool,
    pub url: Option<String>,
}

impl Run {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            code: false,
            url: None,
        }
    }
}

pub enum Marker {
    Bullet,
    Number(u32),
    Task(bool),
}

pub struct Item {
    pub marker: Marker,
    pub runs: Vec<Run>,
    pub children: Vec<Item>,
}

pub enum Rule {
    Thin,
    Double,
    Asterisk,
    Stars,
}

pub enum Block {
    Title(String),
    Alias(String),
    Heading(u8, Vec<Run>),
    Paragraph(Vec<Run>),
    List(Vec<Item>),
    Quote(u8, Vec<Run>),
    Rule(Rule),
    Code(String),
}

struct Style<'a> {
    text: &'a str,
    face: Face,
    size: f32,
    leading: f32,
    gap: f32,
}

const TITLE_SIZE: f32 = 22.0;
const TITLE_LEADING: f32 = 28.0;
const ALIAS_SIZE: f32 = 10.0;
const ALIAS_LEADING: f32 = 13.0;
const ALIAS_GAP: f32 = 10.0;
const BODY_SIZE: f32 = 12.0;
const BODY_LEADING: f32 = 16.0;
const PARAGRAPH_GAP: f32 = 12.0;
const HEADING_SIZE: [f32; 4] = [20.0, 16.0, 14.0, 12.0];
const HEADING_LEADING: [f32; 4] = [26.0, 21.0, 18.0, 16.0];
const HEADING_GAP: [f32; 4] = [16.0, 14.0, 12.0, 12.0];
const LIST_INDENT: i32 = 18_000;
const LIST_GAP: i32 = 6_000;
const MIN_LIST_TEXT: i32 = 72_000;
const ITEM_GAP: f32 = 4.0;
const QUOTE_INDENT: i32 = 12_000;
const QUOTE_BAR: i32 = 1_200;
const QUOTE_RIGHT: i32 = 12_000;
const RULE_THICK: i32 = 600;
const STAR_ARM: i32 = 5_000;
const STAR_DIAG: i32 = 2_500;
const STAR_GAP: i32 = 24_000;
const STAR_DOT: i32 = 2_000;
const BAR_JOIN: i32 = 8_000;
const CODE_SIZE: f32 = 10.0;
const CODE_LEADING: f32 = 14.0;

pub fn draw(blocks: &[Block]) -> Result<Vec<u8>, crate::Error> {
    let mut flow = Flow::new();
    for block in blocks {
        match block {
            Block::Paragraph(runs) => paint_runs(
                &mut flow,
                runs,
                Face::Helvetica,
                BODY_SIZE,
                BODY_LEADING,
                PARAGRAPH_GAP,
            ),
            Block::Heading(level, runs) => {
                let level = heading_level(*level);
                paint_runs(
                    &mut flow,
                    runs,
                    Face::HelveticaBold,
                    HEADING_SIZE[level],
                    HEADING_LEADING[level],
                    HEADING_GAP[level],
                )
            }
            Block::List(items) => paint_list(&mut flow, items, 0),
            Block::Quote(depth, runs) => paint_quote(&mut flow, *depth, runs),
            Block::Rule(rule) => paint_rule(&mut flow, rule),
            Block::Code(text) => paint_code(&mut flow, text),
            other => {
                let Some(style) = style(other) else {
                    continue;
                };
                let lines = wrap(style.text, style.face, style.size, place::content_width());
                if lines.is_empty() {
                    continue;
                }
                flow.gap(style.gap);
                for line in &lines {
                    flow.line(line, style.face, style.size, style.leading);
                }
            }
        }
    }
    let mut pages = flow.finish();
    if pages.is_empty() {
        pages.push(Vec::new());
    }
    document(&pages)
}

fn heading_level(level: u8) -> usize {
    match level {
        1 => 0,
        2 => 1,
        3 => 2,
        _ => 3,
    }
}

struct Inset {
    origin: i32,
    width: f32,
    bars: u8,
}

fn paint_runs(flow: &mut Flow, runs: &[Run], text_face: Face, size: f32, leading: f32, gap: f32) {
    paint_inset(
        flow,
        runs,
        text_face,
        size,
        leading,
        gap,
        Inset {
            origin: margin_u(),
            width: place::content_width(),
            bars: 0,
        },
    );
}

fn paint_quote(flow: &mut Flow, depth: u8, runs: &[Run]) {
    let depth = i32::from(depth.max(1));
    let origin = margin_u() + depth * QUOTE_INDENT;
    let inset = (origin - margin_u() + QUOTE_RIGHT) as f32 / 1000.0;
    paint_inset(
        flow,
        runs,
        Face::Helvetica,
        BODY_SIZE,
        BODY_LEADING,
        PARAGRAPH_GAP,
        Inset {
            origin,
            width: place::content_width() - inset,
            bars: depth as u8,
        },
    );
}

fn paint_code(flow: &mut Flow, text: &str) {
    let lines = wrap_code(text, Face::Courier, CODE_SIZE, place::content_width());
    if lines.is_empty() {
        return;
    }
    flow.gap(PARAGRAPH_GAP);
    for line in &lines {
        flow.line(line, Face::Courier, CODE_SIZE, CODE_LEADING);
    }
}

fn paint_rule(flow: &mut Flow, rule: &Rule) {
    let (ascent, descent, decor) = match rule {
        Rule::Thin => (RULE_THICK / 2, -RULE_THICK / 2, Decor::Thin),
        Rule::Double => (
            RULE_THICK + RULE_THICK / 2,
            -(RULE_THICK + RULE_THICK / 2),
            Decor::Double,
        ),
        Rule::Asterisk => (
            STAR_ARM + RULE_THICK / 2,
            -(STAR_ARM + RULE_THICK / 2),
            Decor::Asterisk,
        ),
        Rule::Stars => (
            STAR_ARM + RULE_THICK / 2,
            -(STAR_ARM + RULE_THICK / 2),
            Decor::Stars,
        ),
    };
    flow.gap(PARAGRAPH_GAP);
    flow.band(ascent, descent, decor);
}

fn paint_inset(
    flow: &mut Flow,
    runs: &[Run],
    text_face: Face,
    size: f32,
    leading: f32,
    gap: f32,
    inset: Inset,
) {
    let (glyphs, urls) = glyphs(runs, text_face);
    let lines = wrap_glyphs(&glyphs, size, inset.width);
    if lines.is_empty() {
        return;
    }
    flow.gap(gap);
    for line in lines {
        if line.is_empty() {
            flow.spans_from(
                &[Span {
                    text: String::new(),
                    face: text_face,
                    underline: false,
                    strike: false,
                    link: None,
                    gap: 0,
                    mark: None,
                    bars: inset.bars,
                }],
                size,
                leading,
                inset.origin,
            );
            continue;
        }
        let spans = line
            .into_iter()
            .map(|segment| Span {
                text: segment.text,
                face: segment.face,
                underline: segment.underline,
                strike: segment.strike,
                link: (segment.link > 0).then(|| urls[segment.link as usize - 1].clone()),
                gap: 0,
                mark: None,
                bars: inset.bars,
            })
            .collect::<Vec<_>>();
        flow.spans_from(&spans, size, leading, inset.origin);
    }
}

fn margin_u() -> i32 {
    thousandths(place::MARGIN)
}

fn run_face(bold: bool, italic: bool, code: bool, text_face: Face) -> Face {
    if code {
        return match (bold, italic) {
            (true, true) => Face::CourierBoldOblique,
            (true, false) => Face::CourierBold,
            (false, true) => Face::CourierOblique,
            (false, false) => Face::Courier,
        };
    }
    let bold = bold || text_face == Face::HelveticaBold;
    match (bold, italic) {
        (true, true) => Face::HelveticaBoldOblique,
        (true, false) => Face::HelveticaBold,
        (false, true) => Face::HelveticaOblique,
        (false, false) => text_face,
    }
}

fn glyphs(runs: &[Run], text_face: Face) -> (Vec<Glyph>, Vec<String>) {
    let mut glyphs = Vec::new();
    let mut urls = Vec::new();
    for run in runs {
        let underline = run.underline || run.url.as_deref().is_some_and(web_url);
        let face = run_face(run.bold, run.italic, run.code, text_face);
        let link = if let Some(url) = &run.url {
            urls.push(url.clone());
            urls.len() as u16
        } else {
            0
        };
        let text = encode::compose(&run.text);
        let mut push = |ch: char| {
            glyphs.push(Glyph {
                ch,
                face,
                underline,
                strike: run.strike,
                link,
            });
        };
        for ch in text.chars() {
            if ch == '\t' {
                for _ in 0..4 {
                    push(' ');
                }
            } else {
                push(ch);
            }
        }
    }
    (glyphs, urls)
}

fn paint_list(flow: &mut Flow, items: &[Item], depth: i32) {
    if items.is_empty() {
        return;
    }
    let column = items
        .iter()
        .map(|item| marker_width(&item.marker))
        .max()
        .unwrap_or(0);
    let slot = list_slot(depth, column);
    let text_x = slot + column + LIST_GAP;
    let width = place::content_width() - (text_x - thousandths(place::MARGIN)) as f32 / 1000.0;
    for (index, item) in items.iter().enumerate() {
        if index == 0 && depth == 0 {
            flow.gap(PARAGRAPH_GAP);
        } else if index > 0 {
            flow.gap(ITEM_GAP);
        }
        paint_item(flow, item, slot, column, text_x, width);
        paint_list(flow, &item.children, depth + 1);
    }
}

fn paint_item(flow: &mut Flow, item: &Item, slot: i32, column: i32, text_x: i32, width: f32) {
    let (glyphs, urls) = glyphs(&item.runs, Face::Helvetica);
    let lines = wrap_glyphs(&glyphs, BODY_SIZE, width);
    let marker = marker_span(&item.marker, column - marker_width(&item.marker));
    if lines.is_empty() {
        flow.spans_from(&[marker], BODY_SIZE, BODY_LEADING, slot);
        return;
    }
    for (index, line) in lines.into_iter().enumerate() {
        if line.is_empty() {
            let origin = if index == 0 { slot } else { text_x };
            let spans = if index == 0 {
                vec![marker.clone()]
            } else {
                Vec::new()
            };
            flow.spans_from(&spans, BODY_SIZE, BODY_LEADING, origin);
            continue;
        }
        let mut spans = Vec::new();
        if index == 0 {
            spans.push(marker.clone());
        }
        for (span_index, segment) in line.into_iter().enumerate() {
            spans.push(Span {
                text: segment.text,
                face: segment.face,
                underline: segment.underline,
                strike: segment.strike,
                link: (segment.link > 0).then(|| urls[segment.link as usize - 1].clone()),
                gap: if index == 0 && span_index == 0 {
                    LIST_GAP
                } else {
                    0
                },
                mark: None,
                bars: 0,
            });
        }
        let origin = if index == 0 { slot } else { text_x };
        flow.spans_from(&spans, BODY_SIZE, BODY_LEADING, origin);
    }
}

fn marker_span(marker: &Marker, gap: i32) -> Span {
    let (text, mark) = match marker {
        Marker::Bullet => ("•".to_string(), None),
        Marker::Number(number) => (format!("{number}."), None),
        Marker::Task(checked) => (String::new(), Some(*checked)),
    };
    Span {
        text,
        face: Face::Helvetica,
        underline: false,
        strike: false,
        link: None,
        gap,
        mark,
        bars: 0,
    }
}

fn list_slot(depth: i32, column: i32) -> i32 {
    let margin = thousandths(place::MARGIN);
    let content = thousandths(place::content_width());
    let natural = margin.saturating_add(depth.saturating_mul(LIST_INDENT));
    let max_slot = margin + content - MIN_LIST_TEXT - column - LIST_GAP;
    if max_slot < margin {
        margin
    } else {
        natural.min(max_slot)
    }
}

fn marker_width(marker: &Marker) -> i32 {
    match marker {
        Marker::Bullet => measure("•", Face::Helvetica, BODY_SIZE),
        Marker::Number(number) => measure(&format!("{number}."), Face::Helvetica, BODY_SIZE),
        Marker::Task(_) => thousandths(BODY_SIZE) * 7 / 12,
    }
}

fn thousandths(value: f32) -> i32 {
    (f64::from(value) * 1000.0).round() as i32
}

fn style(block: &Block) -> Option<Style<'_>> {
    let (text, face, size, leading, gap) = match block {
        Block::Title(text) => (text, Face::HelveticaBold, TITLE_SIZE, TITLE_LEADING, 0.0),
        Block::Alias(text) => (text, Face::Helvetica, ALIAS_SIZE, ALIAS_LEADING, ALIAS_GAP),
        Block::Heading(_, _)
        | Block::Paragraph(_)
        | Block::List(_)
        | Block::Quote(_, _)
        | Block::Rule(_)
        | Block::Code(_) => {
            return None;
        }
    };
    if text.is_empty() {
        None
    } else {
        Some(Style {
            text,
            face,
            size,
            leading,
            gap,
        })
    }
}

fn document(pages: &[Vec<FlowLine>]) -> Result<Vec<u8>, crate::Error> {
    let faces = used_faces(pages);
    let mut objects = Vec::with_capacity(2 + pages.len() * 2 + faces.len() * font::OBJECTS);
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    let pages_index = objects.len();
    objects.push(Vec::new());

    let font_object = objects.len() + pages.len() * 2 + 1;
    let fonts = font_resources(&faces, font_object);
    let mut kids = Vec::with_capacity(pages.len());
    for page in pages {
        let page_object = objects.len() + 1;
        let contents_object = page_object + 1;
        kids.push(format!("{page_object} 0 R"));
        objects.push(page_dictionary(contents_object, &fonts, page).into_bytes());
        let stream = content_stream(page);
        objects.push(
            format!("<< /Length {} >>\nstream\n{stream}endstream", stream.len()).into_bytes(),
        );
    }
    objects[pages_index] = format!(
        "<< /Type /Pages /Kids [{}] /Count {} >>",
        kids.join(" "),
        kids.len()
    )
    .into_bytes();
    for (index, face) in faces.iter().enumerate() {
        let mut chars = BTreeSet::new();
        for page in pages {
            for line in page {
                if line.face != *face || line.text.is_empty() {
                    continue;
                }
                for ch in line.text.chars() {
                    chars.insert(encode::shown(ch, *face));
                }
            }
        }
        objects.extend(font::objects(
            *face,
            &chars,
            font_object + index * font::OBJECTS,
        )?);
    }
    Ok(write_pdf(&objects))
}

fn page_dictionary(contents: usize, fonts: &str, lines: &[FlowLine]) -> String {
    format!(
        "<< /Type /Page /Parent 2 0 R /MediaBox [{}] /Contents {contents} 0 R /Resources << /Font << {fonts} >> >>{} >>",
        media_box(),
        annotations(lines)
    )
}

fn web_url(url: &str) -> bool {
    let url = url.trim();
    let Some((scheme, rest)) = url.split_once("://") else {
        return false;
    };
    !rest.is_empty()
        && (scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https"))
}

fn annotations(lines: &[FlowLine]) -> String {
    let mut annots = Vec::new();
    for line in lines {
        let Some(url) = line
            .link
            .as_deref()
            .map(str::trim)
            .filter(|url| web_url(url))
        else {
            continue;
        };
        if line.text.is_empty() || line.width <= 0 {
            continue;
        }
        let (ascender, descender) = font::vertical(line.face);
        let llx = format_thousandths(line.x);
        let lly = format_thousandths(line.y + scale(line.size, descender));
        let urx = format_thousandths(line.x + line.width);
        let ury = format_thousandths(line.y + scale(line.size, ascender));
        let uri = encode::literal(&encode::uri(url));
        annots.push(format!(
            "<< /Type /Annot /Subtype /Link /Rect [{llx} {lly} {urx} {ury}] /Border [0 0 0] /A << /S /URI /URI {uri} >> >>"
        ));
    }
    if annots.is_empty() {
        String::new()
    } else {
        format!(" /Annots [{}]", annots.join(" "))
    }
}

fn used_faces(pages: &[Vec<FlowLine>]) -> Vec<Face> {
    let mut faces = Vec::new();
    for page in pages {
        for line in page {
            if line.text.is_empty() || faces.contains(&line.face) {
                continue;
            }
            faces.push(line.face);
        }
    }
    faces
}

fn font_resources(faces: &[Face], first_font: usize) -> String {
    faces
        .iter()
        .enumerate()
        .map(|(index, face)| {
            format!(
                "/{} {} 0 R",
                font_id(*face),
                first_font + index * font::OBJECTS
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn content_stream(lines: &[FlowLine]) -> String {
    let mut stream = String::from("0 g\n0 G\n");
    for line in lines {
        if line.text.is_empty() && line.mark.is_none() {
            continue;
        }
        if !line.text.is_empty() {
            let shown = encode::literal(&encode::utf16(&line.text, line.face));
            stream.push_str(&format!(
                "BT\n/{} {} Tf\n1 0 0 1 {} {} Tm\n{shown} Tj\nET\n",
                font_id(line.face),
                format_thousandths(line.size),
                format_thousandths(line.x),
                format_thousandths(line.y)
            ));
        }
        let (underline_at, thickness, strike_at, strike_thickness) = font::decor(line.face);
        let stroke = scale(line.size, thickness);
        if let Some(checked) = line.mark {
            checkbox(&mut stream, line.x, line.y, line.width, stroke, checked);
        }
        if line.underline {
            rule(
                &mut stream,
                line.x,
                line.y + scale(line.size, underline_at),
                line.width,
                stroke,
            );
        }
        if line.strike {
            rule(
                &mut stream,
                line.x,
                line.y + scale(line.size, strike_at),
                line.width,
                scale(line.size, strike_thickness),
            );
        }
    }
    for (x, bottom, width, height) in quote_bars(lines) {
        stream.push_str(&format!(
            "{} {} {} {} re\nf\n",
            format_thousandths(x),
            format_thousandths(bottom),
            format_thousandths(width),
            format_thousandths(height)
        ));
    }
    for line in lines {
        if let Some(decor) = line.decor {
            paint_decor(&mut stream, line.x, line.y, line.width, decor);
        }
    }
    stream
}

fn quote_bars(lines: &[FlowLine]) -> Vec<(i32, i32, i32, i32)> {
    struct Row {
        top: i32,
        bottom: i32,
        bars: u8,
    }
    let mut rows: Vec<Row> = Vec::new();
    for line in lines {
        if line.bars == 0 {
            continue;
        }
        let (ascender, descender) = font::vertical(line.face);
        let top = line.y + scale(line.size, ascender);
        let bottom = line.y + scale(line.size, descender);
        if let Some(row) = rows.last_mut() {
            if row.bars == line.bars && row.bottom - top <= BAR_JOIN {
                row.top = row.top.max(top);
                row.bottom = row.bottom.min(bottom);
                continue;
            }
        }
        rows.push(Row {
            top,
            bottom,
            bars: line.bars,
        });
    }
    let mut rects = Vec::new();
    for row in rows {
        let height = row.top - row.bottom;
        if height <= 0 {
            continue;
        }
        for depth in 0..row.bars {
            rects.push((
                margin_u() + i32::from(depth) * QUOTE_INDENT,
                row.bottom,
                QUOTE_BAR,
                height,
            ));
        }
    }
    rects
}

fn paint_decor(stream: &mut String, x: i32, y: i32, width: i32, decor: Decor) {
    match decor {
        Decor::Thin => rule(stream, x, y, width, RULE_THICK),
        Decor::Double => {
            rule(stream, x, y + RULE_THICK, width, RULE_THICK);
            rule(stream, x, y - RULE_THICK, width, RULE_THICK);
        }
        Decor::Asterisk => star(stream, x + width / 2, y),
        Decor::Stars => {
            let center = x + width / 2;
            star(stream, center - STAR_GAP, y);
            star(stream, center, y);
            star(stream, center + STAR_GAP, y);
        }
    }
}

fn star(stream: &mut String, x: i32, y: i32) {
    segment(stream, x - STAR_ARM, y, x + STAR_ARM, y);
    segment(stream, x, y - STAR_ARM, x, y + STAR_ARM);
    segment(
        stream,
        x - STAR_DIAG,
        y - STAR_DIAG,
        x + STAR_DIAG,
        y + STAR_DIAG,
    );
    segment(
        stream,
        x - STAR_DIAG,
        y + STAR_DIAG,
        x + STAR_DIAG,
        y - STAR_DIAG,
    );
    stream.push_str(&format!(
        "{} {} {} {} re\nf\n",
        format_thousandths(x - STAR_DOT / 2),
        format_thousandths(y - STAR_DOT / 2),
        format_thousandths(STAR_DOT),
        format_thousandths(STAR_DOT)
    ));
}

fn segment(stream: &mut String, x1: i32, y1: i32, x2: i32, y2: i32) {
    stream.push_str(&format!(
        "{} w\n{} {} m\n{} {} l\nS\n",
        format_thousandths(RULE_THICK),
        format_thousandths(x1),
        format_thousandths(y1),
        format_thousandths(x2),
        format_thousandths(y2)
    ));
}

fn checkbox(stream: &mut String, x: i32, baseline: i32, side: i32, thickness: i32, checked: bool) {
    if side <= 0 || thickness <= 0 {
        return;
    }
    let bottom = baseline + side / 7;
    let right = x + side;
    let top = bottom + side;
    stream.push_str(&format!(
        "{} w\n{} {} m\n{} {} l\n{} {} l\n{} {} l\nh\nS\n",
        format_thousandths(thickness),
        format_thousandths(x),
        format_thousandths(bottom),
        format_thousandths(right),
        format_thousandths(bottom),
        format_thousandths(right),
        format_thousandths(top),
        format_thousandths(x),
        format_thousandths(top)
    ));
    if !checked {
        return;
    }
    stream.push_str(&format!(
        "{} {} m\n{} {} l\n{} {} l\nS\n",
        format_thousandths(x + side / 5),
        format_thousandths(bottom + side * 2 / 5),
        format_thousandths(x + side * 2 / 5),
        format_thousandths(bottom + side / 5),
        format_thousandths(x + side * 4 / 5),
        format_thousandths(bottom + side * 7 / 10)
    ));
}

fn rule(stream: &mut String, x: i32, y: i32, width: i32, thickness: i32) {
    if width <= 0 || thickness <= 0 {
        return;
    }
    stream.push_str(&format!(
        "{} w\n{} {} m\n{} {} l\nS\n",
        format_thousandths(thickness),
        format_thousandths(x),
        format_thousandths(y),
        format_thousandths(x + width),
        format_thousandths(y)
    ));
}

fn font_id(face: Face) -> &'static str {
    match face {
        Face::Helvetica => "F1",
        Face::HelveticaBold => "F2",
        Face::Courier => "F3",
        Face::CourierBold => "F4",
        Face::HelveticaOblique => "F5",
        Face::HelveticaBoldOblique => "F6",
        Face::CourierOblique => "F7",
        Face::CourierBoldOblique => "F8",
    }
}
