use serde::{Deserialize, Serialize};
use std::fs;
use std::net::{IpAddr, SocketAddr};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub limits: Limits,
    pub server: Server,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    pub port: u16,
    pub address: String,
    pub onion_url: String,
    pub onion_hostname_file: String,
    #[serde(default)]
    pub public_origin: String,
}

fn default_onion_hostname_file() -> String {
    "/var/lib/tor/hidden_service/hostname".to_string()
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
        "wss://relay.primal.net".to_string(),
        "wss://relay.snort.social".to_string(),
        "wss://offchain.pub".to_string(),
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
    /// SOCKS5 proxy for outbound relay connections (Tor in Docker: 127.0.0.1:9050).
    /// Empty means connect directly. A non-empty value that is not a socket address
    /// disables relay connections. When a proxy is set, publish and fetch fail
    /// closed if it is down.
    #[serde(default)]
    pub socks: String,
}

impl Default for Nostr {
    fn default() -> Self {
        Self {
            relays: default_relays(),
            timeout_secs: default_nostr_timeout_secs(),
            socks: String::new(),
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
            },
            server: Server {
                port: 8000,
                address: "127.0.0.1".to_string(),
                onion_url: String::new(),
                onion_hostname_file: default_onion_hostname_file(),
                public_origin: String::new(),
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

pub(crate) fn normalize_onion_url(input: &str) -> Option<String> {
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

pub(crate) fn normalize_https_url(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed.chars().any(|c| c.is_control()) {
        return None;
    }

    let (scheme, rest) = match trimmed.split_once("://") {
        Some((s, r)) => (s.to_ascii_lowercase(), r),
        None => return None,
    };

    if scheme != "https" {
        return None;
    }

    let host_end = rest
        .find(|c| c == '/' || c == '?' || c == '#')
        .unwrap_or(rest.len());
    let host = &rest[..host_end];
    let path = &rest[host_end..];

    let host_lower = host.to_ascii_lowercase();
    if host_lower.ends_with(".onion") {
        return None;
    }

    if host_lower.is_empty()
        || !host_lower
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == ':')
    {
        return None;
    }

    Some(format!("https://{}{}", host_lower, path))
}

impl Config {
    pub fn load_from(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Config::default());
        }

        let content =
            fs::read_to_string(path).map_err(|e| format!("Failed to read Config.toml: {}", e))?;
        let config: Config =
            toml::from_str(&content).map_err(|e| format!("Failed to parse Config.toml: {}", e))?;
        config.normalized_public_origin()?;
        Ok(config)
    }

    pub fn normalized_public_origin(&self) -> Result<String, String> {
        let raw = self.server.public_origin.trim();
        if raw.is_empty() {
            return Ok(String::new());
        }
        match normalize_https_url(raw) {
            Some(url) => Ok(url.trim_end_matches('/').to_string()),
            None => Err(format!(
                "[server].public_origin is not an https URL ({raw})"
            )),
        }
    }

    pub fn load() -> Result<Self, String> {
        Self::load_from(Path::new("Config.toml"))
    }

    pub fn load_with_logging() -> Result<Self, String> {
        match Self::load() {
            Ok(config) => {
                println!("Nostorgraph: ✅ Configuration loaded successfully");
                println!("   Title limit: {} chars", config.limits.title_max_length);
                println!("   Alias limit: {} chars", config.limits.alias_max_length);
                println!(
                    "   Content limit: {} chars",
                    config.limits.content_max_length
                );
                Ok(config)
            }
            Err(error) => {
                eprintln!("Nostorgraph: ⚠️  Configuration error: {}", error);
                Err(error)
            }
        }
    }

    pub fn form_data_limit_bytes(&self) -> u32 {
        let text = self
            .limits
            .content_max_length
            .saturating_add(self.limits.title_max_length)
            .saturating_add(self.limits.alias_max_length);
        let bytes = text.saturating_mul(4).saturating_add(4096);
        u32::try_from(bytes).unwrap_or(u32::MAX)
    }

    pub fn listen_address(&self, override_addr: Option<&str>) -> Result<IpAddr, String> {
        let configured = self.server.address.trim();
        let chosen = override_addr
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(configured);
        chosen
            .parse()
            .map_err(|_| format!("listen address is not an IP address ({chosen})"))
    }

    /// `Ok(None)` connects directly. `Err` means `socks` was set but is not a socket address.
    pub fn socks_addr(&self) -> Result<Option<SocketAddr>, ()> {
        let trimmed = self.nostr.socks.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        match trimmed.parse() {
            Ok(addr) => Ok(Some(addr)),
            Err(_) => Err(()),
        }
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

        if title.chars().count() > self.limits.title_max_length {
            return Err("title_too_long".to_string());
        }

        if content.chars().count() > self.limits.content_max_length {
            return Err("content_too_long".to_string());
        }

        if let Some(alias) = alias {
            if alias.chars().count() > self.limits.alias_max_length {
                return Err("alias_too_long".to_string());
            }
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "../test/config.rs"]
mod tests;
