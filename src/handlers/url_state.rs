use urlencoding::encode;

use crate::handlers::{ListingPageConfig, ListingPageMode, ListingPageOrdering};

/// Represents the state of a page URL, allowing centralized URL generation
/// and modification of individual aspects (file_id, view mode, item index, etc.)
#[derive(Clone, Debug)]
pub struct PageUrlState {
    pub site_prefix: String,
    pub rendering_prefix: String, // "r", "blog", "booru"
    pub page_type: PageType,
    pub file_id: Option<String>,
    pub view_mode: ViewMode,
}

#[derive(Clone, Debug)]
pub enum PageType {
    Slideshow {
        mode: ListingPageMode,
        ordering: ListingPageOrdering,
        index: usize,
    },
    ItemPermalink {
        item_key: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum ViewMode {
    Normal,
    Full,
}

impl PageUrlState {
    /// Create a new slideshow page state
    pub fn slideshow(
        site_prefix: String,
        rendering_prefix: String,
        config: &ListingPageConfig,
        index: usize,
        file_id: String,
        view_mode: ViewMode,
    ) -> Self {
        Self {
            site_prefix,
            rendering_prefix,
            page_type: PageType::Slideshow {
                mode: config.mode.clone(),
                ordering: config.ordering.clone(),
                index,
            },
            file_id: Some(file_id),
            view_mode,
        }
    }

    /// Create a new item permalink page state
    pub fn permalink(
        site_prefix: String,
        rendering_prefix: String,
        item_key: String,
        file_id: String,
        view_mode: ViewMode,
    ) -> Self {
        Self {
            site_prefix,
            rendering_prefix,
            page_type: PageType::ItemPermalink { item_key },
            file_id: Some(file_id),
            view_mode,
        }
    }


    /// Change the file_id
    pub fn with_file_id(&self, file_id: String) -> Self {
        Self {
            file_id: Some(file_id),
            ..self.clone()
        }
    }

    /// Toggle view mode
    pub fn toggle_view_mode(&self) -> Self {
        Self {
            view_mode: match self.view_mode {
                ViewMode::Normal => ViewMode::Full,
                ViewMode::Full => ViewMode::Normal,
            },
            ..self.clone()
        }
    }

    /// Set view mode
    pub fn with_view_mode(&self, view_mode: ViewMode) -> Self {
        Self {
            view_mode,
            ..self.clone()
        }
    }

    /// Change item index (for slideshow)
    pub fn with_item_index(&self, index: usize) -> Self {
        match &self.page_type {
            PageType::Slideshow { mode, ordering, .. } => Self {
                page_type: PageType::Slideshow {
                    mode: mode.clone(),
                    ordering: ordering.clone(),
                    index,
                },
                ..self.clone()
            },
            PageType::ItemPermalink { .. } => self.clone(),
        }
    }

    /// Convert to permalink (if currently slideshow)
    pub fn to_permalink(&self, item_key: String) -> Self {
        Self {
            page_type: PageType::ItemPermalink { item_key },
            ..self.clone()
        }
    }

    /// Convert to slideshow (if currently permalink)
    pub fn to_slideshow(&self, config: &ListingPageConfig, index: usize) -> Self {
        Self {
            page_type: PageType::Slideshow {
                mode: config.mode.clone(),
                ordering: config.ordering.clone(),
                index,
            },
            ..self.clone()
        }
    }

    /// Generate the URL string
    pub fn to_url(&self) -> String {
        let base = match &self.page_type {
            PageType::Slideshow { mode, ordering, index } => {
                let slideshow_part = match mode {
                    ListingPageMode::All => match ordering {
                        ListingPageOrdering::NewestFirst => {
                            format!("/{}/latest/slideshow/{}", self.rendering_prefix, index)
                        }
                        ListingPageOrdering::OldestFirst => {
                            format!("/{}/oldest/slideshow/{}", self.rendering_prefix, index)
                        }
                        ListingPageOrdering::Random => {
                            format!("/{}/random/slideshow/{}", self.rendering_prefix, index)
                        }
                    },
                    ListingPageMode::ByTag { tag } => {
                        format!("/{}/tag/{}/slideshow/{}", self.rendering_prefix, encode(tag), index)
                    }
                    ListingPageMode::ByMonth { year, month } => {
                        format!("/{}/archive/{}/{}/slideshow/{}", self.rendering_prefix, year, month, index)
                    }
                    ListingPageMode::Search { query } => {
                        format!("/{}/search/{}/slideshow/{}", self.rendering_prefix, encode(query), index)
                    }
                };

                if let Some(ref file_id) = self.file_id {
                    format!("/{}{}/{}", self.site_prefix, slideshow_part, encode(file_id))
                } else {
                    format!("/{}{}", self.site_prefix, slideshow_part)
                }
            }
            PageType::ItemPermalink { item_key } => {
                if let Some(ref file_id) = self.file_id {
                    format!("/{}/{}/item/{}/{}", self.site_prefix, self.rendering_prefix, encode(item_key), encode(file_id))
                } else {
                    format!("/{}/{}/item/{}", self.site_prefix, self.rendering_prefix, encode(item_key))
                }
            }
        };

        match self.view_mode {
            ViewMode::Normal => base,
            ViewMode::Full => format!("{}?view=full", base),
        }
    }

    /// Generate URL without site prefix (for route parameter)
    pub fn to_route(&self) -> String {
        let base = match &self.page_type {
            PageType::Slideshow { mode, ordering, index } => {
                let slideshow_part = match mode {
                    ListingPageMode::All => match ordering {
                        ListingPageOrdering::NewestFirst => {
                            format!("/{}/latest/slideshow/{}", self.rendering_prefix, index)
                        }
                        ListingPageOrdering::OldestFirst => {
                            format!("/{}/oldest/slideshow/{}", self.rendering_prefix, index)
                        }
                        ListingPageOrdering::Random => {
                            format!("/{}/random/slideshow/{}", self.rendering_prefix, index)
                        }
                    },
                    ListingPageMode::ByTag { tag } => {
                        format!("/{}/tag/{}/slideshow/{}", self.rendering_prefix, encode(tag), index)
                    }
                    ListingPageMode::ByMonth { year, month } => {
                        format!("/{}/archive/{}/{}/slideshow/{}", self.rendering_prefix, year, month, index)
                    }
                    ListingPageMode::Search { query } => {
                        format!("/{}/search/{}/slideshow/{}", self.rendering_prefix, encode(query), index)
                    }
                };

                if let Some(ref file_id) = self.file_id {
                    format!("{}/{}", slideshow_part, encode(file_id))
                } else {
                    slideshow_part
                }
            }
            PageType::ItemPermalink { item_key } => {
                if let Some(ref file_id) = self.file_id {
                    format!("/{}/item/{}/{}", self.rendering_prefix, encode(item_key), encode(file_id))
                } else {
                    format!("/{}/item/{}", self.rendering_prefix, encode(item_key))
                }
            }
        };

        match self.view_mode {
            ViewMode::Normal => base,
            ViewMode::Full => format!("{}?view=full", base),
        }
    }

    /// Check if this is a slideshow page
    pub fn is_slideshow(&self) -> bool {
        matches!(self.page_type, PageType::Slideshow { .. })
    }

    /// Get the item index if slideshow
    pub fn item_index(&self) -> Option<usize> {
        match &self.page_type {
            PageType::Slideshow { index, .. } => Some(*index),
            PageType::ItemPermalink { .. } => None,
        }
    }
}
