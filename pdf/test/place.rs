use super::*;
use crate::place::place;

fn thousandths(points: f32) -> i32 {
    (f64::from(points) * 1000.0).round() as i32
}

fn numbered(count: usize) -> Vec<String> {
    (0..count).map(|index| format!("line {index}")).collect()
}

#[test]
fn a_line_sits_in_the_a4_content_box() {
    let pages = place(&["Hello", "next"], Face::Helvetica, 12.0, 16.0);
    assert_eq!(pages.len(), 1);
    let page = &pages[0];
    assert_eq!(page.lines.len(), 2);
    assert_eq!(page.lines[0].text, "Hello");
    assert!((page.lines[0].x - MARGIN).abs() < 0.01);
    assert_eq!(thousandths(page.lines[0].y), 758_754);
    assert_eq!(thousandths(page.lines[1].y), 758_754 - 16_000);
    assert!((content_width() - (PAGE_WIDTH - 2.0 * MARGIN)).abs() < 0.01);
}

#[test]
fn a_line_past_the_bottom_margin_starts_a_new_page() {
    let one_page = place(&numbered(43), Face::Helvetica, 12.0, 16.0);
    assert_eq!(one_page.len(), 1);
    assert_eq!(one_page[0].lines.len(), 43);
    let last = thousandths(one_page[0].lines[42].y);
    assert!(last - 2_832 >= 72_000);

    let two_pages = place(&numbered(44), Face::Helvetica, 12.0, 16.0);
    assert_eq!(two_pages.len(), 2);
    assert_eq!(two_pages[0].lines.len(), 43);
    assert_eq!(two_pages[1].lines.len(), 1);
    assert_eq!(two_pages[1].lines[0].text, "line 43");
    assert_eq!(thousandths(two_pages[1].lines[0].y), 758_754);
}

#[test]
fn sans_and_mono_share_a_line_box() {
    let sans = place(&numbered(43), Face::Helvetica, 12.0, 16.0);
    let mono = place(&numbered(43), Face::Courier, 12.0, 16.0);
    assert_eq!(sans.len(), 1);
    assert_eq!(mono.len(), 1);
    assert_eq!(
        thousandths(sans[0].lines[0].y),
        thousandths(mono[0].lines[0].y)
    );
    let overflow = place(&numbered(44), Face::Courier, 12.0, 16.0);
    assert_eq!(overflow[0].lines.len(), 43);
    assert_eq!(overflow[1].lines[0].text, "line 43");
}

#[test]
fn a_line_taller_than_the_page_still_takes_a_page() {
    let pages = place(&["a", "b"], Face::Helvetica, 1000.0, 1200.0);
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[0].lines[0].text, "a");
    assert_eq!(pages[1].lines[0].text, "b");
}
