use std::path::PathBuf;

use crate::thread_safe_work_dir::ThreadSafeWorkDir;

#[derive(Clone)]
pub enum WorkDirDao {
    Local(ThreadSafeWorkDir),
    Remote, // TODO: Implement later
}

impl WorkDirDao {
    pub fn get_underlying_work_dir(&self) -> Option<&ThreadSafeWorkDir> {
        match self {
            WorkDirDao::Local(tswd) => Some(tswd),
            WorkDirDao::Remote => None,
        }
    }

    pub fn slug(&self) -> String {
        match self {
            WorkDirDao::Local(tswd) => tswd.work_dir.read().unwrap().config.slug.clone(),
            WorkDirDao::Remote => todo!(),
        }
    }

    pub fn path(&self) -> PathBuf {
        match self {
            WorkDirDao::Local(tswd) => tswd.work_dir.read().unwrap().path.to_path_buf(),
            WorkDirDao::Remote => todo!(),
        }
    }
}
