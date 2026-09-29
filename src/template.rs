use nonograph_parser::html_attr_escape;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub struct TemplateEngine {
    templates_dir: String,
}

impl TemplateEngine {
    pub fn new(templates_dir: &str) -> Self {
        Self {
            templates_dir: templates_dir.to_string(),
        }
    }

    pub fn render(
        &self,
        template_name: &str,
        context: &HashMap<String, String>,
    ) -> Result<String, String> {
        let template_path = Path::new(&self.templates_dir).join(format!("{}.html", template_name));

        let template_content = fs::read_to_string(&template_path)
            .map_err(|e| format!("Failed to read template {}: {}", template_name, e))?;

        let mut result = template_content;

        // Inject the writemark.js editor source when requested, so templates can
        // embed the editor inline via `{{writemark_js}}`. The file ships as an ES
        // module (it ends with an `export { ... }` statement), but we inline it
        // into a classic `<script>` tag where `export` is a syntax error that
        // would abort the whole script. Strip any top-level `export` statements
        // before injecting; the element self-registers via `customElements.define`,
        // so the exports are unnecessary for inline browser use.
        if result.contains("{{writemark_js}}") {
            let script_path = Path::new(&self.templates_dir).join("writemark.js");
            let script = fs::read_to_string(&script_path).map_err(|e| {
                format!(
                    "Failed to read writemark.js for template {}: {}",
                    template_name, e
                )
            })?;
            let script = strip_module_exports_and_line_comments(&script);
            result = result.replace("{{writemark_js}}", &script);
        }

        // One pass, so a value that itself contains `{{key}}` is left literal.
        // A second pass would rewrite tokens inside post HTML and raw markdown.
        let (result, unreplaced) = substitute_placeholders(&result, context);
        if unreplaced {
            eprintln!(
                "Nonograph: Warning: Template {} contains unreplaced variables",
                template_name
            );
        }

        Ok(result)
    }

    pub fn render_with_defaults(
        &self,
        template_name: &str,
        context: &HashMap<String, String>,
    ) -> Result<String, String> {
        let mut full_context = HashMap::new();

        // Set default values
        full_context.insert("title".to_string(), "Nonograph".to_string());
        full_context.insert("content".to_string(), "".to_string());
        full_context.insert("error".to_string(), "".to_string());
        full_context.insert("success".to_string(), "".to_string());

        // Override with provided context
        for (key, value) in context {
            full_context.insert(key.clone(), value.clone());
        }

        self.render(template_name, &full_context)
    }
}

fn substitute_placeholders(template: &str, context: &HashMap<String, String>) -> (String, bool) {
    let mut result = String::with_capacity(template.len());
    let mut unreplaced = false;
    let mut rest = template;

    while let Some(start) = rest.find("{{") {
        result.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        if let Some(end) = after.find("}}") {
            let key = &after[..end];
            if let Some(value) = context.get(key) {
                result.push_str(&placeholder_value(key, value));
                rest = &after[end + 2..];
                continue;
            }
            if is_placeholder_key(key) {
                unreplaced = true;
            }
        }
        result.push_str("{{");
        rest = after;
    }

    result.push_str(rest);
    (result, unreplaced)
}

fn placeholder_value(key: &str, value: &str) -> String {
    if key == "content" || key == "raw_post_json" {
        value.to_string()
    } else {
        html_attr_escape(value)
    }
}

fn is_placeholder_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn strip_module_exports_and_line_comments(source: &str) -> String {
    source
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !trimmed.starts_with("export ") && !trimmed.starts_with("//")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
#[path = "../test/template.rs"]
mod tests;
