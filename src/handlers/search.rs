use actix_web::{get, web, HttpResponse, Responder};
use maud::html;
use serde::Deserialize;
use urlencoding::{decode, encode};

use crate::handlers::{
    header, scripts, ListingPageConfig, ListingPageMode, ListingPageOrdering, SiteRenderer,
    SiteRendererType, SiteSource,
};
use crate::search::{evaluate_search_expr, parse_search_expr};
use crate::site::CrawlItem;

#[derive(Deserialize)]
pub struct SearchQuery {
    q: Option<String>,
}

#[get("/search")]
pub async fn search_form_handler(
    renderer: web::Data<SiteRendererType>,
    site_source: web::Data<SiteSource>,
    query: web::Query<SearchQuery>,
) -> impl Responder {
    let renderer = renderer.into_inner();
    let site_prefix = site_source.slug();
    let rendering_prefix = renderer.get_prefix();

    // If query parameter is provided, redirect to results page
    if let Some(ref q) = query.q {
        if !q.trim().is_empty() {
            let encoded_query = encode(q);
            return HttpResponse::SeeOther()
                .append_header((
                    "Location",
                    format!(
                        "/{}/{}/search/{}/1",
                        site_prefix, rendering_prefix, encoded_query
                    ),
                ))
                .finish();
        }
    }

    // Otherwise show the search form
    let prefill_value = query.q.as_deref().unwrap_or("");

    let html = html! {
        (maud::DOCTYPE)
        html {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1" {}
                (scripts())
                title { "Search" }
            }
            body.search-page hx-ext="morph" {
                (header(&site_prefix, &rendering_prefix, "/search"))
                main {
                    .search-page-container {
                        form.search-form-container method="get" action=(format!("/{}/{}/search", site_prefix, rendering_prefix)) {
                            input.search-input type="text" name="q" value=(prefill_value) placeholder="(tag \"foobar\")" autofocus {}
                            button.search-submit type="submit" { "Search" }
                            .search-info-icon {
                                "help"
                                .search-tooltip {
                                    h3 { "Available Functions" }
                                    ul {
                                        li { code { "and" } " - all arguments must match (varargs)" }
                                        li { code { "or" } " - any argument must match (varargs)" }
                                        li { code { "not" } " - negates the argument (unary)" }
                                        li { code { "tag" } " - exact tag match (case-insensitive)" }
                                        li { code { "type" } " - file type: \"image\", \"video\", or \"text\"" }
                                        li { code { "site" } " - exact match on source site slug" }
                                        li { code { "fulltext" } " - search in title, meta, description, url, and text files" }
                                        li { code { "title" } " - substring match in title (case-insensitive)" }
                                        li { code { "meta" } " - substring match in any meta value (case-insensitive)" }
                                        li { code { "desc" } " - substring match in description (case-insensitive)" }
                                        li { code { "url" } " - substring match in source URL (case-insensitive)" }
                                        li { code { "after" } " - items published after the given time" }
                                        li { code { "before" } " - items published before the given time" }
                                        li { code { "during" } " - items published during the given time range" }
                                    }
                                    h3 { "Time Formats (for after/before/during)" }
                                    ul {
                                        li { "Relative: " code { "\"2 weeks ago\"" } ", " code { "\"1 year ago\"" } ", " code { "\"a month ago\"" } }
                                        li { "Named: " code { "\"last month\"" } ", " code { "\"this year\"" } ", " code { "\"yesterday\"" } ", " code { "\"today\"" } }
                                        li { "Month: " code { "\"January\"" } " or " code { "\"Jan\"" } " (most recent)" }
                                        li { "Year: " code { "\"2025\"" } }
                                        li { "Date: " code { "\"Jan 15, 2025\"" } ", " code { "\"1/15/2025\"" } ", " code { "\"2025-01-15\"" } }
                                        li { "ISO8601: " code { "\"2024-01-01T00:00:00Z\"" } }
                                        li { "Unix ms: " code { "\"1704067200000\"" } }
                                    }
                                    p.timezone-note { "Times default to US Eastern timezone." }
                                    h3 { "Examples" }
                                    ul {
                                        li { code { "(tag \"foobar\")" } }
                                        li { code { "(and (tag \"cute\") (type \"image\"))" } }
                                        li { code { "(after \"2 weeks ago\")" } }
                                        li { code { "(during \"last month\")" } }
                                        li { code { "(during \"January\")" } }
                                        li { code { "(and (site \"r-aww\") (during \"2024\"))" } }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    };

    HttpResponse::Ok().body(html.0)
}

#[get("/search/{query}/{page}")]
pub async fn search_results_handler(
    renderer: web::Data<SiteRendererType>,
    site_source: web::Data<SiteSource>,
    path: web::Path<(String, usize)>,
) -> impl Responder {
    let (encoded_query, page) = path.into_inner();
    let renderer = renderer.into_inner();
    let site_prefix = site_source.slug();
    let rendering_prefix = renderer.get_prefix();

    // Decode the query
    let decoded_query = match decode(&encoded_query) {
        Ok(decoded) => decoded.to_string(),
        Err(_) => {
            return error_page(
                &site_prefix,
                &rendering_prefix,
                "Invalid URL encoding in search query",
            );
        }
    };

    // Parse the s-expression
    let expr = match parse_search_expr(&decoded_query) {
        Ok(expr) => expr,
        Err(e) => {
            return error_page(
                &site_prefix,
                &rendering_prefix,
                &format!("Parse error: {}", e),
            );
        }
    };

    // Get all items and filter
    let all_items: Vec<CrawlItem> = site_source.all_items();

    let filtered_items: Vec<CrawlItem> = all_items
        .into_iter()
        .filter(|item| evaluate_search_expr(&expr, item))
        .collect();

    // Sort by source_published (newest first)
    let mut sorted_items = filtered_items;
    sorted_items.sort_by_key(|item| -item.source_published);

    // Paginate
    let per_page = 15;
    let total = sorted_items.len();
    let start = (page - 1) * per_page;
    let end = if start + per_page > sorted_items.len() {
        sorted_items.len()
    } else {
        start + per_page
    };

    let paginated_items = if start >= sorted_items.len() {
        Vec::new()
    } else {
        sorted_items[start..end].to_vec()
    };

    // Create a ListingPageConfig for rendering
    let config = ListingPageConfig {
        mode: ListingPageMode::Search {
            query: encoded_query.clone(),
        },
        ordering: ListingPageOrdering::NewestFirst,
        page,
        per_page,
        total,
    };

    // Render the results using the existing renderer
    let route = format!("/search/{}/{}", encoded_query, page);
    let rendered = renderer.render_listing_page(&site_prefix, config, &paginated_items, &route);

    HttpResponse::Ok().body(rendered.0)
}

fn error_page(site_prefix: &str, rendering_prefix: &str, error_msg: &str) -> HttpResponse {
    let html = html! {
        (maud::DOCTYPE)
        html {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1" {}
                (scripts())
                title { "Search Error" }
            }
            body.search-error-page hx-ext="morph" {
                (header(site_prefix, rendering_prefix, "/search"))
                main {
                    .error-page-container {
                        .error-box {
                            h2 { "Search Error" }
                            p { (error_msg) }
                            .error-action {
                                a href=(format!("/{}/{}/search", site_prefix, rendering_prefix)) {
                                    "Start Over"
                                }
                            }
                        }
                    }
                }
            }
        }
    };

    HttpResponse::Ok().body(html.0)
}
