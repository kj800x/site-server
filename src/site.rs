use std::fmt::Display;
use std::path::PathBuf;

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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[serde(untagged)]
#[serde(rename_all = "camelCase")]
pub enum CrawlTag {
    Detailed { group: String, value: String },
    Simple(String),
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

/// Display settings attached to each item from its source site.
/// Populated at runtime when loading WorkDir, not persisted to JSON.
#[derive(Debug, Clone, Default)]
pub struct SiteSettings {
    /// The slug of the site this item belongs to (for asset paths)
    pub site_slug: String,
    /// Override author display with this value if set
    pub forced_author: Option<String>,
    /// Whether to hide titles when rendering this item
    pub hide_titles: bool,
    /// Path to the work directory for this item's site (for thumbnail lookups)
    pub work_dir_path: Option<PathBuf>,
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

    /// Display settings from the source site. Populated at runtime, not persisted.
    #[serde(skip)]
    #[serde(default)]
    pub site_settings: SiteSettings,
}

impl crate::collections::GetKey for CrawlItem {
    fn get_key(&self) -> &str {
        &self.key
    }
}

impl CrawlItem {
    /// Returns the relative path to the thumbnail for this item, if one exists.
    /// Uses `self.site_settings.work_dir_path` internally to check for auto-generated thumbnails.
    pub fn thumbnail_path(&self) -> Option<String> {
        // first check for explicit previews
        let flat_previews = self.flat_previews();
        let first_usable_preview_file = flat_previews
            .values()
            .find(|file| file.is_downloaded() && file.is_image());
        if let Some(file) = first_usable_preview_file {
            return match file {
                FileCrawlType::Image { filename, .. } => Some(filename.clone()),
                _ => panic!("We just checked that this was an image, but it's not"),
            };
        }

        // then check for auto-generated thumbnails
        // Requires work_dir_path to be set in site_settings
        let work_dir_path = self.site_settings.work_dir_path.as_ref()?;

        let flat_files = self.flat_files();
        let first_usable_file = flat_files
            .values()
            .find(|file| file.is_downloaded() && (file.is_image() || file.is_video()));

        if let Some(file) = first_usable_file {
            let auto_path = self.calculate_auto_thumbnail_path(work_dir_path, file);
            if auto_path.exists() {
                Some(
                    auto_path
                        .strip_prefix(work_dir_path)
                        .unwrap()
                        .to_string_lossy()
                        .to_string(),
                )
            } else {
                None
            }
        } else {
            None
        }
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

    pub fn flat_previews(&self) -> IndexMap<String, FileCrawlType> {
        self.previews
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
