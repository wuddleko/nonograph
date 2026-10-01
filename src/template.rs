use nonograph_parser::html_attr_escape;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

pub struct TemplateEngine {
    templates_dir: String,
    loaded: Mutex<HashMap<String, String>>,
}

pub fn shared() -> &'static TemplateEngine {
    static ENGINE: OnceLock<TemplateEngine> = OnceLock::new();
    ENGINE.get_or_init(|| TemplateEngine::new("templates"))
}

impl TemplateEngine {
    pub fn new(templates_dir: &str) -> Self {
        Self {
            templates_dir: templates_dir.to_string(),
            loaded: Mutex::new(HashMap::new()),
        }
    }

    pub fn preload(&self, names: &[&str]) -> Result<(), String> {
        for name in names {
            self.template(name)?;
        }
        Ok(())
    }

    pub fn render(
        &self,
        template_name: &str,
        context: &HashMap<String, String>,
    ) -> Result<String, String> {
        let template_content = self.template(template_name)?;

        // Replace all {{variable}} patterns with values from context
        let (result, unreplaced) = substitute_placeholders(&template_content, context);
        // Check for any remaining unreplaced variables and warn
        if unreplaced {
            eprintln!(
                "Nonograph: Warning: Template {} contains unreplaced variables",
                template_name
            );
        }

        Ok(result)
    }

    fn template(&self, template_name: &str) -> Result<String, String> {
        let mut loaded = self.loaded.lock().unwrap();
        if let Some(cached) = loaded.get(template_name) {
            return Ok(cached.clone());
        }
        let template_path = Path::new(&self.templates_dir).join(format!("{template_name}.html"));
        let template_content = fs::read_to_string(&template_path)
            .map_err(|e| format!("Failed to read template {template_name}: {e}"))?;
        loaded.insert(template_name.to_string(), template_content.clone());
        Ok(template_content)
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
    if matches!(
        key,
        "content" | "scripts" | "content_field" | "fallback_css" | "nostr_link"
    ) {
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

#[cfg(test)]
#[path = "../test/template.rs"]
mod tests;
