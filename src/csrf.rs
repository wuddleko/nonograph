use rocket::request::{FromRequest, Outcome, Request};

pub struct CsrfProtected;

#[rocket::async_trait]
impl<'r> FromRequest<'r> for CsrfProtected {
    type Error = ();

    async fn from_request(_request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        Outcome::Success(CsrfProtected)
    }
}

pub fn generate_csrf_token_with_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let random_part = generate_csrf_token();
    let combined = format!("{timestamp}:{random_part}");

    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    combined.hash(&mut hasher);
    let hash = hasher.finish();

    format!("{combined}.{hash:x}")
}

pub fn is_valid_csrf_token(token: &str) -> bool {
    if token.is_empty() {
        return false;
    }

    // Split token into data and hash parts
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 2 {
        return false;
    }

    let data = parts[0];
    let provided_hash = parts[1];

    // Recreate hash from data
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    data.hash(&mut hasher);
    let expected_hash = format!("{:x}", hasher.finish());

    // Verify hash matches
    if provided_hash != expected_hash {
        return false;
    }

    // Check timestamp (token expires after 1 hour)
    let data_parts: Vec<&str> = data.split(':').collect();
    if data_parts.len() != 2 {
        return false;
    }

    if let Ok(timestamp) = data_parts[0].parse::<u64>() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Token is valid for 24 hours
        current_time - timestamp < 86400
    } else {
        false
    }
}

fn generate_csrf_token() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..32)
        .map(|_| format!("{:02x}", rng.gen::<u8>()))
        .collect::<String>()
}

#[cfg(test)]
#[path = "../test/csrf.rs"]
mod tests;
