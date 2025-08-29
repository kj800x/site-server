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
        // Read-only snapshot (drops before we take the write lock)
        let (prev_ts, workdir_path) = {
            let workdir = self.work_dir.read().expect("work_dir read poisoned");
            (workdir.last_seen_modified, workdir.path.clone())
        };

        // Single stat; treat missing file as timestamp 0
        let latest_ts = std::fs::metadata(&workdir_path.join("crawled.json"))
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        if latest_ts > prev_ts {
            println!("Noticed update for {}", workdir_path.to_string_lossy());

            let replacement = WorkDir::new(workdir_path.clone()).expect("rebuild WorkDir failed");

            let mut workdir = self.work_dir.write().expect("work_dir write poisoned");
            *workdir = replacement;
        }
    }
}
