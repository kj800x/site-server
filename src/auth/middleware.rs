use actix_web::dev::ServiceRequest;
use actix_web::http::header::{AUTHORIZATION, LOCATION};
use actix_web::HttpResponse;
use actix_web_httpauth::extractors::basic::{BasicAuth, Config as BasicConfig};
use actix_web_httpauth::extractors::AuthenticationError;

use super::{basic, configured_credentials, cookie as cookie_mod, jwt};

pub async fn validator(
    req: ServiceRequest,
    credentials: Option<BasicAuth>,
) -> Result<ServiceRequest, (actix_web::Error, ServiceRequest)> {
    if is_public_path(req.path()) {
        return Ok(req);
    }

    let (expected_user, expected_pass) = match configured_credentials() {
        Some(c) => c,
        None => return Ok(req),
    };

    if let Some(creds) = credentials.as_ref() {
        if basic::check(creds, &expected_user, &expected_pass) {
            return Ok(req);
        }
        return Err((unauthorized_error(), req));
    }

    if let Some(cookie) = req.cookie(cookie_mod::COOKIE_NAME) {
        if let Some(keys) = jwt::JwtKeys::from_password(&expected_pass) {
            if jwt::validate_token(&keys, cookie.value()).is_ok() {
                return Ok(req);
            }
        }
    }

    if req.headers().contains_key(AUTHORIZATION) {
        return Err((unauthorized_error(), req));
    }

    let redirect = redirect_to_login(req.path(), req.query_string());
    Err((redirect, req))
}

fn is_public_path(path: &str) -> bool {
    if path == "/healthz"
        || path == "/api/metrics"
        || path == "/login"
        || path == "/logout"
    {
        return true;
    }
    if path.starts_with("/res/") {
        return true;
    }
    false
}

fn redirect_to_login(path: &str, query: &str) -> actix_web::Error {
    let next = if query.is_empty() {
        path.to_string()
    } else {
        format!("{}?{}", path, query)
    };
    let location = format!("/login?next={}", urlencoding::encode(&next));
    let resp = HttpResponse::Found()
        .insert_header((LOCATION, location))
        .finish();
    actix_web::error::InternalError::from_response("", resp).into()
}

fn unauthorized_error() -> actix_web::Error {
    AuthenticationError::from(BasicConfig::default().realm("Site Server")).into()
}
