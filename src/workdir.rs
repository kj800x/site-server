use std::{
    fs::File,
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
};

use crate::{errors::*, site::FileCrawlType};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{
    reprocessors::Reprocessor,
    serde::{deserialize_map_values, serialize_map_values},
    site::{CrawlItem, SiteSettings},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub site: String,
    pub slug: String,
    pub label: String,
    pub forced_author: Option<String>,
    #[serde(default)]
    pub hide_titles: bool,
    #[serde(default)]
    pub reprocessors: Vec<Reprocessor>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SiteItems {
    #[serde(serialize_with = "serialize_map_values")]
    #[serde(deserialize_with = "deserialize_map_values")]
    pub items: IndexMap<String, CrawlItem>,
}

impl SiteItems {
    /// Sort the items by source_published date in descending order, newest first.
    pub fn sort(&mut self) {
        self.items
            .sort_by(|_k1, v1, _k2, v2| v2.source_published.cmp(&v1.source_published));
    }

    pub fn remove_items_without_files(&mut self) {
        self.items.retain(|_k, v| {
            v.files.values().any(|x| match x {
                FileCrawlType::Image { downloaded, .. } => *downloaded,
                FileCrawlType::Video { downloaded, .. } => *downloaded,
                FileCrawlType::Intermediate { downloaded, .. } => *downloaded,
                FileCrawlType::Text { .. } => true,
            })
        });
    }

    pub fn remove_duplicate_tags(&mut self) {
        self.items.iter_mut().for_each(|(_, v)| {
            v.tags.sort();
            v.tags.dedup();
        });
    }
}

impl From<IndexMap<String, CrawlItem>> for SiteItems {
    fn from(value: IndexMap<String, CrawlItem>) -> Self {
        SiteItems { items: value }
    }
}

impl Deref for SiteItems {
    type Target = IndexMap<String, CrawlItem>;

    fn deref(&self) -> &Self::Target {
        &self.items
    }
}

impl DerefMut for SiteItems {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.items
    }
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct WorkDir {
    pub path: Box<Path>,
    pub config: Config,
    pub crawled: SiteItems,
    pub last_seen_modified: u64,
    pub loaded_at: u128,
}

#[allow(dead_code)]
impl WorkDir {
    pub fn new<P: Into<PathBuf>>(p: P) -> Result<Self> {
        let path = p.into();
        let config_path = path.join("config.json");
        let config_file = File::open(config_path).context("Unable to open config.json")?;
        let config: Config =
            serde_json::from_reader(config_file).context("config.json was not well-formatted")?;

        let crawled_path = path.join("crawled.json");
        let mut crawled: SiteItems = {
            if crawled_path.exists() {
                let crawled_file =
                    File::open(crawled_path).context("Unable to open crawled.json")?;
                serde_json::from_reader(crawled_file)
                    .context("crawled.json was not well-formatted")?
            } else {
                IndexMap::new().into()
            }
        };

        let crawled_path = path.join("crawled.json");
        let last_seen_modified = if crawled_path.exists() {
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

        crawled.sort();
        crawled.remove_duplicate_tags();
        if std::env::var("ALLOW_NO_FILES").is_err() {
            crawled.remove_items_without_files();
        }

        // Apply reprocessors
        for reprocessor in &config.reprocessors {
            reprocessor.apply(&mut crawled.items);
        }

        // Attach site settings to each item
        for item in crawled.items.values_mut() {
            item.site_settings = SiteSettings {
                site_slug: config.slug.clone(),
                forced_author: config.forced_author.clone(),
                hide_titles: config.hide_titles,
                work_dir_path: Some(path.clone()),
            };
        }

        let loaded_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();

        Ok(WorkDir {
            path: path.into(),
            crawled,
            config,
            last_seen_modified,
            loaded_at,
        })
    }
}
