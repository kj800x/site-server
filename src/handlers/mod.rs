use maud::{html, Markup};

mod blog;
mod booru;
mod common;
mod reddit;

pub use blog::configure_blog;
pub use booru::configure_booru;
pub use reddit::configure_reddit;

// Re-export common types used by handlers
pub use common::*;

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
