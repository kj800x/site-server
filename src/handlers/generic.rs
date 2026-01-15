use std::collections::HashMap;

use actix_web::{get, web, HttpResponse, Responder};
use chrono::{DateTime, Datelike, TimeZone, Utc};
use indexmap::IndexMap;
use itertools::Itertools;
use rand::seq::SliceRandom;
use serde::Deserialize;
use urlencoding::encode;

use crate::{
    handlers::WorkDirPrefix,
    site::{CrawlItem, CrawlTag, FileCrawlType},
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

        ListingPageMode::Search { .. } => {
            // Search mode is handled separately in search handlers
            // This should not be called for search mode
            vec![]
        }
    }
}

fn apply_selection(items: &[CrawlItem], config: &ListingPageConfig) -> Vec<CrawlItem> {
    let mut items = items.to_vec();
    match config.ordering {
        ListingPageOrdering::NewestFirst => items.sort_by_key(|item| -item.source_published),
        ListingPageOrdering::OldestFirst => items.sort_by_key(|item| item.source_published),
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
        per_page: 15,
        total: items.len(),
    };
    let items = apply_selection(&items, &config);

    renderer.render_listing_page(&workdir, config, &items, &format!("/random"))
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
        per_page: 15,
        total: items.len(),
    };
    let items = apply_selection(&items, &config);

    renderer.render_listing_page(&workdir, config, &items, &format!("/latest"))
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
        page: page.clone(),
        per_page: 15,
        total: items.len(),
    };
    let items = apply_selection(&items, &config);

    renderer.render_listing_page(&workdir, config, &items, &format!("/latest/{page}"))
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
        per_page: 15,
        total: items.len(),
    };
    let items = apply_selection(&items, &config);

    renderer.render_listing_page(&workdir, config, &items, &format!("/oldest"))
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
        page: page.clone(),
        per_page: 15,
        total: items.len(),
    };
    let items = apply_selection(&items, &config);

    renderer.render_listing_page(&workdir, config, &items, &format!("/oldest/{page}"))
}

#[derive(Deserialize)]
pub struct TagParam {
    sort: Option<String>,
}

pub enum TagSort {
    Count,
    Alphabetical,
}

#[get("/tags")]
pub async fn generic_tags_index_handler(
    renderer: web::Data<SiteRendererType>,
    workdir: web::Data<ThreadSafeWorkDir>,
    query: web::Query<TagParam>,
) -> impl Responder {
    let renderer = renderer.into_inner();

    let sort = match &query.sort {
        Some(sort) => match sort.as_str() {
            "count" => TagSort::Count,
            "alpha" => TagSort::Alphabetical,
            _ => TagSort::Count,
        },
        None => TagSort::Count,
    };

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

    let tag_order: Vec<String> = {
        let mut tag_names = tags.keys().cloned().collect::<Vec<_>>();
        match sort {
            TagSort::Count => tag_names.sort_by(|a, b| {
                let a_count = tags.get(a).unwrap_or(&0);
                let b_count = tags.get(b).unwrap_or(&0);
                b_count.cmp(a_count).then(a.cmp(b))
            }),
            TagSort::Alphabetical => tag_names.sort_by_key(|tag| tag.clone()),
        }
        tag_names
    };

    renderer.render_tags_page(&workdir, &tags, &tag_order, &format!("/tags"))
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
        per_page: 15,
        total: items.len(),
    };
    let items = apply_selection(&items, &config);

    renderer.render_listing_page(&workdir, config, &items, &format!("/tag/{tag}"))
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
        per_page: 15,
        total: items.len(),
    };
    let items = apply_selection(&items, &config);

    renderer.render_listing_page(
        &workdir,
        config,
        &items,
        &format!("/tag/{}/{}", path.0, path.1),
    )
}

#[derive(Clone)]
pub struct ArchiveYearMonth {
    pub year: i32,
    pub month: u8,
    pub count: usize,
}

#[derive(Clone)]
pub struct ArchiveYear {
    pub year: i32,
    pub months: Vec<ArchiveYearMonth>,
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

    let mut archive_year_months: Vec<ArchiveYearMonth> = archive
        .iter()
        .map(|((year, month), count)| ArchiveYearMonth {
            year: *year,
            month: *month,
            count: *count,
        })
        .collect();
    archive_year_months.sort_by_key(|item| (-item.year, -(item.month as i32)));

    let mut archive_years: Vec<ArchiveYear> = archive_year_months
        .iter()
        .group_by(|item| item.year)
        .into_iter()
        .map(|(year, items)| ArchiveYear {
            year: year,
            months: items.cloned().collect(),
        })
        .collect();
    archive_years.sort_by_key(|item| -item.year);

    renderer.render_archive_page(&workdir, &archive_years, &format!("/archive"))
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
        per_page: 5000, // TODO: Probably just want to show all items?
        total: items.len(),
    };
    let items = apply_selection(&items, &config);

    renderer.render_listing_page(
        &workdir,
        config,
        &items,
        &format!("/archive/{}/{}", page.0, page.1),
    )
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

    let file_id = {
        item.flat_files()
            .into_iter()
            .filter(|(_, file)| file.is_downloaded())
            .collect::<IndexMap<String, FileCrawlType>>()
            .keys()
            .next()
            .unwrap()
            .to_string()
    };

    HttpResponse::SeeOther()
        .append_header((
            "Location",
            format!(
                "/{}/{}/item/{}/{}",
                workdir_prefix.0,
                renderer.get_prefix(),
                encode(&id),
                encode(&file_id)
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

    let file = { item.flat_files().get(&file_id).unwrap().clone() };

    renderer.render_detail_page(
        &workdir,
        &item,
        &file,
        &format!("/item/{}/{}", encode(&id), encode(&file_id)),
    )
}

#[get("/item-full/{id}/{file_id}")]
pub async fn generic_detail_full_handler(
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

    let file = { item.flat_files().get(&file_id).unwrap().clone() };

    renderer.render_detail_full_page(
        &workdir,
        &item,
        &file,
        &format!("/item-full/{}/{}", encode(&id), encode(&file_id)),
    )
}

#[get("/crawled.json")]
pub async fn serve_crawled_json(
    workdir: web::Data<ThreadSafeWorkDir>,
) -> Result<impl Responder, actix_web::Error> {
    let workdir = get_workdir(&workdir)?;
    let json = serde_json::to_string(&workdir.crawled)
        .map_err(|e| actix_web::error::ErrorInternalServerError(e))?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(json))
}
