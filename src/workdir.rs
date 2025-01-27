use std::{
    fs::File,
    ops::{Deref, DerefMut},
    path::Path,
};

use crate::{errors::*, site::FileCrawlType};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{
    serde::{deserialize_map_values, serialize_map_values},
    site::CrawlItem,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub site: String,
    pub slug: String,
    pub label: String,
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
}

#[allow(dead_code)]
impl WorkDir {
    pub fn new(path_str: String) -> Result<Self> {
        let path = Path::new(&path_str);
        let config_path = path.join("config.json");
        let config_file = File::open(config_path).chain_err(|| "Unable to open config.json")?;
        let config: Config = serde_json::from_reader(config_file)
            .chain_err(|| "config.json was not well-formatted")?;

        let crawled_path = path.join("crawled.json");
        let mut crawled: SiteItems = {
            if crawled_path.exists() {
                let crawled_file =
                    File::open(crawled_path).chain_err(|| "Unable to open crawled.json")?;
                serde_json::from_reader(crawled_file)
                    .chain_err(|| "crawled.json was not well-formatted")?
            } else {
                IndexMap::new().into()
            }
        };

        crawled.sort();
        if std::env::var("ALLOW_NO_FILES").is_err() {
            crawled.remove_items_without_files();
        }

        Ok(WorkDir {
            path: path.into(),
            crawled,
            config,
        })
    }
}
