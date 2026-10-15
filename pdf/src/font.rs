use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use subsetter::GlyphRemapper;
use ttf_parser::{Face as TrueType, GlyphId, Rect};

pub(crate) const OBJECTS: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Face {
    Helvetica,
    HelveticaBold,
    HelveticaOblique,
    HelveticaBoldOblique,
    Courier,
    CourierBold,
    CourierOblique,
    CourierBoldOblique,
}

struct Source {
    bytes: &'static [u8],
    face: OnceLock<TrueType<'static>>,
}

pub(crate) fn covers(face: Face, ch: char) -> bool {
    ch >= ' ' && glyph_id(face, ch).is_some()
}

pub(crate) fn cid(face: Face, ch: char) -> u16 {
    glyph_id(face, ch)
        .or_else(|| glyph_id(face, '?'))
        .map(|glyph| glyph.0)
        .unwrap_or(0)
}

pub(crate) fn units(face: Face, ch: char) -> u16 {
    let font = parsed(face);
    let glyph = glyph_id(face, ch).or_else(|| glyph_id(face, '?'));
    let advance = glyph
        .and_then(|glyph| font.glyph_hor_advance(glyph))
        .unwrap_or(0);
    scale_u(font.units_per_em(), advance)
}

pub(crate) fn objects(
    face: Face,
    chars: &BTreeSet<char>,
    type0: usize,
) -> Result<Vec<Vec<u8>>, crate::Error> {
    let source = source(face);
    let font = parsed(face);
    let mut remapper = GlyphRemapper::new();
    let mut rows = BTreeMap::new();
    for ch in chars {
        let Some(glyph) = glyph_id(face, *ch) else {
            continue;
        };
        let unicode = u16::try_from(u32::from(*ch)).ok();
        let Some(unicode) = unicode else {
            continue;
        };
        let subset_id = remapper.remap(glyph.0);
        rows.entry(glyph.0)
            .or_insert((subset_id, unicode, units(face, *ch)));
    }
    let rows = rows
        .into_iter()
        .map(|(cid, (subset_id, unicode, width))| (cid, subset_id, unicode, width))
        .collect::<Vec<_>>();
    let subset = subsetter::subset(source.bytes, 0, &remapper).map_err(crate::Error::subset)?;

    let name = format!("{}+{}", tag(&rows), ps_name(face));
    let cid_font = type0 + 1;
    let descriptor = type0 + 2;
    let map_object = type0 + 3;
    let file_object = type0 + 4;
    let unicode_object = type0 + 5;
    let type0_dict = format!(
        "<< /Type /Font /Subtype /Type0 /BaseFont /{name} /Encoding /Identity-H /DescendantFonts [{cid_font} 0 R] /ToUnicode {unicode_object} 0 R >>"
    );
    let cid_dict = format!(
        "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /{name} /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor {descriptor} 0 R /CIDToGIDMap {map_object} 0 R /DW {} /W [{}] >>",
        units(face, '?'),
        width_array(&rows)
    );
    let box_ = font.global_bounding_box();
    let descriptor_dict = format!(
        "<< /Type /FontDescriptor /FontName /{name} /Flags {} /FontBBox [{}] /ItalicAngle {} /Ascent {} /Descent {} /CapHeight {} /StemV {} /FontFile2 {file_object} 0 R >>",
        flags(font),
        bbox(font, box_),
        angle(font.italic_angle()),
        scale_i(font.units_per_em(), font.ascender()),
        scale_i(font.units_per_em(), font.descender()),
        scale_i(font.units_per_em(), cap_height(font)),
        stem(font)
    );
    Ok(vec![
        type0_dict.into_bytes(),
        cid_dict.into_bytes(),
        descriptor_dict.into_bytes(),
        stream(&gid_map(&rows)),
        font_stream(&subset),
        stream(to_unicode(&rows).as_bytes()),
    ])
}

fn source(face: Face) -> &'static Source {
    macro_rules! load {
        ($slot:ident, $path:literal) => {{
            static $slot: Source = Source {
                bytes: include_bytes!($path),
                face: OnceLock::new(),
            };
            &$slot
        }};
    }
    match face {
        Face::Helvetica => load!(SANS, "../fonts/DejaVuSans.ttf"),
        Face::HelveticaBold => load!(SANS_BOLD, "../fonts/DejaVuSans-Bold.ttf"),
        Face::HelveticaOblique => load!(SANS_ITALIC, "../fonts/DejaVuSans-Oblique.ttf"),
        Face::HelveticaBoldOblique => load!(SANS_BOLD_ITALIC, "../fonts/DejaVuSans-BoldOblique.ttf"),
        Face::Courier => load!(MONO, "../fonts/DejaVuSansMono.ttf"),
        Face::CourierBold => load!(MONO_BOLD, "../fonts/DejaVuSansMono-Bold.ttf"),
        Face::CourierOblique => load!(MONO_ITALIC, "../fonts/DejaVuSansMono-Oblique.ttf"),
        Face::CourierBoldOblique => {
            load!(MONO_BOLD_ITALIC, "../fonts/DejaVuSansMono-BoldOblique.ttf")
        }
    }
}

fn parsed(face: Face) -> &'static TrueType<'static> {
    let source = source(face);
    source
        .face
        .get_or_init(|| TrueType::parse(source.bytes, 0).expect("TrueType font"))
}

fn glyph_id(face: Face, ch: char) -> Option<GlyphId> {
    parsed(face).glyph_index(ch).filter(|glyph| glyph.0 != 0)
}

fn cap_height(font: &TrueType<'_>) -> i16 {
    glyph_id_of(font, 'H')
        .and_then(|glyph| font.glyph_bounding_box(glyph))
        .map(|rect| rect.y_max)
        .unwrap_or_else(|| font.ascender())
}

fn glyph_id_of(font: &TrueType<'_>, ch: char) -> Option<GlyphId> {
    font.glyph_index(ch).filter(|glyph| glyph.0 != 0)
}

fn scale_u(upem: u16, units: u16) -> u16 {
    let upem = u32::from(upem);
    ((u32::from(units) * 1000 + upem / 2) / upem) as u16
}

pub(crate) fn vertical(face: Face) -> (i32, i32) {
    let font = parsed(face);
    let upem = font.units_per_em();
    (
        scale_i(upem, font.ascender()),
        scale_i(upem, font.descender()),
    )
}

pub(crate) fn decor(face: Face) -> (i32, i32, i32, i32) {
    let font = parsed(face);
    let upem = font.units_per_em();
    let underline = font.underline_metrics();
    let strike = font.strikeout_metrics();
    (
        scale_i(upem, underline.map(|line| line.position).unwrap_or(-63)),
        scale_i(upem, underline.map(|line| line.thickness).unwrap_or(44)),
        scale_i(upem, strike.map(|line| line.position).unwrap_or(259)),
        scale_i(upem, strike.map(|line| line.thickness).unwrap_or(50)),
    )
}

fn scale_i(upem: u16, units: i16) -> i32 {
    let upem = i32::from(upem);
    let value = i32::from(units);
    if value >= 0 {
        (value * 1000 + upem / 2) / upem
    } else {
        (value * 1000 - upem / 2) / upem
    }
}

fn bbox(font: &TrueType<'_>, rect: Rect) -> String {
    [
        scale_i(font.units_per_em(), rect.x_min),
        scale_i(font.units_per_em(), rect.y_min),
        scale_i(font.units_per_em(), rect.x_max),
        scale_i(font.units_per_em(), rect.y_max),
    ]
    .map(|value| value.to_string())
    .join(" ")
}

fn flags(font: &TrueType<'_>) -> u32 {
    // Symbolic: Greek and Cyrillic sit outside the Adobe Latin set.
    let mut flags = 4;
    if font.italic_angle() != 0.0 {
        flags |= 64;
    }
    if font.is_monospaced() {
        flags |= 1;
    }
    flags
}

fn stem(font: &TrueType<'_>) -> u32 {
    if font.is_bold() {
        140
    } else {
        80
    }
}

fn angle(value: f32) -> String {
    let text = format!("{value:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn ps_name(face: Face) -> &'static str {
    match face {
        Face::Helvetica => "DejaVuSans",
        Face::HelveticaBold => "DejaVuSans-Bold",
        Face::HelveticaOblique => "DejaVuSans-Oblique",
        Face::HelveticaBoldOblique => "DejaVuSans-BoldOblique",
        Face::Courier => "DejaVuSansMono",
        Face::CourierBold => "DejaVuSansMono-Bold",
        Face::CourierOblique => "DejaVuSansMono-Oblique",
        Face::CourierBoldOblique => "DejaVuSansMono-BoldOblique",
    }
}

fn tag(rows: &[(u16, u16, u16, u16)]) -> String {
    let mut hash = 0x811C_9DC5u32;
    for (cid, gid, unicode, _) in rows {
        hash ^= u32::from(*cid);
        hash = hash.wrapping_mul(0x0100_0193);
        hash ^= u32::from(*gid);
        hash = hash.wrapping_mul(0x0100_0193);
        hash ^= u32::from(*unicode);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    (0..6)
        .map(|shift| {
            let index = ((hash >> (shift * 5)) & 31) as u8 % 26;
            char::from(b'A' + index)
        })
        .collect()
}

fn width_array(rows: &[(u16, u16, u16, u16)]) -> String {
    let mut out = String::new();
    for (cid, _, _, width) in rows {
        out.push_str(&format!("{cid} [{width}] "));
    }
    out.pop();
    out
}

fn gid_map(rows: &[(u16, u16, u16, u16)]) -> Vec<u8> {
    let max = rows.iter().map(|(cid, _, _, _)| *cid).max().unwrap_or(0) as usize;
    let mut map = vec![0u8; (max + 1) * 2];
    for (cid, gid, _, _) in rows {
        let at = *cid as usize * 2;
        map[at..at + 2].copy_from_slice(&gid.to_be_bytes());
    }
    map
}

fn to_unicode(rows: &[(u16, u16, u16, u16)]) -> String {
    let mut out = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    for chunk in rows.chunks(100) {
        out.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (cid, _, unicode, _) in chunk {
            out.push_str(&format!("<{cid:04X}> <{unicode:04X}>\n"));
        }
        out.push_str("endbfchar\n");
    }
    out.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    out
}

fn stream(data: &[u8]) -> Vec<u8> {
    framed(data, &format!("<< /Length {} >>", data.len()))
}

fn font_stream(data: &[u8]) -> Vec<u8> {
    framed(
        data,
        &format!("<< /Length {} /Length1 {} >>", data.len(), data.len()),
    )
}

fn framed(data: &[u8], dict: &str) -> Vec<u8> {
    let mut out = format!("{dict}\nstream\n").into_bytes();
    out.extend_from_slice(data);
    if !data.ends_with(b"\n") {
        out.push(b'\n');
    }
    out.extend_from_slice(b"endstream");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_subset_keeps_latin_greek_and_cyrillic() {
        assert!(covers(Face::Helvetica, 'é'));
        assert!(covers(Face::Helvetica, 'α'));
        assert!(covers(Face::Helvetica, 'я'));
        assert!(!covers(Face::Helvetica, '你'));
        assert_eq!(units(Face::Helvetica, 'A'), 684);

        let mut chars = BTreeSet::new();
        chars.extend("café λόγος я".chars());
        let objects = objects(Face::Helvetica, &chars, 1).expect("subset font");
        let descriptor = String::from_utf8_lossy(&objects[2]);
        assert!(descriptor.contains(" /Flags 4 "));
        let file = &objects[4];
        assert!(file.windows(9).any(|window| window == b"/Length1 "));
        assert!(file.len() < source(Face::Helvetica).bytes.len());
        let unicode = String::from_utf8_lossy(&objects[5]);
        assert!(unicode.contains("<00E9>"));
        assert!(unicode.contains("<03BB>"));
        assert!(unicode.contains("<044F>"));
    }

    #[test]
    fn the_content_cid_selects_that_glyph() {
        for ch in ['é', 'λ', 'я'] {
            let mut chars = BTreeSet::new();
            chars.insert(ch);
            let built = objects(Face::Helvetica, &chars, 1).expect("subset font");
            let cid = cid(Face::Helvetica, ch);
            let pair = format!("<{cid:04X}> <{:04X}>", u32::from(ch));
            let unicode = String::from_utf8_lossy(&built[5]);
            assert!(unicode.contains(&pair), "{pair} missing from {unicode}");

            let map = stream_bytes(&built[3]);
            let subset_gid = u16::from_be_bytes([map[cid as usize * 2], map[cid as usize * 2 + 1]]);
            let file = stream_bytes(&built[4]);
            let subset = TrueType::parse(&file, 0).expect("subset font");
            let drawn = subset.glyph_bounding_box(GlyphId(subset_gid)).unwrap();
            let source = super::parsed(Face::Helvetica);
            let original = source
                .glyph_bounding_box(super::glyph_id(Face::Helvetica, ch).unwrap())
                .unwrap();
            assert_eq!(
                (drawn.x_min, drawn.y_min, drawn.x_max, drawn.y_max),
                (
                    original.x_min,
                    original.y_min,
                    original.x_max,
                    original.y_max
                )
            );
            assert!(drawn.y_max > drawn.y_min);
        }
    }

    #[test]
    fn a_decomposed_letter_embeds_the_composed_glyph() {
        let pdf = crate::draw(&[crate::Block::Paragraph(vec![crate::Run::plain(
            "cafe\u{0301}",
        )])])
        .expect("subset font");
        let shown = crate::encode::tj("café");
        assert!(pdf
            .windows(shown.len())
            .any(|window| window == shown.as_slice()));

        let cid = cid(Face::Helvetica, 'é') as usize;
        let map = stream_bytes(pdf_object(&pdf, pdf_number_after(&pdf, b"/CIDToGIDMap ")));
        let subset_gid = u16::from_be_bytes([map[cid * 2], map[cid * 2 + 1]]);
        assert_ne!(subset_gid, 0);
        let file = stream_bytes(pdf_object(&pdf, pdf_number_after(&pdf, b"/FontFile2 ")));
        let subset = TrueType::parse(&file, 0).expect("subset font");
        let drawn = subset.glyph_bounding_box(GlyphId(subset_gid)).unwrap();
        let source = parsed(Face::Helvetica);
        let original = source
            .glyph_bounding_box(glyph_id(Face::Helvetica, 'é').unwrap())
            .unwrap();
        assert_eq!(
            (drawn.x_min, drawn.y_min, drawn.x_max, drawn.y_max),
            (
                original.x_min,
                original.y_min,
                original.x_max,
                original.y_max
            )
        );
    }

    #[test]
    fn a_footnote_arrow_has_its_own_glyph() {
        for face in [
            Face::Helvetica,
            Face::HelveticaBold,
            Face::HelveticaOblique,
            Face::HelveticaBoldOblique,
            Face::Courier,
            Face::CourierBold,
            Face::CourierOblique,
            Face::CourierBoldOblique,
        ] {
            assert!(covers(face, '\u{21A9}'), "{face:?}");
            assert_ne!(cid(face, '\u{21A9}'), cid(face, '^'), "{face:?}");
        }
        let font = parsed(Face::Helvetica);
        let arrow = font
            .glyph_bounding_box(glyph_id(Face::Helvetica, '\u{21A9}').unwrap())
            .unwrap();
        let caret = font
            .glyph_bounding_box(glyph_id(Face::Helvetica, '^').unwrap())
            .unwrap();
        assert_ne!(
            (arrow.x_min, arrow.y_min, arrow.x_max, arrow.y_max),
            (caret.x_min, caret.y_min, caret.x_max, caret.y_max)
        );
        let (ascender, descender) = vertical(Face::Helvetica);
        assert_eq!((ascender, descender), (928, -236));
        let (underline, underline_thickness, strike, strike_thickness) = decor(Face::Helvetica);
        assert_eq!(
            (underline, underline_thickness, strike, strike_thickness),
            (-63, 44, 259, 50)
        );
        assert_eq!(vertical(Face::Courier), vertical(Face::Helvetica));
        assert_eq!(decor(Face::Courier), decor(Face::Helvetica));
    }

    fn pdf_number_after(pdf: &[u8], key: &[u8]) -> usize {
        let at = pdf
            .windows(key.len())
            .position(|window| window == key)
            .unwrap()
            + key.len();
        let text = std::str::from_utf8(&pdf[at..at + 16]).unwrap();
        text.split_whitespace().next().unwrap().parse().unwrap()
    }

    fn pdf_object(pdf: &[u8], number: usize) -> &[u8] {
        let marker = format!("\n{number} 0 obj\n");
        let at = pdf
            .windows(marker.len())
            .position(|window| window == marker.as_bytes())
            .unwrap()
            + marker.len();
        let end = pdf[at..]
            .windows(8)
            .position(|window| window == b"\nendobj\n")
            .unwrap();
        &pdf[at..at + end]
    }

    fn stream_bytes(object: &[u8]) -> Vec<u8> {
        let start = object
            .windows(7)
            .position(|window| window == b"stream\n")
            .unwrap()
            + 7;
        let head = std::str::from_utf8(&object[..start]).unwrap();
        let at = head.find("/Length ").unwrap() + "/Length ".len();
        let len: usize = head[at..]
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        object[start..start + len].to_vec()
    }
}
