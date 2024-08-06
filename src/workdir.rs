use std::{
    fs::File,
    ops::{Deref, DerefMut},
    path::Path,
};

use crate::errors::*;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{
    serde::{deserialize_map_values, serialize_map_values},
    site::CrawlItem,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub site: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SiteItems {
    #[serde(serialize_with = "serialize_map_values")]
    #[serde(deserialize_with = "deserialize_map_values")]
    pub items: IndexMap<String, CrawlItem>,
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
        let crawled = {
            if crawled_path.exists() {
                let crawled_file =
                    File::open(crawled_path).chain_err(|| "Unable to open crawled.json")?;
                serde_json::from_reader(crawled_file)
                    .chain_err(|| "crawled.json was not well-formatted")?
            } else {
                IndexMap::new().into()
            }
        };

        Ok(WorkDir {
            path: path.into(),
            crawled,
            config,
        })
    }
}
