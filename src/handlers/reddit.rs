use chrono::Utc;
use maud::{html, Markup};
use std::collections::HashMap;
use urlencoding::encode;

use super::{ListingPageConfig, ListingPageMode};
use crate::handlers::PaginatorPrefix;
use crate::site::{CrawlItem, CrawlTag, FileCrawlType};
use crate::thread_safe_work_dir::ThreadSafeWorkDir;

// Helper functions for rendering reddit components
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

fn reddit_layout(title: &str, content: Markup, site: &str, route: &str) -> Markup {
    html! {
        (super::Css("/res/styles.css"))
        (super::header(site, "r", route))
        .reddit_layout {
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
                    a href=(format!("/{}/r/item/{}", site, encode(&item.key))) { (item.title) }
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
                                span.post_tag { (x) },
                            CrawlTag::Detailed { value, .. } =>
                                span.post_tag { (value) },
                        }
                    }
                }
            }
        }
        hr.post_separator {}
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
        ListingPageMode::All => String::new(),
        ListingPageMode::ByTag { tag } => format!("Posts tagged \"{}\"", tag),
        ListingPageMode::ByMonth { year, month } => {
            format!("Posts from {}/{}", year, month)
        }
    };

    let content = html! {
        .reddit_posts {
            @for item in items {
                (reddit_post_card(item, &site))
            }
        }
        // FIXME: Don't include a paginator if the sort order is random
        (super::paginator(config.page, config.total, config.per_page, &config.paginator_prefix(&site, "r")))
    };

    reddit_layout(&title, content, &site, route)
}

pub fn render_detail_page(
    work_dir: &ThreadSafeWorkDir,
    item: &CrawlItem,
    file: &FileCrawlType,
    route: &str,
) -> Markup {
    let workdir = work_dir.work_dir.read().unwrap();
    let site = workdir.config.slug.clone();
    let timeago = timeago(item.source_published as u64);

    let content = html! {
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
                    @match file {
                        FileCrawlType::Image { filename, downloaded, .. } => {
                            @if *downloaded {
                                figure.post_figure {
                                    img.post_image src=(format!("/{}/assets/{}", site, filename)) alt=(item.title) {}
                                }
                            }
                        }
                        FileCrawlType::Video { filename, downloaded, .. } => {
                            @if *downloaded {
                                @let coerced_filename = filename.split('.').next().unwrap_or("").to_string() + ".mp4";
                                figure.post_figure {
                                    video.post_video controls {
                                        source src=(format!("/{}/assets/{}", site, coerced_filename)) {}
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

                .post_tags {
                    @for tag in &item.tags {
                        @match tag {
                            CrawlTag::Simple(x) =>
                                span.post_tag { (x) },
                            CrawlTag::Detailed { value, .. } =>
                                span.post_tag { (value) },
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
    };

    reddit_layout("", content, &site, route)
}

pub fn render_tags_page(
    work_dir: &ThreadSafeWorkDir,
    tags: &HashMap<String, usize>,
    route: &str,
) -> Markup {
    let workdir = work_dir.work_dir.read().unwrap();
    let site = workdir.config.slug.clone();

    let content = html! {
        .tag_list_page {
            h2 { "Tags" }
            ul.tag_list {
                @for (tag, count) in tags {
                    li.tag_item {
                        a href=(format!("/{}/r/tag/{}", site, encode(tag))) {
                            span.tag_name { (tag) }
                            span.tag_count { " (" (count) ")" }
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
    archive: &HashMap<(i32, u8), usize>,
    route: &str,
) -> Markup {
    let workdir = work_dir.work_dir.read().unwrap();
    let site = workdir.config.slug.clone();

    let content = html! {
        .archive_page {
            h2 { "Archive" }
            ul.archive_list {
                @for ((year, month), count) in archive {
                    li.archive_item {
                        a href=(format!("/{}/r/archive/{}/{:02}", site, year, month)) {
                            span.archive_date { (format!("{}/{:02}", year, month)) }
                            span.archive_count { " (" (count) ")" }
                        }
                    }
                }
            }
        }
    };

    reddit_layout("Archive", content, &site, route)
}
