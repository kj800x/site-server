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
    ) -> Markup;
    fn render_detail_page(
        &self,
        work_dir: &ThreadSafeWorkDir,
        item: &CrawlItem,
        file: &FileCrawlType,
    ) -> Markup;
    fn render_tags_page(
        &self,
        work_dir: &ThreadSafeWorkDir,
        tags: &HashMap<String, usize>,
    ) -> Markup;
    fn render_archive_page(
        &self,
        work_dir: &ThreadSafeWorkDir,
        archive: &HashMap<(i32, u8), usize>,
    ) -> Markup;
    fn get_prefix(&self) -> &str;
}

impl SiteRenderer for SiteRendererType {
    fn render_listing_page(
        &self,
        work_dir: &ThreadSafeWorkDir,
        config: ListingPageConfig,
        items: &[CrawlItem],
    ) -> Markup {
        match self {
            SiteRendererType::Blog => blog::render_listing_page(work_dir, config, items),
            SiteRendererType::Booru => booru::render_listing_page(work_dir, config, items),
            SiteRendererType::Reddit => reddit::render_listing_page(work_dir, config, items),
        }
    }

    fn render_detail_page(
        &self,
        work_dir: &ThreadSafeWorkDir,
        item: &CrawlItem,
        file: &FileCrawlType,
    ) -> Markup {
        match self {
            SiteRendererType::Blog => blog::render_detail_page(work_dir, item, file),
            SiteRendererType::Booru => booru::render_detail_page(work_dir, item, file),
            SiteRendererType::Reddit => reddit::render_detail_page(work_dir, item, file),
        }
    }

    fn render_tags_page(
        &self,
        work_dir: &ThreadSafeWorkDir,
        tags: &HashMap<String, usize>,
    ) -> Markup {
        match self {
            SiteRendererType::Blog => blog::render_tags_page(work_dir, tags),
            SiteRendererType::Booru => booru::render_tags_page(work_dir, tags),
            SiteRendererType::Reddit => reddit::render_tags_page(work_dir, tags),
        }
    }

    fn render_archive_page(
        &self,
        work_dir: &ThreadSafeWorkDir,
        archive: &HashMap<(i32, u8), usize>,
    ) -> Markup {
        match self {
            SiteRendererType::Blog => blog::render_archive_page(work_dir, archive),
            SiteRendererType::Booru => booru::render_archive_page(work_dir, archive),
            SiteRendererType::Reddit => reddit::render_archive_page(work_dir, archive),
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
