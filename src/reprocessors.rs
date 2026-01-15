use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::site::{CrawlItem, CrawlTag, FileCrawlType, FormattedText};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Reprocessor {
    #[serde(rename = "sort-videos-first")]
    SortVideosFirst,
    #[serde(rename = "add-tags")]
    AddTags { candidates: Vec<String> },
    #[serde(rename = "map-tags")]
    MapTags { mappings: HashMap<String, String> },
    #[serde(rename = "remove-tags")]
    RemoveTags { tags: Vec<String> },
    #[serde(rename = "normalize-tags")]
    NormalizeTags,
    #[serde(rename = "filter-out-items-with-tag")]
    FilterOutItemsWithTag { tags: Vec<String> },
}

impl Reprocessor {
    pub fn apply(&self, items: &mut IndexMap<String, CrawlItem>) {
        match self {
            Reprocessor::SortVideosFirst => {
                for item in items.values_mut() {
                    let mut videos = Vec::new();
                    let mut non_videos = Vec::new();

                    for (key, file) in item.files.iter() {
                        if matches!(file, FileCrawlType::Video { .. }) {
                            videos.push((key.clone(), file.clone()));
                        } else {
                            non_videos.push((key.clone(), file.clone()));
                        }
                    }

                    let mut new_files = IndexMap::new();
                    for (key, file) in videos {
                        new_files.insert(key, file);
                    }
                    for (key, file) in non_videos {
                        new_files.insert(key, file);
                    }
                    item.files = new_files;
                }
            }
            Reprocessor::AddTags { candidates } => {
                for item in items.values_mut() {
                    for candidate in candidates {
                        // Check if tag already exists (case-insensitive)
                        let tag_exists = item
                            .tags
                            .iter()
                            .any(|tag| tag.to_string().to_lowercase() == candidate.to_lowercase());

                        if tag_exists {
                            continue;
                        }

                        // Check if candidate text appears in title (case-insensitive)
                        let found_in_title = item
                            .title
                            .to_lowercase()
                            .contains(&candidate.to_lowercase());

                        // Check if candidate text appears in description (case-insensitive)
                        let description_text = extract_text_from_formatted_text(&item.description);
                        let found_in_description = description_text
                            .to_lowercase()
                            .contains(&candidate.to_lowercase());

                        // Check if candidate text appears in meta (recursive, case-insensitive)
                        let found_in_meta = search_json_value_recursive(&item.meta, candidate);

                        if found_in_title || found_in_description || found_in_meta {
                            item.tags.push(CrawlTag::Simple(candidate.clone()));
                        }
                    }
                }
            }
            Reprocessor::MapTags { mappings } => {
                for item in items.values_mut() {
                    for tag in item.tags.iter_mut() {
                        let tag_value = tag.to_string();
                        // Check if this tag should be mapped (case-insensitive)
                        if let Some(mapped_value) = mappings
                            .iter()
                            .find(|(k, _)| k.to_lowercase() == tag_value.to_lowercase())
                        {
                            *tag = CrawlTag::Simple(mapped_value.1.clone());
                        }
                    }
                }
            }
            Reprocessor::RemoveTags { tags } => {
                for item in items.values_mut() {
                    item.tags.retain(|tag| {
                        let tag_value = tag.to_string();
                        !tags
                            .iter()
                            .any(|t| t.to_lowercase() == tag_value.to_lowercase())
                    });
                }
            }
            Reprocessor::NormalizeTags => {
                for item in items.values_mut() {
                    for tag in item.tags.iter_mut() {
                        match tag {
                            CrawlTag::Simple(value) => {
                                *tag = CrawlTag::Simple(value.trim().to_lowercase());
                            }
                            CrawlTag::Detailed { value, .. } => {
                                *value = value.trim().to_lowercase();
                            }
                        }
                    }
                }
            }
            Reprocessor::FilterOutItemsWithTag { tags } => {
                items.retain(|_, item| {
                    !item.tags.iter().any(|t| {
                        let tag_value = t.to_string().to_lowercase();
                        tags.iter()
                            .any(|filter_tag| filter_tag.to_lowercase() == tag_value)
                    })
                });
            }
        }
    }
}

pub fn extract_text_from_formatted_text(ft: &FormattedText) -> String {
    match ft {
        FormattedText::Markdown { value } => value.clone(),
        FormattedText::Plaintext { value } => value.clone(),
        FormattedText::Html { value } => value.clone(),
    }
}

pub fn search_json_value_recursive(value: &Value, search_text: &str) -> bool {
    let search_lower = search_text.to_lowercase();

    match value {
        Value::String(s) => s.to_lowercase().contains(&search_lower),
        Value::Object(map) => {
            for (key, val) in map.iter() {
                if key.to_lowercase().contains(&search_lower) {
                    return true;
                }
                if search_json_value_recursive(val, search_text) {
                    return true;
                }
            }
            false
        }
        Value::Array(arr) => {
            for val in arr.iter() {
                if search_json_value_recursive(val, search_text) {
                    return true;
                }
            }
            false
        }
        Value::Number(n) => n.to_string().to_lowercase().contains(&search_lower),
        Value::Bool(_) | Value::Null => false,
    }
}
