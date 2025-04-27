use maud::{html, Markup};
use std::collections::HashMap;
use urlencoding::encode;

use crate::handlers::PaginatorPrefix;
use crate::site::{CrawlItem, CrawlTag, FileCrawlType};
use crate::thread_safe_work_dir::ThreadSafeWorkDir;

use super::{ListingPageConfig, ListingPageMode};

// Helper functions for rendering booru components
fn booru_layout(title: &str, content: Markup, site: &str, route: &str) -> Markup {
    html! {
        (super::Css("/res/styles.css"))
        (super::header(site, "booru", route))
        .booru_layout {
            main.booru_main {
                @if !title.is_empty() {
                    h1.page_title { (title) }
                }
                (content)
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
                        CrawlTag::Simple(x) => .tag { (x) },
                        CrawlTag::Detailed { value, .. } => .tag { (value) },
                    }
                }
            }
        }
    }
}

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
        ListingPageMode::ByTag { tag } => format!("Items tagged \"{}\"", tag),
        ListingPageMode::ByMonth { year, month } => format!("Items from {}/{}", year, month),
    };

    let content = html! {
        ( super::paginator(config.page, config.total, config.per_page, &config.paginator_prefix(&site, "booru")) )
        .item_thumb_grid {
            @for item in items {
                ( item_thumbnail(item, &site) )
            }
        }
        ( super::paginator(config.page, config.total, config.per_page, &config.paginator_prefix(&site, "booru")) )
    };

    booru_layout(&title, content, &site, route)
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
        article.post {
            h1 { (item.title) }
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
                .post_tags {
                    @for tag in &item.tags {
                        @match tag {
                            CrawlTag::Simple(x) =>
                                a.post_tag href=(format!("/{}/booru/tag/{}", site, encode(x))) { (x) },
                            CrawlTag::Detailed { value, .. } =>
                                a.post_tag href=(format!("/{}/booru/tag/{}", site, encode(value))) { (value) },
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

    booru_layout(&item.title, content, &site, route)
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
                        a href=(format!("/{}/booru/tag/{}", site, encode(tag))) {
                            span.tag_name { (tag) }
                            span.tag_count { " (" (count) ")" }
                        }
                    }
                }
            }
        }
    };

    booru_layout("Tags", content, &site, route)
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
                        a href=(format!("/{}/booru/archive/{}/{:02}", site, year, month)) {
                            span.archive_date { (format!("{}/{:02}", year, month)) }
                            span.archive_count { " (" (count) ")" }
                        }
                    }
                }
            }
        }
    };

    booru_layout("Archive", content, &site, route)
}
