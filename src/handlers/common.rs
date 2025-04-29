use actix_web::web;
use actix_web_httpauth::extractors::basic::{BasicAuth, Config};
use actix_web_httpauth::extractors::AuthenticationError;

// Authentication validator function
pub async fn validator(
    req: actix_web::dev::ServiceRequest,
    credentials: Option<BasicAuth>,
) -> Result<actix_web::dev::ServiceRequest, (actix_web::Error, actix_web::dev::ServiceRequest)> {
    // Allow metrics and healthz requests to pass through
    if req.path() == "/api/metrics" || req.path() == "/healthz" {
        return Ok(req);
    }

    // Get auth credentials from environment
    let expected_username = std::env::var("BASIC_AUTH_USERNAME").unwrap_or_default();
    let expected_password = std::env::var("BASIC_AUTH_PASSWORD").unwrap_or_default();

    // If auth environment variables are not set, don't enforce authentication
    if expected_username.is_empty() || expected_password.is_empty() {
        return Ok(req);
    }

    let credentials = if let Some(credentials) = credentials {
        credentials
    } else {
        return Err((
            actix_web::error::ErrorBadRequest("no basic auth header"),
            req,
        ));
    };

    // Check if credentials match
    let password = credentials.password().unwrap_or_default();
    if credentials.user_id() == expected_username && password == expected_password {
        Ok(req)
    } else {
        // Return 401 Unauthorized with proper WWW-Authenticate header
        let config = req
            .app_data::<Config>()
            .cloned()
            .unwrap_or_default()
            .realm("Site Server");

        Err((AuthenticationError::from(config).into(), req))
    }
}

// Common error response for locked workdir
pub fn workdir_locked_error() -> actix_web::Error {
    actix_web::Error::from(actix_web::error::ErrorServiceUnavailable(
        "Work directory is locked",
    ))
}

// Helper function to get workdir from ThreadSafeWorkDir
pub fn get_workdir<'a>(
    workdir: &'a web::Data<super::ThreadSafeWorkDir>,
) -> Result<std::sync::RwLockReadGuard<'a, crate::workdir::WorkDir>, actix_web::Error> {
    let workdir_lock = workdir.work_dir.try_read();
    match workdir_lock {
        Ok(x) => Ok(x),
        Err(_) => Err(workdir_locked_error()),
    }
}

pub fn date_time_element(timestamp: Option<u64>) -> maud::Markup {
    use chrono::{TimeZone, Utc};
    use maud::html;

    if let Some(ts) = timestamp {
        let time = Utc.timestamp_millis_opt(ts as i64).unwrap();
        html! {
            time datetime=(time.to_rfc3339()) title=(time.to_rfc3339()) {
                (time.format("%B %d, %Y"))
            }
        }
    } else {
        html! {
            span { "never" }
        }
    }
}
