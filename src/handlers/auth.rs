use actix_web::http::header::LOCATION;
use actix_web::{get, post, web, HttpRequest, HttpResponse, Responder};
use maud::{html, Markup, DOCTYPE};
use serde::Deserialize;

use crate::auth::{self, configured_credentials, JwtKeys};

#[derive(Deserialize)]
pub struct LoginQuery {
    pub next: Option<String>,
    pub error: Option<String>,
}

#[derive(Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
    pub next: Option<String>,
}

#[get("/login")]
pub async fn login_form(query: web::Query<LoginQuery>) -> impl Responder {
    let next = sanitize_next(query.next.as_deref());
    let error = query.error.as_deref();
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(login_page(&next, error).into_string())
}

#[post("/login")]
pub async fn login_submit(req: HttpRequest, form: web::Form<LoginForm>) -> impl Responder {
    let LoginForm {
        username,
        password,
        next,
    } = form.into_inner();
    let safe_next = sanitize_next(next.as_deref());

    let (expected_user, expected_pass) = match configured_credentials() {
        Some(c) => c,
        None => return HttpResponse::InternalServerError().body("auth not configured"),
    };

    if username != expected_user || password != expected_pass {
        let url = format!(
            "/login?next={}&error=invalid",
            urlencoding::encode(&safe_next)
        );
        return HttpResponse::Found()
            .insert_header((LOCATION, url))
            .finish();
    }

    let keys = match JwtKeys::from_password(&expected_pass) {
        Some(k) => k,
        None => return HttpResponse::InternalServerError().body("auth misconfigured"),
    };
    let token = match auth::issue_token(&keys, &username) {
        Ok(t) => t,
        Err(_) => return HttpResponse::InternalServerError().body("token issue failed"),
    };

    let secure = req.connection_info().scheme() == "https";
    let cookie = auth::build_session_cookie(token, secure);

    HttpResponse::Found()
        .cookie(cookie)
        .insert_header((LOCATION, safe_next))
        .finish()
}

#[post("/logout")]
pub async fn logout(req: HttpRequest) -> impl Responder {
    let secure = req.connection_info().scheme() == "https";
    let clear = auth::clear_session_cookie(secure);
    HttpResponse::Found()
        .cookie(clear)
        .insert_header((LOCATION, "/login"))
        .finish()
}

fn sanitize_next(next: Option<&str>) -> String {
    match next {
        Some(n) if n.starts_with('/') && !n.starts_with("//") => n.to_string(),
        _ => "/".to_string(),
    }
}

fn login_page(next: &str, error: Option<&str>) -> Markup {
    html! {
        (DOCTYPE)
        html {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Sign in — Site Server" }
                link rel="stylesheet" href="/res/styles.css";
            }
            body {
                main style="max-width: 24rem; margin: 4rem auto; padding: 0 1rem;" {
                    h1 { "Site Server" }
                    @if error.is_some() {
                        p style="color: #e66;" { "Invalid username or password." }
                    }
                    form method="post" action="/login" {
                        input type="hidden" name="next" value=(next);
                        p {
                            label for="username" { "Username" }
                            br;
                            input type="text" id="username" name="username"
                                  autocomplete="username" required style="width: 100%;";
                        }
                        p {
                            label for="password" { "Password" }
                            br;
                            input type="password" id="password" name="password"
                                  autocomplete="current-password" required style="width: 100%;";
                        }
                        p { button type="submit" { "Sign in" } }
                    }
                }
            }
        }
    }
}
