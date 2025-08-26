use std::{path::PathBuf, process::Command};

use crate::{
    errors::Error,
    site::{CrawlItem, FileCrawlType},
    workdir::WorkDir,
};

pub trait Bake {
    fn bake_all(&self);
}

impl FileCrawlType {
    pub fn is_image(&self) -> bool {
        matches!(self, FileCrawlType::Image { .. })
    }

    pub fn is_video(&self) -> bool {
        matches!(self, FileCrawlType::Video { .. })
    }
}

impl CrawlItem {
    pub fn calculate_auto_thumbnail_path(
        &self,
        work_dir_path: &PathBuf,
        thumbnail_of: &FileCrawlType,
    ) -> PathBuf {
        let hash = md5::compute(self.key.as_bytes());
        let hash_str = format!("{:x}", hash);

        let extension = match thumbnail_of {
            FileCrawlType::Image { .. } => "jpg",
            FileCrawlType::Video { .. } => "gif",
            _ => panic!("Cannot create thumbnail for non-image or non-video file"),
        };

        work_dir_path
            .join("auto_thumbnails")
            .join(hash_str)
            .with_extension(extension)
    }

    fn create_thumbnail(
        &self,
        work_dir_path: &PathBuf,
        thumbnail_of: &FileCrawlType,
    ) -> Result<(), Error> {
        let thumbnail_path = self.calculate_auto_thumbnail_path(work_dir_path, thumbnail_of);

        if !thumbnail_path.parent().unwrap().exists() {
            std::fs::create_dir_all(thumbnail_path.parent().unwrap()).unwrap();
            println!(
                "Created auto thumbnail directory ({})",
                thumbnail_path.parent().unwrap().display()
            );
        }

        match thumbnail_of {
            FileCrawlType::Video { filename, .. } => {
                let _output = Command::new("ffmpeg")
                    .arg("-ss")
                    .arg("30")
                    .arg("-t")
                    .arg("3")
                    .arg("-i")
                    .arg(work_dir_path.join(filename).to_str().unwrap())
                    .arg("-vf")
                    .arg("fps=10,scale=320:-1:flags=lanczos,split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse")
                    .arg("-loop")
                    .arg("0")
                    .arg(thumbnail_path.to_str().unwrap())
                    .output()
                    .expect("Failed to create video thumbnail");
            }

            FileCrawlType::Image { filename, .. } => {
                let _output = Command::new("ffmpeg")
                    .arg("-i")
                    .arg(work_dir_path.join(filename).to_str().unwrap())
                    .arg("-vf")
                    .arg("scale=320:-1")
                    .arg(thumbnail_path.to_str().unwrap())
                    .output()
                    .expect("Failed to create image thumbnail");
            }

            _ => {
                panic!("Cannot create thumbnail for non-image or non-video file");
            }
        }

        Ok(())
    }
}

/// Ensure that all items have previews available. If an explicit preview was
/// not provided by the site, attempt to generate a thumbnail.
impl Bake for WorkDir {
    fn bake_all(&self) {
        let items = self.crawled.clone();
        for item in items.values().into_iter() {
            if item.previews.is_empty() {
                let flat_files = item.flat_files();
                let first_usable_file = flat_files
                    .values()
                    .find(|file| file.is_downloaded() && (file.is_image() || file.is_video()));

                if let Some(first_usable_file) = first_usable_file {
                    let thumbnail_path = item.calculate_auto_thumbnail_path(
                        &PathBuf::from(self.path.clone()),
                        first_usable_file,
                    );
                    if !thumbnail_path.exists() {
                        item.create_thumbnail(&PathBuf::from(self.path.clone()), first_usable_file)
                            .expect("Failed to create thumbnail");
                        println!(
                            "{} created auto thumbnail ({})",
                            item.key,
                            thumbnail_path.display()
                        );
                    } else {
                        println!(
                            "{} already has auto thumbnail ({})",
                            item.key,
                            thumbnail_path.display()
                        );
                    }
                } else {
                    println!("{} has no usable files", item.key);
                }
            } else {
                println!("{} has explicit preview", item.key);
            }
        }
    }
}
