use actix_web::{get, web, Responder};
use chrono::{Datelike, Month, TimeZone, Utc};
use indexmap::IndexMap;
use maud::{html, Markup};
use std::collections::BTreeMap;
use urlencoding::{decode, encode};

use crate::site::{CrawlItem, CrawlTag};
use crate::workdir::WorkDir;

// Blogger style components
pub fn blogger_layout(title: &str, content: Markup, site: &str, workdir: &WorkDir) -> Markup {
    html! {
        (super::Css("/res/styles.css"))
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
    let mut tags: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for item in workdir.crawled.items.values() {
        for tag in &item.tags {
            let tag = match tag {
                CrawlTag::Simple(x) => x,
                CrawlTag::Detailed { value, .. } => value,
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
                            CrawlTag::Simple(x) =>
                                a.post_tag href=(format!("/{}/blog/tag/{}", site, encode(x))) { (x) },
                            CrawlTag::Detailed { value, .. } =>
                                a.post_tag href=(format!("/{}/blog/tag/{}", site, encode(value))) { (value) },
                        }
                    }
                }
            }
        }
    }
}

// Blogger style handlers
#[get("/")]
pub async fn blog_home_handler(
    site: web::Data<super::WorkDirPrefix>,
    workdir: web::Data<super::ThreadSafeWorkDir>,
) -> Result<impl Responder, actix_web::Error> {
    generic_blog_home_handler(site, workdir, web::Path::from(1)).await
}

#[get("/page/{page}")]
pub async fn blog_home_page_handler(
    site: web::Data<super::WorkDirPrefix>,
    workdir: web::Data<super::ThreadSafeWorkDir>,
    path: web::Path<usize>,
) -> Result<impl Responder, actix_web::Error> {
    generic_blog_home_handler(site, workdir, path).await
}

async fn generic_blog_home_handler(
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
        (super::paginator(page, total_items, per_page, &format!("/{}/blog/page", &site.0)))
    };

    Ok(blogger_layout("", content, &site.0, &workdir))
}

#[get("/tag/{tag}")]
pub async fn blog_tag_handler(
    site: web::Data<super::WorkDirPrefix>,
    workdir: web::Data<super::ThreadSafeWorkDir>,
    path: web::Path<String>,
) -> Result<impl Responder, actix_web::Error> {
    generic_blog_tag_handler(site, workdir, path, web::Path::from(1)).await
}

#[get("/tag/{tag}/page/{page}")]
pub async fn blog_tag_page_handler(
    site: web::Data<super::WorkDirPrefix>,
    workdir: web::Data<super::ThreadSafeWorkDir>,
    path: web::Path<(String, usize)>,
) -> Result<impl Responder, actix_web::Error> {
    let (tag, page) = path.into_inner();
    generic_blog_tag_handler(site, workdir, web::Path::from(tag), web::Path::from(page)).await
}

async fn generic_blog_tag_handler(
    site: web::Data<super::WorkDirPrefix>,
    workdir: web::Data<super::ThreadSafeWorkDir>,
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
        (super::paginator(page, total_items, per_page, &format!("/{}/blog/tag/{}/page", &site.0, encode(&tag))))
    };

    Ok(blogger_layout(
        &format!("Posts tagged \"{}\"", tag),
        content,
        &site.0,
        &workdir,
    ))
}

#[get("/archive/{year}/{month}")]
pub async fn blog_archive_handler(
    site: web::Data<super::WorkDirPrefix>,
    workdir: web::Data<super::ThreadSafeWorkDir>,
    path: web::Path<(i32, u8)>,
) -> Result<impl Responder, actix_web::Error> {
    generic_blog_archive_handler(site, workdir, path, web::Path::from(1)).await
}

#[get("/archive/{year}/{month}/page/{page}")]
pub async fn blog_archive_page_handler(
    site: web::Data<super::WorkDirPrefix>,
    workdir: web::Data<super::ThreadSafeWorkDir>,
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
    site: web::Data<super::WorkDirPrefix>,
    workdir: web::Data<super::ThreadSafeWorkDir>,
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
        (super::paginator(page, total_items, per_page, &format!("/{}/blog/archive/{}/{:02}/page", &site.0, year, month)))
    };

    Ok(blogger_layout(
        &format!("Posts from {month_name} {year}"),
        content,
        &site.0,
        &workdir,
    ))
}

#[get("/archive")]
pub async fn blog_archive_root_handler(
    site: web::Data<super::WorkDirPrefix>,
    workdir: web::Data<super::ThreadSafeWorkDir>,
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
pub async fn blog_post_handler(
    site: web::Data<super::WorkDirPrefix>,
    workdir: web::Data<super::ThreadSafeWorkDir>,
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
                        crate::site::FileCrawlType::Image { filename, downloaded, .. } => {
                            @if *downloaded {
                                figure.post_figure {
                                    img.post_image src=(format!("/{}/assets/{}", site.0, filename)) alt=(item.title) {}
                                }
                            }
                        }
                        crate::site::FileCrawlType::Video { filename, downloaded, .. } => {
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
                            CrawlTag::Simple(x) =>
                                a.post_tag href=(format!("/{}/blog/tag/{}", site.0, encode(x))) { (x) },
                            CrawlTag::Detailed { value, .. } =>
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

#[get("/tags")]
pub async fn tags_handler(
    site: web::Data<super::WorkDirPrefix>,
    workdir: web::Data<super::ThreadSafeWorkDir>,
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

    let mut tags: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for item in workdir.crawled.items.values() {
        for tag in &item.tags {
            let tag_str = match tag {
                CrawlTag::Simple(x) => x.clone(),
                CrawlTag::Detailed { value, .. } => value.clone(),
            };
            tags.entry(tag_str).or_default().push(item.key.clone());
        }
    }

    let content = html! {
        .tag_list_page {
            h2 { "Tags" }
            ul.tag_list {
                @for (tag, items) in &tags {
                    li.tag_item {
                        a href=(format!("/{}/blog/tag/{}", site.0, encode(tag))) {
                            span.tag_name { (tag) }
                            span.tag_count { " (" (items.len()) ")" }
                        }
                    }
                }
            }
        }
    };

    Ok(blogger_layout("Tags", content, &site.0, &workdir))
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
