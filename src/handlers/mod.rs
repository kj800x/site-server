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
        todo!()
    }

    fn render_detail_page(
        &self,
        work_dir: &ThreadSafeWorkDir,
        item: &CrawlItem,
        file: &FileCrawlType,
    ) -> Markup {
        todo!()
    }

    fn render_tags_page(
        &self,
        work_dir: &ThreadSafeWorkDir,
        tags: &HashMap<String, usize>,
    ) -> Markup {
        todo!()
    }

    fn render_archive_page(
        &self,
        work_dir: &ThreadSafeWorkDir,
        archive: &HashMap<(i32, u8), usize>,
    ) -> Markup {
        todo!()
    }

    fn get_prefix(&self) -> &str {
        match self {
            SiteRendererType::Blog => "blog",
            SiteRendererType::Booru => "booru",
            SiteRendererType::Reddit => "r",
        }
    }
}
