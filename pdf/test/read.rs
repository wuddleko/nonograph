use super::*;

fn render(blocks: &[Block]) -> Vec<u8> {
    super::draw(blocks).expect("subset font")
}

use crate::place::measure;
use crate::write::format_thousandths;

fn plain(block: &Block) -> (Option<u8>, String) {
    match block {
        Block::Paragraph(runs) => (None, join(runs)),
        Block::Heading(level, runs) => (Some(*level), join(runs)),
        _ => panic!("unexpected block"),
    }
}

fn join(runs: &[Run]) -> String {
    let mut out = String::new();
    for run in runs {
        out.push_str(words(run));
    }
    out
}

fn words(run: &Run) -> &str {
    &run.text
}

fn show(run: &Run) -> String {
    let mut name = Vec::new();
    if run.bold {
        name.push("bold");
    }
    if run.italic {
        name.push("italic");
    }
    if run.underline {
        name.push("under");
    }
    if run.strike {
        name.push("strike");
    }
    if run.code {
        name.push("code");
    }
    if run.url.is_some() {
        name.push("link");
    }
    let name = if name.is_empty() {
        "text".to_string()
    } else {
        name.join("+")
    };
    match &run.url {
        Some(url) => format!("{name}:{}|{url}", run.text),
        None => format!("{name}:{}", run.text),
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[test]
fn paragraphs_headings_and_line_breaks_become_blocks() {
    let html = concat!(
        "<h1 id=\"h1\">Title<a href=\"#h1\" class=\"header-anchor\" rel=\"noopener noreferrer\">#</a></h1>\n",
        "<p>One<br>Two</p>\n",
        "<h2 id=\"h2\">Next<a href=\"#h2\" class=\"header-anchor\" rel=\"noopener noreferrer\">#</a></h2>\n",
        "<h3 id=\"h3\">Third<a class=\"header-anchor\" href=\"#h3\">#</a></h3>",
        "<h4 id=\"h4\">Fourth<a href=\"#h4\" class=\"header-anchor\">#</a></h4>",
        "<p>See <em>this</em></p>",
        "<p>a &amp; b &lt;c&gt; &#x27;ok&#39;</p>",
        "<p><br/>gap<br />again</p>",
    );
    let blocks = read(html);
    let text: Vec<_> = blocks.iter().map(plain).collect();
    let expected = [
        (Some(1), "Title"),
        (None, "One\nTwo"),
        (Some(2), "Next"),
        (Some(3), "Third"),
        (Some(4), "Fourth"),
        (None, "See this"),
        (None, "a & b <c> 'ok'"),
        (None, "\ngap\nagain"),
    ];
    assert_eq!(text.len(), expected.len());
    for (got, want) in text.iter().zip(expected) {
        assert_eq!(got.0, want.0);
        assert_eq!(got.1, want.1);
    }
    match &blocks[5] {
        Block::Paragraph(runs) => {
            assert_eq!(
                runs.iter().map(show).collect::<Vec<_>>(),
                vec!["text:See ".to_string(), "italic:this".to_string()]
            );
        }
        _ => panic!("expected paragraph"),
    }

    let drawn = render(&read(
        "<h1 id=\"h1\">Title<a href=\"#h1\" class=\"header-anchor\" rel=\"noopener noreferrer\">#</a></h1><p>Hello<br>there</p>",
    ));
    assert!(contains(&drawn, &crate::encode::tj("Title")));
    assert!(contains(&drawn, &crate::encode::tj("Hello")));
    assert!(contains(&drawn, &crate::encode::tj("there")));
    assert!(!contains(&drawn, &crate::encode::tj("#")));
    assert!(contains(&drawn, b"1 0 0 1 72 751.33 Tm"));
    assert!(contains(&drawn, b"1 0 0 1 72 720.754 Tm"));
    assert!(contains(&drawn, b"1 0 0 1 72 704.754 Tm"));

    let alone = render(&read("<p>Hello</p>"));
    assert!(contains(&alone, b"1 0 0 1 72 758.754 Tm"));
    assert!(read("<h1 id=\"h1\"><a class=\"header-anchor\" href=\"#h1\">#</a></h1>").is_empty());
}

#[test]
fn inline_tags_and_links_become_runs() {
    let html = concat!(
        "<p>See <strong>bold</strong> <em>lean</em> <u>under</u> <del>gone</del> <code>a &lt;b&gt;</code> ",
        "<a href=\"https://example.com/a(b)\" target=\"_blank\" rel=\"noopener noreferrer\">go</a></p>",
        "<p>x<sup>2</sup> <mark>sun</mark> <span class=\"secret\">hide</span></p>",
        "<p><strong>a<sup>2</sup><span class=\"secret\">z</span></strong> ",
        "<a href=\"https://example.com?a=1&amp;b=2\"><em>t</em></a></p>",
        "<p><strong>see <a href=\"https://example.com\">this</a> now</strong></p>",
        "<p><strong><em>both</em></strong></p>",
        "<p><strong><u>both</u></strong></p>",
        "<p><u><del>both</del></u></p>",
        "<p><em><u>both</u></em></p>",
        "<p><strong><code>both</code></strong></p>",
        "<p><u><code>both</code></u></p>",
        "<p><em><code>both</code></em></p>",
        "<p><strong><em><u>all</u></em></strong></p>",
        "<p><del><strong>gone bold</strong></del></p>",
        "<p><strong>bold <em>and</em> bold</strong></p>",
        "<p><strong><a href=\"https://example.com\"><em>t</em></a></strong></p>",
        "<p><span><a href=\"https://example.com/span\">go</a></span></p>",
        "<p><mark><strong>bold</strong></mark></p>",
        "<p><span class=\"secret\"><code>hide</code></span></p>",
        "<p><sup><strong>up</strong></sup></p>",
        "<p><mark><code>code</code></mark></p>",
        "<p><mark><a href=\"https://example.com\">go</a></mark></p>",
        "<p><span class=\"secret\">secret <a href=\"https://example.com/hidden\">go</a> still</span></p>",
        "<p>word<sup><a href=\"#FN1\" id=\"fnref1\" rel=\"noopener noreferrer\">1</a></sup></p>",
        "<p><em><mark>hi</mark></em></p>",
        "<p><strong>a<br>b</strong></p>",
        "<h1 id=\"h1\">A <strong>bold</strong> title<a href=\"#h1\" class=\"header-anchor\" rel=\"noopener noreferrer\">#</a></h1>",
    );
    let blocks = read(html);
    let shown: Vec<Vec<_>> = blocks
        .iter()
        .map(|block| match block {
            Block::Paragraph(runs) | Block::Heading(_, runs) => runs.iter().map(show).collect(),
            _ => panic!("unexpected block"),
        })
        .collect();
    assert_eq!(
        shown,
        vec![
            vec![
                "text:See ",
                "bold:bold",
                "text: ",
                "italic:lean",
                "text: ",
                "under:under",
                "text: ",
                "strike:gone",
                "text: ",
                "code:a <b>",
                "text: ",
                "link:go|https://example.com/a(b)",
            ],
            vec!["text:x2 sun hide"],
            vec![
                "bold:a2z",
                "text: ",
                "italic+link:t|https://example.com?a=1&b=2",
            ],
            vec![
                "bold:see ",
                "bold+link:this|https://example.com",
                "bold: now",
            ],
            vec!["bold+italic:both"],
            vec!["bold+under:both"],
            vec!["under+strike:both"],
            vec!["italic+under:both"],
            vec!["bold+code:both"],
            vec!["under+code:both"],
            vec!["italic+code:both"],
            vec!["bold+italic+under:all"],
            vec!["bold+strike:gone bold"],
            vec!["bold:bold ", "bold+italic:and", "bold: bold"],
            vec!["bold+italic+link:t|https://example.com"],
            vec!["link:go|https://example.com/span"],
            vec!["bold:bold"],
            vec!["code:hide"],
            vec!["bold:up"],
            vec!["code:code"],
            vec!["link:go|https://example.com"],
            vec![
                "text:secret ",
                "link:go|https://example.com/hidden",
                "text: still",
            ],
            vec!["text:word", "link:1|#FN1"],
            vec!["italic:hi"],
            vec!["bold:a\nb"],
            vec!["text:A ", "bold:bold", "text: title"],
        ]
        .into_iter()
        .map(|runs| runs.into_iter().map(str::to_string).collect::<Vec<_>>())
        .collect::<Vec<_>>()
    );
    assert!(read("<p><strong></strong><em></em><mark></mark><sup></sup></p>").is_empty());

    let pdf = render(&read(concat!(
        "<p>See <strong>bold</strong> <em>lean</em> <u>under</u> <del>gone</del> <code>yes</code> ",
        "<a href=\"https://example.com/a(b)\" target=\"_blank\">go</a></p>",
    )));
    assert!(contains(&pdf, b"/F2 12 Tf"));
    assert!(contains(&pdf, b"/F5 12 Tf"));
    assert!(contains(&pdf, b"/F3 12 Tf"));
    assert!(contains(&pdf, br"/URI (https://example.com/a\(b\))"));
    let mut x = 72_000;
    assert_eq!(position_of(&pdf, &crate::encode::tj("See ")), (x, 758_754));
    x += measure("See ", Face::Helvetica, 12.0);
    assert_eq!(position_of(&pdf, &crate::encode::tj("bold")), (x, 758_754));
    x += measure("bold", Face::HelveticaBold, 12.0);
    x += measure(" ", Face::Helvetica, 12.0);
    assert_eq!(position_of(&pdf, &crate::encode::tj("lean")), (x, 758_754));
    x += measure("lean", Face::HelveticaOblique, 12.0);
    x += measure(" ", Face::Helvetica, 12.0);
    let under = x;
    assert_eq!(
        position_of(&pdf, &crate::encode::tj("under")),
        (under, 758_754)
    );
    x += measure("under", Face::Helvetica, 12.0);
    x += measure(" ", Face::Helvetica, 12.0);
    let gone = x;
    assert_eq!(
        position_of(&pdf, &crate::encode::tj("gone")),
        (gone, 758_754)
    );
    x += measure("gone", Face::Helvetica, 12.0);
    x += measure(" ", Face::Helvetica, 12.0);
    assert_eq!(position_of(&pdf, &crate::encode::tj("yes")), (x, 758_754));
    x += measure("yes", Face::Courier, 12.0);
    x += measure(" ", Face::Helvetica, 12.0);
    let go = x;
    let go_width = measure("go", Face::Helvetica, 12.0);
    assert_eq!(position_of(&pdf, &crate::encode::tj("go")), (go, 758_754));
    assert_eq!(rects(&pdf), vec![(go, 755_922, go + go_width, 769_890)]);
    assert!(contains(
        &pdf,
        format!(
            "{} 757.998 m\n{} 757.998 l\n",
            format_thousandths(under),
            format_thousandths(under + measure("under", Face::Helvetica, 12.0))
        )
        .as_bytes()
    ));
    assert!(contains(
        &pdf,
        format!(
            "{} 761.862 m\n{} 761.862 l\n",
            format_thousandths(gone),
            format_thousandths(gone + measure("gone", Face::Helvetica, 12.0))
        )
        .as_bytes()
    ));

    let broken = render(&read("<p><strong>a<br>b</strong></p>"));
    assert_eq!(
        position_of(&broken, &crate::encode::tj("a")),
        (72_000, 758_754)
    );
    assert_eq!(
        position_of(&broken, &crate::encode::tj("b")),
        (72_000, 742_754)
    );
    let at = find(&broken, &crate::encode::tj("b")).unwrap();
    assert!(contains(&broken[at - 40..at], b"/F2 12 Tf"));

    let plain = render(&read(
        "<p>x<sup>2</sup> <mark>sun</mark> <span class=\"secret\">hide</span></p>",
    ));
    assert_eq!(
        position_of(&plain, &crate::encode::tj("x2 sun hide")),
        (72_000, 758_754)
    );
    assert!(!contains(&plain, b"/URI"));

    let linked = render(&read("<p>word<sup><a href=\"#FN1\">1</a></sup></p>"));
    assert!(contains(&linked, &crate::encode::tj("word")));
    assert!(contains(&linked, &crate::encode::tj("1")));
    assert!(!contains(&linked, b"/URI"));
    assert!(rects(&linked).is_empty());
    let one = 72_000 + measure("word", Face::Helvetica, 12.0);
    assert_eq!(
        position_of(&linked, &crate::encode::tj("word")),
        (72_000, 758_754)
    );
    assert_eq!(
        position_of(&linked, &crate::encode::tj("1")),
        (one, 758_754)
    );

    let marked = render(&read("<p><mark><strong>bold</strong></mark></p>"));
    assert!(contains(&marked, b"/F2 12 Tf"));
    assert_eq!(
        position_of(&marked, &crate::encode::tj("bold")),
        (72_000, 758_754)
    );
}

#[test]
fn nested_styles_stay_on_the_same_words() {
    let both = render(&read("<p><strong><em>both</em></strong> next</p>"));
    assert!(contains(&both, b"+DejaVuSans-BoldOblique"));
    assert!(contains(&both, b"/F6 12 Tf"));
    assert_eq!(
        position_of(&both, &crate::encode::tj("both")),
        (72_000, 758_754)
    );
    assert_eq!(
        position_of(&both, &crate::encode::tj(" next")).0,
        72_000 + measure("both", Face::HelveticaBoldOblique, 12.0)
    );

    let under = render(&read("<p><strong><u>both</u></strong></p>"));
    let under_width = measure("both", Face::HelveticaBold, 12.0);
    assert!(contains(&under, b"/F2 12 Tf"));
    assert_eq!(
        position_of(&under, &crate::encode::tj("both")),
        (72_000, 758_754)
    );
    assert!(contains(
        &under,
        format!(
            "72 757.998 m\n{} 757.998 l\n",
            format_thousandths(72_000 + under_width)
        )
        .as_bytes()
    ));

    let struck = render(&read("<p><u><del>both</del></u></p>"));
    let struck_width = measure("both", Face::Helvetica, 12.0);
    assert!(contains(&struck, b"/F1 12 Tf"));
    assert!(contains(
        &struck,
        format!(
            "72 757.998 m\n{} 757.998 l\n",
            format_thousandths(72_000 + struck_width)
        )
        .as_bytes()
    ));
    assert!(contains(
        &struck,
        format!(
            "72 761.862 m\n{} 761.862 l\n",
            format_thousandths(72_000 + struck_width)
        )
        .as_bytes()
    ));

    let code = render(&read("<p><strong><code>both</code></strong></p>"));
    assert!(contains(&code, b"+DejaVuSansMono-Bold"));
    assert!(contains(&code, b"/F4 12 Tf"));
    assert_eq!(
        position_of(&code, &crate::encode::tj("both")),
        (72_000, 758_754)
    );

    let lean = render(&read("<p><em><code>both</code></em></p>"));
    assert!(contains(&lean, b"+DejaVuSansMono-Oblique"));
    assert!(contains(&lean, b"/F7 12 Tf"));

    let linked = render(&read(
        "<p><strong>see <a href=\"https://example.com\">this</a> now</strong></p>",
    ));
    let see = measure("see ", Face::HelveticaBold, 12.0);
    let this = measure("this", Face::HelveticaBold, 12.0);
    assert_eq!(
        position_of(&linked, &crate::encode::tj("see ")),
        (72_000, 758_754)
    );
    assert_eq!(
        position_of(&linked, &crate::encode::tj("this")),
        (72_000 + see, 758_754)
    );
    assert_eq!(
        position_of(&linked, &crate::encode::tj(" now")).0,
        72_000 + see + this
    );
    assert_eq!(
        rects(&linked),
        vec![(72_000 + see, 755_922, 72_000 + see + this, 769_890)]
    );
    assert!(contains(&linked, b"/URI (https://example.com)"));
    let at = find(&linked, &crate::encode::tj("this")).unwrap();
    assert!(contains(&linked[at.saturating_sub(80)..at], b"/F2 12 Tf"));
}

fn show_item(item: &Item) -> String {
    let marker = match item.marker {
        Marker::Bullet => "bullet".to_string(),
        Marker::Number(number) => format!("n{number}"),
        Marker::Task(true) => "checked".to_string(),
        Marker::Task(false) => "open".to_string(),
    };
    let runs = item.runs.iter().map(show).collect::<Vec<_>>().join(",");
    let children = item
        .children
        .iter()
        .map(show_item)
        .collect::<Vec<_>>()
        .join(";");
    format!("{marker}[{runs}]{{{children}}}")
}

fn one_list(html: &str) -> Vec<String> {
    match read(html).as_slice() {
        [Block::List(items)] => items.iter().map(show_item).collect(),
        _ => panic!("expected one list"),
    }
}

fn assert_list(html: &str, expected: &[&str]) {
    let got = one_list(html);
    assert_eq!(got.len(), expected.len());
    for (got, want) in got.iter().zip(expected) {
        assert_eq!(got, want);
    }
}

#[test]
fn lists_keep_their_markers_and_nested_items() {
    assert_list(
        "<ul><li>one</li><li>two</li></ul>",
        &["bullet[text:one]{}", "bullet[text:two]{}"],
    );
    assert_list(
        "<ol><li>one</li><li>two</li></ol>",
        &["n1[text:one]{}", "n2[text:two]{}"],
    );
    assert_list(
        "<ul><li>a<ul><li>b<ul><li>c</li></ul></li><li>d</li></ul></li><li>e</li></ul>",
        &[
            "bullet[text:a]{bullet[text:b]{bullet[text:c]{}};bullet[text:d]{}}",
            "bullet[text:e]{}",
        ],
    );
    assert_list(
        concat!(
            "<ul class=\"contains-task-list\">",
            "<li class=\"task-list-item\"><input type=\"checkbox\"> open</li>",
            "<li class=\"task-list-item\"><input type=\"checkbox\" checked=\"\"> done</li>",
            "<li>plain</li>",
            "</ul>",
        ),
        &[
            "open[text:open]{}",
            "checked[text:done]{}",
            "bullet[text:plain]{}",
        ],
    );
    assert_list(
        concat!(
            "<ul class=\"contains-task-list\">",
            "<li class=\"task-list-item\"><input type=\"checkbox\"> Parent",
            "<ul class=\"contains-task-list\">",
            "<li class=\"task-list-item\"><input type=\"checkbox\" checked=\"\"> Child</li>",
            "</ul></li></ul>",
        ),
        &["open[text:Parent]{checked[text:Child]{}}"],
    );
    assert_list(
        concat!(
            "<ul><li>See <em>this</em> and ",
            "<a href=\"https://example.com/a(b)\" target=\"_blank\" rel=\"noopener noreferrer\">go</a>",
            "</li></ul>",
        ),
        &["bullet[text:See ,italic:this,text: and ,link:go|https://example.com/a(b)]{}"],
    );
    assert_list(
        "<ul><li><code>yes</code> next</li></ul>",
        &["bullet[code:yes,text: next]{}"],
    );
    assert_list(
        concat!(
            "<ul class=\"contains-task-list\">",
            "<li class=\"task-list-item\"><input type=\"checkbox\"> a <strong>b</strong></li>",
            "<li class=\"task-list-item\"><input type=\"checkbox\"> </li>",
            "<li class=\"task-list-item\"><input checked type=\"checkbox\"></li>",
            "<li class=\"task-list-item\"><input type=\"checkbox\" checked=\"checked\"> yes</li>",
            "</ul>",
        ),
        &[
            "open[text:a ,bold:b]{}",
            "open[]{}",
            "checked[]{}",
            "checked[text:yes]{}",
        ],
    );
    assert!(read("<ul></ul><ol></ol>").is_empty());

    let around = read("<p>Hello</p><p></p><ul><li>one</li></ul><p></p><p>There</p>");
    assert_eq!(around.len(), 3);
    match &around[1] {
        Block::List(items) => assert_eq!(
            items.iter().map(show_item).collect::<Vec<_>>(),
            ["bullet[text:one]{}"]
        ),
        _ => panic!("expected the list between the paragraphs"),
    }

    let bullet_x = 72_000 + measure("•", Face::Helvetica, 12.0) + 6_000;
    let bullets = render(&read("<p></p><ul><li>one</li><li>two</li></ul>"));
    assert_eq!(
        position_of(&bullets, &crate::encode::tj("one")),
        (bullet_x, 758_754)
    );
    assert_eq!(
        position_of(&bullets, &crate::encode::tj("two")),
        (bullet_x, 738_754)
    );
    assert_eq!(
        position_of(&bullets, &crate::encode::tj("•")),
        (72_000, 758_754)
    );

    let nested = render(&read(
        "<ul><li>one<ul><li>child</li></ul></li><li>two</li></ul>",
    ));
    assert_eq!(
        position_of(&nested, &crate::encode::tj("child")),
        (bullet_x + 18_000, 742_754)
    );
    assert_eq!(
        position_of(&nested, &crate::encode::tj("two")),
        (bullet_x, 722_754)
    );

    let number_x = 72_000 + measure("1.", Face::Helvetica, 12.0) + 6_000;
    let numbered = render(&read("<ol><li>one</li><li>two</li></ol>"));
    assert_eq!(
        position_of(&numbered, &crate::encode::tj("1.")),
        (72_000, 758_754)
    );
    assert_eq!(
        position_of(&numbered, &crate::encode::tj("one")).0,
        number_x
    );

    let mut wide_html = String::from("<ol>");
    for number in 1..=10 {
        wide_html.push_str(&format!("<li>n{number}</li>"));
    }
    wide_html.push_str("</ol>");
    let wide = render(&read(&wide_html));
    let ten = measure("10.", Face::Helvetica, 12.0);
    let text_x = 72_000 + ten + 6_000;
    assert_eq!(position_of(&wide, &crate::encode::tj("10.")).0, 72_000);
    assert_eq!(
        position_of(&wide, &crate::encode::tj("1.")).0,
        72_000 + ten - measure("1.", Face::Helvetica, 12.0)
    );
    assert_eq!(position_of(&wide, &crate::encode::tj("n1")).0, text_x);
    assert_eq!(
        position_of(&wide, &crate::encode::tj("n10")),
        (text_x, 578_754)
    );

    let tasks = render(&read(concat!(
        "<ul class=\"contains-task-list\">",
        "<li class=\"task-list-item\"><input type=\"checkbox\"> open</li>",
        "<li class=\"task-list-item\"><input type=\"checkbox\" checked=\"\"> done</li>",
        "<li>plain</li>",
        "</ul>",
    )));
    let side = 12_000 * 7 / 12;
    let marker = side.max(measure("•", Face::Helvetica, 12.0));
    let task_x = 72_000 + marker + 6_000;
    let box_x = 72_000 + marker - side;
    assert_eq!(
        position_of(&tasks, &crate::encode::tj("open")),
        (task_x, 758_754)
    );
    assert_eq!(
        position_of(&tasks, &crate::encode::tj("done")),
        (task_x, 738_754)
    );
    assert_eq!(
        position_of(&tasks, &crate::encode::tj("plain")),
        (task_x, 718_754)
    );
    let bottom = 758_754 + side / 7;
    assert!(contains(
        &tasks,
        format!(
            "{} {} m\n{} {} l\n{} {} l\n{} {} l\nh\nS\n",
            format_thousandths(box_x),
            format_thousandths(bottom),
            format_thousandths(box_x + side),
            format_thousandths(bottom),
            format_thousandths(box_x + side),
            format_thousandths(bottom + side),
            format_thousandths(box_x),
            format_thousandths(bottom + side)
        )
        .as_bytes()
    ));
    let check = 738_754 + side / 7;
    assert!(contains(
        &tasks,
        format!(
            "{} {} m\n{} {} l\n{} {} l\nS\n",
            format_thousandths(box_x + side / 5),
            format_thousandths(check + side * 2 / 5),
            format_thousandths(box_x + side * 2 / 5),
            format_thousandths(check + side / 5),
            format_thousandths(box_x + side * 4 / 5),
            format_thousandths(check + side * 7 / 10)
        )
        .as_bytes()
    ));
    assert!(!contains(&tasks, b"/Subtype /Widget"));

    let linked = render(&read(
        "<ul><li>See <a href=\"https://example.com/a(b)\">go</a></li></ul>",
    ));
    let link_x = bullet_x + measure("See ", Face::Helvetica, 12.0);
    assert_eq!(
        rects(&linked),
        vec![(
            link_x,
            755_922,
            link_x + measure("go", Face::Helvetica, 12.0),
            769_890
        )]
    );
    assert!(contains(&linked, br"/URI (https://example.com/a\(b\))"));

    let code = render(&read("<ul><li><code>yes</code> next</li></ul>"));
    assert_eq!(
        position_of(&code, &crate::encode::tj("yes")),
        (bullet_x, 758_754)
    );
    assert_eq!(
        position_of(&code, &crate::encode::tj(" next")).0,
        bullet_x + measure("yes", Face::Courier, 12.0)
    );
    assert!(contains(&code, b"/F3 12 Tf"));

    let between = render(&read(
        "<p>Hello</p><p></p><ul><li>one</li></ul><p></p><p>There</p>",
    ));
    assert_eq!(
        position_of(&between, &crate::encode::tj("one")),
        (bullet_x, 730_754)
    );
    assert_eq!(
        position_of(&between, &crate::encode::tj("There")),
        (72_000, 702_754)
    );
}

fn show_block(block: &Block) -> String {
    match block {
        Block::Paragraph(runs) => format!("p:{}", shown_runs(runs)),
        Block::Quote(depth, runs) => format!("q{depth}:{}", shown_runs(runs)),
        Block::Rule(Rule::Thin) => "thin".to_string(),
        Block::Rule(Rule::Double) => "double".to_string(),
        Block::Rule(Rule::Asterisk) => "asterisk".to_string(),
        Block::Rule(Rule::Stars) => "stars".to_string(),
        _ => panic!("unexpected block"),
    }
}

fn shown_runs(runs: &[Run]) -> String {
    runs.iter().map(show).collect::<Vec<_>>().join(",")
}

fn assert_blocks(html: &str, expected: &[&str]) {
    let got: Vec<_> = read(html).iter().map(show_block).collect();
    assert_eq!(got.len(), expected.len());
    for (got, want) in got.iter().zip(expected) {
        assert_eq!(got, want);
    }
}

#[test]
fn quotes_and_rules_become_blocks() {
    assert_blocks(
        "<blockquote>Hello<br>there</blockquote>",
        &["q1:text:Hello\nthere"],
    );
    assert_blocks(
        "<blockquote>outer<br>&gt; inner</blockquote>",
        &["q1:text:outer\n> inner"],
    );
    assert_blocks(
        concat!(
            "<blockquote>See <em>this</em> and ",
            "<a href=\"https://example.com/a(b)\" target=\"_blank\" rel=\"noopener noreferrer\">go</a>",
            "</blockquote>",
        ),
        &["q1:text:See ,italic:this,text: and ,link:go|https://example.com/a(b)"],
    );
    assert_blocks(
        "<blockquote>out<blockquote>in</blockquote> tail</blockquote>",
        &["q1:text:out", "q2:text:in", "q1:text: tail"],
    );
    assert_blocks(
        "<blockquote><blockquote>in</blockquote></blockquote>",
        &["q2:text:in"],
    );
    assert!(read("<blockquote></blockquote>").is_empty());
    assert_blocks(
        "<p>Hello</p><blockquote>Said</blockquote><p>There</p>",
        &["p:text:Hello", "q1:text:Said", "p:text:There"],
    );
    assert_blocks(
        concat!(
            "<p></p><hr class=\"divider-thin\"><p></p>",
            "<hr class=\"divider-double\"/>",
            "<div class=\"divider-asterisk\"><div class=\"center\"></div></div>",
            "<div class=\"divider-stars\"><div class=\"asterisk\"><div class=\"center\"></div></div>",
            "<div class=\"asterisk\"><div class=\"center\"></div></div>",
            "<div class=\"asterisk\"><div class=\"center\"></div></div></div>",
        ),
        &["thin", "double", "asterisk", "stars"],
    );

    let quote = render(&read("<blockquote>Hello<br>there</blockquote>"));
    assert_eq!(
        position_of(&quote, &crate::encode::tj("Hello")),
        (84_000, 758_754)
    );
    assert_eq!(
        position_of(&quote, &crate::encode::tj("there")),
        (84_000, 742_754)
    );
    assert!(contains(&quote, b"72 739.922 1.2 29.968 re\nf\n"));

    let nested = render(&read(
        "<blockquote><blockquote>in</blockquote></blockquote>",
    ));
    assert_eq!(
        position_of(&nested, &crate::encode::tj("in")),
        (96_000, 758_754)
    );
    assert!(contains(&nested, b"72 755.922 1.2 13.968 re\nf\n"));
    assert!(contains(&nested, b"84 755.922 1.2 13.968 re\nf\n"));

    let linked = render(&read(
        "<blockquote>See <a href=\"https://example.com/a(b)\">go</a></blockquote>",
    ));
    let link_x = 84_000 + measure("See ", Face::Helvetica, 12.0);
    assert_eq!(
        rects(&linked),
        vec![(
            link_x,
            755_922,
            link_x + measure("go", Face::Helvetica, 12.0),
            769_890
        )]
    );
    assert!(contains(&linked, br"/URI (https://example.com/a\(b\))"));

    let code = render(&read("<blockquote><code>yes</code> next</blockquote>"));
    assert_eq!(
        position_of(&code, &crate::encode::tj("yes")),
        (84_000, 758_754)
    );
    assert!(contains(&code, b"/F3 12 Tf"));

    let around = render(&read(
        "<p>Hello</p><blockquote>Said</blockquote><p>There</p>",
    ));
    assert_eq!(
        position_of(&around, &crate::encode::tj("Said")),
        (84_000, 730_754)
    );
    assert_eq!(
        position_of(&around, &crate::encode::tj("There")),
        (72_000, 702_754)
    );

    let thin = render(&read("<p></p><hr class=\"divider-thin\"><p></p>"));
    assert!(contains(&thin, b"0.6 w\n72 769.59 m\n523.28 769.59 l\nS\n"));
    let double = render(&read("<hr class=\"divider-double\">"));
    assert!(contains(
        &double,
        b"0.6 w\n72 769.59 m\n523.28 769.59 l\nS\n"
    ));
    assert!(contains(
        &double,
        b"0.6 w\n72 768.39 m\n523.28 768.39 l\nS\n"
    ));
    let asterisk = render(&read(
        "<div class=\"divider-asterisk\"><div class=\"center\"></div></div>",
    ));
    assert!(contains(
        &asterisk,
        b"0.6 w\n292.64 764.59 m\n302.64 764.59 l\nS\n"
    ));
    assert!(contains(&asterisk, b"296.64 763.59 2 2 re\nf\n"));
    let stars = render(&read(concat!(
        "<div class=\"divider-stars\">",
        "<div class=\"asterisk\"><div class=\"center\"></div></div>",
        "<div class=\"asterisk\"><div class=\"center\"></div></div>",
        "<div class=\"asterisk\"><div class=\"center\"></div></div>",
        "</div>",
    )));
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

    let between = render(&read(
        "<p>Hello</p><p></p><hr class=\"divider-thin\"><p></p><p>Next</p>",
    ));
    assert!(contains(&between, b"72 741.59 m\n523.28 741.59 l\nS\n"));
    assert_eq!(
        position_of(&between, &crate::encode::tj("Next")),
        (72_000, 718_154)
    );
}

fn one_code(html: &str) -> String {
    match read(html).as_slice() {
        [Block::Code(text)] => text.clone(),
        _ => panic!("expected one code block"),
    }
}

#[test]
fn code_blocks_keep_the_plain_text() {
    assert_eq!(
        one_code(concat!(
            "<pre class=\"language-rust\">",
            "<div class=\"code-header\"><span class=\"code-language\">RUST</span>",
            "<div class=\"code-controls\">",
            "<button class=\"wrap-button\"><span class=\"btn-label\">Wrap</span></button>",
            "<button class=\"collapse-button\"><span class=\"btn-label\">Collapse</span></button>",
            "<button class=\"copy-button\"><span class=\"btn-label\">Copy</span></button>",
            "</div></div>",
            "<div class=\"line-numbers\"><span class=\"line-number\">1</span><span class=\"line-number\">2</span></div>",
            "<div class=\"code-wrapper\"><code><span class=\"code-line\">",
            "<span style=\"color:#c0c5ce\">fn </span><span style=\"color:#d08770\">main</span>",
            "</span></code></div></pre>",
        )),
        "fn main"
    );
    assert_eq!(
        one_code(concat!(
            "<pre><div class=\"code-wrapper\"><code>",
            "<span class=\"code-line\"><span>  a  b\n</span></span>",
            "<span class=\"code-line\"><span>\n</span></span>",
            "<span class=\"code-line\"><span>c</span></span>",
            "</code></div></pre>",
        )),
        "  a  b\n\nc"
    );
    assert_eq!(
        one_code("<pre><code><span class=\"code-line\"><span>&lt;a&gt;</span></span></code></pre>"),
        "<a>"
    );
    assert_eq!(one_code("<pre><code>\tx</code></pre>"), "\tx");
    assert!(read("<pre></pre>").is_empty());
    assert!(read(
        "<pre><div class=\"code-header\"><span class=\"code-language\">RUST</span></div><div class=\"code-wrapper\"><code></code></div></pre>"
    )
    .is_empty());

    let around = read(concat!(
        "<p>Use <code>inline</code> here.</p>",
        "<pre><code>block</code></pre>",
    ));
    match around.as_slice() {
        [Block::Paragraph(runs), Block::Code(text)] => {
            assert_eq!(
                runs.iter().map(show).collect::<Vec<_>>(),
                ["text:Use ", "code:inline", "text: here."]
            );
            assert_eq!(text, "block");
        }
        _ => panic!("expected a paragraph and a code block"),
    }

    let spaced = render(&read(
        "<pre><div class=\"code-wrapper\"><code><span class=\"code-line\"><span>  a  b</span></span></code></div></pre>",
    ));
    assert!(contains(&spaced, b"/F3 10 Tf"));
    assert!(contains(&spaced, b"+DejaVuSansMono "));
    assert!(!contains(&spaced, b"DejaVuSansMono-Bold"));
    assert!(!contains(&spaced, b"#c0c5ce"));
    assert_eq!(
        position_of(&spaced, &crate::encode::tj("  a  b")),
        (72_000, 760_610)
    );

    let lines = render(&read(concat!(
        "<pre><code>",
        "<span class=\"code-line\"><span>  a  b\n</span></span>",
        "<span class=\"code-line\"><span>\n</span></span>",
        "<span class=\"code-line\"><span>c</span></span>",
        "</code></pre>",
    )));
    assert_eq!(
        position_of(&lines, &crate::encode::tj("  a  b")),
        (72_000, 760_610)
    );
    assert_eq!(
        position_of(&lines, &crate::encode::tj("c")),
        (72_000, 732_610)
    );

    let colored = render(&read(concat!(
        "<pre><code><span style=\"color:#c0c5ce\">fn </span>",
        "<span style=\"color:#d08770;font-weight:bold\">main</span></code></pre>",
    )));
    assert_eq!(
        position_of(&colored, &crate::encode::tj("fn main")),
        (72_000, 760_610)
    );
    assert!(contains(&colored, b"/F3 10 Tf"));
    assert!(!contains(&colored, b"DejaVuSansMono-Bold"));
    assert!(!contains(&colored, b"#d08770"));

    let escaped = render(&read("<pre><code>&lt;a&gt;</code></pre>"));
    assert!(contains(&escaped, &crate::encode::tj("<a>")));
    assert!(!contains(&escaped, b"/URI"));

    let text = "a".repeat(80);
    let wrapped = wrap_code(&text, Face::Courier, 10.0, content_width());
    let long = render(&read(&format!("<pre><code>{text}</code></pre>")));
    assert_eq!(
        position_of(&long, &crate::encode::tj(&wrapped[0])),
        (72_000, 760_610)
    );
    assert_eq!(
        position_of(&long, &crate::encode::tj(&wrapped[1])),
        (72_000, 746_610)
    );

    let tab = render(&read("<pre><code>\tx</code></pre>"));
    assert_eq!(
        position_of(&tab, &crate::encode::tj("    x")),
        (72_000, 760_610)
    );

    let between = render(&read("<p>Hello</p><pre><code>a</code></pre><p>There</p>"));
    assert_eq!(
        position_of(&between, &crate::encode::tj("a")),
        (72_000, 732_610)
    );
    assert_eq!(
        position_of(&between, &crate::encode::tj("There")),
        (72_000, 704_754)
    );
}

#[test]
fn tables_images_and_videos_become_address_links() {
    let image_html = concat!(
        "<p></p><div class=\"media-with-caption\">",
        "<img src=\"https://example.com/image.jpg\" alt=\"Alt text\">",
        "<div class=\"media-caption\">Alt text</div></div><p></p>",
    );
    assert_blocks(
        image_html,
        &["p:link:https://example.com/image.jpg|https://example.com/image.jpg"],
    );
    assert_blocks(
        "<img src=\"https://example.com/test.png\" alt=\"\">",
        &["p:link:https://example.com/test.png|https://example.com/test.png"],
    );
    assert_blocks(
        "<img src=\"https://example.com/?a=1&amp;b=2\" alt=\"x\" />",
        &["p:link:https://example.com/?a=1&b=2|https://example.com/?a=1&b=2"],
    );
    assert!(read("<img src=\"\" alt=\"gone\">").is_empty());

    let video_html = concat!(
        "<p></p><div class=\"media-with-caption\"><video controls=\"\">",
        "<source src=\"https://example.com/video.mp4\" type=\"video/mp4\">",
        "Your browser does not support the video tag.</video>",
        "<div class=\"media-caption\">Video caption</div></div><p></p>",
    );
    assert_blocks(
        video_html,
        &["p:link:https://example.com/video.mp4|https://example.com/video.mp4"],
    );
    assert_blocks(
        concat!(
            "<video controls=\"\"><source src=\"https://example.com/video.webm\" type=\"video/webm\">",
            "Your browser does not support the video tag.</video>",
        ),
        &["p:link:https://example.com/video.webm|https://example.com/video.webm"],
    );
    assert!(read("<video>Your browser does not support the video tag.</video>").is_empty());

    assert_blocks(
        "<p>See <img src=\"https://example.com/a.jpg\" alt=\"pic\"> now</p>",
        &["p:text:See ,link:https://example.com/a.jpg|https://example.com/a.jpg,text: now"],
    );
    assert_blocks(
        "<blockquote>See <img src=\"https://example.com/a.jpg\"></blockquote>",
        &["q1:text:See ,link:https://example.com/a.jpg|https://example.com/a.jpg"],
    );
    assert_blocks(
        "<p><strong>See <img src=\"https://example.com/a.jpg\"></strong></p>",
        &["p:bold:See ,link:https://example.com/a.jpg|https://example.com/a.jpg"],
    );
    assert_list(
        "<ul><li>See <img src=\"https://example.com/a.jpg\"></li></ul>",
        &["bullet[text:See ,link:https://example.com/a.jpg|https://example.com/a.jpg]{}"],
    );
    let sentence_html = concat!(
        "<p>Here is an image: </p><div class=\"media-with-caption\">",
        "<img src=\"https://example.com/image.jpg\" alt=\"Alt text\">",
        "<div class=\"media-caption\">Alt text</div></div> in a line.<p></p>",
    );
    assert_blocks(
        sentence_html,
        &["p:text:Here is an image: ,link:https://example.com/image.jpg|https://example.com/image.jpg,text: in a line."],
    );
    assert_blocks(
        concat!(
            "<p>Start </p><div class=\"media-with-caption\">",
            "<img src=\"https://example.com/1.jpg\" alt=\"First\">",
            "<div class=\"media-caption\">First</div></div> and ",
            "<div class=\"media-with-caption\">",
            "<img src=\"https://example.com/2.png\" alt=\"Second\">",
            "<div class=\"media-caption\">Second</div></div> end.<p></p>",
        ),
        &["p:text:Start ,link:https://example.com/1.jpg|https://example.com/1.jpg,text: and ,link:https://example.com/2.png|https://example.com/2.png,text: end."],
    );
    assert_blocks(
        concat!(
            "<p>Here is an image: </p><div class=\"media-with-caption\">",
            "<img src=\"https://example.com/image.jpg\" alt=\"Alt text\">",
            "<div class=\"media-caption\">Alt text</div></div>",
            "<br>still the same paragraph<p></p>",
        ),
        &["p:text:Here is an image: ,link:https://example.com/image.jpg|https://example.com/image.jpg,text:\nstill the same paragraph"],
    );
    assert_blocks(
        concat!(
            "<p></p><div class=\"media-with-caption\">",
            "<img src=\"https://example.com/image.jpg\" alt=\"Alt text\">",
            "<div class=\"media-caption\">Alt text</div></div> <strong>bold</strong><p></p>",
        ),
        &["p:link:https://example.com/image.jpg|https://example.com/image.jpg,text: ,bold:bold"],
    );
    assert_blocks(
        concat!(
            "<p>Watch </p><div class=\"media-with-caption\"><video controls=\"\">",
            "<source src=\"https://example.com/video.mp4\" type=\"video/mp4\">",
            "Your browser does not support the video tag.</video>",
            "<div class=\"media-caption\">clip</div></div> please.<p></p>",
        ),
        &["p:text:Watch ,link:https://example.com/video.mp4|https://example.com/video.mp4,text: please."],
    );
    assert_blocks(
        "<img src=\"https://example.com/a.jpg\" alt=\"\"> now",
        &["p:link:https://example.com/a.jpg|https://example.com/a.jpg,text: now"],
    );
    assert_blocks(
        "<img src=\"https://example.com/a.jpg\" alt=\"\"> <strong>bold</strong>",
        &["p:link:https://example.com/a.jpg|https://example.com/a.jpg,text: ,bold:bold"],
    );
    assert_blocks(
        "<img src=\"https://example.com/a.jpg\" alt=\"\">\nnow",
        &["p:link:https://example.com/a.jpg|https://example.com/a.jpg,text:\nnow"],
    );
    assert_blocks(
        concat!(
            "<video controls=\"\"><source src=\"https://example.com/video.webm\" type=\"video/webm\">",
            "Your browser does not support the video tag.</video> now",
        ),
        &["p:link:https://example.com/video.webm|https://example.com/video.webm,text: now"],
    );
    assert_blocks(
        concat!(
            "<p></p><div class=\"media-with-caption\"><img src=\"https://example.com/a.jpg\" alt=\"Alt\">",
            "<div class=\"media-caption\">Alt</div></div><br>now<p></p>",
        ),
        &["p:link:https://example.com/a.jpg|https://example.com/a.jpg,text:\nnow"],
    );
    assert_blocks(
        concat!(
            "<p>Before</p>\n<p></p><div class=\"media-with-caption\">",
            "<img src=\"https://example.com/image.jpg\" alt=\"Alt text\">",
            "<div class=\"media-caption\">Alt text</div></div><p></p>\n<p>After</p>",
        ),
        &[
            "p:text:Before",
            "p:link:https://example.com/image.jpg|https://example.com/image.jpg",
            "p:text:After",
        ],
    );

    let table_html = concat!(
        "<table><thead><tr><th>Left</th><th>Right</th></tr></thead><tbody>",
        "<tr><td>A</td><td>B</td></tr>",
        "<tr><td>C</td><td><a href=\"https://example.com/a(b)\" target=\"_blank\" rel=\"noopener noreferrer\">go</a></td></tr>",
        "</tbody></table>",
    );
    assert_blocks(
        table_html,
        &["p:text:Left Right\nA B\nC ,link:go|https://example.com/a(b)"],
    );
    assert!(read("<table></table>").is_empty());
    assert!(read("<table><tr><td></td><td></td></tr></table>").is_empty());
    assert_blocks(
        "<table><tr><td><em>lean</em></td></tr></table>",
        &["p:italic:lean"],
    );
    assert_blocks(
        "<table><tr><td><img src=\"https://example.com/a.jpg\" alt=\"x\"></td></tr></table>",
        &["p:link:https://example.com/a.jpg|https://example.com/a.jpg"],
    );

    let around_html = "<p>Hello</p><img src=\"https://example.com/a.jpg\" alt=\"\"><p>There</p>";
    assert_blocks(
        around_html,
        &[
            "p:text:Hello",
            "p:link:https://example.com/a.jpg|https://example.com/a.jpg",
            "p:text:There",
        ],
    );

    let sentence_pdf = render(&read(sentence_html));
    assert!(contains(&sentence_pdf, &crate::encode::tj(" in a line.")));
    assert!(!contains(&sentence_pdf, &crate::encode::tj("Alt text")));
    assert_eq!(
        position_of(&sentence_pdf, &crate::encode::tj("Here is an image: ")).1,
        position_of(&sentence_pdf, &crate::encode::tj(" in a line.")).1
    );

    let image_pdf = render(&read(image_html));
    assert!(!contains(&image_pdf, &crate::encode::tj("Alt text")));
    assert!(contains(
        &image_pdf,
        &crate::encode::tj("https://example.com/image.jpg")
    ));
    assert!(contains(
        &image_pdf,
        b"/URI (https://example.com/image.jpg)"
    ));
    assert_eq!(
        position_of(
            &image_pdf,
            &crate::encode::tj("https://example.com/image.jpg")
        ),
        (72_000, 758_754)
    );
    let image_width = measure("https://example.com/image.jpg", Face::Helvetica, 12.0);
    assert_eq!(
        rects(&image_pdf),
        vec![(72_000, 755_922, 72_000 + image_width, 769_890)]
    );

    let video_pdf = render(&read(video_html));
    assert!(!contains(&video_pdf, &crate::encode::tj("Your browser")));
    assert!(!contains(&video_pdf, &crate::encode::tj("Video caption")));
    assert!(contains(
        &video_pdf,
        b"/URI (https://example.com/video.mp4)"
    ));

    let table_pdf = render(&read(table_html));
    assert_eq!(
        position_of(&table_pdf, &crate::encode::tj("Left Right")),
        (72_000, 758_754)
    );
    assert_eq!(
        position_of(&table_pdf, &crate::encode::tj("A B")),
        (72_000, 742_754)
    );
    assert_eq!(
        position_of(&table_pdf, &crate::encode::tj("C ")),
        (72_000, 726_754)
    );
    let go_x = 72_000 + measure("C ", Face::Helvetica, 12.0);
    assert_eq!(
        position_of(&table_pdf, &crate::encode::tj("go")),
        (go_x, 726_754)
    );
    assert!(contains(&table_pdf, b"/URI (https://example.com/a\\(b\\))"));
    assert_eq!(
        rects(&table_pdf),
        vec![(
            go_x,
            723_922,
            go_x + measure("go", Face::Helvetica, 12.0),
            737_890
        )]
    );

    let around_pdf = render(&read(around_html));
    assert_eq!(
        position_of(&around_pdf, &crate::encode::tj("https://example.com/a.jpg")),
        (72_000, 730_754)
    );

    assert_eq!(one_code("<pre><code>img src</code></pre>"), "img src");
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
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

#[test]
fn nesting_deeper_than_the_cap_is_left_out() {
    let cap = usize::from(crate::read::MAX_NESTING);
    assert!(block_text(&read(&repeat_tag("blockquote", cap, "kept"))).contains("kept"));
    assert!(
        !block_text(&read(&repeat_tag("blockquote", cap + 8, "too deep"))).contains("too deep")
    );
    assert!(block_text(&read(&repeat_list(cap, "kept"))).contains("kept"));
    assert!(!block_text(&read(&repeat_list(cap + 8, "too deep"))).contains("too deep"));
    assert!(block_text(&read(&repeat_styles(cap, "kept"))).contains("kept"));
    assert!(!block_text(&read(&repeat_styles(cap + 8, "too deep"))).contains("too deep"));
    assert!(block_text(&read(&repeat_table(cap, "kept"))).contains("kept"));
    assert!(!block_text(&read(&repeat_table(cap + 8, "too deep"))).contains("too deep"));
}

fn repeat_tag(name: &str, times: usize, inner: &str) -> String {
    let mut html = String::new();
    for _ in 0..times {
        html.push('<');
        html.push_str(name);
        html.push('>');
    }
    html.push_str(inner);
    for _ in 0..times {
        html.push_str("</");
        html.push_str(name);
        html.push('>');
    }
    html
}

fn repeat_list(levels: usize, leaf: &str) -> String {
    let mut html = String::new();
    for index in 1..=levels {
        html.push_str("<ul><li>");
        if index == levels {
            html.push_str(leaf);
        }
    }
    for _ in 0..levels {
        html.push_str("</li></ul>");
    }
    html
}

fn repeat_table(levels: usize, leaf: &str) -> String {
    let mut html = String::new();
    for _ in 0..levels {
        html.push_str("<table><tr><td>");
    }
    html.push_str(leaf);
    for _ in 0..levels {
        html.push_str("</td></tr></table>");
    }
    html
}

fn repeat_styles(levels: usize, leaf: &str) -> String {
    const TAGS: [&str; 5] = ["strong", "em", "u", "del", "code"];
    let mut html = String::from("<p>");
    for index in 0..levels {
        html.push('<');
        html.push_str(TAGS[index % TAGS.len()]);
        html.push('>');
    }
    html.push_str(leaf);
    for index in (0..levels).rev() {
        html.push_str("</");
        html.push_str(TAGS[index % TAGS.len()]);
        html.push('>');
    }
    html.push_str("</p>");
    html
}

fn block_text(blocks: &[Block]) -> String {
    let mut out = String::new();
    for block in blocks {
        match block {
            Block::Title(text) | Block::Alias(text) | Block::Code(text) => out.push_str(text),
            Block::Heading(_, runs) | Block::Paragraph(runs) | Block::Quote(_, runs) => {
                for run in runs {
                    out.push_str(&run.text);
                }
            }
            Block::List(items) => item_text(&mut out, items),
            Block::Rule(_) => {}
        }
    }
    out
}

fn item_text(out: &mut String, items: &[Item]) {
    for item in items {
        for run in &item.runs {
            out.push_str(&run.text);
        }
        item_text(out, &item.children);
    }
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
