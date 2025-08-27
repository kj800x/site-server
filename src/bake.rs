use std::{cmp::min, path::PathBuf, process::Command};

use crate::{
    errors::Error,
    site::{CrawlItem, FileCrawlType},
    workdir::WorkDir,
};

fn is_audio_only(filename: &str) -> bool {
    let output = Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-show_entries")
        .arg("stream=codec_type")
        .arg("-of")
        .arg("default=nw=1")
        .arg(filename)
        .output()
        .expect("Failed to get video length");

    let stdout = String::from_utf8_lossy(&output.stdout);

    let contains_video = stdout.contains("codec_type=video");
    let contains_audio = stdout.contains("codec_type=audio");

    contains_audio && !contains_video
}

fn video_length(filename: &str) -> f64 {
    let output = Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-show_entries")
        .arg("format=duration")
        .arg("-v")
        .arg("quiet")
        .arg("-of")
        .arg("csv=p=0")
        .arg(filename)
        .output()
        .expect("Failed to get video length");

    let duration =
        String::from_utf8(output.stdout).expect("Failed to parse ffprobe output as UTF-8");
    let duration = duration
        .split_whitespace()
        .next()
        .expect("Failed to get duration from ffprobe output");
    duration
        .parse::<f64>()
        .expect("Failed to parse duration as float")
}

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
            FileCrawlType::Video { .. } => "mp4",
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
        let thumbnail_dir = thumbnail_path
            .parent()
            .expect("Failed to resolve auto thumbnail directory");

        if !thumbnail_dir.exists() {
            std::fs::create_dir_all(thumbnail_dir)
                .expect("Failed to create auto thumbnail directory");

            println!(
                "Created auto thumbnail directory ({})",
                thumbnail_dir.display()
            );
        }

        match thumbnail_of {
            FileCrawlType::Video { filename, .. } => {
                let video_path = work_dir_path.join(filename);
                if !video_path.exists() {
                    println!("{} does not exist, skipping thumbnail", filename);
                    return Ok(());
                }

                let video_path_str = video_path.to_str().unwrap();
                let thumbnail_path_str = thumbnail_path.to_str().unwrap();

                if is_audio_only(video_path_str) {
                    println!("{} is audio only, skipping thumbnail", filename);
                    return Ok(());
                }

                let length = video_length(video_path_str);
                let offset = (length / 3.0).round() as u64;
                let duration = min(offset, 3);

                let output = Command::new("ffmpeg")
                    .arg("-ss")
                    .arg(offset.to_string())
                    .arg("-t")
                    .arg(duration.to_string())
                    .arg("-i")
                    .arg(video_path_str)
                    .arg("-vf")
                    .arg("scale=320:-2,fps=15")
                    .arg("-c:v")
                    .arg("libx264")
                    .arg("-preset")
                    .arg("slow")
                    .arg("-crf")
                    .arg("28")
                    .arg("-an")
                    .arg("-movflags")
                    .arg("+faststart")
                    .arg(thumbnail_path_str)
                    .output()
                    .expect("Failed to create video thumbnail");

                if !output.status.success() {
                    println!(
                        "Failed to create video thumbnail: {}\n{}",
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr)
                    );
                    // panic!("Failed to create video thumbnail");
                }
            }

            FileCrawlType::Image { filename, .. } => {
                let image_path = work_dir_path.join(filename);
                if !image_path.exists() {
                    println!("{} does not exist, skipping thumbnail", filename);
                    return Ok(());
                }

                let image_path_str = image_path.to_str().unwrap();
                let thumbnail_path_str = thumbnail_path.to_str().unwrap();

                let output = Command::new("ffmpeg")
                    .arg("-i")
                    .arg(image_path_str)
                    .arg("-vf")
                    .arg("scale=320:-1")
                    .arg(thumbnail_path_str)
                    .output()
                    .expect("Failed to create image thumbnail");

                if !output.status.success() {
                    println!(
                        "Failed to create image thumbnail: {}\n{}",
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr)
                    );
                    // panic!("Failed to create image thumbnail");
                }
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
