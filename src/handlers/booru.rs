use actix_web::{get, web, HttpResponse, Responder};
use maud::{html, Markup};
use rand::seq::IteratorRandom;
use urlencoding::{decode, encode};

use crate::site::CrawlItem;

fn booru_layout(title: &str, content: Markup, site: &str) -> Markup {
    html! {
        (maud::DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                title { (title) }
                (super::Css("/res/styles.css"))
            }
            body {
                .booru_layout {
                    header.booru_header {
                        nav.booru_nav {
                            a href=(format!("/{}/booru/latest", site)) { "Latest" }
                            a href=(format!("/{}/booru/random", site)) { "Random" }
                        }
                    }
                    main.booru_main {
                        @if !title.is_empty() {
                            h1.page_title { (title) }
                        }
                        (content)
                    }
                }
            }
        }
    }
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
                        crate::site::CrawlTag::Simple(x) => .tag { (x) },
                        crate::site::CrawlTag::Detailed { value, .. } => .tag { (value) },
                    }
                }
            }
        }
    }
}

#[get("/random")]
pub async fn random_handler(
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

    let rng = &mut rand::thread_rng();
    let items = workdir
        .crawled
        .items
        .values()
        .into_iter()
        .choose_multiple(rng, 40);

    let content = html! {
        .item_thumb_grid {
            @for item in &items {
                ( item_thumbnail(&item, &site.0) )
            }
        }
        .paginator {
            a href=(format!("/{}/booru/random", &site.0)) { "See more" }
        }
    };

    Ok(booru_layout("Random items", content, &site.0))
}

async fn generic_latest_handler(
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
    let items: Vec<&CrawlItem> = workdir
        .crawled
        .items
        .values()
        .into_iter()
        .skip((page - 1) * 40)
        .take(40)
        .collect();

    let content = html! {
        ( super::paginator(page, workdir.crawled.items.len(), 40, &format!("/{}/booru/latest", &site.0)) )
        .item_thumb_grid {
            @for item in &items {
                ( item_thumbnail(&item, &site.0) )
            }
        }
        ( super::paginator(page, workdir.crawled.items.len(), 40, &format!("/{}/booru/latest", &site.0)) )
    };

    Ok(booru_layout("Latest items", content, &site.0))
}

#[get("/latest/{page}")]
pub async fn latest_page_handler(
    site: web::Data<super::WorkDirPrefix>,
    workdir: web::Data<super::ThreadSafeWorkDir>,
    path: web::Path<usize>,
) -> Result<impl Responder, actix_web::Error> {
    generic_latest_handler(site, workdir, path).await
}

#[get("/latest")]
pub async fn latest_handler(
    site: web::Data<super::WorkDirPrefix>,
    workdir: web::Data<super::ThreadSafeWorkDir>,
) -> Result<impl Responder, actix_web::Error> {
    generic_latest_handler(site, workdir, web::Path::from(1)).await
}

async fn generic_oldest_handler(
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
    let items: Vec<&CrawlItem> = workdir
        .crawled
        .items
        .values()
        .rev()
        .into_iter()
        .skip((page - 1) * 40)
        .take(40)
        .collect();

    let content = html! {
        ( super::paginator(page, workdir.crawled.items.len(), 40, &format!("/{}/booru/oldest", &site.0)) )
        .item_thumb_grid {
            @for item in &items {
                ( item_thumbnail(&item, &site.0) )
            }
        }
        ( super::paginator(page, workdir.crawled.items.len(), 40, &format!("/{}/booru/oldest", &site.0)) )
    };

    Ok(booru_layout("Oldest items", content, &site.0))
}

#[get("/oldest/{page}")]
pub async fn oldest_page_handler(
    site: web::Data<super::WorkDirPrefix>,
    workdir: web::Data<super::ThreadSafeWorkDir>,
    path: web::Path<usize>,
) -> Result<impl Responder, actix_web::Error> {
    generic_oldest_handler(site, workdir, path).await
}

#[get("/oldest")]
pub async fn oldest_handler(
    site: web::Data<super::WorkDirPrefix>,
    workdir: web::Data<super::ThreadSafeWorkDir>,
) -> Result<impl Responder, actix_web::Error> {
    generic_oldest_handler(site, workdir, web::Path::from(1)).await
}

async fn generic_tag_handler(
    site: web::Data<super::WorkDirPrefix>,
    workdir: web::Data<super::ThreadSafeWorkDir>,
    tag: String,
    page: usize,
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

    let content = html! {
        ( super::paginator(page, filtered_items_len, 40, &format!("/{}/booru/tag/{}", &site.0, encode(&tag))) )
        .item_thumb_grid {
            @for item in &items {
                ( item_thumbnail(&item, &site.0) )
            }
        }
        ( super::paginator(page, filtered_items_len, 40, &format!("/{}/booru/tag/{}", &site.0, encode(&tag))) )
    };

    Ok(booru_layout(
        &format!("Items tagged \"{}\"", tag),
        content,
        &site.0,
    ))
}

#[get("/tag/{tag}/{page}")]
pub async fn tag_page_handler(
    site: web::Data<super::WorkDirPrefix>,
    workdir: web::Data<super::ThreadSafeWorkDir>,
    path: web::Path<(String, usize)>,
) -> Result<impl Responder, actix_web::Error> {
    let (tag, page) = path.into_inner();
    generic_tag_handler(site, workdir, decode(&tag).unwrap().into_owned(), page).await
}

#[get("/tag/{tag}")]
pub async fn tag_handler(
    site: web::Data<super::WorkDirPrefix>,
    workdir: web::Data<super::ThreadSafeWorkDir>,
    path: web::Path<String>,
) -> Result<impl Responder, actix_web::Error> {
    let tag = path.into_inner();
    generic_tag_handler(site, workdir, decode(&tag).unwrap().into_owned(), 1).await
}

#[get("/")]
pub async fn root_redirect() -> impl Responder {
    HttpResponse::Found()
        .append_header(("Location", "/blog"))
        .finish()
}

#[get("/item/{id}")]
pub async fn item_no_file_handler(
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

    let id = path.into_inner();
    if let Some(item) = workdir.crawled.items.get(&id) {
        if let Some(first_file) = item.flat_files().keys().next() {
            Ok(HttpResponse::Found()
                .append_header((
                    "Location",
                    format!(
                        "/{}/booru/item/{}/{}",
                        site.0,
                        encode(&id),
                        encode(first_file)
                    ),
                ))
                .finish())
        } else {
            Ok(HttpResponse::NotFound().finish())
        }
    } else {
        Ok(HttpResponse::NotFound().finish())
    }
}

#[get("/item/{id}/{file_index}")]
pub async fn item_handler(
    site: web::Data<super::WorkDirPrefix>,
    workdir: web::Data<super::ThreadSafeWorkDir>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, actix_web::Error> {
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

    let (id, file_index) = path.into_inner();
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
                            crate::site::CrawlTag::Simple(x) =>
                                a.tag href=(format!("/{}/booru/tag/{}", site.0, encode(x))) { (x) },
                            crate::site::CrawlTag::Detailed { value, .. } =>
                                a.tag href=(format!("/{}/booru/tag/{}", site.0, encode(value))) { (value) },
                        }
                    }
                }
            }
        };
        Ok(HttpResponse::Ok().body(booru_layout(&item.title, content, &site.0).into_string()))
    } else {
        Ok(HttpResponse::NotFound().finish())
    }
}

#[get("/items/{id}")]
pub async fn item_redirect(path: web::Path<String>) -> impl Responder {
    let id = path.into_inner();
    HttpResponse::Found()
        .append_header(("Location", format!("/blog/item/{}", id)))
        .finish()
}

pub fn configure_booru(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/booru")
            .service(random_handler)
            .service(latest_handler)
            .service(latest_page_handler)
            .service(oldest_handler)
            .service(oldest_page_handler)
            .service(root_redirect)
            .service(tag_handler)
            .service(tag_page_handler)
            .service(item_no_file_handler)
            .service(item_handler)
            .service(item_redirect),
    );
}
