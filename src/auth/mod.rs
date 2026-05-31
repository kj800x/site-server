pub mod basic;
pub mod cookie;
pub mod jwt;
pub mod middleware;

pub use cookie::{build_session_cookie, clear_session_cookie, COOKIE_NAME};
pub use jwt::{issue_token, validate_token, JwtKeys};
pub use middleware::validator;

/// Read configured username/password from env. Returns None if either is unset/empty.
pub fn configured_credentials() -> Option<(String, String)> {
    let user = std::env::var("BASIC_AUTH_USERNAME")
        .ok()
        .filter(|s| !s.is_empty())?;
    let pass = std::env::var("BASIC_AUTH_PASSWORD")
        .ok()
        .filter(|s| !s.is_empty())?;
    Some((user, pass))
}
