use std::borrow::Cow;

use log::{Level, LevelFilter, Log, Metadata, Record};
use rocket::fairing::{Fairing, Info, Kind};
use rocket::{Request, Response};

pub(crate) fn install() {
    let _ = log::set_boxed_logger(Box::new(RedactingLogger));
    log::set_max_level(LevelFilter::Info);
}

struct RedactingLogger;

impl Log for RedactingLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let module = record.module_path().unwrap_or("");
        if module.starts_with("hyper") || module.starts_with("rustls") || module.starts_with("r2d2")
        {
            return;
        }

        let raw = record.args().to_string();
        let text = redact_k_values(&raw);
        let launch = record.target().contains("rocket::launch");
        let indented = record.target().ends_with('_');
        let level = if launch { Level::Info } else { record.level() };
        let label = if indented || launch {
            ""
        } else {
            match level {
                Level::Error => "Error: ",
                Level::Warn => "Warning: ",
                _ => "",
            }
        };
        if indented {
            emit(&format!("   >> {label}{text}"));
        } else {
            emit(&format!("{label}{text}"));
        }
    }

    fn flush(&self) {}
}

pub(crate) fn redact_k_values(input: &str) -> Cow<'_, str> {
    let bytes = input.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    let mut changed = false;
    while i < bytes.len() {
        if is_k_param(bytes, i) {
            if !changed {
                out.push_str(&input[..i]);
                changed = true;
            }
            out.push(bytes[i] as char);
            out.push_str("k=");
            i += 3;
            while i < bytes.len() && is_k_value_byte(bytes[i]) {
                i += 1;
            }
            continue;
        }
        let ch = input[i..].chars().next().unwrap();
        if changed {
            out.push(ch);
        }
        i += ch.len_utf8();
    }
    if changed {
        Cow::Owned(out)
    } else {
        Cow::Borrowed(input)
    }
}

fn is_k_param(bytes: &[u8], i: usize) -> bool {
    (bytes[i] == b'?' || bytes[i] == b'&')
        && bytes.get(i + 1) == Some(&b'k')
        && bytes.get(i + 2) == Some(&b'=')
}

pub struct LocationLog;

#[rocket::async_trait]
impl Fairing for LocationLog {
    fn info(&self) -> Info {
        Info {
            name: "Redacted Location",
            kind: Kind::Response,
        }
    }

    async fn on_response<'r>(&self, _request: &'r Request<'_>, response: &mut Response<'r>) {
        let Some(location) = response.headers().get_one("Location") else {
            return;
        };
        let Some(line) = location_log_line(location) else {
            return;
        };
        emit(&line);
    }
}

fn location_log_line(location: &str) -> Option<String> {
    let redacted = redact_k_values(location);
    if redacted.as_ref() == location {
        None
    } else {
        Some(format!("   >> Location: {redacted}"))
    }
}

fn emit(line: &str) {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
}

fn is_k_value_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(byte, b'-' | b'_' | b'.' | b'~' | b'%' | b'+' | b'/' | b'=')
}

#[cfg(test)]
mod tests {
    use super::redact_k_values;

    #[test]
    fn request_line_keeps_the_method_and_path_and_drops_the_key() {
        assert_eq!(
            redact_k_values("GET /nojs/s/0123456789abcdef0123?k=LOGSECRETVALUE:"),
            "GET /nojs/s/0123456789abcdef0123?k=:"
        );
        assert_eq!(
            redact_k_values("GET /nojs/s/0123456789abcdef0123?k=SE-CR_ET&error=title_required"),
            "GET /nojs/s/0123456789abcdef0123?k=&error=title_required"
        );
        assert_eq!(
            redact_k_values("GET /nojs?error=title_required&k=LOGSECRETVALUE"),
            "GET /nojs?error=title_required&k="
        );
    }

    #[test]
    fn location_drops_the_key_and_other_lines_stay() {
        assert_eq!(
            redact_k_values("Location: /nojs/s/0123456789abcdef0123?k=LOGSECRETVALUE"),
            "Location: /nojs/s/0123456789abcdef0123?k="
        );
        assert_eq!(
            redact_k_values("Location: https://nonograph.example/nojs/s/abc?k=LOGSECRETVALUE"),
            "Location: https://nonograph.example/nojs/s/abc?k="
        );
        let plain = "GET /nojs?error=title_required";
        assert_eq!(redact_k_values(plain), plain);
        assert_eq!(
            redact_k_values("Matched: (nojs_sealed_view) GET /nojs/s/<locator>?<k>"),
            "Matched: (nojs_sealed_view) GET /nojs/s/<locator>?<k>"
        );
        let colored = "\u{1b}[32mGET\u{1b}[0m \u{1b}[34m/nojs/s/abc?k=LOGSECRETVALUE\u{1b}[0m:";
        let redacted = redact_k_values(colored);
        assert!(!redacted.contains("LOGSECRETVALUE"));
        assert!(redacted.contains("?k="));
        assert!(redacted.contains("\u{1b}[32mGET\u{1b}[0m"));
    }

    #[test]
    fn a_location_with_k_is_logged_without_the_value() {
        let line = super::location_log_line("/nojs/s/0123456789abcdef0123?k=LOGSECRETVALUE")
            .expect("location");
        assert_eq!(line, "   >> Location: /nojs/s/0123456789abcdef0123?k=");
        assert!(super::location_log_line("/6f3a").is_none());
        assert!(super::location_log_line("/nojs?error=title_required").is_none());
    }

    #[test]
    fn the_logger_is_installed_before_rocket_launches() {
        let main = include_str!("main.rs");
        let install = main.find("access_log::install()").unwrap();
        let launch = main.find(".launch()").unwrap();
        assert!(install < launch);
    }
}
