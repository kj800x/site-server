
#[macro_export]
macro_rules! serve_static_file {
    ($file:expr) => {
        web::resource(concat!("res/", $file)).route(web::get().to(|| async move {
            let path = std::path::Path::new("src/res").join($file);

            if path.exists() && path.is_file() {
                let mut file = std::fs::File::open(path).unwrap();
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

pub mod collections;
pub mod errors;
pub mod handlers;
pub mod serde;
pub mod site;
pub mod thread_safe_work_dir;
pub mod workdir;
