use actix_web::{get, web, Responder};
use indexmap::IndexMap;
use maud::{html, Markup};
use std::collections::HashMap;
use urlencoding::encode;

use super::{ArchiveYear, ListingPageConfig, ListingPageMode, ListingPageOrdering, SiteSource};
use crate::collections::GetKey;
use crate::handlers::{format_year_month, timeago, ExtensionFix, Fa, PaginatorPrefix};
use crate::site::{CrawlItem, CrawlTag, FileCrawlType};

fn reddit_layout(title: &str, content: Markup, site: &str, route: &str) -> Markup {
    html! {
        (maud::DOCTYPE)
        html {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1" {}
                (super::scripts())
                title { (title) }
            }
            body hx-ext="morph" {
                (super::header(site, "r", route))
                .reddit_layout {
                    main.reddit_main {
                        (content)
                    }
                }
            }
        }
    }
}

fn reddit_layout_full(title: &str, content: Markup, __site: &str, __route: &str) -> Markup {
    html! {
        (maud::DOCTYPE)
        html {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1" {}
                (super::scripts())
                title { (title) }
            }
            body hx-ext="morph" {
                main.reddit_layout_full {
                    (content)
                }
            }
        }
    }
}

fn file_counts(item: &CrawlItem) -> Markup {
    let image_count = item.flat_files().iter().filter(|x| x.1.is_image()).count();
    let video_count = item.flat_files().iter().filter(|x| x.1.is_video()).count();

    html! {
        span.post_file_counts {
            @if image_count > 0 {
                (image_count) " " (Fa("camera"))
            }
            @if image_count > 0 && video_count > 0 {
                " "
            }
            @if video_count > 0 {
                (video_count) " " (Fa("video-camera"))
            }
        }
    }
}

fn reddit_post_card(item: &CrawlItem, site_prefix: &str) -> Markup {
    // Read display settings from item.site_settings
    let forced_author = &item.site_settings.forced_author;
    let hide_titles = item.site_settings.hide_titles;
    // Use item's source site for asset paths
    let asset_site = &item.site_settings.site_slug;

    let post_href = format!("/{}/r/item/{}", site_prefix, encode(&item.key));
    let title_id = format!("post-title-{}", encode(&item.key));

    html! {
        article.reddit_post_card {
            // Overlay link (covers the whole card)
            a.card_overlay
                href=(post_href)
                aria-labelledby=(title_id) {}

            header.post_header {
                span.post_author {
                    @if let Some(author) = item.meta.get("author").iter().flat_map(|x| x.as_str()).next() {
                        (author)
                    } @else if let Some(forced_author) = forced_author.as_ref() {
                        (forced_author)
                    } @else {
                        (asset_site)
                    }
                }
                span.post_time { (timeago(item.source_published as u64)) }
                (file_counts(item))
            }

            .post_content {
                @if !hide_titles {
                    h2.post_title id=(title_id) { (item.title) }
                }

                .post_tags {
                    @for tag in &item.tags {
                        @match tag {
                            CrawlTag::Simple(x) =>
                                a.post_tag href=(format!("/{}/r/tag/{}", site_prefix, encode(x))) { (x) },
                            CrawlTag::Detailed { value, .. } =>
                                a.post_tag href=(format!("/{}/r/tag/{}", site_prefix, encode(value))) { (value) },
                        }
                    }
                }

                @if let Some(thumb) = item.thumbnail_path() {
                    @if thumb.ends_with(".mp4") {
                        .post_preview {
                            video.thumbnail_preview width="320" height="auto" autoplay loop muted playsinline {
                                source src=(format!("/{}/assets/{}", asset_site, thumb)) {}
                            }
                        }
                    } @else {
                        .post_preview {
                            img src=(format!("/{}/assets/{}", asset_site, thumb)) alt=(item.title) {}
                        }
                    }
                }
            }
        }
        div.post_separator {}
    }
}

// Public functions required by SiteRenderer trait
pub fn render_listing_page(
    site_prefix: &str,
    config: ListingPageConfig,
    items: &[CrawlItem],
    route: &str,
) -> Markup {
    let title = match &config.mode {
        ListingPageMode::All => match config.ordering {
            ListingPageOrdering::NewestFirst => "Newest Posts".to_string(),
            ListingPageOrdering::OldestFirst => "Oldest Posts".to_string(),
            ListingPageOrdering::Random => "Random Posts".to_string(),
        },
        ListingPageMode::ByTag { tag } => format!("Posts tagged \"{}\"", tag),
        ListingPageMode::ByMonth { year, month } => {
            format!(
                "Posts from {}",
                format_year_month(*year as i32, *month as u8)
            )
        }
        ListingPageMode::Search { query } => format!("Search: {}", query),
    };

    let content = html! {
        .reddit_posts_container {
            @if !title.is_empty() && !matches!(config.mode, ListingPageMode::All) {
                h1.page_title { (title) }
            }
            .reddit_posts {
                @for item in items {
                    (reddit_post_card(item, site_prefix))
                }
            }
            // FIXME: Don't include a paginator if the sort order is random
            (super::paginator(config.page, config.total, config.per_page, &config.paginator_prefix(site_prefix, "r")))
        }
        .reddit_right_bar {}
    };

    reddit_layout(&title, content, site_prefix, route)
}

fn get_file_index_info(item: &CrawlItem, current_file: &FileCrawlType) -> Option<(usize, usize)> {
    let flat_files = item
        .flat_files()
        .into_iter()
        .filter(|(_, file)| file.is_downloaded())
        .collect::<IndexMap<String, FileCrawlType>>();

    let total = flat_files.len();
    if total == 0 {
        return None;
    }

    let current_file_index_0based = flat_files.get_index_of(current_file.get_key())?;
    let current_file_index_1based = current_file_index_0based + 1;

    Some((current_file_index_1based, total))
}

pub fn post_file_paginator(
    item: &CrawlItem,
    site: &str,
    route_base: &str,
    current_file: &FileCrawlType,
) -> Markup {
    let flat_files = item
        .flat_files()
        .into_iter()
        .filter(|(_, file)| file.is_downloaded())
        .collect::<IndexMap<String, FileCrawlType>>();

    let current_file_index = flat_files.get_index_of(current_file.get_key()).unwrap();
    let prev_file = flat_files.get_index(current_file_index.wrapping_sub(1));
    let next_file = flat_files.get_index(current_file_index.wrapping_add(1));

    html! {
        div.post_file_paginator {
            @if let Some(prev_file) = prev_file {
                a.prev
                    href=(format!("/{}/r/{}/{}/{}", site, route_base, encode(&item.key), encode(&prev_file.0)))
                    data-is-prev
                    data-replace-history
                    // hx-get=(format!("/{}/r/item-fragment/{}/{}", site, encode(&item.key), encode(&prev_file.0)))
                    // hx-trigger="click, keyup[key=ArrowLeft] from:body once"
                    // hx-target="closest .media_viewer"
                    // hx-swap="morph"
                {
                    "‹"
                }
            }
            @if let Some(next_file) = next_file {
                a.next
                    href=(format!("/{}/r/{}/{}/{}", site, route_base, encode(&item.key), encode(&next_file.0)))
                    data-is-next
                    data-replace-history
                    // hx-get=(format!("/{}/r/item-fragment/{}/{}", site, encode(&item.key), encode(&next_file.0)))
                    // hx-trigger="click, keyup[key=ArrowRight] from:body once"
                    // hx-target="closest .media_viewer"
                    // hx-swap="morph"
                {
                    "›"
                }
            }
        }
    }
}

pub fn render_media_viewer(site_prefix: &str, item: &CrawlItem, file: &FileCrawlType) -> Markup {
    let index_info = get_file_index_info(item, file);
    // Use item's source site for asset paths
    let asset_site = &item.site_settings.site_slug;
    html!(
        .media_viewer {
            @if let Some((current, total)) = index_info {
                span.media_viewer_counter {
                    (current) " / " (total)
                }
            }
            @match file {
                FileCrawlType::Image { filename, downloaded, .. } => {
                    @if *downloaded {
                        figure.post_figure {
                            img.post_image src=(format!("/{}/assets/{}", asset_site, filename)) alt=(item.title) {}
                            a.fullscreen_click_target data-replace-history href=(format!("/{}/r/item-full/{}/{}", site_prefix, encode(&item.key), encode(&file.get_key()))) {}
                            (post_file_paginator(item, site_prefix, "item", &file))
                        }
                    }
                }
                FileCrawlType::Video { filename, downloaded, .. } => {
                    @if *downloaded {
                        @let coerced_filename = filename.as_mp4();
                        figure.post_figure {
                            video.post_video controls autoplay {
                                source src=(format!("/{}/assets/{}", asset_site, coerced_filename)) {}
                            }
                            a.fullscreen_link data-replace-history href=(format!("/{}/r/item-full/{}/{}", site_prefix, encode(&item.key), encode(&file.get_key()))) {
                                "⏶"
                            }
                            (post_file_paginator(item, site_prefix, "item", &file))
                        }
                    }
                }
                _ => {}
            }
        }
    )
}

#[get("/item-fragment/{id}/{file_id}")]
pub async fn media_viewer_fragment_handler(
    site_source: web::Data<SiteSource>,
    path: web::Path<(String, String)>,
) -> impl Responder {
    let (id, file_id) = path.into_inner();
    let site_prefix = site_source.slug();
    let item = site_source.get_item(&id).unwrap();

    let file = { item.flat_files().get(&file_id).unwrap().clone() };

    render_media_viewer(&site_prefix, &item, &file)
}

pub fn render_full_media_viewer(
    site_prefix: &str,
    item: &CrawlItem,
    file: &FileCrawlType,
) -> Markup {
    let index_info = get_file_index_info(item, file);
    // Use item's source site for asset paths
    let asset_site = &item.site_settings.site_slug;
    html!(
        .media_viewer {
            @if let Some((current, total)) = index_info {
                span.media_viewer_counter {
                    (current) " / " (total)
                }
            }
            @match file {
                FileCrawlType::Image { filename, downloaded, .. } => {
                    @if *downloaded {
                        figure.post_figure {
                            img.post_image src=(format!("/{}/assets/{}", asset_site, filename)) alt=(item.title) {}
                            (post_file_paginator(item, site_prefix, "item-full", &file))
                        }
                    }
                }
                FileCrawlType::Video { filename, downloaded, .. } => {
                    @if *downloaded {
                        @let coerced_filename = filename.as_mp4();
                        figure.post_figure {
                            video.post_video controls autoplay {
                                source src=(format!("/{}/assets/{}", asset_site, coerced_filename)) {}
                            }
                            (post_file_paginator(item, site_prefix, "item-full", &file))
                        }
                    }
                }
                _ => {}
            }
            a.quit
                href=(format!("/{}/r/item/{}/{}", site_prefix, encode(&item.key), encode(&file.get_key())))
                data-is-quit
                data-replace-history
            {
                "🗙"
            }
        }
    )
}

pub fn render_detail_page(
    site_prefix: &str,
    item: &CrawlItem,
    file: &FileCrawlType,
    route: &str,
) -> Markup {
    // Read display settings from item.site_settings
    let forced_author = &item.site_settings.forced_author;

    let content = html! {
        article.reddit_post_detail {
            header.post_header {
                span.post_author {
                    @if let Some(author) = item.meta.get("author").iter().flat_map(|x| x.as_str()).next() {
                        (author)
                    } @else if let Some(forced_author) = forced_author.as_ref() {
                        (forced_author)
                    } @else {
                        "unknown"
                    }
                }
                span.post_time { (timeago(item.source_published as u64)) }
                (file_counts(item))
            }
            h1.post_title { (item.title) }
            .post_content {
                (render_media_viewer(site_prefix, &item, &file))

                .post_description {
                    p { (item.description) }
                }

                .post_tags {
                    @for tag in &item.tags {
                        @match tag {
                            CrawlTag::Simple(x) =>
                                a.post_tag href=(format!("/{}/r/tag/{}", site_prefix, encode(x))) { (x) },
                            CrawlTag::Detailed { value, .. } =>
                                a.post_tag href=(format!("/{}/r/tag/{}", site_prefix, encode(value))) { (value) },
                        }
                    }
                }

                p.post_source {
                    "Source: "
                    a href=(item.url) { (item.url) }
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
            }
        }
    };

    reddit_layout(&item.title, content, site_prefix, route)
}

pub fn render_detail_full_page(
    site_prefix: &str,
    item: &CrawlItem,
    file: &FileCrawlType,
    route: &str,
) -> Markup {
    let content = html! {
        article.reddit_post_detail_full {
            (render_full_media_viewer(site_prefix, &item, &file))
        }
    };

    reddit_layout_full(&item.title, content, site_prefix, route)
}

pub fn render_tags_page(
    site_prefix: &str,
    tags: &HashMap<String, usize>,
    tag_order: &Vec<String>,
    route: &str,
) -> Markup {
    let content = html! {
        .tag_list_page {
            h2 { "Tags" }
            ul.tag_list {
                @for tag in tag_order {
                    li.tag_item {
                        a href=(format!("/{}/r/tag/{}", site_prefix, encode(tag))) {
                            span.tag_name { (tag) }
                            span.tag_count { " (" (tags.get(tag).unwrap_or(&0)) ")" }
                        }
                    }
                }
            }
        }
    };

    reddit_layout("Tags", content, site_prefix, route)
}

pub fn render_archive_page(site_prefix: &str, archive: &Vec<ArchiveYear>, route: &str) -> Markup {
    let archive_months = archive
        .iter()
        .map(|year| year.months.iter())
        .flatten()
        .collect::<Vec<_>>();

    let content = html! {
        .archive_page {
            h2 { "Archive" }
            ul.archive_list {
                @for month in archive_months {
                    li.archive_item {
                        a href=(format!("/{}/r/archive/{}/{:02}", site_prefix, month.year, month.month)) {
                            span.archive_date { (format_year_month(month.year, month.month)) }
                            span.archive_count { " (" (month.count) ")" }
                        }
                    }
                }
            }
        }
    };

    reddit_layout("Archive", content, site_prefix, route)
}
