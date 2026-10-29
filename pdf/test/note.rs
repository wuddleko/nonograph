use super::*;

fn render(blocks: &[Block]) -> Vec<u8> {
    super::draw(blocks).expect("subset font")
}

fn note_bytes(title: &str, alias: &str, html: &str) -> Vec<u8> {
    super::note_pdf(title, alias, html).expect("subset font")
}

use crate::place::measure;
use crate::write::format_thousandths;

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    find(haystack, needle).is_some()
}

fn mono_slots() -> usize {
    let max_units = (f64::from(content_width()) / 10.0 * 1000.0 + 0.01).floor() as u32;
    (max_units / advance("a", Face::Courier)) as usize
}

fn prose(text: &str) -> Block {
    Block::Paragraph(vec![Run::plain(text.to_string())])
}

fn heading(level: u8, text: &str) -> Block {
    Block::Heading(level, vec![Run::plain(text.to_string())])
}

fn deep_list(levels: usize, leaf: &str) -> Block {
    let mut item = item(Marker::Bullet, leaf);
    for _ in 1..levels {
        item = Item {
            marker: Marker::Bullet,
            runs: Vec::new(),
            children: vec![item],
        };
    }
    Block::List(vec![item])
}

fn item(marker: Marker, text: &str) -> Item {
    Item {
        marker,
        runs: vec![Run::plain(text.to_string())],
        children: Vec::new(),
    }
}

fn thousandths_of(number: &str) -> i32 {
    let negative = number.starts_with('-');
    let number = number.trim_start_matches('-');
    let (whole, fraction) = number.split_once('.').unwrap_or((number, ""));
    let mut value = whole.parse::<i32>().unwrap() * 1000;
    if !fraction.is_empty() {
        let mut digits = fraction.to_string();
        assert!(digits.len() <= 3, "{number}");
        while digits.len() < 3 {
            digits.push('0');
        }
        value += digits.parse::<i32>().unwrap();
    }
    if negative {
        -value
    } else {
        value
    }
}

fn position_of(pdf: &[u8], shown: &[u8]) -> (i32, i32) {
    let at = find(pdf, shown).unwrap();
    let marker = b"1 0 0 1 ";
    let start = pdf[..at]
        .windows(marker.len())
        .rposition(|window| window == marker)
        .unwrap()
        + marker.len();
    let end = pdf[start..].iter().position(|byte| *byte == b'\n').unwrap();
    let text = std::str::from_utf8(&pdf[start..start + end]).unwrap();
    let mut parts = text.split_whitespace();
    let x = thousandths_of(parts.next().unwrap());
    let y = thousandths_of(parts.next().unwrap());
    (x, y)
}

fn object(pdf: &[u8], number: usize) -> &[u8] {
    let marker = format!("\n{number} 0 obj\n");
    let at = find(pdf, marker.as_bytes()).unwrap() + marker.len();
    let end = find(&pdf[at..], b"\nendobj\n").unwrap();
    &pdf[at..at + end]
}

fn between<'a>(haystack: &'a [u8], start: &[u8], end: &[u8]) -> &'a [u8] {
    let at = find(haystack, start).unwrap() + start.len();
    let stop = find(&haystack[at..], end).unwrap();
    &haystack[at..at + stop]
}

fn page_objects(pdf: &[u8]) -> Vec<&[u8]> {
    let kids = between(object(pdf, 2), b"/Kids [", b"]");
    let kids = std::str::from_utf8(kids).unwrap();
    kids.split(" R")
        .filter_map(|kid| {
            let page_number = kid.split_whitespace().next()?.parse().ok()?;
            Some(object(pdf, page_number))
        })
        .collect()
}

fn page_streams(pdf: &[u8]) -> Vec<&[u8]> {
    let kids = between(object(pdf, 2), b"/Kids [", b"]");
    let kids = std::str::from_utf8(kids).unwrap();
    kids.split(" R")
        .filter_map(|kid| {
            let page_number = kid.split_whitespace().next()?.parse().ok()?;
            let page = object(pdf, page_number);
            let contents: usize = std::str::from_utf8(between(page, b"/Contents ", b" 0 R"))
                .unwrap()
                .parse()
                .unwrap();
            let body = object(pdf, contents);
            let start = find(body, b"\nstream\n").unwrap() + "\nstream\n".len();
            let end = find(&body[start..], b"endstream").unwrap();
            Some(&body[start..start + end])
        })
        .collect()
}

#[test]
fn a_title_an_alias_and_a_paragraph_are_drawn_in_order() {
    let pdf = render(&[
        Block::Title("Title".to_string()),
        Block::Alias("Ada".to_string()),
        prose("Hello"),
    ]);
    let title = find(&pdf, &crate::encode::tj("Title")).unwrap();
    let alias = find(&pdf, &crate::encode::tj("Ada")).unwrap();
    let paragraph = find(&pdf, &crate::encode::tj("Hello")).unwrap();
    assert!(title < alias && alias < paragraph);
    assert!(contains(&pdf, b"+DejaVuSans-Bold"));
    assert!(contains(&pdf, b"/F2 22 Tf"));
    assert!(contains(&pdf, b"/F1 10 Tf"));
    assert!(contains(&pdf, b"/F1 12 Tf"));
    assert_eq!(
        position_of(&pdf, &crate::encode::tj("Title")),
        (72_000, 749_474)
    );
    assert_eq!(
        position_of(&pdf, &crate::encode::tj("Ada")),
        (72_000, 722_610)
    );
    assert_eq!(
        position_of(&pdf, &crate::encode::tj("Hello")),
        (72_000, 695_754)
    );
    assert!(contains(&pdf, b"/Count 1"));
}

#[test]
fn a_paragraph_alone_starts_at_the_top_of_the_content_box() {
    let pdf = render(&[prose("Hello")]);
    assert!(contains(&pdf, b"/F1 12 Tf"));
    assert_eq!(
        position_of(&pdf, &crate::encode::tj("Hello")),
        (72_000, 758_754)
    );
    assert!(contains(&pdf, &crate::encode::tj("Hello")));
    assert!(!contains(&pdf, b"DejaVuSans-Bold"));
}

#[test]
fn note_pdf_reads_the_html_and_draws_it() {
    let html = concat!(
        "<h1>Chapter</h1><p>See <a href=\"https://example.com\">go</a></p>",
        "<img src=\"https://example.com/a.jpg\" alt=\"pic\">",
    );
    let pdf = note_bytes("Title", "Ada", html);
    assert_eq!(
        pdf,
        render(&[
            Block::Title("TITLE".to_string()),
            Block::Alias("Ada".to_string()),
            heading(1, "Chapter"),
            Block::Paragraph(vec![
                Run::plain("See "),
                Run {
                    url: Some("https://example.com".to_string()),
                    ..Run::plain("go")
                },
            ]),
            Block::Paragraph(vec![Run {
                url: Some("https://example.com/a.jpg".to_string()),
                ..Run::plain("https://example.com/a.jpg")
            }]),
        ])
    );
    assert_eq!(
        position_of(&pdf, &crate::encode::tj("TITLE")),
        (72_000, 749_474)
    );
    assert_eq!(
        position_of(&pdf, &crate::encode::tj("Ada")),
        (72_000, 722_610)
    );
    assert_eq!(
        position_of(&pdf, &crate::encode::tj("Chapter")),
        (72_000, 684_330)
    );
    assert_eq!(
        position_of(&pdf, &crate::encode::tj("See ")),
        (72_000, 653_754)
    );
    let go_x = 72_000 + measure("See ", Face::Helvetica, 12.0);
    assert_eq!(position_of(&pdf, &crate::encode::tj("go")), (go_x, 653_754));
    assert!(contains(&pdf, b"/URI (https://example.com)"));
    assert_eq!(
        position_of(&pdf, &crate::encode::tj("https://example.com/a.jpg")),
        (72_000, 625_754)
    );

    let titled = note_bytes("Title", "", "<p>Hello</p>");
    assert_eq!(
        titled,
        render(&[Block::Title("TITLE".to_string()), prose("Hello")])
    );
    assert_eq!(
        position_of(&titled, &crate::encode::tj("Hello")),
        (72_000, 718_754)
    );
    assert!(!contains(&titled, b"/F1 10 Tf"));

    let aliased = note_bytes("", "Ada", "<p>Hello</p>");
    assert_eq!(
        aliased,
        render(&[Block::Alias("Ada".to_string()), prose("Hello")])
    );
    assert_eq!(
        position_of(&aliased, &crate::encode::tj("Ada")),
        (72_000, 760_610)
    );
    assert_eq!(
        position_of(&aliased, &crate::encode::tj("Hello")),
        (72_000, 733_754)
    );

    assert_eq!(
        note_bytes("", "", "<p>Hello</p>"),
        render(&[prose("Hello")])
    );
    assert_eq!(note_bytes("", "", ""), render(&[]));
}

#[test]
fn an_empty_alias_leaves_no_line() {
    let pdf = render(&[
        Block::Title("Title".to_string()),
        Block::Alias(String::new()),
        prose("Hello"),
    ]);
    assert!(!contains(&pdf, b"/F1 10 Tf"));
    assert!(contains(&pdf, &crate::encode::tj("Title")));
    assert!(contains(&pdf, &crate::encode::tj("Hello")));
}

#[test]
fn a_long_paragraph_continues_on_the_next_page() {
    let mut words = vec!["alpha".to_string()];
    let text = loop {
        let text = words.join(" ");
        let count = wrap(&text, Face::Helvetica, 12.0, content_width()).len();
        if count == 44 {
            break text;
        }
        assert!(count < 44, "wrap jumped to {count}");
        words.push("alpha".to_string());
    };
    let lines = wrap(&text, Face::Helvetica, 12.0, content_width());
    let pdf = render(&[Block::Paragraph(vec![Run::plain(text)])]);
    let streams = page_streams(&pdf);
    assert_eq!(streams.len(), 2);
    let first = crate::encode::tj(&lines[0]);
    let last = crate::encode::tj(&lines[43]);
    assert!(contains(streams[0], &first));
    assert!(contains(streams[1], &last));
    assert!(!contains(streams[0], &last));
}

#[test]
fn a_javascript_link_keeps_its_words_and_has_no_action() {
    let pdf = render(&read("<p><a href=\"javascript:alert(1)\">x</a></p>"));
    assert!(contains(&pdf, &crate::encode::tj("x")));
    assert!(!contains(&pdf, b"/URI"));
    assert!(!contains(&pdf, b"javascript:"));
    assert!(!contains(&pdf, b" m\n"));
    assert!(rects(&pdf).is_empty());

    let data = render(&read("<p><a href=\"data:text/html,hi\">y</a></p>"));
    assert!(contains(&data, &crate::encode::tj("y")));
    assert!(!contains(&data, b"/URI"));
    assert!(!contains(&data, b"data:"));

    let mail = render(&read("<p><a href=\"mailto:a@b.com\">z</a></p>"));
    assert!(contains(&mail, &crate::encode::tj("z")));
    assert!(!contains(&mail, b"/URI"));

    let http = render(&read("<p><a href=\"HTTP://example.com/a\">go</a></p>"));
    assert!(contains(&http, &crate::encode::tj("go")));
    assert!(contains(&http, b"/URI (HTTP://example.com/a)"));
    assert_eq!(rects(&http).len(), 1);
}

#[test]
fn paragraph_text_is_escaped_and_unprintable_characters_become_a_question_mark() {
    let pdf = render(&[Block::Title(r"a(b)\c".to_string()), prose("a你b")]);
    assert!(contains(&pdf, &crate::encode::tj(r"a(b)\c")));
    assert!(contains(&pdf, &crate::encode::tj("a?b")));

    let tabbed = render(&[prose("a\tb")]);
    assert!(contains(&tabbed, &crate::encode::tj("a    b")));
    assert!(!contains(&tabbed, &crate::encode::tj("a?b")));
    let from_html = render(&read("<p>a\tb</p>"));
    assert!(contains(&from_html, &crate::encode::tj("a    b")));
    let bold = render(&[Block::Paragraph(vec![Run {
        bold: true,
        ..Run::plain("a\tb".to_string())
    }])]);
    assert!(contains(&bold, b"/F2 12 Tf"));
    assert!(contains(&bold, &crate::encode::tj("a    b")));
}

fn rects(page: &[u8]) -> Vec<(i32, i32, i32, i32)> {
    let mut found = Vec::new();
    let mut rest = page;
    while let Some(at) = find(rest, b"/Rect [") {
        let start = at + "/Rect [".len();
        let end = find(&rest[start..], b"]").unwrap();
        let text = std::str::from_utf8(&rest[start..start + end]).unwrap();
        let mut numbers = text.split_whitespace();
        found.push((
            thousandths_of(numbers.next().unwrap()),
            thousandths_of(numbers.next().unwrap()),
            thousandths_of(numbers.next().unwrap()),
            thousandths_of(numbers.next().unwrap()),
        ));
        rest = &rest[start + end..];
    }
    found
}

#[test]
fn inline_runs_use_their_own_face_and_rules() {
    let pdf = render(&[Block::Paragraph(vec![
        Run {
            bold: true,
            ..Run::plain("Hello")
        },
        Run::plain("!"),
        Run {
            italic: true,
            ..Run::plain(" there")
        },
        Run {
            code: true,
            ..Run::plain(" yes")
        },
    ])]);
    assert!(contains(&pdf, b"+DejaVuSans-Bold"));
    assert!(contains(&pdf, b"+DejaVuSans-Oblique"));
    assert!(contains(&pdf, b"+DejaVuSansMono"));
    assert!(contains(&pdf, b"/F2 12 Tf"));
    assert!(contains(&pdf, b"/F5 12 Tf"));
    assert!(contains(&pdf, b"/F3 12 Tf"));
    assert_eq!(
        position_of(&pdf, &crate::encode::tj("!")).0,
        72_000 + measure("Hello", Face::HelveticaBold, 12.0)
    );
    assert!(measure("Hello", Face::HelveticaBold, 12.0) > measure("Hello", Face::Helvetica, 12.0));
    assert_eq!(
        position_of(&pdf, &crate::encode::tj(" yes")).0,
        position_of(&pdf, &crate::encode::tj(" there")).0
            + measure(" there", Face::HelveticaOblique, 12.0)
    );

    let ruled = render(&[Block::Paragraph(vec![
        Run {
            underline: true,
            ..Run::plain("Hello")
        },
        Run {
            strike: true,
            ..Run::plain("Hello")
        },
    ])]);
    let hello = measure("Hello", Face::Helvetica, 12.0);
    assert!(contains(
        &ruled,
        format!(
            "72 757.998 m\n{} 757.998 l\n",
            format_thousandths(72_000 + hello)
        )
        .as_bytes()
    ));
    let strike = measure("Hello", Face::Helvetica, 12.0);
    let strike_x = 72_000 + hello + strike;
    assert!(contains(
        &ruled,
        format!(
            "{} 761.862 m\n{} 761.862 l\n",
            format_thousandths(72_000 + hello),
            format_thousandths(strike_x)
        )
        .as_bytes()
    ));
}

#[test]
fn a_link_keeps_a_rectangle_for_each_wrapped_fragment() {
    let url = "https://example.com/a(b)";
    let pdf = render(&[Block::Paragraph(vec![
        Run::plain("See "),
        Run {
            url: Some(url.to_string()),
            ..Run::plain("this")
        },
    ])]);
    let page = page_objects(&pdf)[0];
    let rect = rects(page);
    assert_eq!(rect.len(), 1);
    let left = 72_000 + measure("See ", Face::Helvetica, 12.0);
    let this_width = measure("this", Face::Helvetica, 12.0);
    assert_eq!(rect[0], (left, 755_922, left + this_width, 769_890));
    assert!(contains(page, br"/URI (https://example.com/a\(b\))"));
    expect_underline(
        page_streams(&pdf)[0],
        &crate::encode::tj("this"),
        left,
        this_width,
    );

    let mut words = vec!["beta".to_string()];
    let link_text = loop {
        let text = words.join(" ");
        let count = wrap(&text, Face::Helvetica, 12.0, content_width()).len();
        if count == 2 {
            break text;
        }
        assert!(count < 2, "wrap jumped to {count}");
        words.push("beta".to_string());
    };
    let wrapped = wrap(&link_text, Face::Helvetica, 12.0, content_width());
    let across = render(&[Block::Paragraph(vec![
        Run::plain("alpha\n".repeat(42)),
        Run {
            url: Some("https://example.com/wrapped".to_string()),
            ..Run::plain(link_text)
        },
    ])]);
    let streams = page_streams(&across);
    let pages = page_objects(&across);
    assert_eq!(pages.len(), 2);
    assert!(contains(streams[0], &crate::encode::tj(&wrapped[0])));
    assert!(contains(streams[1], &crate::encode::tj(&wrapped[1])));
    let first = rects(pages[0]);
    let second = rects(pages[1]);
    assert_eq!(first.len(), 1);
    assert_eq!(second.len(), 1);
    assert_eq!(
        first[0].2 - first[0].0,
        measure(&wrapped[0], Face::Helvetica, 12.0)
    );
    assert_eq!(
        second[0].2 - second[0].0,
        measure(&wrapped[1], Face::Helvetica, 12.0)
    );
    assert_ne!(first[0].1, second[0].1);
    assert!(contains(pages[0], b"/URI (https://example.com/wrapped)"));
    assert!(contains(pages[1], b"/URI (https://example.com/wrapped)"));
    expect_underline(
        streams[0],
        &crate::encode::tj(&wrapped[0]),
        first[0].0,
        first[0].2 - first[0].0,
    );
    expect_underline(
        streams[1],
        &crate::encode::tj(&wrapped[1]),
        second[0].0,
        second[0].2 - second[0].0,
    );
}

fn expect_underline(pdf: &[u8], shown: &[u8], x: i32, width: i32) {
    let (_, baseline) = position_of(pdf, shown);
    let y = baseline - 756;
    assert!(contains(
        pdf,
        format!(
            "{} {} m\n{} {} l\n",
            format_thousandths(x),
            format_thousandths(y),
            format_thousandths(x + width),
            format_thousandths(y)
        )
        .as_bytes()
    ));
}

#[test]
fn headings_step_down_in_bold() {
    let pdf = render(&[
        heading(1, "One"),
        heading(2, "Two"),
        heading(3, "Three"),
        heading(4, "Four"),
        heading(9, "Five"),
    ]);
    assert!(contains(&pdf, b"/F2 20 Tf"));
    assert!(contains(&pdf, b"/F2 16 Tf"));
    assert!(contains(&pdf, b"/F2 14 Tf"));
    assert!(contains(&pdf, b"/F2 12 Tf"));
    assert!(!contains(&pdf, b"+DejaVuSans "));
    assert_eq!(
        position_of(&pdf, &crate::encode::tj("One")),
        (72_000, 751_330)
    );
    assert_eq!(
        position_of(&pdf, &crate::encode::tj("Two")),
        (72_000, 715_042)
    );
    assert_eq!(
        position_of(&pdf, &crate::encode::tj("Three")),
        (72_000, 683_898)
    );
    assert_eq!(
        position_of(&pdf, &crate::encode::tj("Four")),
        (72_000, 655_754)
    );
    assert_eq!(
        position_of(&pdf, &crate::encode::tj("Five")),
        (72_000, 627_754)
    );

    let with_a_gap = render(&[
        Block::Title("Title".to_string()),
        Block::Heading(1, Vec::new()),
        Block::Alias("Ada".to_string()),
        prose("Hello"),
    ]);
    assert_eq!(
        position_of(&with_a_gap, &crate::encode::tj("Hello")),
        (72_000, 695_754)
    );

    let after = render(&[prose("Hello"), heading(1, "Next")]);
    assert_eq!(
        position_of(&after, &crate::encode::tj("Next")),
        (72_000, 719_330)
    );
}

#[test]
fn a_heading_keeps_its_runs_and_moves_to_the_next_page() {
    let pdf = render(&[Block::Heading(
        1,
        vec![
            Run {
                code: true,
                ..Run::plain("yes")
            },
            Run::plain(" next"),
            Run {
                italic: true,
                ..Run::plain(" lean")
            },
        ],
    )]);
    assert!(contains(&pdf, b"/F3 20 Tf"));
    assert!(contains(&pdf, b"+DejaVuSans-BoldOblique"));
    assert!(contains(&pdf, b"/F6 20 Tf"));
    assert!(!contains(&pdf, b"/F5 20 Tf"));
    assert_eq!(
        position_of(&pdf, &crate::encode::tj(" next")).0,
        72_000 + measure("yes", Face::Courier, 20.0)
    );
    assert_eq!(position_of(&pdf, &crate::encode::tj(" next")).1, 751_330);
    assert_eq!(
        position_of(&pdf, &crate::encode::tj(" lean")).0,
        position_of(&pdf, &crate::encode::tj(" next")).0
            + measure(" next", Face::HelveticaBold, 20.0)
    );
    let emphasized = render(&read("<h1>A <em>word</em></h1>"));
    assert!(contains(&emphasized, b"/F2 20 Tf"));
    assert!(contains(&emphasized, b"/F6 20 Tf"));
    assert!(!contains(&emphasized, b"/F5 "));
    assert_eq!(
        position_of(&emphasized, &crate::encode::tj("word")).0,
        72_000 + measure("A ", Face::HelveticaBold, 20.0)
    );

    let mut words = vec!["beta".to_string()];
    let link_text = loop {
        let text = words.join(" ");
        let count = wrap(&text, Face::HelveticaBold, 20.0, content_width()).len();
        if count == 2 {
            break text;
        }
        assert!(count < 2, "wrap jumped to {count}");
        words.push("beta".to_string());
    };
    let wrapped = wrap(&link_text, Face::HelveticaBold, 20.0, content_width());
    let linked = render(&[Block::Heading(
        1,
        vec![Run {
            url: Some("https://example.com/h".to_string()),
            ..Run::plain(link_text)
        }],
    )]);
    let page = page_objects(&linked)[0];
    let rect = rects(page);
    assert_eq!(rect.len(), 2);
    assert_eq!(
        rect[0],
        (
            72_000,
            746_610,
            72_000 + measure(&wrapped[0], Face::HelveticaBold, 20.0),
            769_890
        )
    );
    assert_eq!(rect[1].1, 720_610);
    assert_eq!(rect[1].3 - rect[1].1, rect[0].3 - rect[0].1);
    assert!(contains(page, b"/URI (https://example.com/h)"));

    let broken = render(&[prose(&"alpha\n".repeat(43)), heading(1, "Later")]);
    let streams = page_streams(&broken);
    assert_eq!(streams.len(), 2);
    assert!(!contains(streams[0], &crate::encode::tj("Later")));
    assert!(contains(streams[1], &crate::encode::tj("Later")));
    assert_eq!(
        position_of(&broken, &crate::encode::tj("Later")),
        (72_000, 751_330)
    );
}

#[test]
fn a_list_draws_a_marker_beside_each_item() {
    let bullet_x = 72_000 + measure("•", Face::Helvetica, 12.0) + 6_000;
    let pdf = render(&[Block::List(vec![
        item(Marker::Bullet, "one"),
        item(Marker::Bullet, "two"),
    ])]);
    assert_eq!(
        position_of(&pdf, &crate::encode::tj("one")),
        (bullet_x, 758_754)
    );
    assert_eq!(
        position_of(&pdf, &crate::encode::tj("two")),
        (bullet_x, 738_754)
    );
    let bullet = crate::encode::tj("•");
    assert_eq!(position_of(&pdf, &bullet), (72_000, 758_754));
    assert_eq!(
        pdf.windows(bullet.len())
            .filter(|window| *window == bullet.as_slice())
            .count(),
        2
    );

    let number_x = 72_000 + measure("1.", Face::Helvetica, 12.0) + 6_000;
    let numbered = render(&[Block::List(vec![
        item(Marker::Number(1), "one"),
        item(Marker::Number(2), "two"),
    ])]);
    assert_eq!(
        position_of(&numbered, &crate::encode::tj("1.")),
        (72_000, 758_754)
    );
    assert_eq!(
        position_of(&numbered, &crate::encode::tj("2.")),
        (72_000, 738_754)
    );
    assert_eq!(
        position_of(&numbered, &crate::encode::tj("one")).0,
        number_x
    );
    assert_eq!(
        position_of(&numbered, &crate::encode::tj("two")).0,
        number_x
    );

    let wide = render(&[Block::List(vec![
        item(Marker::Number(1), "one"),
        item(Marker::Number(10), "ten"),
    ])]);
    let wide_x = 72_000 + measure("10.", Face::Helvetica, 12.0) + 6_000;
    assert_eq!(position_of(&wide, &crate::encode::tj("10.")).0, 72_000);
    assert_eq!(position_of(&wide, &crate::encode::tj("one")).0, wide_x);
    assert_eq!(position_of(&wide, &crate::encode::tj("ten")).0, wide_x);

    let nested = render(&[Block::List(vec![
        Item {
            marker: Marker::Bullet,
            runs: vec![Run::plain("one".to_string())],
            children: vec![item(Marker::Bullet, "child")],
        },
        item(Marker::Bullet, "two"),
    ])]);
    assert_eq!(
        position_of(&nested, &crate::encode::tj("child")),
        (bullet_x + 18_000, 742_754)
    );
    assert_eq!(
        position_of(&nested, &crate::encode::tj("two")),
        (bullet_x, 722_754)
    );

    let deep = render(&[deep_list(32, "kept")]);
    let kept_x = position_of(&deep, &crate::encode::tj("kept")).0;
    let kept_width = measure("kept", Face::Helvetica, 12.0);
    let right = 595_280 - 72_000;
    assert!(kept_x >= 72_000);
    assert!(kept_x + kept_width <= right);
    let unclamped = 72_000 + 31 * 18_000 + measure("•", Face::Helvetica, 12.0) + 6_000;
    assert!(kept_x < unclamped);

    let alone = render(&[Block::List(Vec::new()), prose("Hello")]);
    assert_eq!(
        position_of(&alone, &crate::encode::tj("Hello")),
        (72_000, 758_754)
    );
    let after = render(&[
        prose("Hello"),
        Block::List(vec![item(Marker::Bullet, "one")]),
    ]);
    assert_eq!(position_of(&after, &crate::encode::tj("one")).1, 730_754);
}

#[test]
fn a_task_checkbox_is_a_marker_and_a_wrapped_item_keeps_its_text_column() {
    let text_x = 72_000 + 7_000 + 6_000;
    let pdf = render(&[Block::List(vec![
        item(Marker::Task(false), "open"),
        item(Marker::Task(true), "done"),
    ])]);
    assert_eq!(
        position_of(&pdf, &crate::encode::tj("open")),
        (text_x, 758_754)
    );
    assert_eq!(
        position_of(&pdf, &crate::encode::tj("done")),
        (text_x, 738_754)
    );
    assert!(contains(
        &pdf,
        b"72 759.754 m\n79 759.754 l\n79 766.754 l\n72 766.754 l\nh\nS\n"
    ));
    assert!(contains(
        &pdf,
        b"73.4 742.554 m\n74.8 741.154 l\n77.6 744.654 l\nS\n"
    ));
    assert!(!contains(&pdf, b"/Subtype /Widget"));

    let bullet_x = 72_000 + measure("\u{2022}", Face::Helvetica, 12.0) + 6_000;
    let width = content_width() - (bullet_x - 72_000) as f32 / 1000.0;
    let mut words = vec!["beta".to_string()];
    let text = loop {
        let text = words.join(" ");
        let count = wrap(&text, Face::Helvetica, 12.0, width).len();
        if count == 2 {
            break text;
        }
        assert!(count < 2, "wrap jumped to {count}");
        words.push("beta".to_string());
    };
    let wrapped = wrap(&text, Face::Helvetica, 12.0, width);
    let lines = render(&[Block::List(vec![item(Marker::Bullet, &text)])]);
    assert_eq!(
        position_of(&lines, &crate::encode::tj(&wrapped[0])).0,
        bullet_x
    );
    assert_eq!(
        position_of(&lines, &crate::encode::tj(&wrapped[1])),
        (bullet_x, 742_754)
    );
    let bullet = crate::encode::tj("•");
    assert_eq!(
        lines
            .windows(bullet.len())
            .filter(|window| *window == bullet.as_slice())
            .count(),
        1
    );

    let marked = render(&[Block::List(vec![Item {
        marker: Marker::Bullet,
        runs: vec![
            Run {
                code: true,
                ..Run::plain("yes")
            },
            Run::plain(" next"),
            Run {
                url: Some("https://example.com/a(b)".to_string()),
                ..Run::plain("go")
            },
        ],
        children: Vec::new(),
    }])]);
    let link_x =
        bullet_x + measure("yes", Face::Courier, 12.0) + measure(" next", Face::Helvetica, 12.0);
    assert_eq!(
        position_of(&marked, &crate::encode::tj(" next")).0,
        bullet_x + measure("yes", Face::Courier, 12.0)
    );
    let page = page_objects(&marked)[0];
    assert_eq!(
        rects(page)[0],
        (
            link_x,
            755_922,
            link_x + measure("go", Face::Helvetica, 12.0),
            769_890
        )
    );
    assert!(contains(page, br"/URI (https://example.com/a\(b\))"));

    let mut items = (0..35)
        .map(|index| item(Marker::Bullet, &format!("n{index}")))
        .collect::<Vec<_>>();
    items.push(item(Marker::Bullet, "tail"));
    let broken = render(&[Block::List(items)]);
    let streams = page_streams(&broken);
    assert_eq!(streams.len(), 2);
    assert!(!contains(streams[0], &crate::encode::tj("tail")));
    assert!(contains(streams[1], &crate::encode::tj("tail")));
    assert_eq!(
        position_of(&broken, &crate::encode::tj("tail")),
        (bullet_x, 758_754)
    );
    assert!(contains(streams[1], &crate::encode::tj("•")));
}

#[test]
fn a_quote_keeps_a_bar_beside_its_text() {
    let pdf = render(&[Block::Quote(
        1,
        vec![Run::plain("Hello\nthere".to_string())],
    )]);
    assert_eq!(
        position_of(&pdf, &crate::encode::tj("Hello")),
        (84_000, 758_754)
    );
    assert_eq!(
        position_of(&pdf, &crate::encode::tj("there")),
        (84_000, 742_754)
    );
    assert!(contains(&pdf, b"72 739.922 1.2 29.968 re\nf\n"));

    let nested = render(&[Block::Quote(2, vec![Run::plain("Hello".to_string())])]);
    assert_eq!(position_of(&nested, &crate::encode::tj("Hello")).0, 96_000);
    assert!(contains(&nested, b"72 755.922 1.2 13.968 re\nf\n"));
    assert!(contains(&nested, b"84 755.922 1.2 13.968 re\nf\n"));

    let marked = render(&[Block::Quote(
        1,
        vec![
            Run {
                code: true,
                ..Run::plain("yes")
            },
            Run::plain(" next"),
            Run {
                url: Some("https://example.com/a(b)".to_string()),
                ..Run::plain("go")
            },
        ],
    )]);
    let link_x =
        84_000 + measure("yes", Face::Courier, 12.0) + measure(" next", Face::Helvetica, 12.0);
    assert_eq!(
        position_of(&marked, &crate::encode::tj(" next")).0,
        84_000 + measure("yes", Face::Courier, 12.0)
    );
    assert_eq!(
        rects(page_objects(&marked)[0])[0],
        (
            link_x,
            755_922,
            link_x + measure("go", Face::Helvetica, 12.0),
            769_890
        )
    );

    let width = content_width() - 24.0;
    let mut words = vec!["beta".to_string()];
    let text = loop {
        let text = words.join(" ");
        let count = wrap(&text, Face::Helvetica, 12.0, width).len();
        if count == 2 {
            break text;
        }
        assert!(count < 2, "wrap jumped to {count}");
        words.push("beta".to_string());
    };
    let wrapped = wrap(&text, Face::Helvetica, 12.0, width);
    let lines = render(&[Block::Quote(1, vec![Run::plain(text)])]);
    assert_eq!(
        position_of(&lines, &crate::encode::tj(&wrapped[1])),
        (84_000, 742_754)
    );
    assert_eq!(
        lines
            .windows(b" re\nf\n".len())
            .filter(|window| *window == b" re\nf\n")
            .count(),
        1
    );

    let alone = render(&[Block::Quote(1, Vec::new()), prose("Hello")]);
    assert_eq!(
        position_of(&alone, &crate::encode::tj("Hello")),
        (72_000, 758_754)
    );
    let after = render(&[
        prose("Hello"),
        Block::Quote(0, vec![Run::plain("Said".to_string())]),
    ]);
    assert_eq!(
        position_of(&after, &crate::encode::tj("Said")),
        (84_000, 730_754)
    );
}

#[test]
fn rules_span_the_content_box() {
    let thin = render(&[Block::Rule(Rule::Thin)]);
    assert!(contains(&thin, b"0.6 w\n72 769.59 m\n523.28 769.59 l\nS\n"));
    let double = render(&[Block::Rule(Rule::Double)]);
    assert!(contains(
        &double,
        b"0.6 w\n72 769.59 m\n523.28 769.59 l\nS\n"
    ));
    assert!(contains(
        &double,
        b"0.6 w\n72 768.39 m\n523.28 768.39 l\nS\n"
    ));
    let asterisk = render(&[Block::Rule(Rule::Asterisk)]);
    assert!(contains(
        &asterisk,
        b"0.6 w\n292.64 764.59 m\n302.64 764.59 l\nS\n"
    ));
    assert!(contains(&asterisk, b"296.64 763.59 2 2 re\nf\n"));
    let stars = render(&[Block::Rule(Rule::Stars)]);
    assert!(contains(
        &stars,
        b"0.6 w\n268.64 764.59 m\n278.64 764.59 l\nS\n"
    ));
    assert!(contains(
        &stars,
        b"0.6 w\n292.64 764.59 m\n302.64 764.59 l\nS\n"
    ));
    assert!(contains(
        &stars,
        b"0.6 w\n316.64 764.59 m\n326.64 764.59 l\nS\n"
    ));

    let between = render(&[prose("Hello"), Block::Rule(Rule::Thin), prose("Next")]);
    assert!(contains(&between, b"72 741.59 m\n523.28 741.59 l\nS\n"));
    assert_eq!(
        position_of(&between, &crate::encode::tj("Next")),
        (72_000, 718_154)
    );

    let broken = render(&[prose(&"alpha\n".repeat(43)), Block::Rule(Rule::Thin)]);
    let streams = page_streams(&broken);
    assert_eq!(streams.len(), 2);
    assert!(!contains(streams[0], b"72 769.59 m\n"));
    assert!(contains(streams[1], b"72 769.59 m\n523.28 769.59 l\nS\n"));
}

#[test]
fn a_code_block_keeps_spaces_and_wraps_in_courier() {
    let pdf = render(&[Block::Code("  a  b".to_string())]);
    assert!(contains(&pdf, b"+DejaVuSansMono "));
    assert!(contains(&pdf, b"/Encoding /Identity-H"));
    assert!(contains(&pdf, b"/F3 10 Tf"));
    assert!(!contains(&pdf, b"DejaVuSansMono-Bold"));
    assert_eq!(
        position_of(&pdf, &crate::encode::tj("  a  b")),
        (72_000, 760_610)
    );
    assert_eq!(
        position_of(
            &render(&[Block::Code("a\nb".to_string())]),
            &crate::encode::tj("b")
        )
        .1,
        746_610
    );
    assert_eq!(
        position_of(
            &render(&[Block::Code("a\n\nb".to_string())]),
            &crate::encode::tj("b")
        )
        .1,
        732_610
    );

    let text = "a".repeat(80);
    let lines = wrap_code(&text, Face::Courier, 10.0, content_width());
    assert!(lines.len() > 1);
    assert_eq!(lines.join(""), text);
    let wrapped = render(&[Block::Code(text)]);
    assert_eq!(
        position_of(&wrapped, &crate::encode::tj(&lines[0])),
        (72_000, 760_610)
    );
    assert_eq!(
        position_of(&wrapped, &crate::encode::tj(&lines[1])),
        (72_000, 746_610)
    );

    let slots = mono_slots();
    let spaced = "a".repeat(slots - 1) + " beta";
    let parts = wrap_code(&spaced, Face::Courier, 10.0, content_width());
    assert_eq!(parts.join(""), spaced);
    assert!(parts[0].ends_with(' '));
    let broken = render(&[Block::Code(spaced)]);
    assert!(contains(&broken, &crate::encode::tj(&parts[0])));
    assert_eq!(
        position_of(&broken, &crate::encode::tj(&parts[1])).1,
        746_610
    );

    let alone = render(&[Block::Code(String::new()), prose("Hello")]);
    assert_eq!(
        position_of(&alone, &crate::encode::tj("Hello")),
        (72_000, 758_754)
    );
    let after = render(&[prose("Hello"), Block::Code("a".to_string())]);
    assert_eq!(
        position_of(&after, &crate::encode::tj("a")),
        (72_000, 732_610)
    );
    assert!(contains(
        &render(&[Block::Code("a你b".to_string())]),
        &crate::encode::tj("a?b")
    ));

    let boundary = format!("{} beta", "a".repeat(slots));
    let boundary_lines = wrap_code(&boundary, Face::Courier, 10.0, content_width());
    assert_eq!(
        boundary_lines,
        vec![format!("{} ", "a".repeat(slots)), "beta".to_string()]
    );
    let boundary_pdf = render(&[Block::Code(boundary)]);
    assert_eq!(
        position_of(&boundary_pdf, &crate::encode::tj("beta")),
        (72_000, 746_610)
    );

    let glued = format!("{} {}", "a".repeat(slots), "b".repeat(slots + 5));
    let glued_lines = wrap_code(&glued, Face::Courier, 10.0, content_width());
    assert_eq!(glued_lines[0], format!("{} ", "a".repeat(slots)));
    assert_eq!(glued_lines[1], "b".repeat(slots));
    let glued_pdf = render(&[Block::Code(glued)]);
    assert!(!contains(&glued_pdf, &crate::encode::tj(" ")));
    assert_eq!(
        position_of(&glued_pdf, &crate::encode::tj(&"b".repeat(slots))),
        (72_000, 746_610)
    );

    let indented = format!("\t{}", "a".repeat(slots + 5));
    let indented_lines = wrap_code(&indented, Face::Courier, 10.0, content_width());
    assert_eq!(indented_lines[0], format!("    {}", "a".repeat(slots - 4)));
    assert_eq!(indented_lines[1], "a".repeat(9));
    let indented_pdf = render(&[Block::Code(indented)]);
    assert!(!contains(&indented_pdf, &crate::encode::tj("    ")));
    assert_eq!(
        position_of(&indented_pdf, &crate::encode::tj("aaaaaaaaa")),
        (72_000, 746_610)
    );

    let tabs = render(&[Block::Code("\t\treturn".to_string())]);
    assert!(contains(&tabs, &crate::encode::tj("        return")));
    assert!(!contains(page_streams(&tabs)[0], b"?"));

    let page = render(&[Block::Code(format!("{}tail", "a\n".repeat(50)))]);
    let streams = page_streams(&page);
    assert_eq!(streams.len(), 2);
    assert!(!contains(streams[0], &crate::encode::tj("tail")));
    assert_eq!(
        position_of(&page, &crate::encode::tj("tail")),
        (72_000, 760_610)
    );
}

#[test]
fn latin_greek_and_cyrillic_round_trip_and_chinese_becomes_a_question_mark() {
    let pdf = render(&[prose("café λόγος я")]);
    assert!(contains(&pdf, &crate::encode::tj("café λόγος я")));
    assert!(contains(&pdf, b"/Encoding /Identity-H"));
    assert!(contains(&pdf, b"/Subtype /Type0"));
    assert!(contains(&pdf, b"/FontFile2"));
    assert!(contains(&pdf, b"<00E9>"));
    assert!(contains(&pdf, b"<03BB>"));
    assert!(contains(&pdf, b"<044F>"));

    let missing = render(&[prose("你")]);
    assert!(contains(&missing, &crate::encode::tj("?")));
    assert!(!contains(page_streams(&missing)[0], &[0x4F, 0x60]));

    let decomposed = render(&[prose("cafe\u{0301} λόγος я")]);
    assert!(contains(&decomposed, &crate::encode::tj("café λόγος я")));
    assert_eq!(
        advance("cafe\u{0301}", Face::Helvetica),
        advance("café", Face::Helvetica)
    );

    let arrow = render(&[prose("back \u{21A9}")]);
    assert!(contains(&arrow, &crate::encode::tj("back \u{21A9}")));
    assert!(contains(&arrow, b"<21A9>"));
}

#[test]
fn a_non_ascii_web_link_is_percent_encoded() {
    let pdf = render(&[Block::Paragraph(vec![Run {
        url: Some("https://example.com/é".to_string()),
        ..Run::plain("é")
    }])]);
    assert!(contains(&pdf, b"/URI (https://example.com/%C3%A9)"));
    assert!(contains(&pdf, &crate::encode::tj("é")));
    assert!(!contains(&pdf, b"/URI (https://example.com/\xC3\xA9)"));
}
