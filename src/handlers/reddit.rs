use actix_web::{get, web, Responder};
use chrono::Utc;
use maud::{html, Markup};
use std::collections::HashMap;
use urlencoding::encode;

use crate::site::{CrawlItem, CrawlTag, FileCrawlType};

fn timeago(timestamp: u64) -> String {
    let now = Utc::now().timestamp_millis() as u64;
    let diff = now - timestamp;
    let hours = diff / (1000 * 60 * 60);
    let days = hours / 24;

    if days > 0 {
        format!("{} days ago", days)
    } else if hours > 0 {
        format!("{} hours ago", hours)
    } else {
        "just now".to_string()
    }
}

fn reddit_layout(title: &str, content: Markup, site: &str) -> Markup {
    html! {
        (super::Css("/res/styles.css"))
        .reddit_layout {
            header.reddit_header {
                nav.reddit_nav {
                    a href=(format!("/{}/r", site)) { "Home" }
                }
            }
            main.reddit_main {
                @if !title.is_empty() {
                    h1.page_title { (title) }
                }
                (content)
            }
        }
    }
}

fn reddit_post_card(item: &CrawlItem, site: &str) -> Markup {
    let timeago = timeago(item.source_published as u64);

    html! {
        article.reddit_post_card {
            header.post_header {
                span.post_author {
                    @if let Some(author) = item.meta.get("author") {
                        (author.as_str().unwrap_or("unknown user"))
                    } @else {
                        "unknown user"
                    }
                }
                span.post_time { (timeago) }
            }
            .post_content {
                h2.post_title {
                    a href=(format!("/{}/r/post/{}", site, encode(&item.key))) { (item.title) }
                }
                @if let Some(thumb) = item.thumbnail_path() {
                    .post_preview {
                        img src=(format!("/{}/assets/{}", site, thumb)) alt=(item.title) {}
                    }
                }
                .post_tags {
                    @for tag in &item.tags {
                        @match tag {
                            CrawlTag::Simple(x) =>
                                span.post_tag href=(format!("/{}/r/tag/{}", site, encode(x))) { (x) },
                            CrawlTag::Detailed { value, .. } =>
                                span.post_tag href=(format!("/{}/r/tag/{}", site, encode(value))) { (value) },
                        }
                    }
                }
            }
        }
        hr.post_separator {}
    }
}

fn reddit_post_detail(item: &CrawlItem, site: &str, current_file_index: usize) -> Markup {
    let timeago = timeago(item.source_published as u64);
    let flat_files = item.flat_files();
    let files: Vec<_> = flat_files.values().collect();
    let total_files = files.len();

    html! {
        article.reddit_post_detail {
            header.post_header {
                span.post_author {
                    @if let Some(author) = item.meta.get("author") {
                        (author.as_str().unwrap_or("unknown user"))
                    } @else {
                        "unknown user"
                    }
                }
                span.post_time { (timeago) }
            }
            h1.post_title { (item.title) }
            .post_content {
                .media_viewer {
                    @if current_file_index > 0 {
                        a.prev_file href=(format!("/{}/r/post/{}?file={}", site, encode(&item.key), current_file_index - 1)) {
                            span.nav_arrow { "←" }
                        }
                    }
                    .media_container {
                        @if let Some(file) = files.get(current_file_index) {
                            @match file {
                                FileCrawlType::Image { filename, downloaded, .. } => {
                                    @if *downloaded {
                                        img.post_image src=(format!("/{}/assets/{}", site, filename)) alt=(item.title) {}
                                    }
                                }
                                FileCrawlType::Video { filename, downloaded, .. } => {
                                    @if *downloaded {
                                        @let coerced_filename = filename.split('.').next().unwrap_or("").to_string() + ".mp4";
                                        video.post_video controls {
                                            source src=(format!("/{}/assets/{}", site, coerced_filename)) {}
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    @if current_file_index < total_files - 1 {
                        a.next_file href=(format!("/{}/r/post/{}?file={}", site, encode(&item.key), current_file_index + 1)) {
                            span.nav_arrow { "→" }
                        }
                    }
                }
                .post_description {
                    p { (item.description) }
                }
                .post_tags {
                    @for tag in &item.tags {
                        @match tag {
                            CrawlTag::Simple(x) =>
                                span.post_tag href=(format!("/{}/r/tag/{}", site, encode(x))) { (x) },
                            CrawlTag::Detailed { value, .. } =>
                                span.post_tag href=(format!("/{}/r/tag/{}", site, encode(value))) { (value) },
                        }
                    }
                }
                @if !item.meta.is_object() || !item.meta.as_object().unwrap().is_empty() {
                    .post_meta {
                        @for (key, value) in item.meta.as_object().unwrap() {
                            @if key != "author" {
                                .meta_item {
                                    span.meta_key { (key) ": " }
                                    span.meta_value { (value) }
                                }
                            }
                        }
                    }
                }
                p.post_source {
                    "Source: "
                    a href=(item.url) { (item.url) }
                }
            }
        }
    }
}

#[get("")]
pub async fn reddit_home_handler(
    site: web::Data<super::WorkDirPrefix>,
    workdir: web::Data<super::ThreadSafeWorkDir>,
) -> Result<impl Responder, actix_web::Error> {
    generic_reddit_home_handler(site, workdir, web::Path::from(1)).await
}

#[get("/page/{page}")]
pub async fn reddit_home_page_handler(
    site: web::Data<super::WorkDirPrefix>,
    workdir: web::Data<super::ThreadSafeWorkDir>,
    path: web::Path<usize>,
) -> Result<impl Responder, actix_web::Error> {
    generic_reddit_home_handler(site, workdir, path).await
}

async fn generic_reddit_home_handler(
    site: web::Data<super::WorkDirPrefix>,
    workdir: web::Data<super::ThreadSafeWorkDir>,
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
    let per_page = 25; // Reddit-style pagination
    let total_items = workdir.crawled.items.len();

    let items: Vec<&CrawlItem> = workdir
        .crawled
        .items
        .values()
        .skip((page - 1) * per_page)
        .take(per_page)
        .collect();

    let content = html! {
        .reddit_posts {
            @for item in items {
                (reddit_post_card(item, &site.0))
            }
        }
        (super::paginator(page, total_items, per_page, &format!("/{}/r/page", &site.0)))
    };

    Ok(reddit_layout("", content, &site.0))
}

#[get("/post/{post}")]
pub async fn reddit_post_handler(
    site: web::Data<super::WorkDirPrefix>,
    workdir: web::Data<super::ThreadSafeWorkDir>,
    path: web::Path<String>,
    query: web::Query<HashMap<String, String>>,
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

    let post_key = urlencoding::decode(&path.into_inner())
        .unwrap()
        .into_owned();
    let current_file_index = query
        .get("file")
        .and_then(|f| f.parse::<usize>().ok())
        .unwrap_or(0);

    let Some(item) = workdir.crawled.items.get(&post_key) else {
        return Ok(reddit_layout(
            "Post not found",
            html! { p { "The requested post could not be found." } },
            &site.0,
        )
        .into());
    };

    let content = reddit_post_detail(item, &site.0, current_file_index);
    Ok(reddit_layout("", content, &site.0))
}

pub fn configure_reddit(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/r")
            .service(reddit_home_handler)
            .service(reddit_home_page_handler)
            .service(reddit_post_handler),
    );
}
