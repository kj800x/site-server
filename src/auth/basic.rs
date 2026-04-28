use actix_web_httpauth::extractors::basic::BasicAuth;

pub fn check(creds: &BasicAuth, expected_user: &str, expected_pass: &str) -> bool {
    creds.user_id() == expected_user && creds.password() == Some(expected_pass)
}
