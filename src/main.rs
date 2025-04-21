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
use std::io::Read;
use std::{thread, time::Duration};

use site_server::{
    errors,
    handlers::{
        self, generic_archive_index_handler, generic_archive_page_handler, generic_detail_handler,
        generic_detail_redirect, generic_index_handler, generic_index_root_handler,
        generic_latest_handler, generic_oldest_handler, generic_random_handler,
        generic_tag_handler, generic_tag_page_handler, generic_tags_index_handler, SiteRenderer,
    },
    serve_static_file, thread_safe_work_dir, workdir,
};

use handlers::{date_time_element, get_workdir, ThreadSafeWorkDir, WorkDirPrefix};
use thread_safe_work_dir::ThreadSafeWorkDir as ThreadSafeWorkDirImpl;
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
}

#[get("/info")]
async fn info_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<ThreadSafeWorkDir>,
    start_time: web::Data<StartTime>,
) -> Result<impl Responder, actix_web::Error> {
    use maud::html;

    let workdir = get_workdir(&workdir)?;
    let latest_update = workdir.crawled.items.values().map(|x| x.last_seen).max();
    let first_update = workdir.crawled.items.values().map(|x| x.first_seen).min();

    return Ok(html! {
        (handlers::Css("/res/styles.css"))
        h1 { "Hello, world!" }
        p {
            "The current site is: "
            code { (site.0) }
        }
        p {
            "The first update was on "
            (date_time_element(first_update))
        }
        p {
            "The latest update was on "
            (date_time_element(latest_update))
        }
        p {
            "The site server was started on "
            (date_time_element(Some(start_time.0.try_into().unwrap())))
        }
        p {
            "This site has " (workdir.crawled.iter().count()) " items"
        }
    });
}

#[get("/")]
async fn root_index_handler(
    site: web::Data<Vec<ThreadSafeWorkDir>>,
) -> Result<impl Responder, actix_web::Error> {
    use maud::html;

    return Ok(html! {
        (handlers::Css("/res/styles.css"))
        h1.page_title { "Loaded sites" }
        ul.site_list {
            @for site in site.iter() {
                @let site = site.work_dir.read().unwrap();
                li {
                    a.site_link href=(format!("/{}/booru/latest", site.config.slug)) { (site.config.label) }
                    " ("
                    a.site_link href=(format!("/{}/info", site.config.slug)) { "info" }
                    ")"
                }
            }
        }
    });
}

async fn run() -> errors::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let cli = Cli::parse();

    match &cli.command {
        Commands::Serve { work_dirs } => {
            println!("Loading WorkDirs...");
            let mut work_dirs_vec = vec![];
            for work_dir in work_dirs.into_iter() {
                println!("Loading WorkDir: {}", work_dir);
                let work_dir = WorkDir::new(work_dir.to_string()).expect("Failed to load WorkDir");
                let threadsafe_work_dir = ThreadSafeWorkDirImpl::new(work_dir);
                let update_clone = threadsafe_work_dir.clone();
                work_dirs_vec.push(threadsafe_work_dir);

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
                let auth = HttpAuthentication::basic(handlers::validator);

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
                    .wrap(middleware::Logger::default())
                    .service(serve_static_file!("styles.css"))
                    .service(serve_static_file!("detail_page.js"))
                    .service(root_index_handler);

                for workdir in work_dirs_vec.iter() {
                    let slug = workdir.work_dir.read().unwrap().config.slug.clone();

                    let renderers = vec![
                        handlers::SiteRendererType::Blog,
                        handlers::SiteRendererType::Booru,
                        handlers::SiteRendererType::Reddit,
                    ];

                    for renderer in renderers.iter() {
                        println!("Adding renderer: {}", renderer.get_prefix());
                        app = app.service(
                            web::scope(&format!("{}/{}", slug, renderer.get_prefix()))
                                .app_data(web::Data::new(workdir.clone()))
                                .app_data(web::Data::new(renderer.clone()))
                                .app_data(web::Data::new(WorkDirPrefix(slug.clone())))
                                .service(generic_index_handler)
                                .service(generic_index_root_handler)
                                .service(generic_random_handler)
                                .service(generic_latest_handler)
                                .service(generic_oldest_handler)
                                .service(generic_tags_index_handler)
                                .service(generic_tag_handler)
                                .service(generic_tag_page_handler)
                                .service(generic_archive_index_handler)
                                .service(generic_archive_page_handler)
                                .service(generic_detail_handler)
                                .service(generic_detail_redirect),
                        );
                    }

                    app = app.service(
                        web::scope(&slug)
                            .app_data(web::Data::new(workdir.clone()))
                            .app_data(web::Data::new(WorkDirPrefix(slug.clone())))
                            .service(info_handler)
                            .service(
                                Files::new(
                                    "/assets",
                                    workdir.work_dir.read().unwrap().path.clone(),
                                )
                                .prefer_utf8(true),
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
