#[cfg(test)]
use std::collections::BTreeSet;

#[cfg(test)]
use crate::encode;
#[cfg(test)]
use crate::font;
use crate::place::{PAGE_HEIGHT_U, PAGE_WIDTH_U};
#[cfg(test)]
use crate::font::Face;

#[cfg(test)]
pub(crate) fn line_pdf(line: &str) -> Vec<u8> {
    let line = encode::compose(line);
    let shown = encode::literal(&encode::utf16(&line, Face::Helvetica));
    let stream = format!("BT\n/F1 12 Tf\n72 760 Td\n{shown} Tj\nET\n");
    let mut chars = BTreeSet::new();
    for ch in line.chars() {
        chars.insert(encode::shown(ch, Face::Helvetica));
    }
    let mut objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [{}] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
            media_box()
        )
        .into_bytes(),
        format!("<< /Length {} >>\nstream\n{stream}endstream", stream.len()).into_bytes(),
    ];
    objects.extend(font::objects(Face::Helvetica, &chars, 5).expect("subset font"));
    write_pdf(&objects)
}

pub(crate) fn format_thousandths(value: i32) -> String {
    let sign = if value < 0 { "-" } else { "" };
    let absolute = value.unsigned_abs();
    let whole = absolute / 1000;
    let fraction = absolute % 1000;
    if fraction == 0 {
        format!("{sign}{whole}")
    } else {
        let digits = format!("{fraction:03}");
        format!("{sign}{whole}.{}", digits.trim_end_matches('0'))
    }
}

pub(crate) fn media_box() -> String {
    format!(
        "0 0 {} {}",
        format_thousandths(PAGE_WIDTH_U),
        format_thousandths(PAGE_HEIGHT_U)
    )
}

pub(crate) fn write_pdf(objects: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"%PDF-1.4\n");
    out.extend_from_slice(b"%\xE2\xE3\xCF\xD3\n");

    let mut offsets = Vec::with_capacity(objects.len());
    for (index, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        let header = format!("{} 0 obj\n", index + 1);
        out.extend_from_slice(header.as_bytes());
        out.extend_from_slice(object);
        out.extend_from_slice(b"\nendobj\n");
    }

    let xref_at = out.len();
    let size = objects.len() + 1;
    out.extend_from_slice(format!("xref\n0 {size}\n").as_bytes());
    out.extend_from_slice(b"0000000000 65535 f\r\n");
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n\r\n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {size} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF\n")
            .as_bytes(),
    );
    out
}
