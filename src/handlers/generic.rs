// .service(generic_index_handler)
// .service(generic_index_page_handler)
// .service(generic_tags_index_handler)
// .service(generic_tag_handler)
// .service(generic_tag_page_handler)
// .service(generic_archive_handler)
// .service(generic_archive_page_handler)
// .service(generic_detail_handler),

use std::collections::HashMap;

use actix_web::{get, web, HttpResponse, Responder};
use chrono::{DateTime, Datelike, TimeZone, Utc};
use rand::seq::SliceRandom;

use crate::{
    handlers::WorkDirPrefix,
    site::{CrawlItem, CrawlTag},
};

use super::{
    get_workdir, ListingPageConfig, ListingPageMode, ListingPageOrdering, SiteRenderer,
    SiteRendererType, ThreadSafeWorkDir,
};

fn resolve_listing_page(
    workdir: &web::Data<ThreadSafeWorkDir>,
    mode: &ListingPageMode,
) -> Vec<CrawlItem> {
    let workdir = get_workdir(workdir).unwrap();

    match mode {
        ListingPageMode::All => workdir
            .crawled
            .clone()
            .iter()
            .map(|(_, item)| item)
            .cloned()
            .collect(),

        ListingPageMode::ByTag { tag } => workdir
            .crawled
            .clone()
            .iter()
            .filter(|(_, item)| item.tags.iter().map(|t| t.to_string()).any(|t| t == *tag))
            .map(|(_, item)| item)
            .cloned()
            .collect(),

        ListingPageMode::ByMonth { year, month } => workdir
            .crawled
            .clone()
            .iter()
            .filter(|(_, item)| {
                let date = item.source_published;
                let date = DateTime::from_timestamp_millis(date).unwrap();
                date.year() as u32 == *year && date.month() as u32 == *month
            })
            .map(|(_, item)| item)
            .cloned()
            .collect(),
    }
}

fn apply_selection(items: &[CrawlItem], config: &ListingPageConfig) -> Vec<CrawlItem> {
    let mut items = items.to_vec();
    match config.ordering {
        ListingPageOrdering::NewestFirst => items.sort_by_key(|item| item.source_published),
        ListingPageOrdering::OldestFirst => items.sort_by_key(|item| -item.source_published),
        ListingPageOrdering::Random => items.shuffle(&mut rand::thread_rng()),
    };
    let start = (config.page - 1) * config.per_page;
    let end = start + config.per_page;
    if end > items.len() {
        items[start..].to_vec()
    } else {
        items[start..end].to_vec()
    }
}

#[get("")]
pub async fn generic_index_handler(
    workdir_prefix: web::Data<WorkDirPrefix>,
    renderer: web::Data<SiteRendererType>,
) -> HttpResponse {
    HttpResponse::SeeOther()
        .append_header((
            "Location",
            format!("/{}/{}/latest", workdir_prefix.0, renderer.get_prefix()),
        ))
        .finish()
}

#[get("/")]
pub async fn generic_index_root_handler(
    workdir_prefix: web::Data<WorkDirPrefix>,
    renderer: web::Data<SiteRendererType>,
) -> HttpResponse {
    HttpResponse::SeeOther()
        .append_header((
            "Location",
            format!("/{}/{}/latest", workdir_prefix.0, renderer.get_prefix()),
        ))
        .finish()
}

#[get("/random")]
pub async fn generic_random_handler(
    renderer: web::Data<SiteRendererType>,
    workdir: web::Data<ThreadSafeWorkDir>,
) -> impl Responder {
    let renderer = renderer.into_inner();
    let items = resolve_listing_page(&workdir, &ListingPageMode::All);
    let config = ListingPageConfig {
        mode: ListingPageMode::All,
        ordering: ListingPageOrdering::Random,
        page: 1,
        per_page: 10,
        total: items.len(),
    };
    let items = apply_selection(&items, &config);

    renderer.render_listing_page(&workdir, config, &items)
}

#[get("/latest")]
pub async fn generic_latest_handler(
    renderer: web::Data<SiteRendererType>,
    workdir: web::Data<ThreadSafeWorkDir>,
) -> impl Responder {
    let renderer = renderer.into_inner();
    let items = resolve_listing_page(&workdir, &ListingPageMode::All);
    let config = ListingPageConfig {
        mode: ListingPageMode::All,
        ordering: ListingPageOrdering::NewestFirst,
        page: 1,
        per_page: 10,
        total: items.len(),
    };
    let items = apply_selection(&items, &config);

    renderer.render_listing_page(&workdir, config, &items)
}

#[get("/latest/{page}")]
pub async fn generic_latest_page_handler(
    renderer: web::Data<SiteRendererType>,
    workdir: web::Data<ThreadSafeWorkDir>,
    page: web::Path<usize>,
) -> impl Responder {
    let renderer = renderer.into_inner();
    let items = resolve_listing_page(&workdir, &ListingPageMode::All);
    let config = ListingPageConfig {
        mode: ListingPageMode::All,
        ordering: ListingPageOrdering::NewestFirst,
        page: page.into_inner(),
        per_page: 10,
        total: items.len(),
    };
    let items = apply_selection(&items, &config);

    renderer.render_listing_page(&workdir, config, &items)
}

#[get("/oldest")]
pub async fn generic_oldest_handler(
    renderer: web::Data<SiteRendererType>,
    workdir: web::Data<ThreadSafeWorkDir>,
) -> impl Responder {
    let renderer = renderer.into_inner();
    let items = resolve_listing_page(&workdir, &ListingPageMode::All);
    let config = ListingPageConfig {
        mode: ListingPageMode::All,
        ordering: ListingPageOrdering::OldestFirst,
        page: 1,
        per_page: 10,
        total: items.len(),
    };
    let items = apply_selection(&items, &config);

    renderer.render_listing_page(&workdir, config, &items)
}

#[get("/oldest/{page}")]
pub async fn generic_oldest_page_handler(
    renderer: web::Data<SiteRendererType>,
    workdir: web::Data<ThreadSafeWorkDir>,
    page: web::Path<usize>,
) -> impl Responder {
    let renderer = renderer.into_inner();
    let items = resolve_listing_page(&workdir, &ListingPageMode::All);
    let config = ListingPageConfig {
        mode: ListingPageMode::All,
        ordering: ListingPageOrdering::OldestFirst,
        page: page.into_inner(),
        per_page: 10,
        total: items.len(),
    };
    let items = apply_selection(&items, &config);

    renderer.render_listing_page(&workdir, config, &items)
}

#[get("/tags")]
pub async fn generic_tags_index_handler(
    renderer: web::Data<SiteRendererType>,
    workdir: web::Data<ThreadSafeWorkDir>,
) -> impl Responder {
    let renderer = renderer.into_inner();

    let tags = {
        let workdir = get_workdir(&workdir).unwrap();

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
        tags
    };

    renderer.render_tags_page(&workdir, &tags)
}

#[get("/tag/{tag}")]
pub async fn generic_tag_handler(
    renderer: web::Data<SiteRendererType>,
    workdir: web::Data<ThreadSafeWorkDir>,
    tag: web::Path<String>,
) -> impl Responder {
    let renderer = renderer.into_inner();
    let items = resolve_listing_page(&workdir, &ListingPageMode::ByTag { tag: tag.clone() });
    let config = ListingPageConfig {
        mode: ListingPageMode::ByTag { tag: tag.clone() },
        ordering: ListingPageOrdering::NewestFirst,
        page: 1,
        per_page: 10,
        total: items.len(),
    };
    let items = apply_selection(&items, &config);

    renderer.render_listing_page(&workdir, config, &items)
}

#[get("/tag/{tag}/{page}")]
pub async fn generic_tag_page_handler(
    renderer: web::Data<SiteRendererType>,
    workdir: web::Data<ThreadSafeWorkDir>,
    path: web::Path<(String, usize)>,
) -> impl Responder {
    let renderer = renderer.into_inner();
    let items = resolve_listing_page(
        &workdir,
        &ListingPageMode::ByTag {
            tag: path.0.clone(),
        },
    );
    let config = ListingPageConfig {
        mode: ListingPageMode::ByTag {
            tag: path.0.clone(),
        },
        ordering: ListingPageOrdering::NewestFirst,
        page: path.1,
        per_page: 10,
        total: items.len(),
    };
    let items = apply_selection(&items, &config);

    renderer.render_listing_page(&workdir, config, &items)
}

#[get("/archive")]
pub async fn generic_archive_index_handler(
    renderer: web::Data<SiteRendererType>,
    workdir: web::Data<ThreadSafeWorkDir>,
) -> impl Responder {
    let renderer = renderer.into_inner();
    let archive = {
        let workdir = get_workdir(&workdir).unwrap();
        let mut archive: HashMap<(i32, u8), usize> = HashMap::new();

        for item in workdir.crawled.items.values() {
            let time = Utc
                .timestamp_millis_opt(item.source_published as i64)
                .unwrap();
            let year = time.year();
            let month = time.month() as u8;
            *archive.entry((year, month)).or_insert(0) += 1;
        }

        archive
    };

    renderer.render_archive_page(&workdir, &archive)
}

#[get("/archive/{year}/{month}")]
pub async fn generic_archive_page_handler(
    renderer: web::Data<SiteRendererType>,
    workdir: web::Data<ThreadSafeWorkDir>,
    page: web::Path<(usize, usize)>,
) -> impl Responder {
    let renderer = renderer.into_inner();
    let items = resolve_listing_page(
        &workdir,
        &ListingPageMode::ByMonth {
            year: page.0 as u32,
            month: page.1 as u32,
        },
    );
    let config = ListingPageConfig {
        mode: ListingPageMode::ByMonth {
            year: page.0 as u32,
            month: page.1 as u32,
        },
        ordering: ListingPageOrdering::NewestFirst,
        page: 1,
        per_page: 1000, // TODO: Probably just want to show all items?
        total: items.len(),
    };
    let items = apply_selection(&items, &config);

    renderer.render_listing_page(&workdir, config, &items)
}

#[get("/item/{id}")]
pub async fn generic_detail_redirect(
    renderer: web::Data<SiteRendererType>,
    workdir: web::Data<ThreadSafeWorkDir>,
    path: web::Path<String>,
    workdir_prefix: web::Data<WorkDirPrefix>,
) -> impl Responder {
    let id = path.into_inner();
    let renderer = renderer.into_inner();
    let item = {
        let workdir = get_workdir(&workdir).unwrap();
        let item = workdir.crawled.get(&id).unwrap().clone();
        item
    };

    let file_id = { item.files.keys().next().unwrap().to_string() };

    HttpResponse::SeeOther()
        .append_header((
            "Location",
            format!(
                "/{}/{}/item/{}/{}",
                workdir_prefix.0,
                renderer.get_prefix(),
                id,
                file_id
            ),
        ))
        .finish()
}

#[get("/item/{id}/{file_id}")]
pub async fn generic_detail_handler(
    renderer: web::Data<SiteRendererType>,
    workdir: web::Data<ThreadSafeWorkDir>,
    path: web::Path<(String, String)>,
) -> impl Responder {
    let (id, file_id) = path.into_inner();
    let renderer = renderer.into_inner();
    let item = {
        let workdir = get_workdir(&workdir).unwrap();
        let item = workdir.crawled.get(&id).unwrap().clone();
        item
    };

    let file = { item.files.get(&file_id).unwrap().clone() };

    renderer.render_detail_page(&workdir, &item, &file)
}
