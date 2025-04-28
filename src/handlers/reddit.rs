use indexmap::IndexMap;
use maud::{html, Markup};
use std::collections::HashMap;
use urlencoding::encode;

use super::{ArchiveYear, ListingPageConfig, ListingPageMode, ListingPageOrdering};
use crate::collections::GetKey;
use crate::handlers::{format_year_month, timeago, PaginatorPrefix};
use crate::site::{CrawlItem, CrawlTag, FileCrawlType};
use crate::thread_safe_work_dir::ThreadSafeWorkDir;

fn reddit_layout(title: &str, content: Markup, site: &str, route: &str) -> Markup {
    html! {
        (maud::DOCTYPE)
        html {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1" {}
                (super::Css("/res/styles.css"))
                title { (title) }
            }
            body {
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

fn reddit_post_card(item: &CrawlItem, site: &str) -> Markup {
    html! {
        article.reddit_post_card {
            header.post_header {
                span.post_author {
                    @if let Some(author) = item.meta.get("author") {
                        (author.as_str().unwrap_or("unknown user"))
                    } @else {
                        "unknown_user"
                    }
                }
                span.post_time { (timeago(item.source_published as u64)) }
            }
            .post_content {
                h2.post_title {
                    a href=(format!("/{}/r/item/{}", site, encode(&item.key))) { (item.title) }
                }
                .post_tags {
                    @for tag in &item.tags {
                        @match tag {
                            CrawlTag::Simple(x) =>
                                a.post_tag href=(format!("/{}/r/tag/{}", site, encode(x))) { (x) },
                            CrawlTag::Detailed { value, .. } =>
                                a.post_tag href=(format!("/{}/r/tag/{}", site, encode(value))) { (value) },
                        }
                    }
                }
                @if let Some(thumb) = item.thumbnail_path() {
                    .post_preview {
                        img src=(format!("/{}/assets/{}", site, thumb)) alt=(item.title) {}
                    }
                }
            }
        }
        div.post_separator {}
    }
}

// Public functions required by SiteRenderer trait
pub fn render_listing_page(
    work_dir: &ThreadSafeWorkDir,
    config: ListingPageConfig,
    items: &[CrawlItem],
    route: &str,
) -> Markup {
    let workdir = work_dir.work_dir.read().unwrap();
    let site = workdir.config.slug.clone();

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
    };

    let content = html! {
        .reddit_posts_container {
            @if !title.is_empty() && !matches!(config.mode, ListingPageMode::All) {
                h1.page_title { (title) }
            }
            .reddit_posts {
                @for item in items {
                    (reddit_post_card(item, &site))
                }
            }
            // FIXME: Don't include a paginator if the sort order is random
            (super::paginator(config.page, config.total, config.per_page, &config.paginator_prefix(&site, "r")))
        }
        .reddit_right_bar {}
    };

    reddit_layout(&title, content, &site, route)
}

pub fn post_file_paginator(item: &CrawlItem, site: &str, current_file: &FileCrawlType) -> Markup {
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
                a.prev href=(format!("/{}/r/item/{}/{}", site, encode(&item.key), encode(&prev_file.0))) {
                    "<"
                }
            }
            @if let Some(next_file) = next_file {
                a.next href=(format!("/{}/r/item/{}/{}", site, encode(&item.key), encode(&next_file.0))) {
                    ">"
                }
            }
        }
    }
}

pub fn render_detail_page(
    work_dir: &ThreadSafeWorkDir,
    item: &CrawlItem,
    file: &FileCrawlType,
    route: &str,
) -> Markup {
    let workdir = work_dir.work_dir.read().unwrap();
    let site = workdir.config.slug.clone();

    let content = html! {
        article.reddit_post_detail {
            header.post_header {
                span.post_author {
                    @if let Some(author) = item.meta.get("author") {
                        (author.as_str().unwrap_or("unknown_user"))
                    } @else {
                        "unknown_user"
                    }
                }
                span.post_time { (timeago(item.source_published as u64)) }
            }
            h1.post_title { (item.title) }
            .post_content {
                .media_viewer {
                    @match file {
                        FileCrawlType::Image { filename, downloaded, .. } => {
                            @if *downloaded {
                                figure.post_figure {
                                    img.post_image src=(format!("/{}/assets/{}", site, filename)) alt=(item.title) {}
                                    (post_file_paginator(item, &site, &file))
                                }
                            }
                        }
                        FileCrawlType::Video { filename, downloaded, .. } => {
                            @if *downloaded {
                                @let coerced_filename = filename.split('.').next().unwrap_or("").to_string() + ".mp4";
                                figure.post_figure {
                                    video.post_video controls autoplay {
                                        source src=(format!("/{}/assets/{}", site, coerced_filename)) {}
                                    }
                                    (post_file_paginator(item, &site, &file))
                                }
                            }
                        }
                        _ => {}
                    }
                }

                .post_description {
                    p { (item.description) }
                }

                .post_tags {
                    @for tag in &item.tags {
                        @match tag {
                            CrawlTag::Simple(x) =>
                                a.post_tag href=(format!("/{}/r/tag/{}", site, encode(x))) { (x) },
                            CrawlTag::Detailed { value, .. } =>
                                a.post_tag href=(format!("/{}/r/tag/{}", site, encode(value))) { (value) },
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

    reddit_layout(&item.title, content, &site, route)
}

pub fn render_tags_page(
    work_dir: &ThreadSafeWorkDir,
    tags: &HashMap<String, usize>,
    tag_order: &Vec<String>,
    route: &str,
) -> Markup {
    let workdir = work_dir.work_dir.read().unwrap();
    let site = workdir.config.slug.clone();

    let content = html! {
        .tag_list_page {
            h2 { "Tags" }
            ul.tag_list {
                @for tag in tag_order {
                    li.tag_item {
                        a href=(format!("/{}/r/tag/{}", site, encode(tag))) {
                            span.tag_name { (tag) }
                            span.tag_count { " (" (tags.get(tag).unwrap_or(&0)) ")" }
                        }
                    }
                }
            }
        }
    };

    reddit_layout("Tags", content, &site, route)
}

pub fn render_archive_page(
    work_dir: &ThreadSafeWorkDir,
    archive: &Vec<ArchiveYear>,
    route: &str,
) -> Markup {
    let workdir = work_dir.work_dir.read().unwrap();
    let site = workdir.config.slug.clone();

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
                        a href=(format!("/{}/r/archive/{}/{:02}", site, month.year, month.month)) {
                            span.archive_date { (format_year_month(month.year, month.month)) }
                            span.archive_count { " (" (month.count) ")" }
                        }
                    }
                }
            }
        }
    };

    reddit_layout("Archive", content, &site, route)
}
