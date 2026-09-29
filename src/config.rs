use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub limits: Limits,
    pub server: Server,
    pub cache: Cache,
    pub performance: Performance,
    pub security: Security,
    pub theme: Theme,
    #[serde(default)]
    pub nostr: Nostr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Limits {
    pub title_max_length: usize,
    pub alias_max_length: usize,
    pub content_max_length: usize,
    pub form_data_limit_kb: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    pub port: u16,
    pub address: String,
    pub onion_url: String,
    pub onion_hostname_file: String,
}

fn default_onion_hostname_file() -> String {
    "/var/lib/tor/hidden_service/hostname".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cache {
    pub max_cache_size_mb: usize,
    pub stream_buffer_size: usize,
    pub cache_purge_interval_mins: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Performance {
    pub large_content_threshold: usize,
    pub streaming_threshold: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Security {
    pub max_url_length: usize,
    pub external_link_security: bool,
    pub csrf_protection_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    pub syntax_highlighting: String,
}

fn default_relays() -> Vec<String> {
    vec![
        "wss://relay.damus.io".to_string(),
        "wss://nos.lol".to_string(),
        "wss://relay.nostr.band".to_string(),
    ]
}

fn default_nostr_timeout_secs() -> u64 {
    3
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Nostr {
    #[serde(default = "default_relays")]
    pub relays: Vec<String>,
    #[serde(default = "default_nostr_timeout_secs")]
    pub timeout_secs: u64,
}

impl Default for Nostr {
    fn default() -> Self {
        Self {
            relays: default_relays(),
            timeout_secs: default_nostr_timeout_secs(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            limits: Limits {
                title_max_length: 128,
                alias_max_length: 32,
                content_max_length: 128000,
                form_data_limit_kb: 512,
            },
            server: Server {
                port: 8000,
                address: "127.0.0.1".to_string(),
                onion_url: String::new(),
                onion_hostname_file: default_onion_hostname_file(),
            },
            cache: Cache {
                max_cache_size_mb: 128,
                stream_buffer_size: 8192,
                cache_purge_interval_mins: 60,
            },
            performance: Performance {
                large_content_threshold: 30000,
                streaming_threshold: 50000,
            },
            security: Security {
                max_url_length: 4096,
                external_link_security: true,
                csrf_protection_enabled: true,
            },
            theme: Theme {
                syntax_highlighting: "base16-ocean.dark".to_string(),
            },
            nostr: Nostr::default(),
        }
    }
}

fn normalize_onion_url(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed.chars().any(|c| c.is_control()) {
        return None;
    }

    let (scheme, rest) = match trimmed.split_once("://") {
        Some((s, r)) => (s.to_ascii_lowercase(), r),
        None => ("http".to_string(), trimmed),
    };

    if scheme != "http" && scheme != "https" {
        return None;
    }

    let host_end = rest
        .find(|c| c == '/' || c == '?' || c == '#')
        .unwrap_or(rest.len());
    let host = &rest[..host_end];
    let path = &rest[host_end..];

    let host_lower = host.to_ascii_lowercase();
    if !host_lower.ends_with(".onion") {
        return None;
    }

    if !host_lower
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
    {
        return None;
    }

    Some(format!("{}://{}{}", scheme, host_lower, path))
}

impl Config {
    pub fn load() -> Result<Self, String> {
        let config_path = Path::new("Config.toml");

        if config_path.exists() {
            let content = fs::read_to_string(config_path)
                .map_err(|e| format!("Failed to read Config.toml: {}", e))?;

            toml::from_str(&content).map_err(|e| format!("Failed to parse Config.toml: {}", e))
        } else {
            Ok(Config::default())
        }
    }

    pub fn load_with_logging() -> Self {
        match Self::load() {
            Ok(config) => {
                println!("Nonograph: ✅ Configuration loaded successfully");
                println!("   Title limit: {} chars", config.limits.title_max_length);
                println!("   Alias limit: {} chars", config.limits.alias_max_length);
                println!(
                    "   Content limit: {} chars",
                    config.limits.content_max_length
                );
                println!("   Cache size: {} MB", config.cache.max_cache_size_mb);
                config
            }
            Err(e) => {
                eprintln!("Nonograph: ⚠️  Configuration error: {}", e);
                eprintln!("Nonograph:    Using default configuration");
                Config::default()
            }
        }
    }

    pub fn form_data_limit_bytes(&self) -> u32 {
        self.limits.form_data_limit_kb * 1024
    }

    pub fn resolve_onion_url(&self) -> Option<String> {
        let candidate = if !self.server.onion_url.trim().is_empty() {
            self.server.onion_url.trim().to_string()
        } else if let Ok(env_url) = std::env::var("ONION_URL") {
            if env_url.trim().is_empty() {
                return None;
            }
            env_url.trim().to_string()
        } else {
            let host = fs::read_to_string(&self.server.onion_hostname_file).ok()?;
            host.trim().to_string()
        };

        normalize_onion_url(&candidate)
    }

    pub fn validate_post(
        &self,
        title: &str,
        content: &str,
        alias: Option<&str>,
    ) -> Result<(), String> {
        if title.trim().is_empty() {
            return Err("title_required".to_string());
        }

        if content.trim().is_empty() {
            return Err("content_required".to_string());
        }

        if title.len() > self.limits.title_max_length {
            return Err("title_too_long".to_string());
        }

        if content.len() > self.limits.content_max_length {
            return Err("content_too_long".to_string());
        }

        if let Some(alias) = alias {
            if alias.len() > self.limits.alias_max_length {
                return Err("alias_too_long".to_string());
            }
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "../test/config.rs"]
mod tests;
