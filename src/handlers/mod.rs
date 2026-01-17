use std::{collections::HashMap, path::PathBuf};

use chrono::Utc;
use maud::{html, Markup, PreEscaped};

mod blog;
mod booru;
mod common;
mod generic;
mod reddit;
mod search;

pub use common::*;
pub use generic::*;
pub use reddit::media_viewer_fragment_handler;
pub use search::{search_form_handler, search_results_handler};

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
pub trait ExtensionFix {
    fn as_mp4(&self) -> String;
}

impl ExtensionFix for std::string::String {
    fn as_mp4(&self) -> String {
        PathBuf::from(self)
            .with_extension("mp4")
            .to_string_lossy()
            .to_string()
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
                span .active[current_route.starts_with("/search")] {
                    a href=(format!("/{}/{}/search", site_prefix, rendering_prefix)) { "Search"}
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

/// Abstraction over single-site or all-sites data source.
/// Allows handlers to work transparently with either mode.
#[derive(Clone)]
pub enum SiteSource {
    /// A single site's WorkDir
    Single(ThreadSafeWorkDir),
    /// All sites aggregated together
    All { workdirs: Vec<ThreadSafeWorkDir> },
}

impl SiteSource {
    /// Returns the site prefix for URL generation ("mysite" or "all")
    pub fn slug(&self) -> String {
        match self {
            SiteSource::Single(workdir) => workdir.work_dir.read().unwrap().config.slug.clone(),
            SiteSource::All { .. } => "all".to_string(),
        }
    }

    pub fn get_assets_path(&self) -> Option<PathBuf> {
        match self {
            SiteSource::Single(workdir) => {
                let wd = workdir.work_dir.read().unwrap();
                Some(PathBuf::from(wd.path.clone()))
            }
            SiteSource::All { .. } => None,
        }
    }

    /// Returns all items with SiteSettings already attached.
    /// For All variant, items have namespaced keys: "{site_slug}/{item.key}"
    pub fn all_items(&self) -> Vec<CrawlItem> {
        match self {
            SiteSource::Single(workdir) => {
                let wd = workdir.work_dir.read().unwrap();
                wd.crawled.items.values().cloned().collect()
            }
            SiteSource::All { workdirs } => {
                let mut all_items = Vec::new();
                for workdir in workdirs {
                    let wd = workdir.work_dir.read().unwrap();
                    let site_slug = &wd.config.slug;
                    for item in wd.crawled.items.values() {
                        let mut namespaced_item = item.clone();
                        // Namespace the key to avoid collisions
                        namespaced_item.key = format!("{}/{}", site_slug, item.key);
                        all_items.push(namespaced_item);
                    }
                }
                all_items
            }
        }
    }

    /// Get an item by key. For All variant, key should be namespaced as "site_slug/item_key"
    pub fn get_item(&self, key: &str) -> Option<CrawlItem> {
        match self {
            SiteSource::Single(workdir) => {
                let wd = workdir.work_dir.read().unwrap();
                wd.crawled.items.get(key).cloned()
            }
            SiteSource::All { workdirs } => {
                // Parse the namespaced key: "site_slug/item_key"
                let parts: Vec<&str> = key.splitn(2, '/').collect();
                if parts.len() != 2 {
                    return None;
                }
                let (site_slug, item_key) = (parts[0], parts[1]);

                for workdir in workdirs {
                    let wd = workdir.work_dir.read().unwrap();
                    if wd.config.slug == site_slug {
                        if let Some(item) = wd.crawled.items.get(item_key) {
                            let mut namespaced_item = item.clone();
                            namespaced_item.key = key.to_string();
                            return Some(namespaced_item);
                        }
                    }
                }
                None
            }
        }
    }

    /// Get the work directory path for a given site slug (for thumbnail lookups)
    pub fn get_work_dir_path(&self, site_slug: &str) -> Option<PathBuf> {
        match self {
            SiteSource::Single(workdir) => {
                let wd = workdir.work_dir.read().unwrap();
                if wd.config.slug == site_slug {
                    Some(PathBuf::from(wd.path.clone()))
                } else {
                    None
                }
            }
            SiteSource::All { workdirs } => {
                for workdir in workdirs {
                    let wd = workdir.work_dir.read().unwrap();
                    if wd.config.slug == site_slug {
                        return Some(PathBuf::from(wd.path.clone()));
                    }
                }
                None
            }
        }
    }

    /// Get tags and their counts across all items
    pub fn get_tags(&self) -> HashMap<String, usize> {
        use crate::site::CrawlTag;

        let items = self.all_items();
        let mut tags: HashMap<String, usize> = HashMap::new();

        for item in items {
            for tag in &item.tags {
                let tag_str = match tag {
                    CrawlTag::Simple(x) => x.clone(),
                    CrawlTag::Detailed { value, .. } => value.clone(),
                };
                *tags.entry(tag_str).or_insert(0) += 1;
            }
        }

        tags
    }
}

pub enum ListingPageMode {
    All,
    ByTag { tag: String },
    ByMonth { year: u32, month: u32 },
    Search { query: String },
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
            ListingPageMode::Search { query } => {
                format!("/{}/{}/search/{}", site_prefix, rendering_prefix, query)
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
        site_prefix: &str,
        config: ListingPageConfig,
        items: &[CrawlItem],
        route: &str,
    ) -> Markup;
    fn render_detail_page(
        &self,
        site_prefix: &str,
        item: &CrawlItem,
        file: &FileCrawlType,
        route: &str,
    ) -> Markup;
    fn render_tags_page(
        &self,
        site_prefix: &str,
        tags: &HashMap<String, usize>,
        tag_order: &Vec<String>,
        route: &str,
    ) -> Markup;
    fn render_archive_page(
        &self,
        site_prefix: &str,
        archive: &Vec<ArchiveYear>,
        route: &str,
    ) -> Markup;
    fn render_detail_full_page(
        &self,
        site_prefix: &str,
        item: &CrawlItem,
        file: &FileCrawlType,
        route: &str,
    ) -> Markup;
    fn get_prefix(&self) -> &str;
}

impl SiteRenderer for SiteRendererType {
    fn render_listing_page(
        &self,
        site_prefix: &str,
        config: ListingPageConfig,
        items: &[CrawlItem],
        route: &str,
    ) -> Markup {
        match self {
            SiteRendererType::Blog => blog::render_listing_page(site_prefix, config, items, route),
            SiteRendererType::Booru => {
                booru::render_listing_page(site_prefix, config, items, route)
            }
            SiteRendererType::Reddit => {
                reddit::render_listing_page(site_prefix, config, items, route)
            }
        }
    }

    fn render_detail_page(
        &self,
        site_prefix: &str,
        item: &CrawlItem,
        file: &FileCrawlType,
        route: &str,
    ) -> Markup {
        match self {
            SiteRendererType::Blog => blog::render_detail_page(site_prefix, item, file, route),
            SiteRendererType::Booru => booru::render_detail_page(site_prefix, item, file, route),
            SiteRendererType::Reddit => reddit::render_detail_page(site_prefix, item, file, route),
        }
    }

    fn render_tags_page(
        &self,
        site_prefix: &str,
        tags: &HashMap<String, usize>,
        tag_order: &Vec<String>,
        route: &str,
    ) -> Markup {
        match self {
            SiteRendererType::Blog => blog::render_tags_page(site_prefix, tags, tag_order, route),
            SiteRendererType::Booru => booru::render_tags_page(site_prefix, tags, tag_order, route),
            SiteRendererType::Reddit => {
                reddit::render_tags_page(site_prefix, tags, tag_order, route)
            }
        }
    }

    fn render_archive_page(
        &self,
        site_prefix: &str,
        archive: &Vec<ArchiveYear>,
        route: &str,
    ) -> Markup {
        match self {
            SiteRendererType::Blog => blog::render_archive_page(site_prefix, archive, route),
            SiteRendererType::Booru => booru::render_archive_page(site_prefix, archive, route),
            SiteRendererType::Reddit => reddit::render_archive_page(site_prefix, archive, route),
        }
    }

    fn render_detail_full_page(
        &self,
        site_prefix: &str,
        item: &CrawlItem,
        file: &FileCrawlType,
        route: &str,
    ) -> Markup {
        match self {
            SiteRendererType::Blog => blog::render_detail_page(site_prefix, item, file, route),
            SiteRendererType::Booru => booru::render_detail_page(site_prefix, item, file, route),
            SiteRendererType::Reddit => {
                reddit::render_detail_full_page(site_prefix, item, file, route)
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
