use chrono::{Month, TimeZone, Utc};
use maud::{html, Markup};
use std::collections::HashMap;
use urlencoding::encode;

use super::{ArchiveYear, ListingPageConfig, ListingPageMode, PageUrlState, ViewMode};
use crate::handlers::{calculate_item_index, ExtensionFix, Fa, PaginatorPrefix};
use crate::site::{CrawlItem, CrawlTag, FileCrawlType};

// Helper functions for rendering blog components
fn blog_post_card(item: &CrawlItem, site_prefix: &str, config: &ListingPageConfig, position_in_page: usize) -> Markup {
    let time = Utc
        .timestamp_millis_opt(item.source_published as i64)
        .unwrap();
    // Use item's source site for asset paths
    let asset_site = &item.site_settings.site_slug;
    let slideshow_index = calculate_item_index(config, position_in_page);
    let first_file_id = crate::handlers::common::get_first_downloaded_file_id(item)
        .unwrap_or_default();
    let slideshow_url_path = PageUrlState::slideshow(
        site_prefix.to_string(),
        "blog".to_string(),
        config,
        slideshow_index,
        first_file_id.clone(),
        ViewMode::Normal,
    ).to_url();

    html! {
        article.blog_post_card {
            header.post_header {
                h3.post_title {
                    a href=(slideshow_url_path) { (item.title) }
                }
            }
            .post_meta {
                time datetime=(time.to_rfc3339()) {
                    (time.format("%B %d, %Y"))
                }
            }
            @if let Some(thumb) = item.thumbnail_path() {
                @if thumb.ends_with(".mp4") {
                    .post_preview {
                        video.thumbnail_preview autoplay loop muted playsinline {
                            source src=(format!("/{}/assets/{}", asset_site, thumb)) {}
                        }
                    }
                } @else {
                    .post_preview {
                        img src=(format!("/{}/assets/{}", asset_site, thumb)) alt=(item.title) {}
                    }
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
                                a.post_tag href=(format!("/{}/blog/tag/{}", site_prefix, encode(x))) { (x) },
                            CrawlTag::Detailed { value, .. } =>
                                a.post_tag href=(format!("/{}/blog/tag/{}", site_prefix, encode(value))) { (value) },
                        }
                    }
                }
            }
        }
    }
}

fn blog_layout(title: &str, content: Markup, site: &str, route: &str) -> Markup {
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
                (super::header(site, "blog", route))
                .blogger_layout {
                    .blogger_content {
                        main.blog_main {
                            @if !title.is_empty() {
                                h2.page_title { (title) }
                            }
                            (content)
                        }
                    }
                }
            }
        }
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
        ListingPageMode::All => String::new(),
        ListingPageMode::ByTag { tag } => format!("Posts tagged \"{}\"", tag),
        ListingPageMode::ByMonth { year, month } => {
            format!(
                "Posts from {} {}",
                Month::try_from(*month as u8).unwrap().name(),
                year
            )
        }
        ListingPageMode::Search { query } => format!("Search: {}", query),
    };

    let content = html! {
        .blog_posts {
            @for (idx, item) in items.iter().enumerate() {
                (blog_post_card(item, site_prefix, &config, idx))
            }
        }
        (super::paginator(config.page, config.total, config.per_page, &config.paginator_prefix(site_prefix, "blog")))
    };

    blog_layout(&title, content, site_prefix, route)
}

pub fn render_detail_page(
    site_prefix: &str,
    item: &CrawlItem,
    file: &FileCrawlType,
    url_state: &PageUrlState,
) -> Markup {
    let route = url_state.to_route();
    // Use item's source site for asset paths
    let asset_site = &item.site_settings.site_slug;
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
                @match file {
                    FileCrawlType::Image { filename, downloaded, .. } => {
                        @if *downloaded {
                            figure.post_figure {
                                img.post_image src=(format!("/{}/assets/{}", asset_site, filename)) alt=(item.title) {}
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
                            }
                        }
                    }
                    _ => {}
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
                                a.post_tag href=(format!("/{}/blog/tag/{}", site_prefix, encode(x))) { (x) },
                            CrawlTag::Detailed { value, .. } =>
                                a.post_tag href=(format!("/{}/blog/tag/{}", site_prefix, encode(value))) { (value) },
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

    blog_layout("", content, site_prefix, &route)
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
                        a href=(format!("/{}/blog/tag/{}", site_prefix, encode(tag))) {
                            span.tag_name { (tag) }
                            span.tag_count { " (" (tags.get(tag).unwrap_or(&0)) ")" }
                        }
                    }
                }
            }
        }
    };

    blog_layout("Tags", content, site_prefix, route)
}

pub fn render_archive_page(site_prefix: &str, archive: &Vec<ArchiveYear>, route: &str) -> Markup {
    let content = html! {
        .blog_archive_page {
            h2 { "Archive" }
            ul.blog_archive_list.full_archive_list {
                @for year in archive.iter() {
                    li.archive_year {
                        h3.year_name { (year.year) }
                        ul.month_list {
                            @for month in year.months.iter().rev() {
                                li.archive_month {
                                    a href=(format!("/{}/blog/archive/{}/{:02}", site_prefix, year.year, month.month)) {
                                        span.month_name { (Month::try_from(month.month).unwrap().name()) }
                                        span.month_count { "(" (month.count) ")" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    };

    blog_layout("Archive", content, site_prefix, route)
}

pub fn render_slideshow_detail_page(
    site_prefix: &str,
    item: &CrawlItem,
    file: &FileCrawlType,
    url_state: &PageUrlState,
    prev_url: Option<&str>,
    next_url: Option<&str>,
) -> Markup {
    let route = url_state.to_route();
    // Use item's source site for asset paths
    let asset_site = &item.site_settings.site_slug;
    let time = Utc
        .timestamp_millis_opt(item.source_published as i64)
        .unwrap();

    // Get file_id for permalink
    let file_id = item
        .flat_files()
        .into_iter()
        .filter(|(_, f)| f.is_downloaded())
        .next()
        .map(|(id, _)| id);

    let content = html! {
        article.blog_post {
            .slideshow_navigation {
                @if let Some(prev) = prev_url {
                    a.slideshow_prev href=(prev) data-item-prev { "← Previous" }
                }
                @if let Some(file_id) = &file_id {
                    a.slideshow_permalink href=(format!("/{}/blog/item/{}/{}", site_prefix, encode(&item.key), encode(file_id))) { "Permalink" }
                }
                @if let Some(next) = next_url {
                    a.slideshow_next href=(next) data-item-next { "Next →" }
                }
            }
            header.post_header {
                h1.post_title { (item.title) }
                .post_meta {
                    time datetime=(time.to_rfc3339()) {
                        (time.format("%B %d, %Y"))
                    }
                }
            }
            .post_content {
                @match file {
                    FileCrawlType::Image { filename, downloaded, .. } => {
                        @if *downloaded {
                            figure.post_figure {
                                img.post_image src=(format!("/{}/assets/{}", asset_site, filename)) alt=(item.title) {}
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
                            }
                        }
                    }
                    _ => {}
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
                .slideshow_navigation {
                    @if let Some(prev) = prev_url {
                        a.slideshow_prev href=(prev) data-item-prev { "← Previous" }
                    }
                    @if let Some(file_id) = &file_id {
                        a.slideshow_permalink href=(format!("/{}/blog/item/{}/{}", site_prefix, encode(&item.key), encode(file_id))) { "Permalink" }
                    }
                    @if let Some(next) = next_url {
                        a.slideshow_next href=(next) data-item-next { "Next →" }
                    }
                }
                .post_tags {
                    @for tag in &item.tags {
                        @match tag {
                            CrawlTag::Simple(x) =>
                                a.post_tag href=(format!("/{}/blog/tag/{}", site_prefix, encode(x))) { (x) },
                            CrawlTag::Detailed { value, .. } =>
                                a.post_tag href=(format!("/{}/blog/tag/{}", site_prefix, encode(value))) { (value) },
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

    blog_layout("", content, site_prefix, &route)
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
    let route = url_state.to_route();
    // Use item's source site for asset paths
    let asset_site = &item.site_settings.site_slug;

    let content = html! {
        article.blog_post_full {
            .slideshow_navigation {
                @if let Some(prev) = prev_url {
                    a.slideshow_prev href=(prev) data-item-prev { (Fa("chevron-left")) }
                }
                a.slideshow_back href=(back_url) data-is-quit data-toggle-full { (Fa("chevron-left")) }
                @if let Some(next) = next_url {
                    a.slideshow_next href=(next) data-item-next { (Fa("chevron-right")) }
                }
            }
            .post_content {
                @match file {
                    FileCrawlType::Image { filename, downloaded, .. } => {
                        @if *downloaded {
                            figure.post_figure {
                                img.post_image src=(format!("/{}/assets/{}", asset_site, filename)) alt=(item.title) {}
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
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    };

    blog_layout("", content, site_prefix, &route)
}
