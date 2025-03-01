use actix_files::Files;
use actix_session::{storage::CookieSessionStore, SessionMiddleware};
use actix_web::{
    cookie::Key,
    get, middleware,
    web::{self},
    App, HttpServer, Responder,
};
use actix_web::{Either, HttpResponse};
use actix_web_httpauth::extractors::basic::{BasicAuth, Config};
use actix_web_httpauth::extractors::AuthenticationError;
use actix_web_httpauth::middleware::HttpAuthentication;
use actix_web_opentelemetry::{PrometheusMetricsHandler, RequestMetrics, RequestTracing};
use chrono::{TimeZone, Utc};
use clap::Parser;
use maud::{html, Markup, Render};
use opentelemetry::global;
use opentelemetry_sdk::metrics::MeterProvider;
use rand::seq::IteratorRandom;
use site::CrawlItem;
use std::{collections::HashMap, fs::File, io::Read, path::Path};
use urlencoding::{decode, encode};
use workdir::WorkDir;

mod collections;
mod errors;
mod serde;
mod site;
mod workdir;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

struct StartTime(i64);
struct WorkDirPrefix(String);

#[derive(clap::Subcommand)]
enum Commands {
    Serve { work_dirs: Vec<String> },
}

/// Links to a CSS stylesheet at the given path.
struct Css(&'static str);

impl Render for Css {
    fn render(&self) -> Markup {
        html! {
            link rel="stylesheet" type="text/css" href=(self.0);
        }
    }
}

/// Links to a JS source file at the given path.
struct Js(&'static str);

impl Render for Js {
    fn render(&self) -> Markup {
        html! {
            script type="text/javascript" src=(self.0) {}
        }
    }
}

macro_rules! serve_static_file {
    ($file:expr) => {
        web::resource(concat!("res/", $file)).route(web::get().to(|| async move {
            let path = Path::new("src/res").join($file);

            if path.exists() && path.is_file() {
                let mut file = File::open(path).unwrap();
                let mut contents = String::new();
                file.read_to_string(&mut contents).unwrap();
                HttpResponse::Ok()
                    .append_header(("x-resource-source", "disk"))
                    .body(contents)
            } else {
                HttpResponse::Ok()
                    .append_header(("x-resource-source", "embedded"))
                    .body(include_str!(concat!("res/", $file)))
            }
        }))
    };
}

// #[derive(Debug, Serialize, Deserialize)]
// struct HydratedClass {
//     id: i64,
//     name: String,
//     latest: Option<EventResult>,
// }

// #[get("/api/class")]
// async fn event_class_listing(
//     pool: web::Data<Pool<SqliteConnectionManager>>,
//     user_id: UserId,
// ) -> Result<impl Responder, actix_web::Error> {
//     let classes = get_classes(&pool, user_id.into_inner()?).await.unwrap();
//     Ok(web::Json(classes))
// }

// #[get("/api/ui/homepage")]
// async fn home_page_omnibus(
//     pool: web::Data<Pool<SqliteConnectionManager>>,
//     user_id: UserId,
// ) -> Result<impl Responder, actix_web::Error> {
//     let uid = user_id.into_inner()?;

//     let classes = get_classes(&pool, uid).await.unwrap();

//     let hydrated_classes: Vec<HydratedClass> = join_all(classes.iter().map(|x| async {
//         HydratedClass {
//             id: x.id,
//             name: x.name.clone(),
//             latest: get_latest_event(&pool, x.id, uid).await.unwrap(),
//         }
//     }))
//     .await;

//     Ok(web::Json(hydrated_classes))
// }

// #[derive(Debug, Serialize, Deserialize)]
// struct StatsResponse {
//     class: ClassResult,
//     events: Vec<EventResult>,
// }

fn date_time_element(timestamp: Option<u64>) -> Markup {
    match timestamp {
        Some(x) => {
            let time = Utc.timestamp_millis_opt(x as i64).unwrap();

            html! {
                time datetime=(time.to_rfc3339()) {
                    (time.to_string())
                }
            }
        }
        None => {
            html! {
                b {
                    "None"
                }
            }
        }
    }
}

#[get("/info")]
async fn info_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<WorkDir>,
    start_time: web::Data<StartTime>,
) -> Result<impl Responder, actix_web::Error> {
    let latest_update = workdir.crawled.items.values().map(|x| x.last_seen).max();
    let first_update = workdir.crawled.items.values().map(|x| x.first_seen).min();

    return Ok(html! {
        (Css("/res/styles.css"))
        h1 { "Hello, world!" }
        // p.intro {
        //     "This is an example of the "
        //     a href="https://github.com/lambda-fairy/maud" { "Maud" }
        //     " template language."
        // }
        p {
            "The current site is: "
            code { (site.0) }
        }
        p {
            "The first update was on "
            (date_time_element(first_update))
        }
        p {
            "The latest update was on "
            (date_time_element(latest_update))
        }
        p {
            "The site server was started on "
            (date_time_element(Some(start_time.0.try_into().unwrap())))
        }
        p {
            "This site has " (workdir.crawled.iter().count()) " items"
        }
    });
}

fn paginator(page: usize, total: usize, per_page: usize, prefix: &str) -> Markup {
    let pages = (total + per_page - 1) / per_page;
    let mut links = vec![];

    if page > 1 {
        links.push(html! {
            a href=(format!("{}/{}", prefix, page - 1)) { "<" }
        });
    }

    for i in 1..=pages {
        if i == page {
            links.push(html! {
                span { (i) }
            });
        } else if (i as isize - page as isize).abs() < 5 {
            links.push(html! {
                a href=(format!("{}/{}", prefix, i)) { (i) }
            });
        }
    }

    if page < pages {
        links.push(html! {
            a href=(format!("{}/{}", prefix, page + 1)) { ">" }
        });
    }

    return html! {
        .paginator {
            @for link in &links {
                (link)
            }
        }
    };
}

fn item_thumbnail(item: &CrawlItem, site: &str) -> Markup {
    html! {
        a.item_thumb_container href=(format!("/{}/item/{}/{}", site, encode(&item.key), encode(item.flat_files().keys().into_iter().next().unwrap_or(&"".to_string())))) {
            .item_thumb_img {
                @if let Some(thumb) = item.thumbnail_path() {
                    img src=(format!("/{}/assets/{}", site, thumb)) {}
                } @else {
                    p.no_thumbnail { "No thumbnail" }
                }
            }
            .item_thumb_tags {
                @for tag in &item.tags {
                    @match tag {
                        site::CrawlTag::Simple(x) => .tag { (x) },
                        site::CrawlTag::Detailed { value, .. } => .tag { (value) },
                    }
                }
            }
        }
    }
}

#[get("/")]
async fn root_index_handler(
    site: web::Data<Vec<WorkDir>>,
) -> Result<impl Responder, actix_web::Error> {
    return Ok(html! {
        (Css("/res/styles.css"))
        h1.page_title { "Loaded sites" }
        ul.site_list {
            @for site in site.iter() {
                li {
                    a.site_link href=(format!("/{}/latest", site.config.slug)) { (site.config.label) }
                    " ("
                    a.site_link href=(format!("/{}/info", site.config.slug)) { "info" }
                    ")"
                }
            }
        }
    });
}

#[get("/random")]
async fn random_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<WorkDir>,
) -> Result<impl Responder, actix_web::Error> {
    let rng = &mut rand::thread_rng();
    let items = workdir
        .crawled
        .items
        .values()
        .into_iter()
        .choose_multiple(rng, 40);

    return Ok(html! {
        (Css("/res/styles.css"))
        h1.page_title { "Random items" }
        .item_thumb_grid {
            @for item in &items {
                ( item_thumbnail(&item, &site.0) )
            }
        }
        .paginator {
            a href=(format!("/{}/random", &site.0)) { "See more" }
        }
    });
}

async fn generic_latest_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<WorkDir>,
    path: web::Path<usize>,
) -> Result<impl Responder, actix_web::Error> {
    let page = path.into_inner();
    let items: Vec<&CrawlItem> = workdir
        .crawled
        .items
        .values()
        .into_iter()
        .skip((page - 1) * 40)
        .take(40)
        .collect();

    return Ok(html! {
        (Css("/res/styles.css"))
        h1.page_title { "Latest items" }
        ( paginator(page, workdir.crawled.items.len(), 40, &format!("/{}/latest", &site.0)) )
        .item_thumb_grid {
            @for item in &items {
                ( item_thumbnail(&item, &site.0) )
            }
        }
        ( paginator(page, workdir.crawled.items.len(), 40, &format!("/{}/latest", &site.0)) )
    });
}

#[get("/latest/{page}")]
async fn latest_page_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<WorkDir>,
    path: web::Path<usize>,
) -> Result<impl Responder, actix_web::Error> {
    generic_latest_handler(site, workdir, path).await
}

#[get("/latest")]
async fn latest_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<WorkDir>,
) -> Result<impl Responder, actix_web::Error> {
    generic_latest_handler(site, workdir, web::Path::from(1)).await
}

async fn generic_oldest_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<WorkDir>,
    path: web::Path<usize>,
) -> Result<impl Responder, actix_web::Error> {
    let page = path.into_inner();
    let items: Vec<&CrawlItem> = workdir
        .crawled
        .items
        .values()
        .rev()
        .into_iter()
        .skip((page - 1) * 40)
        .take(40)
        .collect();

    return Ok(html! {
        (Css("/res/styles.css"))
        h1.page_title { "Oldest items" }
        ( paginator(page, workdir.crawled.items.len(), 40, &format!("/{}/oldest", &site.0)) )
        .item_thumb_grid {
            @for item in &items {
                ( item_thumbnail(&item, &site.0) )
            }
        }
        ( paginator(page, workdir.crawled.items.len(), 40, &format!("/{}/oldest", &site.0)) )
    });
}

#[get("/oldest/{page}")]
async fn oldest_page_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<WorkDir>,
    path: web::Path<usize>,
) -> Result<impl Responder, actix_web::Error> {
    generic_oldest_handler(site, workdir, path).await
}

#[get("/oldest")]
async fn oldest_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<WorkDir>,
) -> Result<impl Responder, actix_web::Error> {
    generic_oldest_handler(site, workdir, web::Path::from(1)).await
}

async fn generic_tag_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<WorkDir>,
    tag: String,
    page: usize,
) -> Result<impl Responder, actix_web::Error> {
    let filtered_items = workdir
        .crawled
        .items
        .values()
        .into_iter()
        .filter(|item| item.tags.iter().any(|x| x.to_string() == tag))
        .collect::<Vec<&CrawlItem>>();

    let filtered_items_len = filtered_items.len();

    let items: Vec<&CrawlItem> = filtered_items
        .into_iter()
        .skip((page - 1) * 40)
        .take(40)
        .collect();

    return Ok(html! {
        (Css("/res/styles.css"))
        h1.page_title { "Items tagged \"" (tag) "\"" }
        ( paginator(page, filtered_items_len, 40, &format!("/{}/tag/{}", &site.0, encode(&tag))) )
        .item_thumb_grid {
            @for item in &items {
                ( item_thumbnail(&item, &site.0) )
            }
        }
        ( paginator(page, filtered_items_len, 40, &format!("/{}/tag/{}", &site.0, encode(&tag))) )
    });
}

#[get("/tag/{tag}/{page}")]
async fn tag_page_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<WorkDir>,
    path: web::Path<(String, usize)>,
) -> Result<impl Responder, actix_web::Error> {
    let (tag, page) = path.into_inner();
    generic_tag_handler(site, workdir, decode(&tag).unwrap().into_owned(), page).await
}

#[get("/tag/{tag}")]
async fn tag_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<WorkDir>,
    path: web::Path<String>,
) -> Result<impl Responder, actix_web::Error> {
    let tag = path.into_inner();
    generic_tag_handler(site, workdir, decode(&tag).unwrap().into_owned(), 1).await
}

#[get("/tags")]
async fn tags_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<WorkDir>,
) -> Result<impl Responder, actix_web::Error> {
    let mut tags: HashMap<String, usize> = HashMap::new();

    for item in workdir.crawled.items.values() {
        for tag in &item.tags {
            let tag = match tag {
                site::CrawlTag::Simple(x) => x,
                site::CrawlTag::Detailed { value, .. } => value,
            };

            *tags.entry(tag.clone()).or_insert(0) += 1;
        }
    }

    let mut tags_vec: Vec<_> = tags.into_iter().collect();
    tags_vec.sort_by(|a, b| b.1.cmp(&a.1));

    return Ok(html! {
        (Css("/res/styles.css"))
        h1.page_title { "Site tags" }
        ul.tag_list {
            @for tag in &tags_vec {
                li {
                    a.tag_link href=(format!("/{}/tag/{}", site.0, encode(&tag.0))) { (tag.0) " (" (tag.1) ")" }
                }
            }
        }
    });
}

#[get("")]
async fn root_redirect(site: web::Data<WorkDirPrefix>) -> Result<HttpResponse, actix_web::Error> {
    Ok(HttpResponse::SeeOther()
        .append_header(("Location", format!("/{}/latest", site.0)))
        .finish())
}

#[get("/item/{item}")]
async fn item_redirect(
    path: web::Path<String>,
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<WorkDir>,
) -> Result<Either<impl Responder, HttpResponse>, actix_web::Error> {
    let item = workdir
        .crawled
        .items
        .get(&decode(&path).unwrap().into_owned());

    let Some(item) = item else {
        return Ok(Either::Left(html! {
            (Css("/res/styles.css"))
            h1 { "Hello!" }
            p { "Item not found" }
        }));
    };

    let file = item.files.keys().into_iter().next();

    let Some(file_key) = file else {
        return Ok(Either::Left(html! {
            (Css("/res/styles.css"))
            h1 { "Hello!" }
            p { "Item had no files" }
        }));
    };

    Ok(Either::Right(
        actix_web::HttpResponse::SeeOther()
            .append_header((
                "Location",
                format!(
                    "/{}/item/{}/{}",
                    site.0,
                    encode(&item.key),
                    encode(file_key)
                ),
            ))
            .finish(),
    ))
}

#[get("/item/{item}/{file}")]
async fn item_handler(
    path: web::Path<(String, String)>,
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<WorkDir>,
) -> Result<impl Responder, actix_web::Error> {
    let item = workdir
        .crawled
        .items
        .get(&decode(path.0.as_str()).unwrap().into_owned());

    let Some(item) = item else {
        return Ok(html! {
            (Css("/res/styles.css"))
            h1 { "Hello!" }
            p { "Item not found" }
        });
    };

    let files = item.flat_files();
    let file = files.get(&decode(path.1.as_str()).unwrap().into_owned());

    // Find filename of next and previous file
    let file_keys = files.keys().collect::<Vec<&String>>();
    let file_index = file_keys.iter().position(|x| **x == path.1);
    let prev_file = file_index.and_then(|index| file_keys.get(index.wrapping_sub(1)).map(|x| *x));
    let next_file = file_index.and_then(|index| file_keys.get(index.wrapping_add(1)).map(|x| *x));

    return Ok(html! {
        (Css("/res/styles.css"))
        (Js("/res/detail_page.js"))
        .item_detail_page_container {
            .item_detail_page_sidebar {
                dt { "Source" }
                // FIXME: Fancy linking, show only the domain name
                dd { (item.url) }
                dt { "Data Directory"}
                dd { (site.0) }
                dt { "Title" }
                dd { (item.title) }
                dt { "Description" }
                dd { (item.description) }
                dt { "Published" }
                // TODO FIXME: date_time_element should take in i64
                // FIXME: These would be nice as timeago style strings
                dd { (date_time_element(Some(item.source_published as u64))) }
                dt { "First Seen" }
                dd { (date_time_element(Some(item.first_seen))) }
                dt { "Last Seen" }
                dd { (date_time_element(Some(item.last_seen))) }

                dt { "Item Key" }
                dd { (item.key) }

                dt { "Files" }
                dd {
                    @for file in &files {
                        @if *file.0 == path.1 {
                            span.file_link { (file.0) }
                        } @else {
                            a.file_link href=(format!("/{}/item/{}/{}", site.0, encode(&item.key), encode(file.0))) "data-is-prev"[prev_file.is_some_and(|x| x == file.0)] "data-is-next"[next_file.is_some_and(|x| x == file.0)] { (file.0) }
                        }
                    }
                }

                // TODO: Dynamically insert item.meta here

                // FIXME: How do we handle no-tags
                dt { "Tags" }
                dd {
                    @for tag in &item.tags {
                        @match tag {
                            site::CrawlTag::Simple(x) => a.tag href=(format!("/{}/tag/{}", site.0, encode(x))) { (x) },
                            site::CrawlTag::Detailed { value, .. } => a.tag href=(format!("/{}/tag/{}", site.0, encode(value))) { (value) },
                        }
                    }
                }
            }
            .item_detail_page_file {
                @match file {
                    None => {
                        p.file_not_found { "File not found" }
                    }
                    Some(file) => {
                        @match file {
                            site::FileCrawlType::Image { filename, downloaded, .. } => {
                                @if *downloaded {
                                    img src=(format!("/{}/assets/{}", site.0, filename)) {}
                                } @else {
                                    p { "Image not downloaded" }
                                }
                            }
                            site::FileCrawlType::Video { filename, downloaded, .. } => {
                                @if *downloaded {
                                    // Replace the extension with mp4
                                    // When we download, we convert everything to mp4
                                    @let coerced_filename = filename.split('.').next().unwrap_or("").to_string() + ".mp4";
                                    video autoplay controls {
                                        source src=(format!("/{}/assets/{}", site.0, coerced_filename)) {}
                                    }
                                } @else {
                                    p { "Video not downloaded" }
                                }
                            }
                            site::FileCrawlType::Intermediate { .. } => {
                                p { "Intermediate file, no preview available" }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    });
}

// #[get("/assets/{tail:.*}")]
// async fn assets_handler(
//     path: web::Path<String>,
//     work_dir: web::Data<WorkDir>,
// ) -> Result<impl Responder, actix_web::Error> {
//     let workdir = work_dir.clone();
//     let path = path.into_inner();

//     let asset = workdir.path.join("assets").join(path);

//     Ok(actix_files::NamedFile::open(asset)?)
// }

// #[post("/api/class")]
// async fn event_class_create(
//     create_class: Json<CreateClass>,
//     pool: web::Data<Pool<SqliteConnectionManager>>,
//     user_id: UserId,
// ) -> Result<impl Responder, actix_web::Error> {
//     let class = insert_class(&pool, create_class.into_inner(), user_id.into_inner()?)
//         .await
//         .unwrap();
//     Ok(web::Json(class))
// }

// #[put("/api/class/{id}")]
// async fn event_class_update(
//     create_class: Json<CreateClass>,
//     id: web::Path<i64>,
//     pool: web::Data<Pool<SqliteConnectionManager>>,
//     user_id: UserId,
// ) -> Result<impl Responder, actix_web::Error> {
//     let class = update_class(
//         &pool,
//         id.into_inner(),
//         create_class.into_inner(),
//         user_id.into_inner()?,
//     )
//     .await
//     .unwrap();
//     Ok(web::Json(class))
// }

// #[delete("/api/class/{id}")]
// async fn event_class_delete(
//     id: web::Path<i64>,
//     pool: web::Data<Pool<SqliteConnectionManager>>,
//     user_id: UserId,
// ) -> Result<impl Responder, actix_web::Error> {
//     delete_class(&pool, id.into_inner(), user_id.into_inner()?)
//         .await
//         .unwrap();

//     Ok(HttpResponse::NoContent())
// }

// #[get("/api/class/{class_id}/events")]
// async fn event_class_events(
//     class_id: web::Path<i64>,
//     pool: web::Data<Pool<SqliteConnectionManager>>,
//     user_id: UserId,
// ) -> Result<impl Responder, actix_web::Error> {
//     let events = get_events(&pool, class_id.into_inner(), user_id.into_inner()?)
//         .await
//         .unwrap();
//     Ok(web::Json(events))
// }

// #[post("/api/class/{class_id}/events")]
// async fn record_event(
//     create_event: Json<CreateEvent>,
//     class_id: web::Path<i64>,
//     pool: web::Data<Pool<SqliteConnectionManager>>,
//     user_id: UserId,
// ) -> Result<impl Responder, actix_web::Error> {
//     let event = insert_event(
//         &pool,
//         class_id.into_inner(),
//         create_event.into_inner(),
//         user_id.into_inner()?,
//     )
//     .await
//     .unwrap();
//     Ok(web::Json(event))
// }

// #[delete("/api/class/{class_id}/event/{event_id}")]
// async fn delete_event(
//     path_params: web::Path<(i64, i64)>,
//     pool: web::Data<Pool<SqliteConnectionManager>>,
//     user_id: UserId,
// ) -> Result<impl Responder, actix_web::Error> {
//     let (class_id, event_id) = path_params.into_inner();

//     db_delete_event(&pool, class_id, event_id, user_id.into_inner()?)
//         .await
//         .unwrap();

//     Ok(HttpResponse::NoContent())
// }

// #[get("/api/class/{class_id}/events/latest")]
// async fn event_class_latest_event(
//     class_id: web::Path<i64>,
//     pool: web::Data<Pool<SqliteConnectionManager>>,
//     user_id: UserId,
// ) -> Result<impl Responder, actix_web::Error> {
//     let event = get_latest_event(&pool, class_id.into_inner(), user_id.into_inner()?)
//         .await
//         .unwrap();
//     Ok(web::Json(event))
// }

// async fn manual_hello() -> impl Responder {
//     HttpResponse::Ok().body("Hey there!")
// }

// #[get("/api/auth")]
// async fn profile(
//     pool: web::Data<Pool<SqliteConnectionManager>>,
//     user_id: UserId,
// ) -> Result<impl Responder, actix_web::Error> {
//     let profile = fetch_profile(&pool, user_id.into_inner()?).await?;

//     Ok(web::Json(profile))
// }

// #[post("/api/auth")]
// async fn login(
//     login: Json<Login>,
//     pool: web::Data<Pool<SqliteConnectionManager>>,
//     session: Session,
// ) -> Result<impl Responder, actix_web::Error> {
//     let uid = authenticate(&pool, login.username.clone(), &login.password).await?;

//     session.insert("user_id", uid)?;

//     Ok(HttpResponse::Ok().body("Login success!"))
// }

// #[post("/api/auth/register")]
// async fn register(
//     registration: Json<Registration>,
//     pool: web::Data<Pool<SqliteConnectionManager>>,
//     session: Session,
// ) -> Result<impl Responder, actix_web::Error> {
//     if std::env::var("ALLOW_REGISTRATION").unwrap_or("false".to_string()) == "false" {
//         return Ok(HttpResponse::Forbidden().body("Registration is disabled"));
//     }

//     let reg = registration.into_inner();

//     if reg.username.trim().is_empty() {
//         return Err(error::ErrorBadRequest("username cannot be empty"));
//     }
//     if reg.password.trim().is_empty() {
//         return Err(error::ErrorBadRequest("password cannot be empty"));
//     }
//     if reg.name.trim().is_empty() {
//         return Err(error::ErrorBadRequest("name cannot be empty"));
//     }

//     let uid = sign_up(&pool, reg).await?;

//     session.insert("user_id", uid)?;

//     Ok(HttpResponse::Ok().body("Registration success!"))
// }

// #[delete("/api/auth")]
// async fn logout(session: Session) -> impl Responder {
//     session.clear();

//     HttpResponse::Ok().body("Logout success!")
// }

async fn validator(
    req: actix_web::dev::ServiceRequest,
    credentials: BasicAuth,
) -> Result<actix_web::dev::ServiceRequest, (actix_web::Error, actix_web::dev::ServiceRequest)> {
    // Get auth credentials from environment
    let expected_username = std::env::var("BASIC_AUTH_USERNAME").unwrap_or_default();
    let expected_password = std::env::var("BASIC_AUTH_PASSWORD").unwrap_or_default();

    // If auth environment variables are not set, don't enforce authentication
    if expected_username.is_empty() || expected_password.is_empty() {
        return Ok(req);
    }

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

async fn run() -> crate::errors::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let cli = Cli::parse();

    match &cli.command {
        Commands::Serve { work_dirs } => {
            println!("Loading WorkDirs...");
            let mut work_dirs_vec = vec![];
            for work_dir in work_dirs.into_iter() {
                println!("Loading WorkDir: {}", work_dir);
                let work_dir = WorkDir::new(work_dir.to_string()).expect("Failed to load WorkDir");
                work_dirs_vec.push(work_dir);
            }

            let registry = prometheus::Registry::new();
            let exporter = opentelemetry_prometheus::exporter()
                .with_registry(registry.clone())
                .build()
                .unwrap();
            let provider = MeterProvider::builder().with_reader(exporter).build();
            global::set_meter_provider(provider);

            let listen_address = std::env::var("LISTEN_ADDRESS").unwrap_or("127.0.0.1".to_owned());

            log::info!("Starting HTTP server at http://{}:8080", listen_address);

            HttpServer::new(move || {
                let auth = HttpAuthentication::basic(validator);

                let mut app = App::new()
                    .wrap(auth) // Guard all routes with HTTP Basic Auth
                    .wrap(RequestTracing::new())
                    .wrap(RequestMetrics::default())
                    .route(
                        "/api/metrics",
                        web::get().to(PrometheusMetricsHandler::new(registry.clone())),
                    )
                    .wrap(
                        SessionMiddleware::builder(
                            CookieSessionStore::default(),
                            Key::from(&[0; 64]),
                        )
                        .cookie_secure(false)
                        .build(),
                    )
                    .app_data(web::Data::new(work_dirs_vec.clone()))
                    .app_data(web::Data::new(StartTime(Utc::now().timestamp_millis())))
                    .wrap(middleware::Logger::default())
                    .service(serve_static_file!("styles.css"))
                    .service(serve_static_file!("detail_page.js"))
                    .service(root_index_handler);

                for workdir in work_dirs_vec.iter() {
                    app = app.service(
                        web::scope(&workdir.config.slug)
                            .app_data(web::Data::new(workdir.clone()))
                            .app_data(web::Data::new(WorkDirPrefix(workdir.config.slug.clone())))
                            .service(info_handler)
                            .service(random_handler)
                            .service(latest_handler)
                            .service(latest_page_handler)
                            .service(oldest_handler)
                            .service(oldest_page_handler)
                            .service(root_redirect)
                            .service(tags_handler)
                            .service(tag_handler)
                            .service(tag_page_handler)
                            .service(item_handler)
                            .service(item_redirect)
                            .service(Files::new("/assets", workdir.path.clone()).prefer_utf8(true)),
                    );
                }

                app
            })
            .bind((listen_address, 8080))?
            .run()
            .await?;

            Ok(())
        }
    }
}

#[actix_web::main]
async fn main() {
    if let Err(ref _e) = run().await {
        // _e.print();
        ::std::process::exit(1);
    }
}
