use chrono::{Month, TimeZone, Utc};
use maud::{html, Markup};
use std::collections::{BTreeMap, HashMap};
use urlencoding::encode;

use super::{ListingPageConfig, ListingPageMode};
use crate::handlers::PaginatorPrefix;
use crate::site::{CrawlItem, CrawlTag, FileCrawlType};
use crate::thread_safe_work_dir::ThreadSafeWorkDir;

// Helper functions for rendering blog components
fn blog_post_card(item: &CrawlItem, site: &str) -> Markup {
    let time = Utc
        .timestamp_millis_opt(item.source_published as i64)
        .unwrap();

    html! {
        article.blog_post_card {
            header.post_header {
                h3.post_title {
                    a href=(format!("/{}/blog/item/{}", site, encode(&item.key))) { (item.title) }
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

fn blog_layout(title: &str, content: Markup, site: &str, route: &str) -> Markup {
    html! {
        (super::Css("/res/styles.css"))
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
            format!(
                "Posts from {} {}",
                Month::try_from(*month as u8).unwrap().name(),
                year
            )
        }
    };

    let content = html! {
        .blog_posts {
            @for item in items {
                (blog_post_card(item, &site))
            }
        }
        (super::paginator(config.page, config.total, config.per_page, &config.paginator_prefix(&site, "blog")))
    };

    blog_layout(&title, content, &site, route)
}

pub fn render_detail_page(
    work_dir: &ThreadSafeWorkDir,
    item: &CrawlItem,
    file: &FileCrawlType,
    route: &str,
) -> Markup {
    let workdir = work_dir.work_dir.read().unwrap();
    let site = workdir.config.slug.clone();
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
                                img.post_image src=(format!("/{}/assets/{}", site, filename)) alt=(item.title) {}
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
                                a.post_tag href=(format!("/{}/blog/tag/{}", site, encode(x))) { (x) },
                            CrawlTag::Detailed { value, .. } =>
                                a.post_tag href=(format!("/{}/blog/tag/{}", site, encode(value))) { (value) },
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

    blog_layout("", content, &site, route)
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
                        a href=(format!("/{}/blog/tag/{}", site, encode(tag))) {
                            span.tag_name { (tag) }
                            span.tag_count { " (" (count) ")" }
                        }
                    }
                }
            }
        }
    };

    blog_layout("Tags", content, &site, route)
}

pub fn render_archive_page(
    work_dir: &ThreadSafeWorkDir,
    archive: &HashMap<(i32, u8), usize>,
    route: &str,
) -> Markup {
    let workdir = work_dir.work_dir.read().unwrap();
    let site = workdir.config.slug.clone();

    // Group by year first
    let mut years: BTreeMap<i32, Vec<(u8, usize)>> = BTreeMap::new();
    for ((year, month), count) in archive {
        years.entry(*year).or_default().push((*month, *count));
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
    };

    blog_layout("Archive", content, &site, route)
}
