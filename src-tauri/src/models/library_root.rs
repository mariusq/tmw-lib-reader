use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryRoot {
    pub id: i64,
    pub path: String,
    pub display_name: String,
    pub added_at: i64,
    pub last_scanned_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryRootSummary {
    #[serde(flatten)]
    pub root: LibraryRoot,
    pub book_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewLibraryRoot<'a> {
    pub path: &'a str,
    pub display_name: &'a str,
}
