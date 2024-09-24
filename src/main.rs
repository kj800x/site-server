use actix_files::Files;
use actix_session::{storage::CookieSessionStore, SessionMiddleware};
use actix_web::{
    cookie::Key,
    get, middleware,
    web::{self},
    App, HttpServer, Responder,
};
use actix_web::{Either, HttpResponse};
use actix_web_opentelemetry::{PrometheusMetricsHandler, RequestMetrics, RequestTracing};
use chrono::{TimeZone, Utc};
use clap::Parser;
use indexmap::IndexMap;
use maud::{html, Markup, Render};
use opentelemetry::global;
use opentelemetry_sdk::metrics::MeterProvider;
use rand::seq::IteratorRandom;
use site::CrawlItem;
use std::{fs::File, io::Read, path::Path};
use workdir::WorkDir;

mod collections;
mod errors;
mod serde;
mod site;
mod workdir;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    work_dir: String,
}

struct StartTime(i64);
struct WorkDirPrefix(String);

#[derive(clap::Subcommand)]
enum Commands {
    Serve,
}

/// Links to a CSS stylesheet at the given path.
struct Css(&'static str);

impl Render for Css {
    fn render(&self) -> Markup {
        html! {
            link rel="stylesheet" type="text/css" href=(self.0);
        }
    }
}

macro_rules! serve_static_file {
    ($file:expr) => {
        web::resource(concat!("res/", $file)).route(web::get().to(|| async move {
            let path = Path::new("src/res").join($file);

            if path.exists() && path.is_file() {
                let mut file = File::open(path).unwrap();
                let mut contents = String::new();
                file.read_to_string(&mut contents).unwrap();
                HttpResponse::Ok()
                    .append_header(("x-resource-source", "disk"))
                    .body(contents)
            } else {
                HttpResponse::Ok()
                    .append_header(("x-resource-source", "embedded"))
                    .body(include_str!(concat!("res/", $file)))
            }
        }))
    };
}

// #[derive(Debug, Serialize, Deserialize)]
// struct HydratedClass {
//     id: i64,
//     name: String,
//     latest: Option<EventResult>,
// }

// #[get("/api/class")]
// async fn event_class_listing(
//     pool: web::Data<Pool<SqliteConnectionManager>>,
//     user_id: UserId,
// ) -> Result<impl Responder, actix_web::Error> {
//     let classes = get_classes(&pool, user_id.into_inner()?).await.unwrap();
//     Ok(web::Json(classes))
// }

// #[get("/api/ui/homepage")]
// async fn home_page_omnibus(
//     pool: web::Data<Pool<SqliteConnectionManager>>,
//     user_id: UserId,
// ) -> Result<impl Responder, actix_web::Error> {
//     let uid = user_id.into_inner()?;

//     let classes = get_classes(&pool, uid).await.unwrap();

//     let hydrated_classes: Vec<HydratedClass> = join_all(classes.iter().map(|x| async {
//         HydratedClass {
//             id: x.id,
//             name: x.name.clone(),
//             latest: get_latest_event(&pool, x.id, uid).await.unwrap(),
//         }
//     }))
//     .await;

//     Ok(web::Json(hydrated_classes))
// }

// #[derive(Debug, Serialize, Deserialize)]
// struct StatsResponse {
//     class: ClassResult,
//     events: Vec<EventResult>,
// }

fn date_time_element(timestamp: Option<u64>) -> Markup {
    match timestamp {
        Some(x) => {
            let time = Utc.timestamp_millis_opt(x as i64).unwrap();

            html! {
                time datetime=(time.to_rfc3339()) {
                    (time.to_string())
                }
            }
        }
        None => {
            html! {
                b {
                    "None"
                }
            }
        }
    }
}

#[get("/info")]
async fn info_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<WorkDir>,
    start_time: web::Data<StartTime>,
) -> Result<impl Responder, actix_web::Error> {
    let latest_update = workdir.crawled.items.values().map(|x| x.last_seen).max();
    let first_update = workdir.crawled.items.values().map(|x| x.first_seen).min();

    return Ok(html! {
        (Css("/res/styles.css"))
        h1 { "Hello, world!" }
        // p.intro {
        //     "This is an example of the "
        //     a href="https://github.com/lambda-fairy/maud" { "Maud" }
        //     " template language."
        // }
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

fn item_thumbnail(item: &CrawlItem, site: &str) -> Markup {
    html! {
        a.item_thumb_container href=(format!("/{}/item/{}/{}", site, item.key, item.files.keys().into_iter().next().unwrap_or(&"".to_string()))) {
            .item_thumb_img {img src=(format!("/{}/assets/{}", site, item.thumbnail_path().unwrap_or("404".to_string()))) {}}
            .item_thumb_tags {
                @for tag in &item.tags {
                    @match tag {
                        site::CrawlTag::Simple(x) => .tag { (x) },
                        site::CrawlTag::Detailed { value, .. } => .tag { (value) },
                    }
                }
            }
        }
    }
}

#[get("/random")]
async fn random_handler(
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<WorkDir>,
) -> Result<impl Responder, actix_web::Error> {
    let rng = &mut rand::thread_rng();
    let items = workdir
        .crawled
        .items
        .values()
        .into_iter()
        .choose_multiple(rng, 40);

    return Ok(html! {
        (Css("/res/styles.css"))
        h1 { "Hello, random world!" }
        @for item in &items {
            ( item_thumbnail(&item, &site.0) )
        }
    });
}

#[get("/item/{item}")]
async fn item_redirect(
    path: web::Path<String>,
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<WorkDir>,
) -> Result<Either<impl Responder, HttpResponse>, actix_web::Error> {
    let item = workdir.crawled.items.get(path.as_str());

    let Some(item) = item else {
        return Ok(Either::Left(html! {
            (Css("/res/styles.css"))
            h1 { "Hello!" }
            p { "Item not found" }
        }));
    };

    let file = item.files.keys().into_iter().next();

    let Some(file_key) = file else {
        return Ok(Either::Left(html! {
            (Css("/res/styles.css"))
            h1 { "Hello!" }
            p { "Item had no files" }
        }));
    };

    Ok(Either::Right(
        actix_web::HttpResponse::SeeOther()
            .append_header((
                "Location",
                format!("/{}/item/{}/{}", site.0, item.key, file_key),
            ))
            .finish(),
    ))
}

#[get("/item/{item}/{file}")]
async fn item_handler(
    path: web::Path<(String, String)>,
    site: web::Data<WorkDirPrefix>,
    workdir: web::Data<WorkDir>,
) -> Result<impl Responder, actix_web::Error> {
    let item = workdir.crawled.items.get(path.0.as_str());

    let Some(item) = item else {
        return Ok(html! {
            (Css("/res/styles.css"))
            h1 { "Hello!" }
            p { "Item not found" }
        });
    };

    let file = item.files.get(path.1.as_str());

    let Some(file) = file else {
        return Ok(html! {
            (Css("/res/styles.css"))
            h1 { "Hello!" }
            p { "File not found" }
        });
    };

    // FIXME: Handle denesting of intermediate files properly

    return Ok(html! {
        (Css("/res/styles.css"))
        .item_detail_page_container {
            .item_detail_page_sidebar {
                dt { "Source" }
                // FIXME: Fancy linking, show only the domain name
                dd { (item.url) }
                dt { "Data Directory"}
                dd { (site.0) }
                dt { "Title" }
                dd { (item.title) }
                dt { "Description" }
                dd { (item.description) }
                dt { "Published" }
                // TODO FIXME: date_time_element should take in i64
                // FIXME: These would be nice as timeago style strings
                dd { (date_time_element(Some(item.source_published as u64))) }
                dt { "First Seen" }
                dd { (date_time_element(Some(item.first_seen))) }
                dt { "Last Seen" }
                dd { (date_time_element(Some(item.last_seen))) }

                dt { "Item Key" }
                dd { (item.key) }

                dt { "Files" }
                dd {
                    @for file in &item.files {
                        a href=(format!("/{}/item/{}/{}", site.0, item.key, file.0)) { (file.0) }
                    }
                }

                // TODO: Dynamically insert item.meta here

                // FIXME: How do we handle no-tags
                dt { "Tags" }
                dd {
                    @for tag in &item.tags {
                        @match tag {
                            site::CrawlTag::Simple(x) => .tag { (x) },
                            site::CrawlTag::Detailed { value, .. } => .tag { (value) },
                        }
                    }
                }
            }
            .item_detail_page_file {
                // FIXME: Support all the file types
                img src=(format!("/{}/assets/{}", site.0, item.thumbnail_path().unwrap_or("404".to_string()))) {}
            }
        }
    });
}

// #[get("/assets/{tail:.*}")]
// async fn assets_handler(
//     path: web::Path<String>,
//     work_dir: web::Data<WorkDir>,
// ) -> Result<impl Responder, actix_web::Error> {
//     let workdir = work_dir.clone();
//     let path = path.into_inner();

//     let asset = workdir.path.join("assets").join(path);

//     Ok(actix_files::NamedFile::open(asset)?)
// }

// #[post("/api/class")]
// async fn event_class_create(
//     create_class: Json<CreateClass>,
//     pool: web::Data<Pool<SqliteConnectionManager>>,
//     user_id: UserId,
// ) -> Result<impl Responder, actix_web::Error> {
//     let class = insert_class(&pool, create_class.into_inner(), user_id.into_inner()?)
//         .await
//         .unwrap();
//     Ok(web::Json(class))
// }

// #[put("/api/class/{id}")]
// async fn event_class_update(
//     create_class: Json<CreateClass>,
//     id: web::Path<i64>,
//     pool: web::Data<Pool<SqliteConnectionManager>>,
//     user_id: UserId,
// ) -> Result<impl Responder, actix_web::Error> {
//     let class = update_class(
//         &pool,
//         id.into_inner(),
//         create_class.into_inner(),
//         user_id.into_inner()?,
//     )
//     .await
//     .unwrap();
//     Ok(web::Json(class))
// }

// #[delete("/api/class/{id}")]
// async fn event_class_delete(
//     id: web::Path<i64>,
//     pool: web::Data<Pool<SqliteConnectionManager>>,
//     user_id: UserId,
// ) -> Result<impl Responder, actix_web::Error> {
//     delete_class(&pool, id.into_inner(), user_id.into_inner()?)
//         .await
//         .unwrap();

//     Ok(HttpResponse::NoContent())
// }

// #[get("/api/class/{class_id}/events")]
// async fn event_class_events(
//     class_id: web::Path<i64>,
//     pool: web::Data<Pool<SqliteConnectionManager>>,
//     user_id: UserId,
// ) -> Result<impl Responder, actix_web::Error> {
//     let events = get_events(&pool, class_id.into_inner(), user_id.into_inner()?)
//         .await
//         .unwrap();
//     Ok(web::Json(events))
// }

// #[post("/api/class/{class_id}/events")]
// async fn record_event(
//     create_event: Json<CreateEvent>,
//     class_id: web::Path<i64>,
//     pool: web::Data<Pool<SqliteConnectionManager>>,
//     user_id: UserId,
// ) -> Result<impl Responder, actix_web::Error> {
//     let event = insert_event(
//         &pool,
//         class_id.into_inner(),
//         create_event.into_inner(),
//         user_id.into_inner()?,
//     )
//     .await
//     .unwrap();
//     Ok(web::Json(event))
// }

// #[delete("/api/class/{class_id}/event/{event_id}")]
// async fn delete_event(
//     path_params: web::Path<(i64, i64)>,
//     pool: web::Data<Pool<SqliteConnectionManager>>,
//     user_id: UserId,
// ) -> Result<impl Responder, actix_web::Error> {
//     let (class_id, event_id) = path_params.into_inner();

//     db_delete_event(&pool, class_id, event_id, user_id.into_inner()?)
//         .await
//         .unwrap();

//     Ok(HttpResponse::NoContent())
// }

// #[get("/api/class/{class_id}/events/latest")]
// async fn event_class_latest_event(
//     class_id: web::Path<i64>,
//     pool: web::Data<Pool<SqliteConnectionManager>>,
//     user_id: UserId,
// ) -> Result<impl Responder, actix_web::Error> {
//     let event = get_latest_event(&pool, class_id.into_inner(), user_id.into_inner()?)
//         .await
//         .unwrap();
//     Ok(web::Json(event))
// }

// async fn manual_hello() -> impl Responder {
//     HttpResponse::Ok().body("Hey there!")
// }

// #[get("/api/auth")]
// async fn profile(
//     pool: web::Data<Pool<SqliteConnectionManager>>,
//     user_id: UserId,
// ) -> Result<impl Responder, actix_web::Error> {
//     let profile = fetch_profile(&pool, user_id.into_inner()?).await?;

//     Ok(web::Json(profile))
// }

// #[post("/api/auth")]
// async fn login(
//     login: Json<Login>,
//     pool: web::Data<Pool<SqliteConnectionManager>>,
//     session: Session,
// ) -> Result<impl Responder, actix_web::Error> {
//     let uid = authenticate(&pool, login.username.clone(), &login.password).await?;

//     session.insert("user_id", uid)?;

//     Ok(HttpResponse::Ok().body("Login success!"))
// }

// #[post("/api/auth/register")]
// async fn register(
//     registration: Json<Registration>,
//     pool: web::Data<Pool<SqliteConnectionManager>>,
//     session: Session,
// ) -> Result<impl Responder, actix_web::Error> {
//     if std::env::var("ALLOW_REGISTRATION").unwrap_or("false".to_string()) == "false" {
//         return Ok(HttpResponse::Forbidden().body("Registration is disabled"));
//     }

//     let reg = registration.into_inner();

//     if reg.username.trim().is_empty() {
//         return Err(error::ErrorBadRequest("username cannot be empty"));
//     }
//     if reg.password.trim().is_empty() {
//         return Err(error::ErrorBadRequest("password cannot be empty"));
//     }
//     if reg.name.trim().is_empty() {
//         return Err(error::ErrorBadRequest("name cannot be empty"));
//     }

//     let uid = sign_up(&pool, reg).await?;

//     session.insert("user_id", uid)?;

//     Ok(HttpResponse::Ok().body("Registration success!"))
// }

// #[delete("/api/auth")]
// async fn logout(session: Session) -> impl Responder {
//     session.clear();

//     HttpResponse::Ok().body("Logout success!")
// }

async fn run() -> crate::errors::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let cli = Cli::parse();
    println!("Loading WorkDir...");
    let work_dir = WorkDir::new(cli.work_dir)?;

    let work_dir_path = work_dir.path.clone();
    let mut work_dir_map = IndexMap::new();
    work_dir_map.insert(
        work_dir_path
            .clone()
            .as_os_str()
            .to_string_lossy()
            .to_string(),
        work_dir,
    );

    let registry = prometheus::Registry::new();
    let exporter = opentelemetry_prometheus::exporter()
        .with_registry(registry.clone())
        .build()
        .unwrap();
    let provider = MeterProvider::builder().with_reader(exporter).build();
    global::set_meter_provider(provider);

    match &cli.command {
        Commands::Serve {} => {
            log::info!("Starting HTTP server at http://localhost:8080");

            HttpServer::new(move || {
                let mut app = App::new()
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
                    .app_data(web::Data::new(work_dir_map.clone()))
                    .app_data(web::Data::new(StartTime(Utc::now().timestamp_millis())))
                    .wrap(middleware::Logger::default())
                    .service(serve_static_file!("styles.css"));

                for (path, workdir) in work_dir_map.iter() {
                    app = app.service(
                        web::scope(path)
                            .app_data(web::Data::new(workdir.clone()))
                            .app_data(web::Data::new(WorkDirPrefix(path.clone())))
                            .service(info_handler)
                            .service(random_handler)
                            .service(item_handler)
                            .service(item_redirect)
                            .service(Files::new("/assets", workdir.path.clone()).prefer_utf8(true)),
                    );
                }

                app
            })
            .bind(("127.0.0.1", 8080))?
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
