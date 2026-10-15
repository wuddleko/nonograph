use super::*;
use crate::write::line_pdf;

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    find(haystack, needle).is_some()
}

fn literal(text: &str) -> Vec<u8> {
    crate::encode::literal(&crate::encode::utf16(text, Face::Helvetica)).into_bytes()
}

#[test]
fn a_coordinate_keeps_a_fraction_and_its_sign() {
    use crate::write::format_thousandths;
    assert_eq!(format_thousandths(724_710), "724.71");
    assert_eq!(format_thousandths(72_000), "72");
    assert_eq!(format_thousandths(-500), "-0.5");
    assert_eq!(format_thousandths(-1_500), "-1.5");
}

#[test]
fn a_line_is_written_as_a_pdf() {
    let bytes = line_pdf("One Helvetica line");
    assert!(bytes.starts_with(b"%PDF"));
    assert!(contains(&bytes, b"/MediaBox [0 0 595.28 841.89]"));
    assert!(contains(&bytes, &literal("One Helvetica line")));
    assert!(bytes.ends_with(b"%%EOF\n"));
    assert!(!contains(&bytes, b"/Filter"));
}

#[test]
fn parentheses_and_backslashes_are_escaped() {
    let bytes = line_pdf(r"a(b)\c");
    assert!(contains(&bytes, &literal(r"a(b)\c")));
}

#[test]
fn a_character_helvetica_cannot_draw_becomes_a_question_mark() {
    let bytes = line_pdf("a你b\nc\u{0080}d");
    assert!(contains(&bytes, &literal("a?b?c?d")));
}

#[test]
fn a_latin_letter_helvetica_can_draw_is_kept() {
    let bytes = line_pdf("café λόγος я");
    assert!(contains(&bytes, &literal("café λόγος я")));
    assert!(contains(&bytes, b"<00E9>"));
    assert!(contains(&bytes, b"<03BB>"));
    assert!(contains(&bytes, b"<044F>"));
    assert!(contains(&bytes, b"/Encoding /Identity-H"));
}

#[test]
fn the_content_stream_has_the_length_it_declares() {
    let bytes = line_pdf("One Helvetica line");
    let length_at = find(&bytes, b"/Length ").expect("length") + "/Length ".len();
    let length_end = bytes[length_at..]
        .iter()
        .position(|byte| !byte.is_ascii_digit())
        .unwrap();
    let length: usize = std::str::from_utf8(&bytes[length_at..length_at + length_end])
        .unwrap()
        .parse()
        .unwrap();
    let stream_at = find(&bytes[length_at..], b"stream\n").unwrap() + length_at + "stream\n".len();
    assert_eq!(
        &bytes[stream_at + length..stream_at + length + b"endstream".len()],
        b"endstream"
    );
    assert!(contains(
        &bytes[stream_at..stream_at + length],
        &literal("One Helvetica line")
    ));
}

#[test]
fn the_cross_reference_lands_on_each_object() {
    let bytes = line_pdf("One Helvetica line");
    let xref = find(&bytes, b"xref\n").unwrap();
    let header = b"xref\n0 ";
    let count_at = find(&bytes, header).unwrap() + header.len();
    let count_end = bytes[count_at..]
        .iter()
        .position(|byte| *byte == b'\n')
        .unwrap();
    let count: usize = std::str::from_utf8(&bytes[count_at..count_at + count_end])
        .unwrap()
        .parse()
        .unwrap();
    let mut entry = count_at + count_end + 1;
    for number in 0..count {
        let record = &bytes[entry..entry + 20];
        let offset: usize = std::str::from_utf8(&record[..10]).unwrap().parse().unwrap();
        assert_eq!(record.len(), 20);
        assert_eq!(
            &record[17..20],
            if number == 0 { b"f\r\n" } else { b"n\r\n" }
        );
        if number > 0 {
            let marker = format!("{number} 0 obj");
            assert_eq!(&bytes[offset..offset + marker.len()], marker.as_bytes());
        }
        entry += 20;
    }
    let start_at = find(&bytes, b"startxref\n").unwrap() + "startxref\n".len();
    let start_end = bytes[start_at..]
        .iter()
        .position(|byte| *byte == b'\n')
        .unwrap();
    let start: usize = std::str::from_utf8(&bytes[start_at..start_at + start_end])
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(start, xref);
}
