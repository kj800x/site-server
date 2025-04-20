use actix_files as fs;
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
use chrono::{Datelike, Month, TimeZone, Utc};
use clap::Parser;
use indexmap::IndexMap;
use maud::{html, Markup, Render};
use opentelemetry::global;
use opentelemetry_sdk::metrics::MeterProvider;
use rand::seq::IteratorRandom;
use site::CrawlItem;
use std::{
    collections::{BTreeMap, HashMap},
    fs::File,
    io::Read,
    path::Path,
    thread::{self},
    time::Duration,
};
use thread_safe_work_dir::ThreadSafeWorkDir;
use urlencoding::{decode, encode};
use workdir::WorkDir;

mod collections;
mod errors;
mod serde;
mod site;
mod thread_safe_work_dir;
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

// Helper function to get archive data
fn get_archive_data(items: &IndexMap<String, CrawlItem>) -> BTreeMap<(i32, u8), usize> {
    let mut archive: BTreeMap<(i32, u8), usize> = BTreeMap::new();

    for item in items.values() {
        let time = Utc
            .timestamp_millis_opt(item.source_published as i64)
            .unwrap();
        let year = time.year();
        let month = time.month() as u8;
        *archive.entry((year, month)).or_insert(0) += 1;
    }

    archive
}

// Blogger style components
fn blogger_layout(title: &str, content: Markup, site: &str, workdir: &WorkDir) -> Markup {
    html! {
        (Css("/res/styles.css"))
        .blogger_layout {
            header.blogger_header {
                h1.blog_title { (workdir.config.label) }
                nav.blog_nav {
                    a href=(format!("/{}/blog", site)) { "Home" }
                    a href=(format!("/{}/blog/tags", site)) { "Tags" }
                    a href=(format!("/{}/blog/archive", site)) { "Archive" }
                }
            }
            .blogger_content {
                main.blog_main {
                    @if !title.is_empty() {
                        h2.page_title { (title) }
                    }
                    (content)
                }
                aside.blog_sidebar {
                    (blogger_tags_card(site, workdir))
                    (blogger_archive_card(site, workdir))
                }
            }
        }
    }
}

fn blogger_tags_card(site: &str, workdir: &WorkDir) -> Markup {
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

    html! {
        .blog_card.tags_card {
            h3 { "Tags" }
            ul.blog_tag_list {
                @for (tag, count) in tags_vec {
                    li {
                        a href=(format!("/{}/blog/tag/{}", site, encode(&tag))) {
                            span.tag_name { (tag) }
                            span.tag_count { "(" (count) ")" }
                        }
                    }
                }
            }
        }
    }
}

fn blogger_archive_card(site: &str, workdir: &WorkDir) -> Markup {
    let archive = get_archive_data(&workdir.crawled.items);

    // Group by year first
    let mut years: BTreeMap<i32, Vec<(u8, usize)>> = BTreeMap::new();
    for ((year, month), count) in archive {
        years.entry(year).or_default().push((month, count));
    }

    html! {
        .blog_card.archive_card {
            h3 { "Archive" }
            ul.blog_archive_list {
                @for (year, months) in years.iter().rev() {
                    li.archive_year {
                        span.year_name { (year) }
                        ul.month_list {
                            @for (month, count) in months.iter().rev() {
                                li.archive_month {
                                    a href=(format!("/{}/blog/archive/{}/{:02}", site, year, month)) {
                                        span.month_name { (Month::try_from(*month).unwrap().name()) }
                                        span.month_count { "(" (count) ")" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn blogger_post_card(item: &CrawlItem, site: &str) -> Markup {
    let time = Utc
        .timestamp_millis_opt(item.source_published as i64)
        .unwrap();

    html! {
        article.blog_post_card {
            header.post_header {
                h3.post_title {
                    a href=(format!("/{}/blog/post/{}", site, encode(&item.key))) { (item.title) }
                }
                .post_meta {
                    time datetime=(time.to_rfc3339()) {
                        (time.format("%B %d, %Y"))
                    }
                }
            }
            @if let Some(thumb) = item.thumbnail_path() {
                .post_thumbnail {
                    img src=(format!("/{}/assets/{}", site, thumb)) alt=(item.title) {}
                }
            }
            .post_excerpt {
                p { (item.description) }
            }
            footer.post_footer {
                .post_tags {
                    @for tag in &item.tags {
                        @match tag {
                            site::CrawlTag::Simple(x) =>
                                a.post_tag href=(format!("/{}/blog/tag/{}", site, encode(x))) { (x) },
                            site::CrawlTag::Detailed { value, .. } =>
                                a.post_tag href=(format!("/{}/blog/tag/{}", site, encode(value))) { (value) },
                        }
                    }
                }
            }
        }
    }
}

#[get("/info")]
async fn info_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<ThreadSafeWorkDir>,
    start_time: web::Data<StartTime>,
) -> Result<impl Responder, actix_web::Error> {
    // 503 if workdir is write locked
    let workdir_data = workdir.into_inner();
    let workdir_lock = workdir_data.work_dir.try_read();
    let workdir = match workdir_lock {
        Ok(x) => x,
        Err(_) => {
            return Err(actix_web::Error::from(
                actix_web::error::ErrorServiceUnavailable("Work directory is locked"),
            ));
        }
    };

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
        a.item_thumb_container href=(format!("/{}/booru/item/{}/{}", site, encode(&item.key), encode(item.flat_files().keys().into_iter().next().unwrap_or(&"".to_string())))) {
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
    site: web::Data<Vec<ThreadSafeWorkDir>>,
) -> Result<impl Responder, actix_web::Error> {
    return Ok(html! {
        (Css("/res/styles.css"))
        h1.page_title { "Loaded sites" }
        ul.site_list {
            @for site in site.iter() {
                @let site = site.work_dir.read().unwrap();
                li {
                    a.site_link href=(format!("/{}/booru/latest", site.config.slug)) { (site.config.label) }
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
    workdir: web::Data<ThreadSafeWorkDir>,
) -> Result<impl Responder, actix_web::Error> {
    // 503 if workdir is write locked
    let workdir_data = workdir.into_inner();
    let workdir_lock = workdir_data.work_dir.try_read();
    let workdir = match workdir_lock {
        Ok(x) => x,
        Err(_) => {
            return Err(actix_web::Error::from(
                actix_web::error::ErrorServiceUnavailable("Work directory is locked"),
            ));
        }
    };

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
            a href=(format!("/{}/booru/random", &site.0)) { "See more" }
        }
    });
}

async fn generic_latest_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<ThreadSafeWorkDir>,
    path: web::Path<usize>,
) -> Result<impl Responder, actix_web::Error> {
    // 503 if workdir is write locked
    let workdir_data = workdir.into_inner();
    let workdir_lock = workdir_data.work_dir.try_read();
    let workdir = match workdir_lock {
        Ok(x) => x,
        Err(_) => {
            return Err(actix_web::Error::from(
                actix_web::error::ErrorServiceUnavailable("Work directory is locked"),
            ));
        }
    };

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
        ( paginator(page, workdir.crawled.items.len(), 40, &format!("/{}/booru/latest", &site.0)) )
        .item_thumb_grid {
            @for item in &items {
                ( item_thumbnail(&item, &site.0) )
            }
        }
        ( paginator(page, workdir.crawled.items.len(), 40, &format!("/{}/booru/latest", &site.0)) )
    });
}

#[get("/latest/{page}")]
async fn latest_page_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<ThreadSafeWorkDir>,
    path: web::Path<usize>,
) -> Result<impl Responder, actix_web::Error> {
    generic_latest_handler(site, workdir, path).await
}

#[get("/latest")]
async fn latest_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<ThreadSafeWorkDir>,
) -> Result<impl Responder, actix_web::Error> {
    generic_latest_handler(site, workdir, web::Path::from(1)).await
}

async fn generic_oldest_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<ThreadSafeWorkDir>,
    path: web::Path<usize>,
) -> Result<impl Responder, actix_web::Error> {
    // 503 if workdir is write locked
    let workdir_data = workdir.into_inner();
    let workdir_lock = workdir_data.work_dir.try_read();
    let workdir = match workdir_lock {
        Ok(x) => x,
        Err(_) => {
            return Err(actix_web::Error::from(
                actix_web::error::ErrorServiceUnavailable("Work directory is locked"),
            ));
        }
    };

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
        ( paginator(page, workdir.crawled.items.len(), 40, &format!("/{}/booru/oldest", &site.0)) )
        .item_thumb_grid {
            @for item in &items {
                ( item_thumbnail(&item, &site.0) )
            }
        }
        ( paginator(page, workdir.crawled.items.len(), 40, &format!("/{}/booru/oldest", &site.0)) )
    });
}

#[get("/oldest/{page}")]
async fn oldest_page_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<ThreadSafeWorkDir>,
    path: web::Path<usize>,
) -> Result<impl Responder, actix_web::Error> {
    generic_oldest_handler(site, workdir, path).await
}

#[get("/oldest")]
async fn oldest_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<ThreadSafeWorkDir>,
) -> Result<impl Responder, actix_web::Error> {
    generic_oldest_handler(site, workdir, web::Path::from(1)).await
}

async fn generic_tag_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<ThreadSafeWorkDir>,
    tag: String,
    page: usize,
) -> Result<impl Responder, actix_web::Error> {
    // 503 if workdir is write locked
    let workdir_data = workdir.into_inner();
    let workdir_lock = workdir_data.work_dir.try_read();
    let workdir = match workdir_lock {
        Ok(x) => x,
        Err(_) => {
            return Err(actix_web::Error::from(
                actix_web::error::ErrorServiceUnavailable("Work directory is locked"),
            ));
        }
    };

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
        ( paginator(page, filtered_items_len, 40, &format!("/{}/booru/tag/{}", &site.0, encode(&tag))) )
        .item_thumb_grid {
            @for item in &items {
                ( item_thumbnail(&item, &site.0) )
            }
        }
        ( paginator(page, filtered_items_len, 40, &format!("/{}/booru/tag/{}", &site.0, encode(&tag))) )
    });
}

#[get("/tag/{tag}/{page}")]
async fn tag_page_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<ThreadSafeWorkDir>,
    path: web::Path<(String, usize)>,
) -> Result<impl Responder, actix_web::Error> {
    let (tag, page) = path.into_inner();
    generic_tag_handler(site, workdir, decode(&tag).unwrap().into_owned(), page).await
}

#[get("/tag/{tag}")]
async fn tag_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<ThreadSafeWorkDir>,
    path: web::Path<String>,
) -> Result<impl Responder, actix_web::Error> {
    let tag = path.into_inner();
    generic_tag_handler(site, workdir, decode(&tag).unwrap().into_owned(), 1).await
}

#[get("/tags")]
async fn tags_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<ThreadSafeWorkDir>,
) -> Result<impl Responder, actix_web::Error> {
    let workdir_data = workdir.into_inner();
    let workdir_lock = workdir_data.work_dir.try_read();
    let workdir = match workdir_lock {
        Ok(x) => x,
        Err(_) => {
            return Err(actix_web::Error::from(
                actix_web::error::ErrorServiceUnavailable("Work directory is locked"),
            ));
        }
    };

    let mut tags: HashMap<String, Vec<String>> = HashMap::new();
    for item in workdir.crawled.items.values() {
        for tag in &item.tags {
            let tag_str = match tag {
                site::CrawlTag::Simple(x) => x.clone(),
                site::CrawlTag::Detailed { value, .. } => value.clone(),
            };
            tags.entry(tag_str).or_default().push(item.key.clone());
        }
    }

    let content = html! {
        h2 { "Tags" }
        ul.tag_list {
            @for (tag, items) in &tags {
                li {
                    a href=(format!("/{}/booru/tag/{}", site.0, encode(tag))) {
                        (tag) " (" (items.len()) ")"
                    }
                }
            }
        }
    };

    Ok(HttpResponse::Ok().body(content.into_string()))
}

#[get("/archive")]
async fn blog_archive_root_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<ThreadSafeWorkDir>,
) -> Result<impl Responder, actix_web::Error> {
    let workdir_data = workdir.into_inner();
    let workdir_lock = workdir_data.work_dir.try_read();
    let workdir = match workdir_lock {
        Ok(x) => x,
        Err(_) => {
            return Err(actix_web::Error::from(
                actix_web::error::ErrorServiceUnavailable("Work directory is locked"),
            ));
        }
    };

    let archive = get_archive_data(&workdir.crawled.items);

    // Group by year first
    let mut years: BTreeMap<i32, Vec<(u8, usize)>> = BTreeMap::new();
    for ((year, month), count) in archive {
        years.entry(year).or_default().push((month, count));
    }

    let content = html! {
        .blog_archive_page {
            h2 { "Archive" }
            ul.blog_archive_list.full_archive_list {
                @for (year, months) in years.iter().rev() {
                    li.archive_year {
                        h3.year_name { (year) }
                        ul.month_list {
                            @for (month, count) in months.iter().rev() {
                                li.archive_month {
                                    a href=(format!("/{}/blog/archive/{}/{:02}", site.0, year, month)) {
                                        span.month_name { (Month::try_from(*month).unwrap().name()) }
                                        span.month_count { "(" (count) ")" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    };

    Ok(blogger_layout("Archive", content, &site.0, &workdir))
}

#[get("/post/{post}")]
async fn blog_post_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<ThreadSafeWorkDir>,
    path: web::Path<String>,
) -> Result<impl Responder, actix_web::Error> {
    let workdir_data = workdir.into_inner();
    let workdir_lock = workdir_data.work_dir.try_read();
    let workdir = match workdir_lock {
        Ok(x) => x,
        Err(_) => {
            return Err(actix_web::Error::from(
                actix_web::error::ErrorServiceUnavailable("Work directory is locked"),
            ));
        }
    };

    let post_key = decode(&path.into_inner()).unwrap().into_owned();
    let item = workdir.crawled.items.get(&post_key);

    let Some(item) = item else {
        return Ok(blogger_layout(
            "Post not found",
            html! { p { "The requested post could not be found." } },
            &site.0,
            &workdir,
        ));
    };

    let time = Utc
        .timestamp_millis_opt(item.source_published as i64)
        .unwrap();

    let content = html! {
        article.blog_post {
            header.post_header {
                h1.post_title { (item.title) }
                .post_meta {
                    time datetime=(time.to_rfc3339()) {
                        (time.format("%B %d, %Y"))
                    }
                }
            }
            .post_content {
                @for file in item.flat_files().values() {
                    @match file {
                        site::FileCrawlType::Image { filename, downloaded, .. } => {
                            @if *downloaded {
                                figure.post_figure {
                                    img.post_image src=(format!("/{}/assets/{}", site.0, filename)) alt=(item.title) {}
                                }
                            }
                        }
                        site::FileCrawlType::Video { filename, downloaded, .. } => {
                            @if *downloaded {
                                @let coerced_filename = filename.split('.').next().unwrap_or("").to_string() + ".mp4";
                                figure.post_figure {
                                    video.post_video controls {
                                        source src=(format!("/{}/assets/{}", site.0, coerced_filename)) {}
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }

                .post_description {
                    p { (item.description) }
                }

                @if !item.meta.is_object() || !item.meta.as_object().unwrap().is_empty() {
                    .post_meta_details {
                        h3 { "Additional Details" }
                        dl {
                            @for (key, value) in item.meta.as_object().unwrap() {
                                dt { (key) }
                                dd { (value) }
                            }
                        }
                    }
                }
            }
            footer.post_footer {
                .post_tags {
                    @for tag in &item.tags {
                        @match tag {
                            site::CrawlTag::Simple(x) =>
                                a.post_tag href=(format!("/{}/blog/tag/{}", site.0, encode(x))) { (x) },
                            site::CrawlTag::Detailed { value, .. } =>
                                a.post_tag href=(format!("/{}/blog/tag/{}", site.0, encode(value))) { (value) },
                        }
                    }
                }
                p.post_source {
                    "Source: "
                    a href=(item.url) { (item.url) }
                }
            }
        }
    };

    Ok(blogger_layout("", content, &site.0, &workdir))
}

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
                let threadsafe_work_dir = ThreadSafeWorkDir::new(work_dir);
                let update_clone = threadsafe_work_dir.clone();
                work_dirs_vec.push(threadsafe_work_dir);

                // Spawn a thread to watch the workdir for changes
                thread::spawn(move || loop {
                    thread::sleep(Duration::from_secs(60));
                    update_clone.check_for_updates();
                });
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
                        web::scope(&workdir.work_dir.read().unwrap().config.slug)
                            .app_data(web::Data::new(workdir.clone()))
                            .app_data(web::Data::new(WorkDirPrefix(
                                workdir.work_dir.read().unwrap().config.slug.clone(),
                            )))
                            // Add info handler at site root level
                            .service(info_handler)
                            // Add a /booru scope for imageboard style routes
                            .service(
                                web::scope("/booru")
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
                                    .service(item_redirect),
                            )
                            // Add a /blog scope for blogger style routes
                            .service(
                                web::scope("/blog")
                                    .service(blog_home_handler)
                                    .service(blog_home_page_handler)
                                    .service(tags_handler)
                                    .service(blog_archive_root_handler)
                                    .service(blog_post_handler)
                                    .service(blog_tag_handler)
                                    .service(blog_archive_handler)
                                    .service(blog_home_page_handler)
                                    .service(blog_tag_page_handler)
                                    .service(blog_archive_page_handler),
                            )
                            // Keep assets at the site root level since they're shared across views
                            .service(
                                Files::new(
                                    "/assets",
                                    workdir.work_dir.read().unwrap().path.clone(),
                                )
                                .prefer_utf8(true),
                            ),
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

// Blogger style handlers
#[get("/")]
async fn blog_home_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<ThreadSafeWorkDir>,
) -> Result<impl Responder, actix_web::Error> {
    generic_blog_home_handler(site, workdir, web::Path::from(1)).await
}

#[get("/page/{page}")]
async fn blog_home_page_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<ThreadSafeWorkDir>,
    path: web::Path<usize>,
) -> Result<impl Responder, actix_web::Error> {
    generic_blog_home_handler(site, workdir, path).await
}

async fn generic_blog_home_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<ThreadSafeWorkDir>,
    path: web::Path<usize>,
) -> Result<impl Responder, actix_web::Error> {
    let workdir_data = workdir.into_inner();
    let workdir_lock = workdir_data.work_dir.try_read();
    let workdir = match workdir_lock {
        Ok(x) => x,
        Err(_) => {
            return Err(actix_web::Error::from(
                actix_web::error::ErrorServiceUnavailable("Work directory is locked"),
            ));
        }
    };

    let page = path.into_inner();
    let per_page = 10;
    let total_items = workdir.crawled.items.len();

    let items: Vec<&CrawlItem> = workdir
        .crawled
        .items
        .values()
        .skip((page - 1) * per_page)
        .take(per_page)
        .collect();

    let content = html! {
        .blog_posts {
            @for item in items {
                (blogger_post_card(item, &site.0))
            }
        }
        (paginator(page, total_items, per_page, &format!("/{}/blog/page", &site.0)))
    };

    Ok(blogger_layout("", content, &site.0, &workdir))
}

#[get("/tag/{tag}")]
async fn blog_tag_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<ThreadSafeWorkDir>,
    path: web::Path<String>,
) -> Result<impl Responder, actix_web::Error> {
    generic_blog_tag_handler(site, workdir, path, web::Path::from(1)).await
}

#[get("/tag/{tag}/page/{page}")]
async fn blog_tag_page_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<ThreadSafeWorkDir>,
    path: web::Path<(String, usize)>,
) -> Result<impl Responder, actix_web::Error> {
    let (tag, page) = path.into_inner();
    generic_blog_tag_handler(site, workdir, web::Path::from(tag), web::Path::from(page)).await
}

async fn generic_blog_tag_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<ThreadSafeWorkDir>,
    tag_path: web::Path<String>,
    page_path: web::Path<usize>,
) -> Result<impl Responder, actix_web::Error> {
    let workdir_data = workdir.into_inner();
    let workdir_lock = workdir_data.work_dir.try_read();
    let workdir = match workdir_lock {
        Ok(x) => x,
        Err(_) => {
            return Err(actix_web::Error::from(
                actix_web::error::ErrorServiceUnavailable("Work directory is locked"),
            ));
        }
    };

    let tag = decode(&tag_path.into_inner()).unwrap().into_owned();
    let page = page_path.into_inner();
    let per_page = 10;

    let filtered_items: Vec<&CrawlItem> = workdir
        .crawled
        .items
        .values()
        .filter(|item| item.tags.iter().any(|x| x.to_string() == tag))
        .collect();

    let total_items = filtered_items.len();

    let items: Vec<&CrawlItem> = filtered_items
        .into_iter()
        .skip((page - 1) * per_page)
        .take(per_page)
        .collect();

    let content = html! {
        .blog_posts {
            @for item in items {
                (blogger_post_card(item, &site.0))
            }
        }
        (paginator(page, total_items, per_page, &format!("/{}/blog/tag/{}/page", &site.0, encode(&tag))))
    };

    Ok(blogger_layout(
        &format!("Posts tagged \"{}\"", tag),
        content,
        &site.0,
        &workdir,
    ))
}

#[get("/archive/{year}/{month}")]
async fn blog_archive_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<ThreadSafeWorkDir>,
    path: web::Path<(i32, u8)>,
) -> Result<impl Responder, actix_web::Error> {
    generic_blog_archive_handler(site, workdir, path, web::Path::from(1)).await
}

#[get("/archive/{year}/{month}/page/{page}")]
async fn blog_archive_page_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<ThreadSafeWorkDir>,
    path: web::Path<(i32, u8, usize)>,
) -> Result<impl Responder, actix_web::Error> {
    let (year, month, page) = path.into_inner();
    generic_blog_archive_handler(
        site,
        workdir,
        web::Path::from((year, month)),
        web::Path::from(page),
    )
    .await
}

async fn generic_blog_archive_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<ThreadSafeWorkDir>,
    archive_path: web::Path<(i32, u8)>,
    page_path: web::Path<usize>,
) -> Result<impl Responder, actix_web::Error> {
    let workdir_data = workdir.into_inner();
    let workdir_lock = workdir_data.work_dir.try_read();
    let workdir = match workdir_lock {
        Ok(x) => x,
        Err(_) => {
            return Err(actix_web::Error::from(
                actix_web::error::ErrorServiceUnavailable("Work directory is locked"),
            ));
        }
    };

    let (year, month) = archive_path.into_inner();
    let page = page_path.into_inner();
    let per_page = 10;
    let month_name = Month::try_from(month).unwrap().name();

    let filtered_items: Vec<&CrawlItem> = workdir
        .crawled
        .items
        .values()
        .filter(|item| {
            let time = Utc
                .timestamp_millis_opt(item.source_published as i64)
                .unwrap();
            time.year() == year && time.month() as u8 == month
        })
        .collect();

    let total_items = filtered_items.len();

    let items: Vec<&CrawlItem> = filtered_items
        .into_iter()
        .skip((page - 1) * per_page)
        .take(per_page)
        .collect();

    let content = html! {
        .blog_posts {
            @for item in items {
                (blogger_post_card(item, &site.0))
            }
        }
        (paginator(page, total_items, per_page, &format!("/{}/blog/archive/{}/{:02}/page", &site.0, year, month)))
    };

    Ok(blogger_layout(
        &format!("Posts from {month_name} {year}"),
        content,
        &site.0,
        &workdir,
    ))
}

pub fn configure_blog(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/blog")
            .service(blog_home_handler)
            .service(tags_handler)
            .service(blog_archive_root_handler)
            .service(blog_post_handler)
            .service(blog_tag_handler)
            .service(blog_archive_handler)
            .service(blog_home_page_handler)
            .service(blog_tag_page_handler)
            .service(blog_archive_page_handler),
    );
}

pub fn configure_app(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("")
            .configure(configure_blog)
            .service(web::scope("/assets").service(fs::Files::new("", "assets"))),
    );
}

#[get("/")]
async fn root_redirect() -> impl Responder {
    HttpResponse::Found()
        .append_header(("Location", "/blog"))
        .finish()
}

#[get("/item/{id}")]
async fn item_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<ThreadSafeWorkDir>,
    path: web::Path<String>,
) -> Result<impl Responder, actix_web::Error> {
    let workdir_data = workdir.into_inner();
    let workdir_lock = workdir_data.work_dir.try_read();
    let workdir = match workdir_lock {
        Ok(x) => x,
        Err(_) => {
            return Err(actix_web::Error::from(
                actix_web::error::ErrorServiceUnavailable("Work directory is locked"),
            ));
        }
    };

    let id = path.into_inner();
    if let Some(item) = workdir.crawled.items.get(&id) {
        let content = html! {
            article.post {
                h1 { (item.title) }
                @if let Some(thumb) = item.thumbnail_path() {
                    img src=(format!("/{}/assets/{}", site.0, thumb)) alt=(item.title) {}
                }
                p { (item.description) }
                .tags {
                    @for tag in &item.tags {
                        @match tag {
                            site::CrawlTag::Simple(x) =>
                                a.tag href=(format!("/{}/booru/tag/{}", site.0, encode(x))) { (x) },
                            site::CrawlTag::Detailed { value, .. } =>
                                a.tag href=(format!("/{}/booru/tag/{}", site.0, encode(value))) { (value) },
                        }
                    }
                }
            }
        };
        Ok(HttpResponse::Ok().body(content.into_string()))
    } else {
        Ok(HttpResponse::NotFound().finish())
    }
}

#[get("/items/{id}")]
async fn item_redirect(path: web::Path<String>) -> impl Responder {
    let id = path.into_inner();
    HttpResponse::Found()
        .append_header(("Location", format!("/blog/item/{}", id)))
        .finish()
}
