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
        let (prev_ts, workdir_path, remote_url) = {
            let workdir = self.work_dir.read().expect("work_dir read poisoned");
            (
                workdir.last_seen_modified,
                workdir.path.clone(),
                workdir.remote_url.clone(),
            )
        };

        if let Some(ref remote_url) = remote_url {
            // Remote site: check version endpoint
            let version_url = format!("{}/v1/version", remote_url);
            let version_result = reqwest::blocking::get(&version_url)
                .and_then(|r| r.json::<serde_json::Value>());
            match version_result {
                Ok(version) => {
                    let latest_ts = version
                        .get("lastUpdatedAt")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);

                    if latest_ts > prev_ts {
                        log::info!(
                            "Noticed remote update for {}",
                            workdir_path.to_string_lossy()
                        );

                        match WorkDir::new(workdir_path.clone()) {
                            Ok(replacement) => {
                                let mut workdir =
                                    self.work_dir.write().expect("work_dir write poisoned");
                                *workdir = replacement;
                            }
                            Err(e) => {
                                log::warn!(
                                    "Failed to reload remote site {}: {}",
                                    workdir_path.to_string_lossy(),
                                    e
                                );
                            }
                        }
                    }
                }
                Err(e) => {
                    log::warn!(
                        "Failed to check remote version for {}: {}",
                        workdir_path.to_string_lossy(),
                        e
                    );
                }
            }
        } else {
            // Local site: existing file mtime check
            let latest_ts = std::fs::metadata(&workdir_path.join("crawled.json"))
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);

            if latest_ts > prev_ts {
                println!("Noticed update for {}", workdir_path.to_string_lossy());

                let replacement =
                    WorkDir::new(workdir_path.clone()).expect("rebuild WorkDir failed");

                let mut workdir = self.work_dir.write().expect("work_dir write poisoned");
                *workdir = replacement;
            }
        }
    }
}
