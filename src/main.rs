use actix_session::{storage::CookieSessionStore, SessionMiddleware};
use actix_web::{
    cookie::Key,
    get, middleware,
    web::{self},
    App, HttpServer, Responder,
};
use actix_web_opentelemetry::{PrometheusMetricsHandler, RequestMetrics, RequestTracing};
use clap::Parser;
use maud::html;
use opentelemetry::global;
use opentelemetry_sdk::metrics::MeterProvider;
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

#[derive(clap::Subcommand)]
enum Commands {
    Serve,
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

#[get("/hello")]
async fn hello_handler(work_dir: web::Data<WorkDir>) -> Result<impl Responder, actix_web::Error> {
    let workdir = work_dir.clone();

    return Ok(html! {
        h1 { "Hello, world!" }
        p.intro {
            "This is an example of the "
            a href="https://github.com/lambda-fairy/maud" { "Maud" }
            " template language."
        }
        p {
            "The current site is: "
            code { (workdir.path.to_string_lossy()) }
        }
        p {
            "The items in the site are: "
            ul {
                @for (key, item) in workdir.crawled.iter() {
                    li { (key) ": " (item.url) }
                }
            }
        }
    });
}

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

    let registry = prometheus::Registry::new();
    let exporter = opentelemetry_prometheus::exporter()
        .with_registry(registry.clone())
        .build()
        .unwrap();
    let provider = MeterProvider::builder().with_reader(exporter).build();
    global::set_meter_provider(provider);

    match &cli.command {
        Commands::Serve {} => {
            log::info!("Starting HTTP server at http://localhost:8080/api");

            HttpServer::new(move || {
                App::new()
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
                    .app_data(web::Data::new(work_dir.clone()))
                    .wrap(middleware::Logger::default())
                    .service(hello_handler)
                // .service(home_page_omnibus)
                // .service(stats_page_omnibus)
                // .service(event_class_listing)
                // .service(event_class_create)
                // .service(event_class_update)
                // .service(event_class_delete)
                // .service(event_class_events)
                // .service(event_class_latest_event)
                // .service(record_event)
                // .service(delete_event)
                // .service(profile)
                // .service(login)
                // .service(logout)
                // .service(register)
                // .route("/api/hey", web::get().to(manual_hello))
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
