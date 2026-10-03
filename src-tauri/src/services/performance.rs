use std::{
    cell::RefCell,
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
pub enum Stage {
    FilesystemDiscovery,
    CatalogUpsert,
    EpubExtraction,
    CoverExtraction,
    ReadingDerivation,
    SearchDocumentUpdate,
    MissingFileReconciliation,
}

#[derive(Clone, Default)]
pub struct Timings {
    pub filesystem_discovery: Duration,
    pub catalog_upserts: Duration,
    pub epub_extraction_inclusive: Duration,
    pub cover_extraction: Duration,
    pub reading_derivation: Duration,
    pub search_document_updates: Duration,
    pub missing_file_reconciliation: Duration,
}

impl Timings {
    pub fn metadata_parsing(&self) -> Duration {
        self.epub_extraction_inclusive
            .saturating_sub(self.cover_extraction)
    }

    pub fn fields(&self) -> Vec<(&'static str, String)> {
        vec![
            ("filesystem_discovery_ms", millis(self.filesystem_discovery)),
            ("catalog_upserts_ms", millis(self.catalog_upserts)),
            ("epub_metadata_parsing_ms", millis(self.metadata_parsing())),
            (
                "cover_extraction_encoding_ms",
                millis(self.cover_extraction),
            ),
            (
                "japanese_reading_derivation_ms",
                millis(self.reading_derivation),
            ),
            (
                "search_document_updates_ms",
                millis(self.search_document_updates),
            ),
            (
                "missing_file_reconciliation_ms",
                millis(self.missing_file_reconciliation),
            ),
        ]
    }
}

thread_local! {
    static ACTIVE: RefCell<Option<Timings>> = const { RefCell::new(None) };
}

/// Starts a timing scope on the current thread. Scans and startup are currently
/// synchronous, so this adds no synchronization or cross-thread contention.
pub fn start() {
    ACTIVE.with(|active| *active.borrow_mut() = Some(Timings::default()));
}

pub fn finish() -> Timings {
    ACTIVE.with(|active| active.borrow_mut().take().unwrap_or_default())
}

pub fn measure<T>(stage: Stage, operation: impl FnOnce() -> T) -> T {
    let enabled = ACTIVE.with(|active| active.borrow().is_some());
    if !enabled {
        return operation();
    }
    let started = Instant::now();
    let result = operation();
    let elapsed = started.elapsed();
    ACTIVE.with(|active| {
        if let Some(timings) = active.borrow_mut().as_mut() {
            match stage {
                Stage::FilesystemDiscovery => timings.filesystem_discovery += elapsed,
                Stage::CatalogUpsert => timings.catalog_upserts += elapsed,
                Stage::EpubExtraction => timings.epub_extraction_inclusive += elapsed,
                Stage::CoverExtraction => timings.cover_extraction += elapsed,
                Stage::ReadingDerivation => timings.reading_derivation += elapsed,
                Stage::SearchDocumentUpdate => timings.search_document_updates += elapsed,
                Stage::MissingFileReconciliation => timings.missing_file_reconciliation += elapsed,
            }
        }
    });
    result
}

pub fn millis(duration: Duration) -> String {
    format!("{:.3}", duration.as_secs_f64() * 1_000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_measurement_does_not_create_a_session() {
        assert_eq!(measure(Stage::CatalogUpsert, || 42), 42);
        assert_eq!(finish().catalog_upserts, Duration::ZERO);
    }
}
