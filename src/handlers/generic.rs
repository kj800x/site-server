use std::collections::HashMap;

use actix_web::{get, web, HttpResponse, Responder};
use chrono::{DateTime, Datelike, TimeZone, Utc};
use indexmap::IndexMap;
use itertools::Itertools;
use rand::{seq::SliceRandom, SeedableRng};
use rand::rngs::StdRng;
use serde::Deserialize;
use urlencoding::encode;

use crate::{
    handlers::WorkDirPrefix,
    search::{evaluate_search_expr, parse_search_expr},
    site::{CrawlItem, CrawlTag, FileCrawlType},
};
use urlencoding::decode;

use super::{
    ListingPageConfig, ListingPageMode, ListingPageOrdering, PageUrlState, SiteRenderer, SiteRendererType,
    SiteSource, ViewMode,
};

fn resolve_listing_page(site_source: &SiteSource, mode: &ListingPageMode) -> Vec<CrawlItem> {
    let items = site_source.all_items();

    match mode {
        ListingPageMode::All => items,

        ListingPageMode::ByTag { tag } => items
            .into_iter()
            .filter(|item| item.tags.iter().map(|t| t.to_string()).any(|t| t == *tag))
            .collect(),

        ListingPageMode::ByMonth { year, month } => items
            .into_iter()
            .filter(|item| {
                let date = item.source_published;
                let date = DateTime::from_timestamp_millis(date).unwrap();
                date.year() as u32 == *year && date.month() as u32 == *month
            })
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

fn apply_ordering(items: &[CrawlItem], ordering: &ListingPageOrdering) -> Vec<CrawlItem> {
    let mut items = items.to_vec();
    match ordering {
        ListingPageOrdering::NewestFirst => items.sort_by_key(|item| -item.source_published),
        ListingPageOrdering::OldestFirst => items.sort_by_key(|item| item.source_published),
        ListingPageOrdering::Random => {
            // Use a deterministic seed based on the items themselves for consistency
            // This ensures the same random order is used across requests
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            items.len().hash(&mut hasher);
            for item in &items {
                item.key.hash(&mut hasher);
            }
            let seed = hasher.finish();
            let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
            items.shuffle(&mut rng);
        }
    };
    items
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
    site_source: web::Data<SiteSource>,
) -> impl Responder {
    let renderer = renderer.into_inner();
    let site_prefix = site_source.slug();
    let items = resolve_listing_page(&site_source, &ListingPageMode::All);
    let config = ListingPageConfig {
        mode: ListingPageMode::All,
        ordering: ListingPageOrdering::Random,
        page: 1,
        per_page: 15,
        total: items.len(),
    };
    let items = apply_selection(&items, &config);

    renderer.render_listing_page(&site_prefix, config, &items, &format!("/random"))
}

#[get("/latest")]
pub async fn generic_latest_handler(
    renderer: web::Data<SiteRendererType>,
    site_source: web::Data<SiteSource>,
) -> impl Responder {
    let renderer = renderer.into_inner();
    let site_prefix = site_source.slug();
    let items = resolve_listing_page(&site_source, &ListingPageMode::All);
    let config = ListingPageConfig {
        mode: ListingPageMode::All,
        ordering: ListingPageOrdering::NewestFirst,
        page: 1,
        per_page: 15,
        total: items.len(),
    };
    let items = apply_selection(&items, &config);

    renderer.render_listing_page(&site_prefix, config, &items, &format!("/latest"))
}

#[get("/latest/{page}")]
pub async fn generic_latest_page_handler(
    renderer: web::Data<SiteRendererType>,
    site_source: web::Data<SiteSource>,
    page: web::Path<usize>,
) -> impl Responder {
    let renderer = renderer.into_inner();
    let site_prefix = site_source.slug();
    let items = resolve_listing_page(&site_source, &ListingPageMode::All);
    let config = ListingPageConfig {
        mode: ListingPageMode::All,
        ordering: ListingPageOrdering::NewestFirst,
        page: page.clone(),
        per_page: 15,
        total: items.len(),
    };
    let items = apply_selection(&items, &config);

    renderer.render_listing_page(&site_prefix, config, &items, &format!("/latest/{page}"))
}

#[get("/oldest")]
pub async fn generic_oldest_handler(
    renderer: web::Data<SiteRendererType>,
    site_source: web::Data<SiteSource>,
) -> impl Responder {
    let renderer = renderer.into_inner();
    let site_prefix = site_source.slug();
    let items = resolve_listing_page(&site_source, &ListingPageMode::All);
    let config = ListingPageConfig {
        mode: ListingPageMode::All,
        ordering: ListingPageOrdering::OldestFirst,
        page: 1,
        per_page: 15,
        total: items.len(),
    };
    let items = apply_selection(&items, &config);

    renderer.render_listing_page(&site_prefix, config, &items, &format!("/oldest"))
}

#[get("/oldest/{page}")]
pub async fn generic_oldest_page_handler(
    renderer: web::Data<SiteRendererType>,
    site_source: web::Data<SiteSource>,
    page: web::Path<usize>,
) -> impl Responder {
    let renderer = renderer.into_inner();
    let site_prefix = site_source.slug();
    let items = resolve_listing_page(&site_source, &ListingPageMode::All);
    let config = ListingPageConfig {
        mode: ListingPageMode::All,
        ordering: ListingPageOrdering::OldestFirst,
        page: page.clone(),
        per_page: 15,
        total: items.len(),
    };
    let items = apply_selection(&items, &config);

    renderer.render_listing_page(&site_prefix, config, &items, &format!("/oldest/{page}"))
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
    site_source: web::Data<SiteSource>,
    query: web::Query<TagParam>,
) -> impl Responder {
    let renderer = renderer.into_inner();
    let site_prefix = site_source.slug();

    let sort = match &query.sort {
        Some(sort) => match sort.as_str() {
            "count" => TagSort::Count,
            "alpha" => TagSort::Alphabetical,
            _ => TagSort::Count,
        },
        None => TagSort::Count,
    };

    let tags = {
        let items = site_source.all_items();
        let mut tags: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for item in items {
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

    renderer.render_tags_page(&site_prefix, &tags, &tag_order, &format!("/tags"))
}
#[get("/tag/{tag}")]
pub async fn generic_tag_handler(
    renderer: web::Data<SiteRendererType>,
    site_source: web::Data<SiteSource>,
    tag: web::Path<String>,
) -> impl Responder {
    let renderer = renderer.into_inner();
    let site_prefix = site_source.slug();
    let items = resolve_listing_page(&site_source, &ListingPageMode::ByTag { tag: tag.clone() });
    let config = ListingPageConfig {
        mode: ListingPageMode::ByTag { tag: tag.clone() },
        ordering: ListingPageOrdering::NewestFirst,
        page: 1,
        per_page: 15,
        total: items.len(),
    };
    let items = apply_selection(&items, &config);

    renderer.render_listing_page(&site_prefix, config, &items, &format!("/tag/{tag}"))
}

#[get("/tag/{tag}/{page}")]
pub async fn generic_tag_page_handler(
    renderer: web::Data<SiteRendererType>,
    site_source: web::Data<SiteSource>,
    path: web::Path<(String, usize)>,
) -> impl Responder {
    let renderer = renderer.into_inner();
    let site_prefix = site_source.slug();
    let items = resolve_listing_page(
        &site_source,
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
        &site_prefix,
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
    site_source: web::Data<SiteSource>,
) -> impl Responder {
    let renderer = renderer.into_inner();
    let site_prefix = site_source.slug();
    let archive = {
        let items = site_source.all_items();
        let mut archive: HashMap<(i32, u8), usize> = HashMap::new();

        for item in items {
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

    renderer.render_archive_page(&site_prefix, &archive_years, &format!("/archive"))
}

#[get("/archive/{year}/{month}")]
pub async fn generic_archive_page_handler(
    renderer: web::Data<SiteRendererType>,
    site_source: web::Data<SiteSource>,
    page: web::Path<(usize, usize)>,
) -> impl Responder {
    let renderer = renderer.into_inner();
    let site_prefix = site_source.slug();
    let items = resolve_listing_page(
        &site_source,
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
        &site_prefix,
        config,
        &items,
        &format!("/archive/{}/{}", page.0, page.1),
    )
}

#[get("/item/{id}")]
pub async fn generic_detail_redirect(
    renderer: web::Data<SiteRendererType>,
    site_source: web::Data<SiteSource>,
    path: web::Path<String>,
) -> impl Responder {
    let id = path.into_inner();
    let renderer = renderer.into_inner();
    let site_prefix = site_source.slug();
    let item = site_source.get_item(&id).unwrap();

    let file_id = super::common::get_first_downloaded_file_id(&item)
        .expect("Item must have at least one downloaded file");

    HttpResponse::SeeOther()
        .append_header((
            "Location",
            format!(
                "/{}/{}/item/{}/{}",
                site_prefix,
                renderer.get_prefix(),
                encode(&id),
                encode(&file_id)
            ),
        ))
        .finish()
}

#[derive(serde::Deserialize)]
struct ViewModeQuery {
    view: Option<String>,
    file: Option<String>,
}

#[get("/item/{id}/{file_id}")]
pub async fn generic_detail_handler(
    renderer: web::Data<SiteRendererType>,
    site_source: web::Data<SiteSource>,
    path: web::Path<(String, String)>,
    query: web::Query<ViewModeQuery>,
) -> impl Responder {
    let (id, file_id) = path.into_inner();
    let renderer = renderer.into_inner();
    let site_prefix = site_source.slug();
    let item = site_source.get_item(&id).unwrap();

    let file = { item.flat_files().get(&file_id).unwrap().clone() };
    let is_full = query.view.as_deref() == Some("full");
    
    // Construct PageUrlState directly from handler context
    let url_state = PageUrlState::permalink(
        site_prefix.clone(),
        renderer.get_prefix().to_string(),
        id.clone(),
        file_id.clone(),
        if is_full { ViewMode::Full } else { ViewMode::Normal },
    );

    if is_full {
        renderer.render_detail_full_page(
            &site_prefix,
            &item,
            &file,
            &url_state,
        )
    } else {
        renderer.render_detail_page(
            &site_prefix,
            &item,
            &file,
            &url_state,
        )
    }
}


#[get("/crawled.json")]
pub async fn serve_crawled_json(
    site_source: web::Data<SiteSource>,
) -> Result<impl Responder, actix_web::Error> {
    let json = serde_json::to_string(&site_source.all_items())
        .map_err(|e| actix_web::error::ErrorInternalServerError(e))?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(json))
}

// Slideshow handlers
#[get("/latest/slideshow/{i}")]
pub async fn generic_latest_slideshow_redirect_handler(
    renderer: web::Data<SiteRendererType>,
    site_source: web::Data<SiteSource>,
    index: web::Path<usize>,
) -> impl Responder {
    let renderer = renderer.into_inner();
    let site_prefix = site_source.slug();
    let rendering_prefix = renderer.get_prefix();
    let i = index.into_inner();
    
    let items = resolve_listing_page(&site_source, &ListingPageMode::All);
    let ordered_items = apply_ordering(&items, &ListingPageOrdering::NewestFirst);
    
    if ordered_items.is_empty() || i == 0 || i > ordered_items.len() {
        return HttpResponse::NotFound().body("No items found");
    }
    
    let current_item = &ordered_items[i - 1];
    let file_id = super::common::get_first_downloaded_file_id(current_item);
    
    if let Some(file_id) = file_id {
        HttpResponse::SeeOther()
            .append_header(("Location", format!("/{}/{}/latest/slideshow/{}/{}", site_prefix, rendering_prefix, i, encode(&file_id))))
            .finish()
    } else {
        HttpResponse::NotFound().body("No file found for item")
    }
}

#[get("/latest/slideshow/{i}/{file_id}")]
pub async fn generic_latest_slideshow_handler(
    renderer: web::Data<SiteRendererType>,
    site_source: web::Data<SiteSource>,
    path: web::Path<(usize, String)>,
    query: web::Query<ViewModeQuery>,
) -> impl Responder {
    let (i, file_id_param) = path.into_inner();
    let renderer = renderer.into_inner();
    let site_prefix = site_source.slug();
    let rendering_prefix = renderer.get_prefix();

    if i == 0 {
        return HttpResponse::SeeOther()
            .append_header(("Location", format!("/{}/{}/latest/slideshow/1", site_prefix, rendering_prefix)))
            .finish();
    }

    let items = resolve_listing_page(&site_source, &ListingPageMode::All);
    let ordered_items = apply_ordering(&items, &ListingPageOrdering::NewestFirst);

    if ordered_items.is_empty() {
        return HttpResponse::NotFound().body("No items found");
    }

    if i > ordered_items.len() {
        return HttpResponse::SeeOther()
            .append_header(("Location", format!("/{}/{}/latest/slideshow/{}", site_prefix, rendering_prefix, ordered_items.len())))
            .finish();
    }

    let current_item = &ordered_items[i - 1];
    let prev_index = if i > 1 { Some(i - 1) } else { None };
    let next_index = if i < ordered_items.len() { Some(i + 1) } else { None };

    // Decode file_id from URL
    let decoded_file_id = match decode(&file_id_param) {
        Ok(decoded) => decoded.to_string(),
        Err(_) => {
            return HttpResponse::BadRequest().body("Invalid file ID encoding");
        }
    };

    // Verify the file exists in the item
    let file = match current_item.flat_files().get(&decoded_file_id) {
        Some(f) if f.is_downloaded() => f.clone(),
        _ => {
            // File not found or not downloaded, redirect to first file
            if let Some(first_file_id) = super::common::get_first_downloaded_file_id(current_item) {
                return HttpResponse::SeeOther()
                    .append_header(("Location", format!("/{}/{}/latest/slideshow/{}/{}", site_prefix, rendering_prefix, i, encode(&first_file_id))))
                    .finish();
            } else {
                return HttpResponse::NotFound().body("No file found for item");
            }
        }
    };

    let is_full = query.view.as_deref() == Some("full");
    let config = ListingPageConfig {
        mode: ListingPageMode::All,
        ordering: ListingPageOrdering::NewestFirst,
        page: 1,
        per_page: 15,
        total: ordered_items.len(),
    };
    
    // Construct PageUrlState directly from handler context
    let url_state = PageUrlState::slideshow(
        site_prefix.clone(),
        rendering_prefix.to_string(),
        &config,
        i,
        decoded_file_id.clone(),
        if is_full { ViewMode::Full } else { ViewMode::Normal },
    );
    let back_url = url_state.with_view_mode(ViewMode::Normal).to_url();
    // For prev/next URLs, we need to get the first file of those items
    let prev_url = prev_index.and_then(|idx| {
        let prev_item = ordered_items.get(idx - 1)?;
        let prev_file_id = super::common::get_first_downloaded_file_id(prev_item)?;
        Some(PageUrlState::slideshow(
            site_prefix.clone(),
            rendering_prefix.to_string(),
            &config,
            idx,
            prev_file_id,
            if is_full { ViewMode::Full } else { ViewMode::Normal },
        ).to_url())
    });
    let next_url = next_index.and_then(|idx| {
        let next_item = ordered_items.get(idx - 1)?;
        let next_file_id = super::common::get_first_downloaded_file_id(next_item)?;
        Some(PageUrlState::slideshow(
            site_prefix.clone(),
            rendering_prefix.to_string(),
            &config,
            idx,
            next_file_id,
            if is_full { ViewMode::Full } else { ViewMode::Normal },
        ).to_url())
    });

    let markup = if is_full {
        renderer.render_slideshow_full_page(
            &site_prefix,
            current_item,
            &file,
            &url_state,
            prev_url.as_deref(),
            next_url.as_deref(),
            &back_url,
        )
    } else {
        renderer.render_slideshow_detail_page(
            &site_prefix,
            current_item,
            &file,
            &url_state,
            prev_url.as_deref(),
            next_url.as_deref(),
        )
    };
    HttpResponse::Ok().body(markup.0)
}

#[get("/oldest/slideshow/{i}")]
pub async fn generic_oldest_slideshow_redirect_handler(
    renderer: web::Data<SiteRendererType>,
    site_source: web::Data<SiteSource>,
    index: web::Path<usize>,
) -> impl Responder {
    let renderer = renderer.into_inner();
    let site_prefix = site_source.slug();
    let rendering_prefix = renderer.get_prefix();
    let i = index.into_inner();
    
    let items = resolve_listing_page(&site_source, &ListingPageMode::All);
    let ordered_items = apply_ordering(&items, &ListingPageOrdering::OldestFirst);
    
    if ordered_items.is_empty() || i == 0 || i > ordered_items.len() {
        return HttpResponse::NotFound().body("No items found");
    }
    
    let current_item = &ordered_items[i - 1];
    let file_id = super::common::get_first_downloaded_file_id(current_item);
    
    if let Some(file_id) = file_id {
        HttpResponse::SeeOther()
            .append_header(("Location", format!("/{}/{}/oldest/slideshow/{}/{}", site_prefix, rendering_prefix, i, encode(&file_id))))
            .finish()
    } else {
        HttpResponse::NotFound().body("No file found for item")
    }
}

#[get("/oldest/slideshow/{i}/{file_id}")]
pub async fn generic_oldest_slideshow_handler(
    renderer: web::Data<SiteRendererType>,
    site_source: web::Data<SiteSource>,
    path: web::Path<(usize, String)>,
    query: web::Query<ViewModeQuery>,
) -> impl Responder {
    let (i, file_id_param) = path.into_inner();
    let renderer = renderer.into_inner();
    let site_prefix = site_source.slug();
    let rendering_prefix = renderer.get_prefix();

    if i == 0 {
        return HttpResponse::SeeOther()
            .append_header(("Location", format!("/{}/{}/oldest/slideshow/1", site_prefix, rendering_prefix)))
            .finish();
    }

    let items = resolve_listing_page(&site_source, &ListingPageMode::All);
    let ordered_items = apply_ordering(&items, &ListingPageOrdering::OldestFirst);

    if ordered_items.is_empty() {
        return HttpResponse::NotFound().body("No items found");
    }

    if i > ordered_items.len() {
        return HttpResponse::SeeOther()
            .append_header(("Location", format!("/{}/{}/oldest/slideshow/{}", site_prefix, rendering_prefix, ordered_items.len())))
            .finish();
    }

    let current_item = &ordered_items[i - 1];
    let prev_index = if i > 1 { Some(i - 1) } else { None };
    let next_index = if i < ordered_items.len() { Some(i + 1) } else { None };

    // Decode file_id from URL
    let decoded_file_id = match decode(&file_id_param) {
        Ok(decoded) => decoded.to_string(),
        Err(_) => {
            return HttpResponse::BadRequest().body("Invalid file ID encoding");
        }
    };

    // Verify the file exists in the item
    let file = match current_item.flat_files().get(&decoded_file_id) {
        Some(f) if f.is_downloaded() => f.clone(),
        _ => {
            // File not found or not downloaded, redirect to first file
            if let Some(first_file_id) = super::common::get_first_downloaded_file_id(current_item) {
                return HttpResponse::SeeOther()
                    .append_header(("Location", format!("/{}/{}/oldest/slideshow/{}/{}", site_prefix, rendering_prefix, i, encode(&first_file_id))))
                    .finish();
            } else {
                return HttpResponse::NotFound().body("No file found for item");
            }
        }
    };

    let is_full = query.view.as_deref() == Some("full");
    let config = ListingPageConfig {
        mode: ListingPageMode::All,
        ordering: ListingPageOrdering::OldestFirst,
        page: 1,
        per_page: 15,
        total: ordered_items.len(),
    };
    
    // Construct PageUrlState directly from handler context
    let url_state = PageUrlState::slideshow(
        site_prefix.clone(),
        rendering_prefix.to_string(),
        &config,
        i,
        decoded_file_id.clone(),
        if is_full { ViewMode::Full } else { ViewMode::Normal },
    );
    let back_url = url_state.with_view_mode(ViewMode::Normal).to_url();
    
    // For prev/next URLs, we need to get the first file of those items
    let prev_url = prev_index.and_then(|idx| {
        let prev_item = ordered_items.get(idx - 1)?;
        let prev_file_id = super::common::get_first_downloaded_file_id(prev_item)?;
        Some(PageUrlState::slideshow(
            site_prefix.clone(),
            rendering_prefix.to_string(),
            &config,
            idx,
            prev_file_id,
            if is_full { ViewMode::Full } else { ViewMode::Normal },
        ).to_url())
    });
    let next_url = next_index.and_then(|idx| {
        let next_item = ordered_items.get(idx - 1)?;
        let next_file_id = super::common::get_first_downloaded_file_id(next_item)?;
        Some(PageUrlState::slideshow(
            site_prefix.clone(),
            rendering_prefix.to_string(),
            &config,
            idx,
            next_file_id,
            if is_full { ViewMode::Full } else { ViewMode::Normal },
        ).to_url())
    });

    let markup = if is_full {
        renderer.render_slideshow_full_page(
            &site_prefix,
            current_item,
            &file,
            &url_state,
            prev_url.as_deref(),
            next_url.as_deref(),
            &back_url,
        )
    } else {
        renderer.render_slideshow_detail_page(
            &site_prefix,
            current_item,
            &file,
            &url_state,
            prev_url.as_deref(),
            next_url.as_deref(),
        )
    };
    HttpResponse::Ok().body(markup.0)
}

#[get("/search/{query}/slideshow/{i}")]
pub async fn generic_search_slideshow_redirect_handler(
    renderer: web::Data<SiteRendererType>,
    site_source: web::Data<SiteSource>,
    path: web::Path<(String, usize)>,
) -> impl Responder {
    let (encoded_query, i) = path.into_inner();
    let renderer = renderer.into_inner();
    let site_prefix = site_source.slug();
    let rendering_prefix = renderer.get_prefix();
    
    // Decode the query
    let decoded_query = match decode(&encoded_query) {
        Ok(decoded) => decoded.to_string(),
        Err(_) => {
            return HttpResponse::BadRequest().body("Invalid URL encoding in search query");
        }
    };

    // Parse the s-expression
    let expr = match parse_search_expr(&decoded_query) {
        Ok(expr) => expr,
        Err(e) => {
            return HttpResponse::BadRequest().body(format!("Parse error: {}", e));
        }
    };

    // Get all items and filter
    let all_items: Vec<CrawlItem> = site_source.all_items();
    let filtered_items: Vec<CrawlItem> = all_items
        .into_iter()
        .filter(|item| evaluate_search_expr(&expr, item))
        .collect();

    // Sort by source_published (newest first)
    let mut sorted_items = filtered_items;
    sorted_items.sort_by_key(|item| -item.source_published);
    
    if sorted_items.is_empty() || i == 0 || i > sorted_items.len() {
        return HttpResponse::NotFound().body("No items found");
    }
    
    let current_item = &sorted_items[i - 1];
    let file_id = current_item
        .flat_files()
        .into_iter()
        .filter(|(_, file)| file.is_downloaded())
        .collect::<IndexMap<String, FileCrawlType>>()
        .keys()
        .next()
        .cloned();
    
    if let Some(file_id) = file_id {
        HttpResponse::SeeOther()
            .append_header(("Location", format!("/{}/{}/search/{}/slideshow/{}/{}", site_prefix, rendering_prefix, encoded_query, i, encode(&file_id))))
            .finish()
    } else {
        HttpResponse::NotFound().body("No file found for item")
    }
}

#[get("/search/{query}/slideshow/{i}/{file_id}")]
pub async fn generic_search_slideshow_handler(
    renderer: web::Data<SiteRendererType>,
    site_source: web::Data<SiteSource>,
    path: web::Path<(String, usize, String)>,
    query: web::Query<ViewModeQuery>,
) -> impl Responder {
    let (encoded_query, i, file_id_param) = path.into_inner();
    let renderer = renderer.into_inner();
    let site_prefix = site_source.slug();
    let rendering_prefix = renderer.get_prefix();

    if i == 0 {
        return HttpResponse::SeeOther()
            .append_header(("Location", format!("/{}/{}/search/{}/slideshow/1", site_prefix, rendering_prefix, encoded_query)))
            .finish();
    }

    if i == 0 {
        return HttpResponse::SeeOther()
            .append_header(("Location", format!("/{}/{}/search/{}/slideshow/1", site_prefix, rendering_prefix, encoded_query)))
            .finish();
    }

    // Decode the query
    let decoded_query = match decode(&encoded_query) {
        Ok(decoded) => decoded.to_string(),
        Err(_) => {
            return HttpResponse::BadRequest().body("Invalid URL encoding in search query");
        }
    };

    // Parse the s-expression
    let expr = match parse_search_expr(&decoded_query) {
        Ok(expr) => expr,
        Err(e) => {
            return HttpResponse::BadRequest().body(format!("Parse error: {}", e));
        }
    };

    // Get all items and filter
    let all_items: Vec<CrawlItem> = site_source.all_items();
    let filtered_items: Vec<CrawlItem> = all_items
        .into_iter()
        .filter(|item| evaluate_search_expr(&expr, item))
        .collect();

    // Sort by source_published (newest first)
    let mut sorted_items = filtered_items;
    sorted_items.sort_by_key(|item| -item.source_published);

    if sorted_items.is_empty() {
        return HttpResponse::NotFound().body("No items found");
    }

    if i > sorted_items.len() {
        return HttpResponse::SeeOther()
            .append_header(("Location", format!("/{}/{}/search/{}/slideshow/{}", site_prefix, rendering_prefix, encoded_query, sorted_items.len())))
            .finish();
    }

    let current_item = &sorted_items[i - 1];
    let prev_index = if i > 1 { Some(i - 1) } else { None };
    let next_index = if i < sorted_items.len() { Some(i + 1) } else { None };

    // Decode file_id from URL
    let decoded_file_id = match decode(&file_id_param) {
        Ok(decoded) => decoded.to_string(),
        Err(_) => {
            return HttpResponse::BadRequest().body("Invalid file ID encoding");
        }
    };

    // Verify the file exists in the item
    let file = match current_item.flat_files().get(&decoded_file_id) {
        Some(f) if f.is_downloaded() => f.clone(),
        _ => {
            // File not found or not downloaded, redirect to first file
            if let Some(first_file_id) = super::common::get_first_downloaded_file_id(current_item) {
                return HttpResponse::SeeOther()
                    .append_header(("Location", format!("/{}/{}/search/{}/slideshow/{}/{}", site_prefix, rendering_prefix, encoded_query, i, encode(&first_file_id))))
                    .finish();
            } else {
                return HttpResponse::NotFound().body("No file found for item");
            }
        }
    };

    let is_full = query.view.as_deref() == Some("full");
    let config = ListingPageConfig {
        mode: ListingPageMode::Search {
            query: encoded_query.clone(),
        },
        ordering: ListingPageOrdering::NewestFirst,
        page: 1,
        per_page: 15,
        total: sorted_items.len(),
    };
    
    // Construct PageUrlState directly from handler context
    let url_state = PageUrlState::slideshow(
        site_prefix.clone(),
        rendering_prefix.to_string(),
        &config,
        i,
        decoded_file_id.clone(),
        if is_full { ViewMode::Full } else { ViewMode::Normal },
    );
    let back_url = url_state.with_view_mode(ViewMode::Normal).to_url();
    
    // For prev/next URLs, we need to get the first file of those items
    let prev_url = prev_index.and_then(|idx| {
        let prev_item = sorted_items.get(idx - 1)?;
        let prev_file_id = prev_item
            .flat_files()
            .into_iter()
            .filter(|(_, file)| file.is_downloaded())
            .collect::<IndexMap<String, FileCrawlType>>()
            .keys()
            .next()?
            .clone();
        Some(PageUrlState::slideshow(
            site_prefix.clone(),
            rendering_prefix.to_string(),
            &config,
            idx,
            prev_file_id.clone(),
            if is_full { ViewMode::Full } else { ViewMode::Normal },
        ).to_url())
    });
    let next_url = next_index.and_then(|idx| {
        let next_item = sorted_items.get(idx - 1)?;
        let next_file_id = next_item
            .flat_files()
            .into_iter()
            .filter(|(_, file)| file.is_downloaded())
            .collect::<IndexMap<String, FileCrawlType>>()
            .keys()
            .next()?
            .clone();
        Some(PageUrlState::slideshow(
            site_prefix.clone(),
            rendering_prefix.to_string(),
            &config,
            idx,
            next_file_id.clone(),
            if is_full { ViewMode::Full } else { ViewMode::Normal },
        ).to_url())
    });

    let markup = if is_full {
        renderer.render_slideshow_full_page(
            &site_prefix,
            current_item,
            &file,
            &url_state,
            prev_url.as_deref(),
            next_url.as_deref(),
            &back_url,
        )
    } else {
        renderer.render_slideshow_detail_page(
            &site_prefix,
            current_item,
            &file,
            &url_state,
            prev_url.as_deref(),
            next_url.as_deref(),
        )
    };
    HttpResponse::Ok().body(markup.0)
}

#[get("/random/slideshow/{i}")]
pub async fn generic_random_slideshow_redirect_handler(
    renderer: web::Data<SiteRendererType>,
    site_source: web::Data<SiteSource>,
    index: web::Path<usize>,
) -> impl Responder {
    let renderer = renderer.into_inner();
    let site_prefix = site_source.slug();
    let rendering_prefix = renderer.get_prefix();
    let i = index.into_inner();
    
    let items = resolve_listing_page(&site_source, &ListingPageMode::All);
    let ordered_items = apply_ordering(&items, &ListingPageOrdering::Random);
    
    if ordered_items.is_empty() || i == 0 || i > ordered_items.len() {
        return HttpResponse::NotFound().body("No items found");
    }
    
    let current_item = &ordered_items[i - 1];
    let file_id = super::common::get_first_downloaded_file_id(current_item);
    
    if let Some(file_id) = file_id {
        HttpResponse::SeeOther()
            .append_header(("Location", format!("/{}/{}/random/slideshow/{}/{}", site_prefix, rendering_prefix, i, encode(&file_id))))
            .finish()
    } else {
        HttpResponse::NotFound().body("No file found for item")
    }
}

#[get("/random/slideshow/{i}/{file_id}")]
pub async fn generic_random_slideshow_handler(
    renderer: web::Data<SiteRendererType>,
    site_source: web::Data<SiteSource>,
    path: web::Path<(usize, String)>,
    query: web::Query<ViewModeQuery>,
) -> impl Responder {
    let (i, file_id_param) = path.into_inner();
    let renderer = renderer.into_inner();
    let site_prefix = site_source.slug();
    let rendering_prefix = renderer.get_prefix();

    if i == 0 {
        return HttpResponse::SeeOther()
            .append_header(("Location", format!("/{}/{}/random/slideshow/1", site_prefix, rendering_prefix)))
            .finish();
    }

    let items = resolve_listing_page(&site_source, &ListingPageMode::All);
    let ordered_items = apply_ordering(&items, &ListingPageOrdering::Random);

    if ordered_items.is_empty() {
        return HttpResponse::NotFound().body("No items found");
    }

    if i > ordered_items.len() {
        return HttpResponse::SeeOther()
            .append_header(("Location", format!("/{}/{}/random/slideshow/{}", site_prefix, rendering_prefix, ordered_items.len())))
            .finish();
    }

    let current_item = &ordered_items[i - 1];
    let prev_index = if i > 1 { Some(i - 1) } else { None };
    let next_index = if i < ordered_items.len() { Some(i + 1) } else { None };

    // Decode file_id from URL
    let decoded_file_id = match decode(&file_id_param) {
        Ok(decoded) => decoded.to_string(),
        Err(_) => {
            return HttpResponse::BadRequest().body("Invalid file ID encoding");
        }
    };

    // Verify the file exists in the item
    let file = match current_item.flat_files().get(&decoded_file_id) {
        Some(f) if f.is_downloaded() => f.clone(),
        _ => {
            // File not found or not downloaded, redirect to first file
            if let Some(first_file_id) = super::common::get_first_downloaded_file_id(current_item) {
                return HttpResponse::SeeOther()
                    .append_header(("Location", format!("/{}/{}/random/slideshow/{}/{}", site_prefix, rendering_prefix, i, encode(&first_file_id))))
                    .finish();
            } else {
                return HttpResponse::NotFound().body("No file found for item");
            }
        }
    };

    let is_full = query.view.as_deref() == Some("full");
    let config = ListingPageConfig {
        mode: ListingPageMode::All,
        ordering: ListingPageOrdering::Random,
        page: 1,
        per_page: 15,
        total: ordered_items.len(),
    };
    
    // Construct PageUrlState directly from handler context
    let url_state = PageUrlState::slideshow(
        site_prefix.clone(),
        rendering_prefix.to_string(),
        &config,
        i,
        decoded_file_id.clone(),
        if is_full { ViewMode::Full } else { ViewMode::Normal },
    );
    let back_url = url_state.with_view_mode(ViewMode::Normal).to_url();
    
    // For prev/next URLs, we need to get the first file of those items
    let prev_url = prev_index.and_then(|idx| {
        let prev_item = ordered_items.get(idx - 1)?;
        let prev_file_id = super::common::get_first_downloaded_file_id(prev_item)?;
        Some(PageUrlState::slideshow(
            site_prefix.clone(),
            rendering_prefix.to_string(),
            &config,
            idx,
            prev_file_id,
            if is_full { ViewMode::Full } else { ViewMode::Normal },
        ).to_url())
    });
    let next_url = next_index.and_then(|idx| {
        let next_item = ordered_items.get(idx - 1)?;
        let next_file_id = super::common::get_first_downloaded_file_id(next_item)?;
        Some(PageUrlState::slideshow(
            site_prefix.clone(),
            rendering_prefix.to_string(),
            &config,
            idx,
            next_file_id,
            if is_full { ViewMode::Full } else { ViewMode::Normal },
        ).to_url())
    });

    let markup = if is_full {
        renderer.render_slideshow_full_page(
            &site_prefix,
            current_item,
            &file,
            &url_state,
            prev_url.as_deref(),
            next_url.as_deref(),
            &back_url,
        )
    } else {
        renderer.render_slideshow_detail_page(
            &site_prefix,
            current_item,
            &file,
            &url_state,
            prev_url.as_deref(),
            next_url.as_deref(),
        )
    };
    HttpResponse::Ok().body(markup.0)
}

#[get("/tag/{tag}/slideshow/{i}")]
pub async fn generic_tag_slideshow_redirect_handler(
    renderer: web::Data<SiteRendererType>,
    site_source: web::Data<SiteSource>,
    path: web::Path<(String, usize)>,
) -> impl Responder {
    let (tag, i) = path.into_inner();
    let renderer = renderer.into_inner();
    let site_prefix = site_source.slug();
    let rendering_prefix = renderer.get_prefix();
    
    let items = resolve_listing_page(&site_source, &ListingPageMode::ByTag { tag: tag.clone() });
    let ordered_items = apply_ordering(&items, &ListingPageOrdering::NewestFirst);
    
    if ordered_items.is_empty() || i == 0 || i > ordered_items.len() {
        return HttpResponse::NotFound().body("No items found");
    }
    
    let current_item = &ordered_items[i - 1];
    let file_id = super::common::get_first_downloaded_file_id(current_item);
    
    if let Some(file_id) = file_id {
        HttpResponse::SeeOther()
            .append_header(("Location", format!("/{}/{}/tag/{}/slideshow/{}/{}", site_prefix, rendering_prefix, encode(&tag), i, encode(&file_id))))
            .finish()
    } else {
        HttpResponse::NotFound().body("No file found for item")
    }
}

#[get("/tag/{tag}/slideshow/{i}/{file_id}")]
pub async fn generic_tag_slideshow_handler(
    renderer: web::Data<SiteRendererType>,
    site_source: web::Data<SiteSource>,
    path: web::Path<(String, usize, String)>,
    query: web::Query<ViewModeQuery>,
) -> impl Responder {
    let (tag, i, file_id_param) = path.into_inner();
    let renderer = renderer.into_inner();
    let site_prefix = site_source.slug();
    let rendering_prefix = renderer.get_prefix();

    if i == 0 {
        return HttpResponse::SeeOther()
            .append_header(("Location", format!("/{}/{}/tag/{}/slideshow/1", site_prefix, rendering_prefix, encode(&tag))))
            .finish();
    }

    let items = resolve_listing_page(&site_source, &ListingPageMode::ByTag { tag: tag.clone() });
    let ordered_items = apply_ordering(&items, &ListingPageOrdering::NewestFirst);

    if ordered_items.is_empty() {
        return HttpResponse::NotFound().body("No items found");
    }

    if i > ordered_items.len() {
        return HttpResponse::SeeOther()
            .append_header(("Location", format!("/{}/{}/tag/{}/slideshow/{}", site_prefix, rendering_prefix, encode(&tag), ordered_items.len())))
            .finish();
    }

    let current_item = &ordered_items[i - 1];
    let prev_index = if i > 1 { Some(i - 1) } else { None };
    let next_index = if i < ordered_items.len() { Some(i + 1) } else { None };

    // Decode file_id from URL
    let decoded_file_id = match decode(&file_id_param) {
        Ok(decoded) => decoded.to_string(),
        Err(_) => {
            return HttpResponse::BadRequest().body("Invalid file ID encoding");
        }
    };

    // Verify the file exists in the item
    let file = match current_item.flat_files().get(&decoded_file_id) {
        Some(f) if f.is_downloaded() => f.clone(),
        _ => {
            // File not found or not downloaded, redirect to first file
            if let Some(first_file_id) = super::common::get_first_downloaded_file_id(current_item) {
                return HttpResponse::SeeOther()
                    .append_header(("Location", format!("/{}/{}/tag/{}/slideshow/{}/{}", site_prefix, rendering_prefix, encode(&tag), i, encode(&first_file_id))))
                    .finish();
            } else {
                return HttpResponse::NotFound().body("No file found for item");
            }
        }
    };

    let is_full = query.view.as_deref() == Some("full");
    let config = ListingPageConfig {
        mode: ListingPageMode::ByTag { tag: tag.clone() },
        ordering: ListingPageOrdering::NewestFirst,
        page: 1,
        per_page: 15,
        total: ordered_items.len(),
    };
    
    // Construct PageUrlState directly from handler context
    let url_state = PageUrlState::slideshow(
        site_prefix.clone(),
        rendering_prefix.to_string(),
        &config,
        i,
        decoded_file_id.clone(),
        if is_full { ViewMode::Full } else { ViewMode::Normal },
    );
    let back_url = url_state.with_view_mode(ViewMode::Normal).to_url();
    
    // For prev/next URLs, we need to get the first file of those items
    let prev_url = prev_index.and_then(|idx| {
        let prev_item = ordered_items.get(idx - 1)?;
        let prev_file_id = super::common::get_first_downloaded_file_id(prev_item)?;
        Some(PageUrlState::slideshow(
            site_prefix.clone(),
            rendering_prefix.to_string(),
            &config,
            idx,
            prev_file_id,
            if is_full { ViewMode::Full } else { ViewMode::Normal },
        ).to_url())
    });
    let next_url = next_index.and_then(|idx| {
        let next_item = ordered_items.get(idx - 1)?;
        let next_file_id = super::common::get_first_downloaded_file_id(next_item)?;
        Some(PageUrlState::slideshow(
            site_prefix.clone(),
            rendering_prefix.to_string(),
            &config,
            idx,
            next_file_id,
            if is_full { ViewMode::Full } else { ViewMode::Normal },
        ).to_url())
    });

    let markup = if is_full {
        renderer.render_slideshow_full_page(
            &site_prefix,
            current_item,
            &file,
            &url_state,
            prev_url.as_deref(),
            next_url.as_deref(),
            &back_url,
        )
    } else {
        renderer.render_slideshow_detail_page(
            &site_prefix,
            current_item,
            &file,
            &url_state,
            prev_url.as_deref(),
            next_url.as_deref(),
        )
    };
    HttpResponse::Ok().body(markup.0)
}

#[get("/archive/{year}/{month}/slideshow/{i}")]
pub async fn generic_archive_slideshow_redirect_handler(
    renderer: web::Data<SiteRendererType>,
    site_source: web::Data<SiteSource>,
    path: web::Path<(usize, usize, usize)>,
) -> impl Responder {
    let (year, month, i) = path.into_inner();
    let renderer = renderer.into_inner();
    let site_prefix = site_source.slug();
    let rendering_prefix = renderer.get_prefix();
    
    let items = resolve_listing_page(
        &site_source,
        &ListingPageMode::ByMonth {
            year: year as u32,
            month: month as u32,
        },
    );
    let ordered_items = apply_ordering(&items, &ListingPageOrdering::NewestFirst);
    
    if ordered_items.is_empty() || i == 0 || i > ordered_items.len() {
        return HttpResponse::NotFound().body("No items found");
    }
    
    let current_item = &ordered_items[i - 1];
    let file_id = super::common::get_first_downloaded_file_id(current_item);
    
    if let Some(file_id) = file_id {
        HttpResponse::SeeOther()
            .append_header(("Location", format!("/{}/{}/archive/{}/{}/slideshow/{}/{}", site_prefix, rendering_prefix, year, month, i, encode(&file_id))))
            .finish()
    } else {
        HttpResponse::NotFound().body("No file found for item")
    }
}

#[get("/archive/{year}/{month}/slideshow/{i}/{file_id}")]
pub async fn generic_archive_slideshow_handler(
    renderer: web::Data<SiteRendererType>,
    site_source: web::Data<SiteSource>,
    path: web::Path<(usize, usize, usize, String)>,
    query: web::Query<ViewModeQuery>,
) -> impl Responder {
    let (year, month, i, file_id_param) = path.into_inner();
    let renderer = renderer.into_inner();
    let site_prefix = site_source.slug();
    let rendering_prefix = renderer.get_prefix();

    if i == 0 {
        return HttpResponse::SeeOther()
            .append_header(("Location", format!("/{}/{}/archive/{}/{}/slideshow/1", site_prefix, rendering_prefix, year, month)))
            .finish();
    }

    let items = resolve_listing_page(
        &site_source,
        &ListingPageMode::ByMonth {
            year: year as u32,
            month: month as u32,
        },
    );
    let ordered_items = apply_ordering(&items, &ListingPageOrdering::NewestFirst);

    if ordered_items.is_empty() {
        return HttpResponse::NotFound().body("No items found");
    }

    if i > ordered_items.len() {
        return HttpResponse::SeeOther()
            .append_header(("Location", format!("/{}/{}/archive/{}/{}/slideshow/{}", site_prefix, rendering_prefix, year, month, ordered_items.len())))
            .finish();
    }

    let current_item = &ordered_items[i - 1];
    let prev_index = if i > 1 { Some(i - 1) } else { None };
    let next_index = if i < ordered_items.len() { Some(i + 1) } else { None };

    // Decode file_id from URL
    let decoded_file_id = match decode(&file_id_param) {
        Ok(decoded) => decoded.to_string(),
        Err(_) => {
            return HttpResponse::BadRequest().body("Invalid file ID encoding");
        }
    };

    // Verify the file exists in the item
    let file = match current_item.flat_files().get(&decoded_file_id) {
        Some(f) if f.is_downloaded() => f.clone(),
        _ => {
            // File not found or not downloaded, redirect to first file
            if let Some(first_file_id) = super::common::get_first_downloaded_file_id(current_item) {
                return HttpResponse::SeeOther()
                    .append_header(("Location", format!("/{}/{}/archive/{}/{}/slideshow/{}/{}", site_prefix, rendering_prefix, year, month, i, encode(&first_file_id))))
                    .finish();
            } else {
                return HttpResponse::NotFound().body("No file found for item");
            }
        }
    };

    let is_full = query.view.as_deref() == Some("full");
    let config = ListingPageConfig {
        mode: ListingPageMode::ByMonth {
            year: year as u32,
            month: month as u32,
        },
        ordering: ListingPageOrdering::NewestFirst,
        page: 1,
        per_page: 15,
        total: ordered_items.len(),
    };
    
    // Construct PageUrlState directly from handler context
    let url_state = PageUrlState::slideshow(
        site_prefix.clone(),
        rendering_prefix.to_string(),
        &config,
        i,
        decoded_file_id.clone(),
        if is_full { ViewMode::Full } else { ViewMode::Normal },
    );
    let back_url = url_state.with_view_mode(ViewMode::Normal).to_url();
    
    // For prev/next URLs, we need to get the first file of those items
    let prev_url = prev_index.and_then(|idx| {
        let prev_item = ordered_items.get(idx - 1)?;
        let prev_file_id = super::common::get_first_downloaded_file_id(prev_item)?;
        Some(PageUrlState::slideshow(
            site_prefix.clone(),
            rendering_prefix.to_string(),
            &config,
            idx,
            prev_file_id,
            if is_full { ViewMode::Full } else { ViewMode::Normal },
        ).to_url())
    });
    let next_url = next_index.and_then(|idx| {
        let next_item = ordered_items.get(idx - 1)?;
        let next_file_id = super::common::get_first_downloaded_file_id(next_item)?;
        Some(PageUrlState::slideshow(
            site_prefix.clone(),
            rendering_prefix.to_string(),
            &config,
            idx,
            next_file_id,
            if is_full { ViewMode::Full } else { ViewMode::Normal },
        ).to_url())
    });

    let markup = if is_full {
        renderer.render_slideshow_full_page(
            &site_prefix,
            current_item,
            &file,
            &url_state,
            prev_url.as_deref(),
            next_url.as_deref(),
            &back_url,
        )
    } else {
        renderer.render_slideshow_detail_page(
            &site_prefix,
            current_item,
            &file,
            &url_state,
            prev_url.as_deref(),
            next_url.as_deref(),
        )
    };
    HttpResponse::Ok().body(markup.0)
}
