use actix_files::Files;
use actix_session::{storage::CookieSessionStore, SessionMiddleware};
use actix_web::{
    cookie::Key,
    get, middleware,
    web::{self},
    App, HttpResponse, HttpServer, Responder,
};
use actix_web_httpauth::middleware::HttpAuthentication;
use actix_web_opentelemetry::{PrometheusMetricsHandler, RequestMetrics, RequestTracing};
use chrono::Utc;
use clap::Parser;
use opentelemetry::global;
use opentelemetry_sdk::metrics::MeterProvider;
use site_server::{bake::Bake, workdir_dao::WorkDirDao};
use std::io::Read;
use std::{thread, time::Duration};

use site_server::{
    errors,
    handlers::{
        self, generic_archive_index_handler, generic_archive_page_handler,
        generic_detail_full_handler, generic_detail_handler, generic_detail_redirect,
        generic_index_handler, generic_index_root_handler, generic_latest_handler,
        generic_latest_page_handler, generic_oldest_handler, generic_oldest_page_handler,
        generic_random_handler, generic_tag_handler, generic_tag_page_handler,
        generic_tags_index_handler, media_viewer_fragment_handler, SiteRenderer,
    },
    serve_static_file, thread_safe_work_dir, workdir,
};

use handlers::{date_time_element, WorkDirPrefix};
use thread_safe_work_dir::ThreadSafeWorkDir;
use workdir::WorkDir;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

struct StartTime(i64);

#[derive(clap::Subcommand)]
enum Commands {
    Serve { work_dirs: Vec<String> },
    Bake { work_dirs: Vec<String> },
}

#[get("/healthz")]
async fn healthz(
    site: web::Data<Vec<WorkDirDao>>,
    start_time: web::Data<StartTime>,
) -> impl Responder {
    let sites = site
        .iter()
        .map(|site| site.work_dir.read().unwrap().config.label.clone())
        .collect::<Vec<String>>();

    HttpResponse::Ok().body(format!(
        "OK. {} sites. Started {}.",
        sites.len(),
        start_time.0
    ))
}

#[get("/")]
async fn root_index_handler(
    site: web::Data<Vec<WorkDirDao>>,
    start_time: web::Data<StartTime>,
) -> Result<impl Responder, actix_web::Error> {
    use maud::html;

    // Create a vector of sites with their latest update times for sorting
    let mut sites_with_updates: Vec<_> = site
        .iter()
        .map(|site| {
            let site = site.work_dir.read().unwrap();
            let latest_update = site.crawled.items.values().map(|x| x.last_seen).max();
            (site, latest_update)
        })
        .collect();

    // Sort by latest update time in descending order (most recent first)
    sites_with_updates
        .sort_by_key(|(_, latest_update)| std::cmp::Reverse(latest_update.unwrap_or(0)));

    return Ok(html! {
        html {
            head {
                (handlers::scripts())
                title { "Site Server"}
            }
            body {
                h1.page_title { "Loaded sites" }
                table.site_table {
                    thead {
                        tr {
                            th { "Site" }
                            th { "First Update" }
                            th { "Latest Update" }
                            th { "Last Reloaded" }
                            th { "Total Items" }
                            th { "Links" }
                        }
                    }
                    tbody {
                        @for (site, _) in sites_with_updates {
                            @let latest_update = site.crawled.items.values().map(|x| x.last_seen).max();
                            @let first_update = site.crawled.items.values().map(|x| x.first_seen).min();
                            @let loaded_at = site.loaded_at;
                            tr {
                                td { (site.config.label) }
                                td { (handlers::date_time_element(first_update)) }
                                td { (handlers::date_time_element(latest_update)) }
                                td { (handlers::date_time_element(Some(loaded_at as u64))) }
                                td { (site.crawled.iter().count()) }
                                td {
                                    a.site_link href=(format!("/{}/booru/latest", site.config.slug)) { "Booru" }
                                    "|"
                                    a.site_link href=(format!("/{}/blog/latest", site.config.slug)) { "Blog" }
                                    "|"
                                    a.site_link href=(format!("/{}/r/latest", site.config.slug)) { "Reddit" }
                                }
                            }
                        }
                    }
                }
                .root_handler_info {
                    p {
                        "The site server was started on "
                        (date_time_element(Some(start_time.0.try_into().unwrap())))
                    }
                }
            }
        }
    });
}

async fn run() -> errors::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let cli = Cli::parse();

    match &cli.command {
        Commands::Bake { work_dirs } => {
            println!("Loading WorkDirs...");
            let mut work_dirs_vec = vec![];
            for work_dir in work_dirs.into_iter() {
                println!("Loading WorkDir: {}", work_dir);
                let work_dir = WorkDir::new(work_dir.to_string()).expect("Failed to load WorkDir");
                work_dirs_vec.push(work_dir);
            }

            for work_dir in work_dirs_vec.iter() {
                println!("Baking WorkDir: {}", work_dir.config.label);
                work_dir.bake_all();
            }

            Ok(())
        }

        Commands::Serve { work_dirs } => {
            println!("Loading WorkDirs...");
            let mut work_dirs_vec: Vec<WorkDirDao> = vec![];

            for work_dir in work_dirs.into_iter() {
                println!("Loading WorkDir: {}", work_dir);
                let work_dir = WorkDir::new(work_dir.to_string()).expect("Failed to load WorkDir");
                let threadsafe_work_dir = ThreadSafeWorkDir::new(work_dir);
                let update_clone = threadsafe_work_dir.clone();
                work_dirs_vec.push(WorkDirDao::Local(threadsafe_work_dir));

                // Spawn a thread to watch the workdir for changes
                thread::spawn(move || loop {
                    thread::sleep(Duration::from_secs(60));
                    update_clone.check_for_updates();
                });
            }

            let registry = prometheus::Registry::new();
            let exporter = opentelemetry_prometheus::exporter()
                .with_registry(registry.clone())
                .build()
                .unwrap();
            let provider = MeterProvider::builder().with_reader(exporter).build();
            global::set_meter_provider(provider);

            let listen_address = std::env::var("LISTEN_ADDRESS").unwrap_or("127.0.0.1".to_owned());

            log::info!("Starting HTTP server at http://{}:8080", listen_address);

            HttpServer::new(move || {
                let auth = HttpAuthentication::with_fn(handlers::validator);

                let mut app = App::new()
                    .wrap(auth) // Guard all routes with HTTP Basic Auth
                    .wrap(RequestTracing::new())
                    .wrap(RequestMetrics::default())
                    .route(
                        "/api/metrics",
                        web::get().to(PrometheusMetricsHandler::new(registry.clone())),
                    )
                    .wrap(
                        SessionMiddleware::builder(
                            CookieSessionStore::default(),
                            Key::from(&[0; 64]),
                        )
                        .cookie_secure(false)
                        .build(),
                    )
                    .app_data(web::Data::new(work_dirs_vec.clone()))
                    .app_data(web::Data::new(StartTime(Utc::now().timestamp_millis())))
                    .wrap(
                        middleware::Logger::default()
                            .exclude("/healthz")
                            .exclude("/api/metrics"),
                    )
                    .service(serve_static_file!("styles.css"))
                    .service(serve_static_file!("page-transitions.css"))
                    .service(serve_static_file!("detail_page.js"))
                    .service(serve_static_file!("idiomorph.min.js"))
                    .service(serve_static_file!("idiomorph-ext.min.js"))
                    .service(serve_static_file!("htmx.min.js"))
                    .service(healthz)
                    .service(root_index_handler);

                for workdir in work_dirs_vec.iter() {
                    let slug = workdir.slug();

                    let renderers = vec![
                        handlers::SiteRendererType::Blog,
                        handlers::SiteRendererType::Booru,
                        handlers::SiteRendererType::Reddit,
                    ];

                    // Ordering matters, do more specific routes first
                    for renderer in renderers.iter() {
                        app = app.service(
                            web::scope(&format!("{}/{}", slug, renderer.get_prefix()))
                                .app_data(web::Data::new(workdir.clone()))
                                .app_data(web::Data::new(renderer.clone()))
                                .app_data(web::Data::new(WorkDirPrefix(slug.clone())))
                                .service(generic_index_handler)
                                .service(generic_index_root_handler)
                                .service(generic_random_handler)
                                .service(generic_latest_page_handler)
                                .service(generic_latest_handler)
                                .service(generic_oldest_page_handler)
                                .service(generic_oldest_handler)
                                .service(generic_tags_index_handler)
                                .service(generic_tag_page_handler)
                                .service(generic_tag_handler)
                                .service(generic_archive_page_handler)
                                .service(generic_archive_index_handler)
                                .service(generic_detail_handler)
                                .service(generic_detail_redirect)
                                .service(generic_detail_full_handler)
                                .service(media_viewer_fragment_handler),
                        );
                    }

                    app = app.service(
                        web::scope(&slug)
                            .app_data(web::Data::new(workdir.clone()))
                            .app_data(web::Data::new(WorkDirPrefix(slug.clone())))
                            .service(
                                // FIXME: Serving these files seems to exhaust the worker pool
                                // and the server stops responding to requests. This aint good.
                                Files::new("/assets", workdir.path()).prefer_utf8(true),
                            ),
                    );
                }

                app
            })
            .bind((listen_address, 8080))?
            .run()
            .await?;

            Ok(())
        }
    }
}

#[actix_web::main]
async fn main() {
    if let Err(ref _e) = run().await {
        // _e.print();
        ::std::process::exit(1);
    }
}
