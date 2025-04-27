use std::collections::HashMap;

use maud::{html, Markup};

mod blog;
mod booru;
mod common;
mod generic;
mod reddit;

pub use common::*;
pub use generic::*;

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
        route: &str,
    ) -> Markup;
    fn render_archive_page(
        &self,
        work_dir: &ThreadSafeWorkDir,
        archive: &HashMap<(i32, u8), usize>,
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
        route: &str,
    ) -> Markup {
        match self {
            SiteRendererType::Blog => blog::render_tags_page(work_dir, tags, route),
            SiteRendererType::Booru => booru::render_tags_page(work_dir, tags, route),
            SiteRendererType::Reddit => reddit::render_tags_page(work_dir, tags, route),
        }
    }

    fn render_archive_page(
        &self,
        work_dir: &ThreadSafeWorkDir,
        archive: &HashMap<(i32, u8), usize>,
        route: &str,
    ) -> Markup {
        match self {
            SiteRendererType::Blog => blog::render_archive_page(work_dir, archive, route),
            SiteRendererType::Booru => booru::render_archive_page(work_dir, archive, route),
            SiteRendererType::Reddit => reddit::render_archive_page(work_dir, archive, route),
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
