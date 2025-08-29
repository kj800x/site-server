use std::collections::HashMap;

use chrono::Utc;
use maud::{html, Markup, PreEscaped};

mod blog;
mod booru;
mod common;
mod generic;
mod reddit;

pub use common::*;
pub use generic::*;
pub use reddit::media_viewer_fragment_handler;

use crate::site::{CrawlItem, FileCrawlType};

// Shared components
pub struct Css(pub &'static str);

impl maud::Render for Css {
    fn render(&self) -> Markup {
        html! {
            link rel="stylesheet" type="text/css" href=(self.0);
        }
    }
}

pub struct Js(pub &'static str);

impl maud::Render for Js {
    fn render(&self) -> Markup {
        html! {
            script type="text/javascript" src=(self.0) {}
        }
    }
}

pub const PRERENDER_RULES: &str = r#"{
    "prerender": [
        { "where": { "selector_matches": "a[data-is-next]" }, "eagerness": "immediate" },
        { "where": { "selector_matches": "a[data-is-prev]" }, "eagerness": "eager" }
    ]
}"#;

pub fn format_year_month(year: i32, month: u8) -> String {
    format!(
        "{} {}",
        match month {
            1 => "January",
            2 => "February",
            3 => "March",
            4 => "April",
            5 => "May",
            6 => "June",
            7 => "July",
            8 => "August",
            9 => "September",
            10 => "October",
            11 => "November",
            12 => "December",
            _ => "Unknown",
        },
        year
    )
}

pub fn timeago(timestamp: u64) -> Markup {
    let dt =
        chrono::DateTime::from_timestamp_millis(timestamp as i64).unwrap_or_else(|| Utc::now());

    let now = Utc::now().timestamp_millis() as u64;
    let diff = now - timestamp;
    let hours = diff / (1000 * 60 * 60);
    let days = hours / 24;
    let months = days / 30;
    let years = days / 365;

    let timeago_text = if years > 0 {
        if years == 1 {
            "1 year ago".to_string()
        } else {
            format!("{} years ago", years)
        }
    } else if months > 0 {
        if months == 1 {
            "1 month ago".to_string()
        } else {
            format!("{} months ago", months)
        }
    } else if days > 0 {
        if days == 1 {
            "1 day ago".to_string()
        } else {
            format!("{} days ago", days)
        }
    } else if hours > 0 {
        if hours == 1 {
            "1 hour ago".to_string()
        } else {
            format!("{} hours ago", hours)
        }
    } else {
        "just now".to_string()
    };

    html! {
        time datetime=(dt.to_rfc3339()) title=(dt.to_rfc3339()) {
            (timeago_text)
        }
    }
}

/// Common scripts for all pages
pub fn scripts() -> Markup {
    html! {
        (Css("/res/page-transitions.css"))
        (Css("/res/styles.css"))
        script src="/res/htmx.min.js" {}
        script src="/res/detail_page.js" {}
        script src="/res/idiomorph.min.js" {}
        script src="/res/idiomorph-ext.min.js" {}
        script type="speculationrules" {
            (PreEscaped(PRERENDER_RULES))
        }
    }
}

pub fn header(site_prefix: &str, rendering_prefix: &str, current_route: &str) -> Markup {
    html! {
        header.page-header {
            nav {
                span .root-link {
                    a href="/" { (site_prefix) }
                }
                span .rendering-mode .active[rendering_prefix == "booru"] {
                    a href=(format!("/{}/booru{}", site_prefix, current_route)) { "Booru"}
                }
                span .rendering-mode .active[rendering_prefix == "blog"] {
                    a href=(format!("/{}/blog{}", site_prefix, current_route)) { "Blog"}
                }
                span .rendering-mode .active[rendering_prefix == "r"] {
                    a href=(format!("/{}/r{}", site_prefix, current_route)) { "Reddit"}
                }
            }
            nav.sub-nav {
                span .active[current_route.starts_with("/latest")] {
                    a href=(format!("/{}/{}/latest", site_prefix, rendering_prefix)) { "Latest"}
                }
                span .active[current_route.starts_with("/oldest")] {
                    a href=(format!("/{}/{}/oldest", site_prefix, rendering_prefix)) { "Oldest"}
                }
                span .active[current_route.starts_with("/random")] {
                    a href=(format!("/{}/{}/random", site_prefix, rendering_prefix)) { "Random"}
                }
                span .active[current_route.starts_with("/tags") || current_route.starts_with("/tag")] {
                    a href=(format!("/{}/{}/tags", site_prefix, rendering_prefix)) { "Tags"}
                }
                span .active[current_route.starts_with("/archive")] {
                    a href=(format!("/{}/{}/archive", site_prefix, rendering_prefix)) { "Archive"}
                }
            }
        }
    }
}

pub fn paginator(page: usize, total: usize, per_page: usize, prefix: &str) -> Markup {
    let pages = (total + per_page - 1) / per_page;
    let mut links = vec![];

    if page > 1 {
        links.push(html! {
            a href=(format!("{}/{}", prefix, page - 1)) { "<" }
        });
    }

    for i in 1..=pages {
        if i == page {
            links.push(html! {
                span { (i) }
            });
        } else if (i as isize - page as isize).abs() < 5 {
            links.push(html! {
                a href=(format!("{}/{}", prefix, i)) { (i) }
            });
        }
    }

    if page < pages {
        links.push(html! {
            a href=(format!("{}/{}", prefix, page + 1)) { ">" }
        });
    }

    return html! {
        .paginator {
            @for link in &links {
                (link)
            }
        }
    };
}

// Common types used across handlers
pub struct WorkDirPrefix(pub String);

pub type ThreadSafeWorkDir = crate::thread_safe_work_dir::ThreadSafeWorkDir;

pub enum ListingPageMode {
    All,
    ByTag { tag: String },
    ByMonth { year: u32, month: u32 },
}

pub enum ListingPageOrdering {
    NewestFirst,
    OldestFirst,
    Random,
}

pub struct ListingPageConfig {
    mode: ListingPageMode,
    ordering: ListingPageOrdering,
    page: usize,
    per_page: usize,
    total: usize,
}

trait PaginatorPrefix {
    fn paginator_prefix(&self, site_prefix: &str, rendering_prefix: &str) -> String;
}

impl PaginatorPrefix for ListingPageConfig {
    fn paginator_prefix(&self, site_prefix: &str, rendering_prefix: &str) -> String {
        match &self.mode {
            ListingPageMode::All => match &self.ordering {
                ListingPageOrdering::NewestFirst => {
                    format!("/{}/{}/latest", site_prefix, rendering_prefix)
                }
                ListingPageOrdering::OldestFirst => {
                    format!("/{}/{}/oldest", site_prefix, rendering_prefix)
                }
                ListingPageOrdering::Random => {
                    format!("/{}/{}/random", site_prefix, rendering_prefix)
                }
            },
            ListingPageMode::ByTag { tag } => {
                format!("/{}/{}/tag/{}", site_prefix, rendering_prefix, tag)
            }
            ListingPageMode::ByMonth { year, month } => {
                format!(
                    "/{}/{}/archive/{}/{}",
                    site_prefix, rendering_prefix, year, month
                )
            }
        }
    }
}

#[derive(Clone, Copy)]
pub enum SiteRendererType {
    Blog,
    Booru,
    Reddit,
}

pub trait SiteRenderer {
    fn render_listing_page(
        &self,
        work_dir: &ThreadSafeWorkDir,
        config: ListingPageConfig,
        items: &[CrawlItem],
        route: &str,
    ) -> Markup;
    fn render_detail_page(
        &self,
        work_dir: &ThreadSafeWorkDir,
        item: &CrawlItem,
        file: &FileCrawlType,
        route: &str,
    ) -> Markup;
    fn render_tags_page(
        &self,
        work_dir: &ThreadSafeWorkDir,
        tags: &HashMap<String, usize>,
        tag_order: &Vec<String>,
        route: &str,
    ) -> Markup;
    fn render_archive_page(
        &self,
        work_dir: &ThreadSafeWorkDir,
        archive: &Vec<ArchiveYear>,
        route: &str,
    ) -> Markup;
    fn render_detail_full_page(
        &self,
        work_dir: &ThreadSafeWorkDir,
        item: &CrawlItem,
        file: &FileCrawlType,
        route: &str,
    ) -> Markup;
    fn get_prefix(&self) -> &str;
}

impl SiteRenderer for SiteRendererType {
    fn render_listing_page(
        &self,
        work_dir: &ThreadSafeWorkDir,
        config: ListingPageConfig,
        items: &[CrawlItem],
        route: &str,
    ) -> Markup {
        match self {
            SiteRendererType::Blog => blog::render_listing_page(work_dir, config, items, route),
            SiteRendererType::Booru => booru::render_listing_page(work_dir, config, items, route),
            SiteRendererType::Reddit => reddit::render_listing_page(work_dir, config, items, route),
        }
    }

    fn render_detail_page(
        &self,
        work_dir: &ThreadSafeWorkDir,
        item: &CrawlItem,
        file: &FileCrawlType,
        route: &str,
    ) -> Markup {
        match self {
            SiteRendererType::Blog => blog::render_detail_page(work_dir, item, file, route),
            SiteRendererType::Booru => booru::render_detail_page(work_dir, item, file, route),
            SiteRendererType::Reddit => reddit::render_detail_page(work_dir, item, file, route),
        }
    }

    fn render_tags_page(
        &self,
        work_dir: &ThreadSafeWorkDir,
        tags: &HashMap<String, usize>,
        tag_order: &Vec<String>,
        route: &str,
    ) -> Markup {
        match self {
            SiteRendererType::Blog => blog::render_tags_page(work_dir, tags, tag_order, route),
            SiteRendererType::Booru => booru::render_tags_page(work_dir, tags, tag_order, route),
            SiteRendererType::Reddit => reddit::render_tags_page(work_dir, tags, tag_order, route),
        }
    }

    fn render_archive_page(
        &self,
        work_dir: &ThreadSafeWorkDir,
        archive: &Vec<ArchiveYear>,
        route: &str,
    ) -> Markup {
        match self {
            SiteRendererType::Blog => blog::render_archive_page(work_dir, archive, route),
            SiteRendererType::Booru => booru::render_archive_page(work_dir, archive, route),
            SiteRendererType::Reddit => reddit::render_archive_page(work_dir, archive, route),
        }
    }

    fn render_detail_full_page(
        &self,
        work_dir: &ThreadSafeWorkDir,
        item: &CrawlItem,
        file: &FileCrawlType,
        route: &str,
    ) -> Markup {
        match self {
            SiteRendererType::Blog => blog::render_detail_page(work_dir, item, file, route),
            SiteRendererType::Booru => booru::render_detail_page(work_dir, item, file, route),
            SiteRendererType::Reddit => {
                reddit::render_detail_full_page(work_dir, item, file, route)
            }
        }
    }

    fn get_prefix(&self) -> &str {
        match self {
            SiteRendererType::Blog => "blog",
            SiteRendererType::Booru => "booru",
            SiteRendererType::Reddit => "r",
        }
    }
}
