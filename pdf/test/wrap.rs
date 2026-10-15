use super::*;
use crate::wrap::{wrap_glyphs, Glyph};

fn em_width(text: &str, face: Face, size: f32) -> f32 {
    advance(text, face) as f32 * size / 1000.0
}

fn glyphs(text: &str, face: Face) -> Vec<Glyph> {
    text.chars()
        .map(|ch| Glyph {
            ch,
            face,
            underline: false,
            strike: false,
            link: 0,
        })
        .collect()
}

#[test]
fn the_same_words_in_bold_are_wider_than_in_regular() {
    let words = "same words";
    let regular = advance(words, Face::Helvetica);
    let bold = advance(words, Face::HelveticaBold);
    assert!(bold > regular);

    let width = regular as f32;
    assert_eq!(wrap(words, Face::Helvetica, 1000.0, width), vec![words]);
    assert!(wrap(words, Face::HelveticaBold, 1000.0, width).len() > 1);
}

#[test]
fn courier_keeps_one_width_for_every_glyph() {
    assert_eq!(advance("i", Face::Courier), advance("m", Face::Courier));
    assert_eq!(
        advance("same words", Face::CourierBold),
        advance("same words", Face::Courier)
    );
    assert_eq!(advance("A", Face::Courier), 602);
    assert_eq!(advance("α", Face::Courier), advance("A", Face::Courier));
    assert_eq!(advance("я", Face::Courier), advance("A", Face::Courier));
}

#[test]
fn a_run_wraps_on_the_space_before_the_word_that_does_not_fit() {
    let width = em_width("one two", Face::Courier, 10.0);
    assert_eq!(
        wrap("one two three", Face::Courier, 10.0, width),
        vec!["one two".to_string(), "three".to_string()]
    );
}

#[test]
fn code_keeps_its_spaces_and_wraps_a_long_line() {
    let face = Face::Courier;
    let size = 10.0;
    assert_eq!(
        wrap_code("  a  b", face, size, 60.0),
        vec!["  a  b".to_string()]
    );
    assert_eq!(
        wrap_code("one two", face, size, em_width("one ", face, size)),
        vec!["one ".to_string(), "two".to_string()]
    );
    assert_eq!(
        wrap_code("one two", face, size, em_width("one", face, size)),
        vec!["one ".to_string(), "two".to_string()]
    );
    assert_eq!(
        wrap_code("  abcd", face, size, em_width("    ", face, size)),
        vec!["  ab".to_string(), "cd".to_string()]
    );
    assert_eq!(
        wrap_code("abcdef", face, size, em_width("ab", face, size)),
        vec!["ab".to_string(), "cd".to_string(), "ef".to_string()]
    );
    assert_eq!(
        wrap_code("     ", face, size, em_width("  ", face, size)),
        vec!["  ".to_string(), "  ".to_string(), " ".to_string()]
    );
    assert_eq!(
        wrap_code("\t\treturn", Face::Courier, 10.0, 200.0),
        vec!["        return".to_string()]
    );
    assert_eq!(
        wrap_code("a\tb", Face::Courier, 10.0, 200.0),
        vec!["a    b".to_string()]
    );
    assert_eq!(
        wrap_code("a\n\nb", Face::Courier, 10.0, 60.0),
        vec!["a".to_string(), String::new(), "b".to_string()]
    );
    assert_eq!(
        wrap_code("a\r\nb", Face::Courier, 10.0, 60.0),
        vec!["a".to_string(), "b".to_string()]
    );
}

#[test]
fn a_word_longer_than_the_line_breaks_by_character() {
    let width = em_width("ab", Face::Courier, 10.0);
    assert_eq!(
        wrap("abcdef", Face::Courier, 10.0, width),
        vec!["ab".to_string(), "cd".to_string(), "ef".to_string()]
    );
}

#[test]
fn a_newline_measures_as_the_stand_in_and_wraps_as_a_break() {
    assert_eq!(
        advance("\n", Face::Helvetica),
        advance("?", Face::Helvetica)
    );
    assert_eq!(
        advance("你", Face::Helvetica),
        advance("?", Face::Helvetica)
    );
    assert_eq!(
        wrap("a\r\nb", Face::Courier, 10.0, 60.0),
        vec!["a".to_string(), "b".to_string()]
    );
}

#[test]
fn oblique_keeps_the_helvetica_widths() {
    assert_eq!(
        advance("same words", Face::HelveticaOblique),
        advance("same words", Face::Helvetica)
    );
    assert_eq!(
        advance("same words", Face::HelveticaBoldOblique),
        advance("same words", Face::HelveticaBold)
    );
    assert_eq!(
        advance("same words", Face::CourierOblique),
        advance("same words", Face::Courier)
    );
    assert_eq!(
        advance("same words", Face::CourierBoldOblique),
        advance("same words", Face::CourierBold)
    );
}

#[test]
fn glyphs_of_one_face_wrap_like_the_text() {
    for (text, face, size, width) in [
        ("one two three", Face::Courier, 10.0, 48.0),
        ("a\r\nb", Face::Courier, 10.0, 60.0),
        ("abcdef", Face::Helvetica, 12.0, content_width()),
    ] {
        let lines = wrap_glyphs(&glyphs(text, face), size, width);
        let joined: Vec<String> = lines
            .iter()
            .map(|line| line.iter().map(|segment| segment.text.clone()).collect())
            .collect();
        assert_eq!(joined, wrap(text, face, size, width));
    }
}

#[test]
fn embedded_widths_come_from_the_font() {
    assert_eq!(advance("A", Face::Helvetica), 684);
    assert_eq!(advance("A", Face::HelveticaBold), 774);
    assert_eq!(advance("'", Face::Helvetica), 275);
    assert_eq!(advance("€", Face::Helvetica), 636);
    assert_eq!(advance("—", Face::Helvetica), 1000);
    assert_ne!(advance("α", Face::Helvetica), advance("?", Face::Helvetica));
    assert_ne!(advance("я", Face::Helvetica), advance("?", Face::Helvetica));
}
