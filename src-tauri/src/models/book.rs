use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Book {
    pub id: i64,
    pub library_root_id: i64,
    pub file_path: String,
    pub parent_folder_path: String,
    pub file_name: String,
    pub file_size: i64,
    pub modified_time: i64,
    pub content_hash: Option<String>,
    pub discovered_title: Option<String>,
    pub discovered_creator: Option<String>,
    pub discovered_language: Option<String>,
    pub discovered_identifier: Option<String>,
    pub discovered_series: Option<String>,
    pub discovered_series_index: Option<String>,
    pub discovered_cover_path: Option<String>,
    pub extraction_status: String,
    pub extraction_error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A catalog row deliberately contains display-ready, effective values.  The UI
/// never needs to know how overrides and EPUB metadata are combined.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserBook {
    pub id: i64,
    pub library_root_id: i64,
    pub file_name: String,
    pub parent_folder_path: String,
    pub effective_title: String,
    pub effective_creator: String,
    pub effective_series: String,
    pub effective_volume: String,
    pub effective_cover_path: Option<String>,
    pub created_at: i64,
    pub modified_time: i64,
    pub needs_metadata: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookDetails {
    pub book: Book,
    pub effective_title: String,
    pub effective_creator: String,
    pub effective_series: String,
    pub effective_volume: String,
    pub effective_cover_path: Option<String>,
    pub override_values: BookOverride,
    pub tags: Vec<(i64, String)>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BookOverride {
    pub title: Option<String>,
    pub creator: Option<String>,
    pub series_name: Option<String>,
    pub volume_label: Option<String>,
    pub cover_path: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchTagRequest {
    pub book_ids: Vec<i64>,
    pub tag_names: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowseBooksRequest {
    pub library_root_id: Option<i64>,
    pub tag_id: Option<i64>,
    pub collection_id: Option<i64>,
    pub needs_metadata: bool,
    #[serde(default)]
    pub query: String,
    pub sort: String,
    pub offset: i64,
    pub limit: i64,
}

#[derive(Debug, Clone, Default)]
pub struct ExtractedBookMetadata {
    pub title: Option<String>,
    pub creator: Option<String>,
    pub language: Option<String>,
    pub identifier: Option<String>,
    pub series: Option<String>,
    pub series_index: Option<String>,
    pub cover_path: Option<String>,
    pub extraction_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewBook<'a> {
    pub library_root_id: i64,
    pub file_path: &'a str,
    pub parent_folder_path: &'a str,
    pub file_name: &'a str,
    pub file_size: i64,
    pub modified_time: i64,
}
