use actix_web::web;

// Common error response for locked workdir
pub fn workdir_locked_error() -> actix_web::Error {
    actix_web::Error::from(actix_web::error::ErrorServiceUnavailable(
        "Work directory is locked",
    ))
}

// Helper function to get workdir from ThreadSafeWorkDir
pub fn get_workdir<'a>(
    workdir: &'a web::Data<super::ThreadSafeWorkDir>,
) -> Result<std::sync::RwLockReadGuard<'a, crate::workdir::WorkDir>, actix_web::Error> {
    let workdir_lock = workdir.work_dir.try_read();
    match workdir_lock {
        Ok(x) => Ok(x),
        Err(_) => Err(workdir_locked_error()),
    }
}

pub fn date_time_element(timestamp: Option<u64>) -> maud::Markup {
    use chrono::{TimeZone, Utc};
    use maud::html;

    if let Some(ts) = timestamp {
        let time = Utc.timestamp_millis_opt(ts as i64).unwrap();
        html! {
            time datetime=(time.to_rfc3339()) title=(time.to_rfc3339()) {
                (time.format("%B %d, %Y"))
            }
        }
    } else {
        html! {
            span { "never" }
        }
    }
}

/// Get the first downloaded file ID from an item
pub fn get_first_downloaded_file_id(item: &crate::site::CrawlItem) -> Option<String> {
    use indexmap::IndexMap;
    use crate::site::FileCrawlType;
    
    item.flat_files()
        .into_iter()
        .filter(|(_, file)| file.is_downloaded())
        .collect::<IndexMap<String, FileCrawlType>>()
        .keys()
        .next()
        .cloned()
}
