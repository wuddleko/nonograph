use crate::encode;
use crate::font::{self, Face};

pub fn advance(text: &str, face: Face) -> u32 {
    encode::compose(text)
        .chars()
        .map(|ch| char_units(ch, face))
        .sum()
}

pub fn wrap(text: &str, face: Face, size: f32, width: f32) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let text = encode::compose(text);
    let max_units = fitting_units(size, width);
    let mut lines = Vec::new();
    for piece in broken_lines(&text) {
        lines.extend(wrap_piece(piece, face, max_units));
    }
    lines
}

pub fn wrap_code(text: &str, face: Face, size: f32, width: f32) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let text = encode::compose(text);
    let max_units = fitting_units(size, width);
    let mut lines = Vec::new();
    for piece in broken_lines(&text) {
        if piece.contains('\t') {
            lines.extend(wrap_fixed(&piece.replace('\t', "    "), face, max_units));
        } else {
            lines.extend(wrap_fixed(piece, face, max_units));
        }
    }
    lines
}

#[derive(Clone, Copy)]
pub(crate) struct Glyph {
    pub ch: char,
    pub face: Face,
    pub underline: bool,
    pub strike: bool,
    pub link: u16,
}

pub(crate) struct Segment {
    pub text: String,
    pub face: Face,
    pub underline: bool,
    pub strike: bool,
    pub link: u16,
}

pub(crate) fn wrap_glyphs(glyphs: &[Glyph], size: f32, width: f32) -> Vec<Vec<Segment>> {
    if glyphs.is_empty() {
        return Vec::new();
    }
    let max_units = fitting_units(size, width);
    let mut lines = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index < glyphs.len() {
        let ch = glyphs[index].ch;
        if ch == '\n' || ch == '\r' {
            lines.extend(wrap_slice(&glyphs[start..index], max_units));
            index += 1;
            if ch == '\r' && glyphs.get(index).is_some_and(|glyph| glyph.ch == '\n') {
                index += 1;
            }
            start = index;
            continue;
        }
        index += 1;
    }
    if start < glyphs.len() {
        lines.extend(wrap_slice(&glyphs[start..], max_units));
    }
    lines
}

fn wrap_slice(glyphs: &[Glyph], max_units: u32) -> Vec<Vec<Segment>> {
    if glyphs.is_empty() {
        return vec![Vec::new()];
    }
    let mut lines = Vec::new();
    let mut start = 0;
    let mut units = 0u32;
    let mut break_at = None;
    let mut index = 0;
    while index < glyphs.len() {
        let glyph = glyphs[index];
        let width = char_units(glyph.ch, glyph.face);
        let fits = units.saturating_add(width) <= max_units;
        if !fits && index > start {
            if glyph.ch == ' ' {
                lines.push(segments(&glyphs[start..index]));
                start = index + 1;
                index = start;
                units = 0;
                break_at = None;
                continue;
            }
            if let Some(space) = break_at {
                lines.push(segments(&glyphs[start..space]));
                start = space + 1;
                index = start;
                units = 0;
                break_at = None;
                continue;
            }
            lines.push(segments(&glyphs[start..index]));
            start = index;
            units = 0;
            break_at = None;
            continue;
        }
        if glyph.ch == ' ' {
            break_at = Some(index);
        }
        units = units.saturating_add(width);
        index += 1;
    }
    if start < glyphs.len() {
        lines.push(segments(&glyphs[start..]));
    }
    lines
}

fn segments(glyphs: &[Glyph]) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::new();
    for glyph in glyphs {
        if let Some(last) = out.last_mut() {
            if last.face == glyph.face
                && last.underline == glyph.underline
                && last.strike == glyph.strike
                && last.link == glyph.link
            {
                last.text.push(glyph.ch);
                continue;
            }
        }
        out.push(Segment {
            text: glyph.ch.to_string(),
            face: glyph.face,
            underline: glyph.underline,
            strike: glyph.strike,
            link: glyph.link,
        });
    }
    out
}

fn char_units(ch: char, face: Face) -> u32 {
    u32::from(font::units(face, ch))
}

fn fitting_units(size: f32, width: f32) -> u32 {
    if size <= 0.0 || width <= 0.0 {
        return 0;
    }
    let units = f64::from(width) / f64::from(size) * 1000.0;
    (units + 0.01).floor() as u32
}

fn broken_lines(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut parts = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\n' => {
                parts.push(&text[start..index]);
                index += 1;
                start = index;
            }
            b'\r' => {
                parts.push(&text[start..index]);
                index += 1;
                if index < bytes.len() && bytes[index] == b'\n' {
                    index += 1;
                }
                start = index;
            }
            _ => index += 1,
        }
    }
    if start < bytes.len() {
        parts.push(&text[start..]);
    }
    parts
}

fn wrap_fixed(text: &str, face: Face, max_units: u32) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    let mut start = 0;
    let mut units = 0u32;
    let mut break_at = None;
    let mut index = 0;
    while index < text.len() {
        let ch = text[index..].chars().next().unwrap();
        let len = ch.len_utf8();
        let glyph = char_units(ch, face);
        let fits = units.saturating_add(glyph) <= max_units;
        if !fits && index > start {
            let next = text[index + len..].chars().next();
            if ch == ' ' && next.is_some_and(|following| following != ' ') {
                let end = index + len;
                lines.push(text[start..end].to_string());
                start = end;
            } else if let Some(space) = break_at {
                let end = space + 1;
                if spaces_only(&text[start..end]) && index > end {
                    lines.push(text[start..index].to_string());
                    start = index;
                } else {
                    lines.push(text[start..end].to_string());
                    start = end;
                }
            } else {
                lines.push(text[start..index].to_string());
                start = index;
            }
            index = start;
            units = 0;
            break_at = None;
            continue;
        }
        if ch == ' ' {
            break_at = Some(index);
        }
        units = units.saturating_add(glyph);
        index += len;
    }
    if start < text.len() {
        lines.push(text[start..].to_string());
    }
    lines
}

fn spaces_only(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte == b' ')
}

fn wrap_piece(text: &str, face: Face, max_units: u32) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    let mut start = 0;
    let mut units = 0u32;
    let mut break_at = None;
    let mut index = 0;
    while index < text.len() {
        let ch = text[index..].chars().next().unwrap();
        let len = ch.len_utf8();
        let glyph = char_units(ch, face);
        let fits = units.saturating_add(glyph) <= max_units;
        if !fits && index > start {
            if ch == ' ' {
                lines.push(text[start..index].to_string());
                start = index + len;
                index = start;
                units = 0;
                break_at = None;
                continue;
            }
            if let Some(space) = break_at {
                lines.push(text[start..space].to_string());
                start = space + 1;
                index = start;
                units = 0;
                break_at = None;
                continue;
            }
            lines.push(text[start..index].to_string());
            start = index;
            units = 0;
            break_at = None;
            continue;
        }
        if ch == ' ' {
            break_at = Some(index);
        }
        units = units.saturating_add(glyph);
        index += len;
    }
    if start < text.len() {
        lines.push(text[start..].to_string());
    }
    lines
}
