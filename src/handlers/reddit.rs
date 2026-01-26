use actix_web::{get, web, Responder};
use indexmap::IndexMap;
use maud::{html, Markup};
use std::collections::HashMap;
use urlencoding::encode;

use super::{
    ArchiveYear, ListingPageConfig, ListingPageMode, ListingPageOrdering, PageUrlState,
    SiteSource, ViewMode,
};
use crate::collections::GetKey;
use crate::handlers::{
    calculate_item_index, format_year_month, timeago, ExtensionFix, Fa,
    PaginatorPrefix,
};
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

fn reddit_post_card(
    item: &CrawlItem,
    site_prefix: &str,
    config: &ListingPageConfig,
    position_in_page: usize,
) -> Markup {
    // Read display settings from item.site_settings
    let forced_author = &item.site_settings.forced_author;
    let hide_titles = item.site_settings.hide_titles;
    // Use item's source site for asset paths
    let asset_site = &item.site_settings.site_slug;

    let slideshow_index = calculate_item_index(config, position_in_page);
    let first_file_id = crate::handlers::common::get_first_downloaded_file_id(item)
        .unwrap_or_default();
    let post_href = PageUrlState::slideshow(
        site_prefix.to_string(),
        "r".to_string(),
        config,
        slideshow_index,
        first_file_id.clone(),
        ViewMode::Normal,
    ).to_url();
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
                @for (idx, item) in items.iter().enumerate() {
                    (reddit_post_card(item, site_prefix, &config, idx))
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
    current_file: &FileCrawlType,
    url_state: &PageUrlState,
) -> Markup {
    let flat_files = item
        .flat_files()
        .into_iter()
        .filter(|(_, file)| file.is_downloaded())
        .collect::<IndexMap<String, FileCrawlType>>();

    let current_file_index = flat_files.get_index_of(current_file.get_key()).unwrap();
    let prev_file = flat_files.get_index(current_file_index.wrapping_sub(1));
    let next_file = flat_files.get_index(current_file_index.wrapping_add(1));
    let first_file = flat_files.first();
    let last_file = flat_files.last();

    let file_url = |file_id: &str| {
        url_state.with_file_id(file_id.to_string()).to_url()
    };

    html! {
        div.post_file_paginator {
            @if let Some(first_file) = first_file {
                @if first_file.0 != current_file.get_key() {
                    a.first
                        href=(file_url(&first_file.0))
                        data-file-first
                        data-replace-history
                        style="display: none;"
                    {}
                }
            }
            @if let Some(last_file) = last_file {
                @if last_file.0 != current_file.get_key() {
                    a.last
                        href=(file_url(&last_file.0))
                        data-file-last
                        data-replace-history
                        style="display: none;"
                    {}
                }
            }
            @if let Some(prev_file) = prev_file {
                a.prev
                    href=(file_url(&prev_file.0))
                    data-file-prev
                    data-replace-history
                {
                    (Fa("chevron-left"))
                }
            } @else {
                span.prev { }
            }
            @if let Some(next_file) = next_file {
                a.next
                    href=(file_url(&next_file.0))
                    data-file-next
                    data-replace-history
                {
                    (Fa("chevron-right"))
                }
            } @else {
                span.next { }
            }
        }
    }
}

pub fn render_media_viewer(
    item: &CrawlItem,
    file: &FileCrawlType,
    url_state: &PageUrlState,
) -> Markup {
    let index_info = get_file_index_info(item, file);
    // Use item's source site for asset paths
    let asset_site = &item.site_settings.site_slug;
    let toggle_url = url_state.toggle_view_mode().to_url();

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
                            a.fullscreen_click_target data-toggle-full data-replace-history href=(toggle_url) {}
                            (post_file_paginator(item, &file, &url_state))
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
                            a.fullscreen_link data-toggle-full data-replace-history href=(toggle_url) {
                                (Fa("expand"))
                            }
                            (post_file_paginator(item, &file, url_state))
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

    let url_state = PageUrlState::permalink(
        site_prefix.clone(),
        "r".to_string(),
        id.clone(),
        file_id.clone(),
        ViewMode::Normal,
    );
    render_media_viewer(&item, &file, &url_state)
}

pub fn render_full_media_viewer(
    item: &CrawlItem,
    file: &FileCrawlType,
    url_state: &PageUrlState,
    quit_url: Option<&str>,
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
                            (post_file_paginator(item, &file, &url_state.with_view_mode(ViewMode::Full)))
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
                            (post_file_paginator(item, &file, &url_state.with_view_mode(ViewMode::Full)))
                        }
                    }
                }
                _ => {}
            }
            @let quit_href = if let Some(back) = quit_url {
                back.to_string()
            } else {
                url_state.to_permalink(item.key.clone()).with_view_mode(ViewMode::Normal).to_url()
            };
            a.quit
                href=(quit_href)
                data-is-quit
                data-toggle-full
                data-replace-history
            {
                (Fa("compress"))
            }
        }
    )
}

pub fn render_detail_page(
    site_prefix: &str,
    item: &CrawlItem,
    file: &FileCrawlType,
    url_state: &PageUrlState,
) -> Markup {
    // Read display settings from item.site_settings
    let forced_author = &item.site_settings.forced_author;

    let toggle_url = url_state.toggle_view_mode().to_url();

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
                @if item.flat_files().iter().filter(|(_, f)| f.is_downloaded()).count() > 0 {
                    a.toggle-full-link data-toggle-full data-replace-history href=(toggle_url) style="display: none;" {}
                }
                (render_media_viewer(&item, &file, &url_state))

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

    reddit_layout(&item.title, content, site_prefix, &url_state.to_route())
}

pub fn render_detail_full_page(
    site_prefix: &str,
    item: &CrawlItem,
    file: &FileCrawlType,
    url_state: &PageUrlState,
) -> Markup {
    let content = html! {
        article.reddit_post_detail_full {
            (render_full_media_viewer(&item, &file, &url_state, None))
        }
    };

    reddit_layout_full(&item.title, content, site_prefix, &url_state.to_route())
}

pub fn render_slideshow_full_page(
    site_prefix: &str,
    item: &CrawlItem,
    file: &FileCrawlType,
    url_state: &PageUrlState,
    prev_url: Option<&str>,
    next_url: Option<&str>,
    back_url: &str,
) -> Markup {
    let content = html! {
        article.reddit_post_detail_full {
            (render_full_media_viewer(&item, &file, &url_state, Some(back_url)))
            .slideshow_navigation {
                @if let Some(prev) = prev_url {
                    a.slideshow_prev href=(prev) data-item-prev data-replace-history { (Fa("chevron-left")) }
                }
                a.slideshow_back href=(back_url) data-is-quit data-toggle-full { (Fa("chevron-left")) }
                @if let Some(next) = next_url {
                    a.slideshow_next href=(next) data-item-next data-replace-history { (Fa("chevron-right")) }
                }
            }
        }
    };

    reddit_layout_full(&item.title, content, site_prefix, &url_state.to_route())
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

pub fn render_slideshow_detail_page(
    site_prefix: &str,
    item: &CrawlItem,
    file: &FileCrawlType,
    url_state: &PageUrlState,
    prev_url: Option<&str>,
    next_url: Option<&str>,
) -> Markup {
    // Read display settings from item.site_settings
    let forced_author = &item.site_settings.forced_author;

    // Get file_id for permalink
    let file_id = item
        .flat_files()
        .into_iter()
        .filter(|(_, f)| f.is_downloaded())
        .next()
        .map(|(id, _)| id);

    let toggle_url = url_state.toggle_view_mode().to_url();

    let content = html! {
        article.reddit_post_detail {
            .slideshow_navigation {
                @if let Some(prev) = prev_url {
                    a.slideshow_prev href=(prev) data-item-prev data-replace-history { (Fa("chevron-left")) }
                }
                @if let Some(file_id) = &file_id {
                    a.slideshow_permalink href=(format!("/{}/r/item/{}/{}", site_prefix, encode(&item.key), encode(file_id))) { (Fa("link")) }
                }
                @if let Some(next) = next_url {
                    a.slideshow_next href=(next) data-item-next data-replace-history { (Fa("chevron-right")) }
                }
            }
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
                @if item.flat_files().iter().filter(|(_, f)| f.is_downloaded()).count() > 0 {
                    a.toggle-full-link data-toggle-full data-replace-history href=(toggle_url) style="display: none;" {}
                }
                (render_media_viewer(&item, &file, &url_state))

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
                            .meta_item {
                                span.meta_key { (key) ": " }
                                span.meta_value { (value) }
                            }
                        }
                    }
                }
            }
            footer.post_footer {
                .slideshow_navigation {
                    @if let Some(prev) = prev_url {
                        a.slideshow_prev href=(prev) data-item-prev data-replace-history { "← Previous" }
                    }
                    @if let Some(file_id) = &file_id {
                        a.slideshow_permalink href=(format!("/{}/r/item/{}/{}", site_prefix, encode(&item.key), encode(file_id))) { "Permalink" }
                    }
                    @if let Some(next) = next_url {
                        a.slideshow_next href=(next) data-item-next data-replace-history { "Next →" }
                    }
                }
            }
        }
    };

    reddit_layout(&item.title, content, site_prefix, &url_state.to_route())
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
