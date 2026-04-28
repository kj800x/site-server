use actix_web::cookie::{time::Duration, Cookie, SameSite};

pub const COOKIE_NAME: &str = "site_server_session";

pub fn build_session_cookie<'a>(token: String, secure: bool) -> Cookie<'a> {
    Cookie::build(COOKIE_NAME, token)
        .http_only(true)
        .same_site(SameSite::Lax)
        .path("/")
        .secure(secure)
        .max_age(Duration::days(30))
        .finish()
}

pub fn clear_session_cookie<'a>(secure: bool) -> Cookie<'a> {
    Cookie::build(COOKIE_NAME, "")
        .http_only(true)
        .same_site(SameSite::Lax)
        .path("/")
        .secure(secure)
        .max_age(Duration::ZERO)
        .finish()
}
