use crate::{Block, Item, Marker, Rule, Run};

pub(crate) const MAX_NESTING: u8 = 32;

pub fn read(html: &str) -> Vec<Block> {
    let mut scan = Scan {
        html,
        index: 0,
        tables: 0,
    };
    let mut blocks = Vec::new();
    while scan.index < scan.html.len() {
        if !scan.at(b'<') {
            scan.skip_until(b'<');
            continue;
        }
        let Some(tag) = scan.tag() else {
            scan.index += 1;
            continue;
        };
        if tag.ignored || tag.closing {
            continue;
        }
        if tag.name == "hr" {
            blocks.push(Block::Rule(hr_rule(&tag.attrs)));
            continue;
        }
        if tag.name == "img" && !tag.closing {
            let mut runs = Vec::new();
            if let Some(run) = address_link(attribute(&tag.attrs, "src")) {
                runs.push(run);
            }
            append_runs(&mut runs, scan.take_inline_tail());
            if !runs.is_empty() {
                blocks.push(Block::Paragraph(runs));
            }
            continue;
        }
        if tag.name == "video" && !tag.closing && !tag.self_closing {
            let mut runs = Vec::new();
            if let Some(run) = scan.video_link() {
                runs.push(run);
            }
            append_runs(&mut runs, scan.take_inline_tail());
            if !runs.is_empty() {
                blocks.push(Block::Paragraph(runs));
            }
            continue;
        }
        if tag.name == "table" && !tag.closing && !tag.self_closing {
            let runs = scan.collect_table();
            if !runs.is_empty() {
                blocks.push(Block::Paragraph(runs));
            }
            continue;
        }
        if tag.name == "div" {
            if let Some(rule) = div_rule(&tag.attrs) {
                if !tag.self_closing {
                    scan.skip_element("div");
                }
                blocks.push(Block::Rule(rule));
                continue;
            }
            if !tag.self_closing && class_has(&tag.attrs, "media-caption") {
                scan.skip_element("div");
                continue;
            }
        }
        if tag.self_closing {
            continue;
        }
        if tag.name == "blockquote" {
            blocks.extend(scan.collect_quote(1));
            continue;
        }
        if tag.name == "pre" {
            let text = scan.collect_code();
            if !text.is_empty() {
                blocks.push(Block::Code(text));
            }
            continue;
        }
        if tag.name == "ul" || tag.name == "ol" {
            let items = scan.collect_list(&tag.name);
            if !items.is_empty() {
                blocks.push(Block::List(items));
            }
            continue;
        }
        let runs = match tag.name.as_str() {
            "p" => scan.collect_runs_in("p", Style::PLAIN, true, 0),
            "h1" | "h2" | "h3" | "h4" => scan.collect_runs(&tag.name, Style::PLAIN),
            _ => continue,
        };
        if runs.is_empty() {
            continue;
        }
        blocks.push(match tag.name.as_str() {
            "h1" => Block::Heading(1, runs),
            "h2" => Block::Heading(2, runs),
            "h3" => Block::Heading(3, runs),
            "h4" => Block::Heading(4, runs),
            _ => Block::Paragraph(runs),
        });
    }
    blocks
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Style {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    code: bool,
}

impl Style {
    const PLAIN: Self = Self {
        bold: false,
        italic: false,
        underline: false,
        strike: false,
        code: false,
    };
}

struct Scan<'a> {
    html: &'a str,
    index: usize,
    tables: u8,
}

struct Tag {
    name: String,
    closing: bool,
    self_closing: bool,
    attrs: String,
    ignored: bool,
}

impl<'a> Scan<'a> {
    fn at(&self, byte: u8) -> bool {
        self.html.as_bytes().get(self.index) == Some(&byte)
    }

    fn rest(&self) -> &'a str {
        &self.html[self.index..]
    }

    fn skip_until(&mut self, byte: u8) {
        while self.index < self.html.len() && !self.at(byte) {
            self.index += 1;
        }
    }

    fn tag(&mut self) -> Option<Tag> {
        if !self.at(b'<') {
            return None;
        }
        if self.rest().starts_with("<!--") {
            self.index += 4;
            if let Some(end) = self.rest().find("-->") {
                self.index += end + 3;
            } else {
                self.index = self.html.len();
            }
            return Some(Tag::ignored());
        }
        if self.rest().starts_with("<!") || self.rest().starts_with("<?") {
            self.index += 2;
            self.skip_until(b'>');
            if self.at(b'>') {
                self.index += 1;
            }
            return Some(Tag::ignored());
        }
        let closing = self.rest().starts_with("</");
        let name_at = self.index + if closing { 2 } else { 1 };
        let bytes = self.html.as_bytes();
        if name_at >= bytes.len() || !bytes[name_at].is_ascii_alphabetic() {
            return None;
        }
        let mut name_end = name_at + 1;
        while name_end < bytes.len() && bytes[name_end].is_ascii_alphanumeric() {
            name_end += 1;
        }
        let name = self.html[name_at..name_end].to_ascii_lowercase();
        self.index = name_end;
        let attrs_at = self.index;
        self.finish_tag();
        let mut attrs = &self.html[attrs_at..self.index.saturating_sub(1)];
        let self_closing = attrs.trim_end().ends_with('/');
        if self_closing {
            attrs = attrs.trim_end().trim_end_matches('/').trim_end();
        }
        Some(Tag {
            name,
            closing,
            self_closing,
            attrs: attrs.to_string(),
            ignored: false,
        })
    }

    fn finish_tag(&mut self) {
        let bytes = self.html.as_bytes();
        let mut quote = 0u8;
        while self.index < bytes.len() {
            let byte = bytes[self.index];
            self.index += 1;
            if quote != 0 {
                if byte == quote {
                    quote = 0;
                }
                continue;
            }
            if byte == b'"' || byte == b'\'' {
                quote = byte;
                continue;
            }
            if byte == b'>' {
                return;
            }
        }
    }

    fn collect_runs(&mut self, name: &str, style: Style) -> Vec<Run> {
        self.collect_runs_in(name, style, false, 0)
    }

    fn collect_runs_in(
        &mut self,
        name: &str,
        style: Style,
        join_media: bool,
        nest: u8,
    ) -> Vec<Run> {
        let mut runs = Vec::new();
        let mut text = String::new();
        let mut depth = 1u32;
        while self.index < self.html.len() && depth > 0 {
            if !self.at(b'<') {
                let start = self.index;
                self.skip_until(b'<');
                text.push_str(&decode(&self.html[start..self.index]));
                continue;
            }
            let Some(tag) = self.tag() else {
                text.push('<');
                self.index += 1;
                continue;
            };
            if tag.ignored {
                continue;
            }
            if tag.name == name && !tag.closing && !tag.self_closing {
                depth += 1;
                continue;
            }
            if tag.name == name && tag.closing {
                depth -= 1;
                if depth == 0 && join_media && self.continues_through_media() {
                    push_run(&mut runs, style, &mut text);
                    append_runs(&mut runs, self.take_inline_tail());
                }
                continue;
            }
            if tag.name == "br" && !tag.closing {
                text.push('\n');
                continue;
            }
            if self.absorb_media(&tag, &mut runs, &mut text, style) {
                continue;
            }
            self.absorb(&tag, &mut runs, &mut text, style, nest);
        }
        push_run(&mut runs, style, &mut text);
        runs
    }

    fn continues_through_media(&mut self) -> bool {
        let mark = self.index;
        while self.index < self.html.len()
            && matches!(self.html.as_bytes()[self.index], b' ' | b'\t')
        {
            self.index += 1;
        }
        let continues = self.at(b'<')
            && self.tag().is_some_and(|tag| {
                !tag.closing && tag.name == "div" && class_has(&tag.attrs, "media-with-caption")
            });
        self.index = mark;
        continues
    }

    fn take_inline_tail(&mut self) -> Vec<Run> {
        let mut runs = Vec::new();
        let mut text = String::new();
        loop {
            if self.index >= self.html.len() {
                break;
            }
            if self.at(b'\n') || self.at(b'\r') {
                if self.ends_inline() {
                    break;
                }
                if self.at(b'\r') {
                    self.index += 1;
                }
                if self.at(b'\n') {
                    self.index += 1;
                }
                text.push('\n');
                continue;
            }
            if !self.at(b'<') {
                let start = self.index;
                while self.index < self.html.len()
                    && !self.at(b'<')
                    && !self.at(b'\n')
                    && !self.at(b'\r')
                {
                    self.index += 1;
                }
                text.push_str(&decode(&self.html[start..self.index]));
                continue;
            }
            let mark = self.index;
            let Some(tag) = self.tag() else {
                text.push('<');
                self.index += 1;
                continue;
            };
            if tag.ignored {
                continue;
            }
            if block_boundary(&tag) {
                self.index = mark;
                break;
            }
            if tag.name == "br" && !tag.closing {
                text.push('\n');
                continue;
            }
            if self.absorb_media(&tag, &mut runs, &mut text, Style::PLAIN) {
                continue;
            }
            self.absorb(&tag, &mut runs, &mut text, Style::PLAIN, 0);
        }
        push_run(&mut runs, Style::PLAIN, &mut text);
        runs
    }

    fn ends_inline(&mut self) -> bool {
        let mark = self.index;
        if self.at(b'\r') {
            self.index += 1;
        }
        if self.at(b'\n') {
            self.index += 1;
        }
        while self.index < self.html.len()
            && matches!(self.html.as_bytes()[self.index], b' ' | b'\t')
        {
            self.index += 1;
        }
        let ends = if self.index >= self.html.len() || self.at(b'\n') || self.at(b'\r') {
            true
        } else if !self.at(b'<') {
            false
        } else {
            self.tag().is_some_and(|tag| block_boundary(&tag))
        };
        self.index = mark;
        ends
    }

    fn collect_quote(&mut self, depth: u8) -> Vec<Block> {
        let mut blocks = Vec::new();
        let mut runs = Vec::new();
        let mut text = String::new();
        let mut nested = 1u32;
        while self.index < self.html.len() && nested > 0 {
            if !self.at(b'<') {
                let start = self.index;
                self.skip_until(b'<');
                text.push_str(&decode(&self.html[start..self.index]));
                continue;
            }
            let Some(tag) = self.tag() else {
                text.push('<');
                self.index += 1;
                continue;
            };
            if tag.ignored {
                continue;
            }
            if tag.name == "blockquote" && tag.closing {
                nested -= 1;
                continue;
            }
            if tag.name == "blockquote" && !tag.closing && !tag.self_closing {
                push_run(&mut runs, Style::PLAIN, &mut text);
                if !runs.is_empty() {
                    blocks.push(Block::Quote(depth, std::mem::take(&mut runs)));
                }
                if depth >= MAX_NESTING {
                    self.skip_element("blockquote");
                } else {
                    blocks.extend(self.collect_quote(depth + 1));
                }
                continue;
            }
            if tag.name == "br" && !tag.closing {
                text.push('\n');
                continue;
            }
            if self.absorb_media(&tag, &mut runs, &mut text, Style::PLAIN) {
                continue;
            }
            self.absorb(&tag, &mut runs, &mut text, Style::PLAIN, 0);
        }
        push_run(&mut runs, Style::PLAIN, &mut text);
        if !runs.is_empty() {
            blocks.push(Block::Quote(depth, runs));
        }
        blocks
    }

    fn collect_code(&mut self) -> String {
        let mut text = String::new();
        let mut depth = 1u32;
        while self.index < self.html.len() && depth > 0 {
            if !self.at(b'<') {
                let start = self.index;
                self.skip_until(b'<');
                text.push_str(&decode(&self.html[start..self.index]));
                continue;
            }
            let Some(tag) = self.tag() else {
                text.push('<');
                self.index += 1;
                continue;
            };
            if tag.ignored {
                continue;
            }
            if tag.name == "pre" && !tag.closing && !tag.self_closing {
                depth += 1;
                continue;
            }
            if tag.name == "pre" && tag.closing {
                depth -= 1;
                continue;
            }
            if tag.name == "br" && !tag.closing {
                text.push('\n');
                continue;
            }
            if tag.name == "div"
                && !tag.closing
                && !tag.self_closing
                && (class_has(&tag.attrs, "code-header") || class_has(&tag.attrs, "line-numbers"))
            {
                self.skip_element("div");
            }
        }
        text
    }

    fn collect_list(&mut self, name: &str) -> Vec<Item> {
        self.collect_list_at(name, 1)
    }

    fn collect_list_at(&mut self, name: &str, nest: u8) -> Vec<Item> {
        let ordered = name == "ol";
        let mut items: Vec<Item> = Vec::new();
        let mut number = 1u32;
        let mut depth = 1u32;
        while self.index < self.html.len() && depth > 0 {
            if !self.at(b'<') {
                self.skip_until(b'<');
                continue;
            }
            let Some(tag) = self.tag() else {
                self.index += 1;
                continue;
            };
            if tag.ignored {
                continue;
            }
            if tag.name == name && tag.closing {
                depth -= 1;
                continue;
            }
            if matches!(tag.name.as_str(), "ul" | "ol") && !tag.closing && !tag.self_closing {
                if nest >= MAX_NESTING {
                    self.skip_element(&tag.name);
                } else {
                    let nested = self.collect_list_at(&tag.name, nest + 1);
                    if let Some(last) = items.last_mut() {
                        last.children.extend(nested);
                    } else {
                        items.extend(nested);
                    }
                }
                continue;
            }
            if tag.name == "li" && !tag.closing {
                let marker = if ordered {
                    let current = number;
                    number += 1;
                    Marker::Number(current)
                } else {
                    Marker::Bullet
                };
                items.push(if tag.self_closing {
                    Item {
                        marker,
                        runs: Vec::new(),
                        children: Vec::new(),
                    }
                } else {
                    self.collect_item(marker, nest)
                });
            }
        }
        items
    }

    fn collect_item(&mut self, mut marker: Marker, nest: u8) -> Item {
        let mut runs = Vec::new();
        let mut text = String::new();
        let mut children = Vec::new();
        let mut depth = 1u32;
        let mut trim_lead = false;
        while self.index < self.html.len() && depth > 0 {
            if !self.at(b'<') {
                let start = self.index;
                self.skip_until(b'<');
                let mut chunk = decode(&self.html[start..self.index]);
                if trim_lead {
                    chunk = chunk.trim_start().to_string();
                    trim_lead = false;
                }
                text.push_str(&chunk);
                continue;
            }
            let Some(tag) = self.tag() else {
                text.push('<');
                self.index += 1;
                continue;
            };
            if tag.ignored {
                continue;
            }
            if tag.name == "li" && !tag.closing && !tag.self_closing {
                depth += 1;
                continue;
            }
            if tag.name == "li" && tag.closing {
                depth -= 1;
                continue;
            }
            if tag.name == "br" && !tag.closing {
                text.push('\n');
                continue;
            }
            if tag.name == "input" && !tag.closing && is_checkbox(&tag.attrs) {
                marker = Marker::Task(attribute(&tag.attrs, "checked").is_some());
                trim_lead = true;
                continue;
            }
            if matches!(tag.name.as_str(), "ul" | "ol") && !tag.closing && !tag.self_closing {
                push_run(&mut runs, Style::PLAIN, &mut text);
                if nest >= MAX_NESTING {
                    self.skip_element(&tag.name);
                } else {
                    children.extend(self.collect_list_at(&tag.name, nest + 1));
                }
                continue;
            }
            if self.absorb_media(&tag, &mut runs, &mut text, Style::PLAIN) {
                continue;
            }
            self.absorb(&tag, &mut runs, &mut text, Style::PLAIN, 0);
        }
        push_run(&mut runs, Style::PLAIN, &mut text);
        Item {
            marker,
            runs,
            children,
        }
    }

    fn absorb_media(
        &mut self,
        tag: &Tag,
        runs: &mut Vec<Run>,
        text: &mut String,
        style: Style,
    ) -> bool {
        if tag.closing {
            return false;
        }
        if tag.name == "img" {
            if let Some(run) = address_link(attribute(&tag.attrs, "src")) {
                push_run(runs, style, text);
                push_owned(runs, run);
            }
            return true;
        }
        if tag.self_closing {
            return false;
        }
        if tag.name == "video" {
            if let Some(run) = self.video_link() {
                push_run(runs, style, text);
                push_owned(runs, run);
            }
            return true;
        }
        if tag.name == "table" {
            push_run(runs, style, text);
            append_runs(runs, self.collect_table());
            return true;
        }
        if tag.name == "div" && class_has(&tag.attrs, "media-caption") {
            self.skip_element("div");
            return true;
        }
        false
    }

    fn video_link(&mut self) -> Option<Run> {
        let mut link = None;
        let mut depth = 1u32;
        while self.index < self.html.len() && depth > 0 {
            if !self.at(b'<') {
                self.skip_until(b'<');
                continue;
            }
            let Some(tag) = self.tag() else {
                self.index += 1;
                continue;
            };
            if tag.ignored {
                continue;
            }
            if tag.name == "video" && !tag.closing && !tag.self_closing {
                depth += 1;
                continue;
            }
            if tag.name == "video" && tag.closing {
                depth -= 1;
                continue;
            }
            if tag.name == "source" && !tag.closing && link.is_none() {
                link = address_link(attribute(&tag.attrs, "src"));
            }
        }
        link
    }

    fn collect_table(&mut self) -> Vec<Run> {
        if self.tables >= MAX_NESTING {
            self.skip_element("table");
            return Vec::new();
        }
        self.tables += 1;
        let runs = self.collect_open_table();
        self.tables -= 1;
        runs
    }

    fn collect_open_table(&mut self) -> Vec<Run> {
        let mut rows: Vec<Vec<Vec<Run>>> = Vec::new();
        let mut row: Vec<Vec<Run>> = Vec::new();
        let mut depth = 1u32;
        while self.index < self.html.len() && depth > 0 {
            if !self.at(b'<') {
                self.skip_until(b'<');
                continue;
            }
            let Some(tag) = self.tag() else {
                self.index += 1;
                continue;
            };
            if tag.ignored {
                continue;
            }
            if tag.name == "table" && !tag.closing && !tag.self_closing {
                let nested = self.collect_table();
                if !nested.is_empty() {
                    row.push(nested);
                }
                continue;
            }
            if tag.name == "table" && tag.closing {
                depth -= 1;
                if depth == 0 && !row.is_empty() {
                    rows.push(std::mem::take(&mut row));
                }
                continue;
            }
            if tag.name == "tr" && tag.closing && !row.is_empty() {
                rows.push(std::mem::take(&mut row));
                continue;
            }
            if matches!(tag.name.as_str(), "td" | "th") && !tag.closing && !tag.self_closing {
                let cell = self.collect_runs(&tag.name, Style::PLAIN);
                if !cell.is_empty() {
                    row.push(cell);
                }
            }
        }
        if !row.is_empty() {
            rows.push(row);
        }
        let mut runs = Vec::new();
        for (index, row) in rows.into_iter().enumerate() {
            if index > 0 {
                push_owned(&mut runs, Run::plain("\n"));
            }
            for (cell_index, cell) in row.into_iter().enumerate() {
                if cell_index > 0 {
                    push_owned(&mut runs, Run::plain(" "));
                }
                append_runs(&mut runs, cell);
            }
        }
        runs
    }

    fn absorb(
        &mut self,
        tag: &Tag,
        runs: &mut Vec<Run>,
        text: &mut String,
        style: Style,
        nest: u8,
    ) {
        if tag.closing || tag.self_closing {
            return;
        }
        if tag.name == "a" {
            if class_has(&tag.attrs, "header-anchor") {
                self.skip_element("a");
                return;
            }
            if let Some(href) = attribute(&tag.attrs, "href").filter(|href| !href.is_empty()) {
                let url = decode(href);
                push_run(runs, style, text);
                for run in self.nest_runs("a", style, nest) {
                    push_owned(runs, with_url(run, &url));
                }
            }
            return;
        }
        if stays_text(tag) {
            push_run(runs, style, text);
            append_runs(runs, self.nest_runs(&tag.name, style, nest));
            return;
        }
        if let Some(inner) = layer(style, &tag.name) {
            push_run(runs, style, text);
            append_runs(runs, self.nest_runs(&tag.name, inner, nest));
        }
    }

    fn nest_runs(&mut self, name: &str, style: Style, nest: u8) -> Vec<Run> {
        if nest >= MAX_NESTING {
            self.skip_element(name);
            Vec::new()
        } else {
            self.collect_runs_in(name, style, false, nest + 1)
        }
    }

    fn skip_element(&mut self, name: &str) {
        let mut depth = 1u32;
        while self.index < self.html.len() && depth > 0 {
            if !self.at(b'<') {
                self.index += 1;
                continue;
            }
            let Some(tag) = self.tag() else {
                self.index += 1;
                continue;
            };
            if tag.ignored || tag.name != name {
                continue;
            }
            if tag.closing {
                depth -= 1;
            } else if !tag.self_closing {
                depth += 1;
            }
        }
    }
}

impl Tag {
    fn ignored() -> Self {
        Self {
            name: String::new(),
            closing: false,
            self_closing: false,
            attrs: String::new(),
            ignored: true,
        }
    }
}

fn layer(style: Style, name: &str) -> Option<Style> {
    let mut next = style;
    match name {
        "strong" => next.bold = true,
        "em" => next.italic = true,
        "u" => next.underline = true,
        "del" => next.strike = true,
        "code" => next.code = true,
        _ => return None,
    }
    Some(next)
}

fn stays_text(tag: &Tag) -> bool {
    tag.name == "sup"
        || tag.name == "mark"
        || (tag.name == "span" && class_has(&tag.attrs, "secret"))
}

fn push_run(runs: &mut Vec<Run>, style: Style, text: &mut String) {
    if text.is_empty() {
        return;
    }
    push_owned(runs, to_run(style, std::mem::take(text), None));
}

fn append_runs(runs: &mut Vec<Run>, extra: Vec<Run>) {
    for run in extra {
        push_owned(runs, run);
    }
}

fn push_owned(runs: &mut Vec<Run>, run: Run) {
    if run.text.is_empty() {
        return;
    }
    if let Some(last) = runs.last_mut() {
        if append_same(last, &run) {
            return;
        }
    }
    runs.push(run);
}

fn append_same(last: &mut Run, next: &Run) -> bool {
    if marks(last) != marks(next) {
        return false;
    }
    last.text.push_str(&next.text);
    true
}

fn with_url(mut run: Run, url: &str) -> Run {
    if run.url.is_some() {
        return run;
    }
    run.url = Some(url.to_string());
    run
}

fn to_run(style: Style, text: String, url: Option<String>) -> Run {
    Run {
        text,
        bold: style.bold,
        italic: style.italic,
        underline: style.underline,
        strike: style.strike,
        code: style.code,
        url,
    }
}

fn marks(run: &Run) -> (Style, Option<&str>) {
    (
        Style {
            bold: run.bold,
            italic: run.italic,
            underline: run.underline,
            strike: run.strike,
            code: run.code,
        },
        run.url.as_deref(),
    )
}

fn block_boundary(tag: &Tag) -> bool {
    if tag.closing || tag.ignored {
        return false;
    }
    match tag.name.as_str() {
        "p" | "h1" | "h2" | "h3" | "h4" | "table" | "ul" | "ol" | "pre" | "blockquote" | "hr" => {
            true
        }
        "div" => {
            !class_has(&tag.attrs, "media-with-caption") && !class_has(&tag.attrs, "media-caption")
        }
        _ => false,
    }
}

fn class_has(attrs: &str, class: &str) -> bool {
    attribute(attrs, "class")
        .map(|value| {
            value
                .split_whitespace()
                .any(|token| token.eq_ignore_ascii_case(class))
        })
        .unwrap_or(false)
}

fn attribute<'a>(attrs: &'a str, name: &str) -> Option<&'a str> {
    let bytes = attrs.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let name_at = index;
        while index < bytes.len() && is_attr_name(bytes[index]) {
            index += 1;
        }
        if index == name_at {
            index += 1;
            continue;
        }
        let found = &attrs[name_at..index];
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() || bytes[index] != b'=' {
            if found.eq_ignore_ascii_case(name) {
                return Some("");
            }
            continue;
        }
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let value = if index < bytes.len() && (bytes[index] == b'"' || bytes[index] == b'\'') {
            let quote = bytes[index];
            index += 1;
            let value_at = index;
            while index < bytes.len() && bytes[index] != quote {
                index += 1;
            }
            let value = &attrs[value_at..index];
            if index < bytes.len() {
                index += 1;
            }
            value
        } else {
            let value_at = index;
            while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            &attrs[value_at..index]
        };
        if found.eq_ignore_ascii_case(name) {
            return Some(value);
        }
    }
    None
}

fn address_link(raw: Option<&str>) -> Option<Run> {
    let url = decode(raw?);
    if url.is_empty() {
        None
    } else {
        Some(Run {
            url: Some(url.clone()),
            ..Run::plain(url)
        })
    }
}

fn hr_rule(attrs: &str) -> Rule {
    if class_has(attrs, "divider-double") {
        Rule::Double
    } else {
        Rule::Thin
    }
}

fn div_rule(attrs: &str) -> Option<Rule> {
    if class_has(attrs, "divider-stars") {
        Some(Rule::Stars)
    } else if class_has(attrs, "divider-asterisk") {
        Some(Rule::Asterisk)
    } else {
        None
    }
}

fn is_checkbox(attrs: &str) -> bool {
    attribute(attrs, "type").is_some_and(|value| value.eq_ignore_ascii_case("checkbox"))
}

fn is_attr_name(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-' || byte == b':' || byte == b'_'
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'&' {
            let start = index;
            index += 1;
            while index < bytes.len() && bytes[index] != b'&' {
                index += 1;
            }
            out.push_str(&text[start..index]);
            continue;
        }
        if let Some((ch, next)) = entity(text, index) {
            out.push(ch);
            index = next;
        } else {
            out.push('&');
            index += 1;
        }
    }
    out
}

fn entity(text: &str, start: usize) -> Option<(char, usize)> {
    let rest = text.get(start + 1..)?;
    let end = rest.find(';')?;
    if end == 0 || end > 32 {
        return None;
    }
    let body = &rest[..end];
    let next = start + 2 + end;
    let ch = if let Some(hex) = body.strip_prefix("#x").or_else(|| body.strip_prefix("#X")) {
        char::from_u32(u32::from_str_radix(hex, 16).ok()?)?
    } else if let Some(decimal) = body.strip_prefix('#') {
        char::from_u32(decimal.parse().ok()?)?
    } else {
        match body.to_ascii_lowercase().as_str() {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            "nbsp" => '\u{00a0}',
            _ => return None,
        }
    };
    Some((ch, next))
}
