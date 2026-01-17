use maud::{html, Markup};
use std::collections::HashMap;
use urlencoding::encode;

use crate::handlers::{ExtensionFix, PaginatorPrefix};
use crate::site::{CrawlItem, CrawlTag, FileCrawlType};

use super::{ArchiveYear, ListingPageConfig, ListingPageMode};

// Helper functions for rendering booru components
fn booru_layout(title: &str, content: Markup, site: &str, route: &str) -> Markup {
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
    }
}

fn item_thumbnail(item: &CrawlItem, site_prefix: &str) -> Markup {
    // Use item's source site for asset paths
    let asset_site = &item.site_settings.site_slug;

    html! {
        a.item_thumb_container href=(format!("/{}/booru/item/{}", site_prefix, encode(&item.key))) {
            .item_thumb_img {
                @if let Some(thumb) = item.thumbnail_path() {
                    @if thumb.ends_with(".mp4") {
                        video.thumbnail_preview autoplay loop muted playsinline {
                            source src=(format!("/{}/assets/{}", asset_site, thumb)) {}
                        }
                    } @else {
                        img src=(format!("/{}/assets/{}", asset_site, thumb)) alt=(item.title) {}
                    }
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
    site_prefix: &str,
    config: ListingPageConfig,
    items: &[CrawlItem],
    route: &str,
) -> Markup {
    let title = match &config.mode {
        ListingPageMode::All => String::new(),
        ListingPageMode::ByTag { tag } => format!("Items tagged \"{}\"", tag),
        ListingPageMode::ByMonth { year, month } => format!("Items from {}/{}", year, month),
        ListingPageMode::Search { query } => format!("Search: {}", query),
    };

    let content = html! {
        ( super::paginator(config.page, config.total, config.per_page, &config.paginator_prefix(site_prefix, "booru")) )
        .item_thumb_grid {
            @for item in items {
                ( item_thumbnail(item, site_prefix) )
            }
        }
        ( super::paginator(config.page, config.total, config.per_page, &config.paginator_prefix(site_prefix, "booru")) )
    };

    booru_layout(&title, content, site_prefix, route)
}

pub fn render_detail_page(
    site_prefix: &str,
    item: &CrawlItem,
    file: &FileCrawlType,
    route: &str,
) -> Markup {
    // Use item's source site for asset paths
    let asset_site = &item.site_settings.site_slug;

    let content = html! {
        article.post {
            h1 { (item.title) }
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
                                a.post_tag href=(format!("/{}/booru/tag/{}", site_prefix, encode(x))) { (x) },
                            CrawlTag::Detailed { value, .. } =>
                                a.post_tag href=(format!("/{}/booru/tag/{}", site_prefix, encode(value))) { (value) },
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

    booru_layout(&item.title, content, site_prefix, route)
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
                        a href=(format!("/{}/booru/tag/{}", site_prefix, encode(tag))) {
                            span.tag_name { (tag) }
                            span.tag_count { " (" (tags.get(tag).unwrap_or(&0)) ")" }
                        }
                    }
                }
            }
        }
    };

    booru_layout("Tags", content, site_prefix, route)
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
                        a href=(format!("/{}/booru/archive/{}/{:02}", site_prefix, month.year, month.month)) {
                            span.archive_date { (format!("{}/{:02}", month.year, month.month)) }
                            span.archive_count { " (" (month.count) ")" }
                        }
                    }
                }
            }
        }
    };

    booru_layout("Archive", content, site_prefix, route)
}
