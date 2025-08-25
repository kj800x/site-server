use std::fmt::Display;

use crate::collections::*;
use crate::serde::*;
use indexmap::IndexMap;
use maud::html;
use maud::Markup;
use maud::PreEscaped;
use maud::Render;
pub use serde::{Deserialize, Serialize};
use serde_json::Value;
pub use std::fmt::Debug;
pub use std::fmt::Write;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "camelCase")]
pub enum FileCrawlType {
    #[serde(rename = "ImageFile")]
    Image {
        key: String,
        filename: String,
        downloaded: bool,
        url: String,
    },
    #[serde(rename = "VideoFile")]
    Video {
        key: String,
        filename: String,
        downloaded: bool,
        url: String,
    },
    #[serde(rename = "IntermediateFile")]
    Intermediate {
        key: String,
        filename: String,
        downloaded: bool,
        #[serde(default)] // Will default to false
        postprocessing_errors: bool,
        url: String,
        #[serde(serialize_with = "serialize_map_values")]
        #[serde(deserialize_with = "deserialize_map_values")]
        nested: IndexMap<String, FileCrawlType>,
    },
    #[serde(rename = "InlineTextFile")]
    Text { key: String, content: String },
}

impl FileCrawlType {
    pub fn is_downloaded(&self) -> bool {
        match self {
            FileCrawlType::Image { downloaded, .. } | FileCrawlType::Video { downloaded, .. } => {
                *downloaded
            }
            FileCrawlType::Intermediate { downloaded, .. } => *downloaded,
            FileCrawlType::Text { .. } => true,
        }
    }
}

impl GetKey for FileCrawlType {
    fn get_key(&self) -> &str {
        match self {
            FileCrawlType::Image { key, .. }
            | FileCrawlType::Video { key, .. }
            | FileCrawlType::Intermediate { key, .. }
            | FileCrawlType::Text { key, .. } => &key,
        }
    }
}

impl Display for FileCrawlType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileCrawlType::Image { filename, .. } | FileCrawlType::Video { filename, .. } => {
                write!(f, "{}", filename)
            }
            FileCrawlType::Intermediate { filename, .. } => {
                write!(f, "Intermediate({})", filename)
            }
            FileCrawlType::Text { key, .. } => write!(f, "Text({})", key),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(untagged)]
#[serde(rename_all = "camelCase")]
pub enum CrawlTag {
    Simple(String),
    Detailed { value: String, group: String },
}

impl CrawlTag {
    pub fn to_string(&self) -> String {
        match self {
            CrawlTag::Simple(value) => value.clone(),
            CrawlTag::Detailed { value, .. } => value.clone(),
        }
    }
}

impl From<String> for CrawlTag {
    fn from(value: String) -> Self {
        CrawlTag::Simple(value)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "format")]
pub enum FormattedText {
    Markdown { value: String }, // Treat as markdown directly
    Plaintext { value: String },
    Html { value: String }, // Implies that the import process should run a to-markdown on this
}

// FIXME: This isn't actually correct
impl Display for FormattedText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FormattedText::Markdown { value } | FormattedText::Plaintext { value } => {
                write!(f, "{}", value)
            }
            FormattedText::Html { value } => write!(f, "Html({})", value),
        }
    }
}

impl Render for FormattedText {
    fn render(&self) -> Markup {
        match self {
            FormattedText::Plaintext { value } => {
                html!( pre.pre-wrap { (value) } )
            }
            FormattedText::Markdown { value } => {
                // todo!();
                html!( pre.pre-wrap { (value) } )
            }
            FormattedText::Html { value } => PreEscaped(value.to_owned()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CrawlItem {
    pub title: String,
    pub key: String,
    pub url: String,
    pub description: FormattedText,
    pub meta: Value,
    #[serde(default = "default_i64_zero", deserialize_with = "null_to_zero")]
    pub source_published: i64,
    pub first_seen: u64,
    pub last_seen: u64,
    // FIXME: This will always be set to true because of a bug
    pub seen_in_last_refresh: bool,
    pub tags: Vec<CrawlTag>,
    #[serde(serialize_with = "serialize_map_values")]
    #[serde(deserialize_with = "deserialize_map_values")]
    pub files: IndexMap<String, FileCrawlType>,
    #[serde(default)]
    #[serde(serialize_with = "serialize_map_values")]
    #[serde(deserialize_with = "deserialize_map_values")]
    /** A preview is a file that can be used as a thumbnail for the CrawlItem in a listing page
     * It's not typically shown on the details page, but is potentially a low resolution image from the item,
     * or potentially a promotionalized image for the shoot. A CrawlItem having one is benefical because it
     * means that whatever is serving the site doesn't need to dynamically generate thumbnails on the fly.
     */
    pub previews: IndexMap<String, FileCrawlType>,
}

impl crate::collections::GetKey for CrawlItem {
    fn get_key(&self) -> &str {
        &self.key
    }
}

fn first_downloaded_image<'a>(mut arr: impl Iterator<Item = &'a FileCrawlType>) -> Option<String> {
    arr.find_map(|file| match file {
        FileCrawlType::Image {
            filename,
            downloaded,
            ..
        } => {
            if *downloaded {
                Some(filename.clone())
            } else {
                None
            }
        }
        FileCrawlType::Video { .. } => None,
        FileCrawlType::Intermediate {
            downloaded, nested, ..
        } => {
            if *downloaded {
                first_downloaded_image(nested.values())
            } else {
                None
            }
        }
        FileCrawlType::Text { .. } => None,
    })
}

impl CrawlItem {
    pub fn thumbnail_path(&self) -> Option<String> {
        first_downloaded_image(self.previews.values().chain(self.files.values()))
    }

    /// Take the files and replace any intermediate files with their nested files
    pub fn flat_files(&self) -> IndexMap<String, FileCrawlType> {
        self.files
            .clone()
            .into_iter()
            .flat_map(|(key, file)| match file {
                FileCrawlType::Intermediate {
                    ref nested,
                    downloaded,
                    ..
                } => {
                    if downloaded {
                        nested.clone()
                    } else {
                        IndexMap::from([(key, file)])
                    }
                }
                _ => IndexMap::from([(key, file)]),
            })
            .collect()
    }
}
