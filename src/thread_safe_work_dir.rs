use crate::errors::ResultExt;
use crate::workdir::WorkDir;
use std::sync::Arc;
use std::sync::RwLock;

#[derive(Clone)]
pub struct ThreadSafeWorkDir {
    pub work_dir: Arc<RwLock<WorkDir>>,
}

impl ThreadSafeWorkDir {
    pub fn new(work_dir: WorkDir) -> Self {
        Self {
            work_dir: Arc::new(RwLock::new(work_dir)),
        }
    }

    pub fn check_for_updates(&self) {
        // Acquire a read lock and check the last seen modified timestamp
        // If the file has been modified, take a write lock and recreate the workdir
        let workdir_data = self.work_dir.read();
        let workdir = workdir_data.unwrap();
        let previous_last_seen_modified = workdir.last_seen_modified;

        let crawled_path = workdir.path.join("crawled.json");
        let latest_last_seen_modified = if crawled_path.exists() {
            let metadata = std::fs::metadata(crawled_path)
                .context("Unable to get metadata for crawled.json")
                .unwrap();

            metadata
                .modified()
                .context("Unable to get modified time for crawled.json")
                .unwrap()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        } else {
            0
        };

        if latest_last_seen_modified > previous_last_seen_modified {
            println!("Noticed update for {}", workdir.path.to_string_lossy());

            let replacement_workdir =
                WorkDir::new(workdir.path.to_string_lossy().to_string()).unwrap();

            let workdir_data = self.work_dir.write();
            let mut workdir = workdir_data.unwrap();

            workdir.last_seen_modified = replacement_workdir.last_seen_modified;
            workdir.crawled = replacement_workdir.crawled;
            workdir.config = replacement_workdir.config;
            workdir.path = replacement_workdir.path;
        }
    }
}
