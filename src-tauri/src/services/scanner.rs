use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant, UNIX_EPOCH},
};

use crossbeam_channel::{bounded, Receiver, Sender};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::{
    db::Database,
    models::book::{ExtractedBookMetadata, NewBook},
    services::{metadata, performance},
};

trait ScanEmitter: Sync {
    fn scan_emit<S: Serialize + Clone>(&self, event: &str, payload: S);
}
impl ScanEmitter for AppHandle {
    fn scan_emit<S: Serialize + Clone>(&self, event: &str, payload: S) {
        let _ = Emitter::emit(self, event, payload);
    }
}
#[cfg(test)]
struct SilentEvents;
#[cfg(test)]
impl ScanEmitter for SilentEvents {
    fn scan_emit<S: Serialize + Clone>(&self, _event: &str, _payload: S) {}
}

const SCAN_BATCH_SIZE: usize = 128;
const METADATA_BATCH_SIZE: usize = 32;
const DEFAULT_EXTRACTION_WORKERS: usize = 2;
const MAX_EXTRACTION_WORKERS: usize = 8;

#[derive(Debug)]
struct ScanCandidate {
    path: PathBuf,
    file_path: String,
    parent_folder_path: String,
    file_name: String,
    file_size: i64,
    modified_time: i64,
}

struct ExtractionJob {
    book_id: i64,
    candidate: ScanCandidate,
}
struct ExtractionResult {
    book_id: i64,
    file_path: String,
    metadata: ExtractedBookMetadata,
}
struct ActiveScan {
    root_id: i64,
    cancellation: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct ScanController {
    scans: Mutex<HashMap<String, ActiveScan>>,
}

impl ScanController {
    pub fn start(&self, scan_id: &str, root_id: i64) -> Result<Arc<AtomicBool>, String> {
        let mut scans = self.scans.lock().expect("scan mutex poisoned");
        if scans.contains_key(scan_id) {
            return Err("A scan with this identifier is already running.".into());
        }
        if scans.values().any(|scan| scan.root_id == root_id) {
            return Err("This library root is already being scanned.".into());
        }
        let cancellation = Arc::new(AtomicBool::new(false));
        scans.insert(
            scan_id.to_owned(),
            ActiveScan {
                root_id,
                cancellation: cancellation.clone(),
            },
        );
        Ok(cancellation)
    }

    pub fn cancel(&self, scan_id: &str) -> bool {
        if let Some(scan) = self.scans.lock().expect("scan mutex poisoned").get(scan_id) {
            scan.cancellation.store(true, Ordering::Release);
            true
        } else {
            false
        }
    }

    fn finish(&self, scan_id: &str) {
        self.scans
            .lock()
            .expect("scan mutex poisoned")
            .remove(scan_id);
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanStarted {
    scan_id: String,
    root_id: i64,
    root_path: String,
    extraction_workers: usize,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanProgress {
    scan_id: String,
    root_id: i64,
    stage: &'static str,
    discovered_count: usize,
    changed_count: usize,
    extracted_count: usize,
    indexed_count: usize,
    current_path: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanCompleted {
    scan_id: String,
    root_id: i64,
    discovered_count: usize,
    changed_count: usize,
    extraction_failures: usize,
    cancelled: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanFailed {
    scan_id: String,
    root_id: i64,
    message: String,
}

#[derive(Default)]
struct PipelineProgress {
    discovered: AtomicUsize,
    changed: AtomicUsize,
    extracted: AtomicUsize,
    indexed: AtomicUsize,
    failures: AtomicUsize,
    discovery_nanos: AtomicU64,
    catalog_nanos: AtomicU64,
    extraction_nanos: AtomicU64,
    indexing_nanos: AtomicU64,
    metadata_parse_nanos: AtomicU64,
    cover_nanos: AtomicU64,
    reading_nanos: AtomicU64,
    search_nanos: AtomicU64,
}

/// Register and launch the importer. The command returns immediately after this
/// function; source EPUBs are opened read-only by bounded background workers.
pub fn start_scan(
    app: AppHandle,
    root_id: i64,
    root_path: String,
    cover_cache_directory: PathBuf,
    scan_id: String,
) -> Result<(), String> {
    let workers = app
        .state::<Database>()
        .setting("import_worker_count")
        .map_err(|error| error.to_string())?
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(DEFAULT_EXTRACTION_WORKERS)
        .clamp(1, MAX_EXTRACTION_WORKERS);
    let cancellation = app.state::<ScanController>().start(&scan_id, root_id)?;
    let _ = app.scan_emit(
        "scan-started",
        ScanStarted {
            scan_id: scan_id.clone(),
            root_id,
            root_path: root_path.clone(),
            extraction_workers: workers,
        },
    );
    tauri::async_runtime::spawn_blocking(move || {
        run_scan(
            app,
            root_id,
            root_path,
            cover_cache_directory,
            scan_id,
            workers,
            cancellation,
        );
    });
    Ok(())
}

fn run_scan(
    app: AppHandle,
    root_id: i64,
    root_path: String,
    cover_cache_directory: PathBuf,
    scan_id: String,
    workers: usize,
    cancellation: Arc<AtomicBool>,
) {
    let started = Instant::now();
    let progress = Arc::new(PipelineProgress::default());
    let database = app.state::<Database>();
    let result = database
        .begin_scan(root_id)
        .map_err(|error| error.to_string())
        .and_then(|_| {
            run_pipeline(
                &app,
                &database,
                root_id,
                Path::new(&root_path),
                &cover_cache_directory,
                &scan_id,
                workers,
                &cancellation,
                &progress,
            )
        });
    let was_cancelled = cancellation.load(Ordering::Acquire);
    let result = result.and_then(|()| {
        if was_cancelled {
            database
                .abandon_scan(root_id)
                .map(|_| (0, Duration::ZERO))
                .map_err(|error| error.to_string())
        } else {
            let reconciliation_started = Instant::now();
            let result = database
                .reconcile_completed_scan(root_id)
                .and_then(|missing| database.mark_root_scanned(root_id).map(|_| missing));
            result
                .map(|missing| (missing, reconciliation_started.elapsed()))
                .map_err(|error| error.to_string())
        }
    });
    app.state::<ScanController>().finish(&scan_id);

    match result {
        Ok((missing_count, reconciliation_time)) => {
            let discovered = progress.discovered.load(Ordering::Relaxed);
            let changed = progress.changed.load(Ordering::Relaxed);
            let failures = progress.failures.load(Ordering::Relaxed);
            let total = started.elapsed();
            crate::services::logging::event(
                "info",
                "scan_performance",
                &[
                    ("total_ms", performance::millis(total)),
                    ("root_id", root_id.to_string()),
                    ("books_discovered", discovered.to_string()),
                    ("books_changed", changed.to_string()),
                    (
                        "books_skipped",
                        discovered.saturating_sub(changed).to_string(),
                    ),
                    ("extraction_failures", failures.to_string()),
                    ("missing_count", missing_count.to_string()),
                    ("cancelled", was_cancelled.to_string()),
                    ("extraction_workers", workers.to_string()),
                    (
                        "pipeline_discovery_wall_ms",
                        nanos_millis(progress.discovery_nanos.load(Ordering::Relaxed)),
                    ),
                    (
                        "filesystem_discovery_ms",
                        nanos_millis(progress.discovery_nanos.load(Ordering::Relaxed)),
                    ),
                    (
                        "pipeline_catalog_write_ms",
                        nanos_millis(progress.catalog_nanos.load(Ordering::Relaxed)),
                    ),
                    (
                        "catalog_upserts_ms",
                        nanos_millis(progress.catalog_nanos.load(Ordering::Relaxed)),
                    ),
                    (
                        "pipeline_extraction_worker_ms",
                        nanos_millis(progress.extraction_nanos.load(Ordering::Relaxed)),
                    ),
                    (
                        "pipeline_index_write_ms",
                        nanos_millis(progress.indexing_nanos.load(Ordering::Relaxed)),
                    ),
                    (
                        "missing_file_reconciliation_ms",
                        performance::millis(reconciliation_time),
                    ),
                    (
                        "epub_metadata_parsing_ms",
                        nanos_millis(progress.metadata_parse_nanos.load(Ordering::Relaxed)),
                    ),
                    (
                        "cover_extraction_encoding_ms",
                        nanos_millis(progress.cover_nanos.load(Ordering::Relaxed)),
                    ),
                    (
                        "japanese_reading_derivation_ms",
                        nanos_millis(progress.reading_nanos.load(Ordering::Relaxed)),
                    ),
                    (
                        "search_document_updates_ms",
                        nanos_millis(progress.search_nanos.load(Ordering::Relaxed)),
                    ),
                    (
                        "throughput_books_per_second",
                        format!(
                            "{:.2}",
                            discovered as f64 / total.as_secs_f64().max(0.000_001)
                        ),
                    ),
                ],
            );
            let _ = app.scan_emit(
                "scan-completed",
                ScanCompleted {
                    scan_id,
                    root_id,
                    discovered_count: discovered,
                    changed_count: changed,
                    extraction_failures: failures,
                    cancelled: was_cancelled,
                },
            );
        }
        Err(message) => {
            let _ = database.abandon_scan(root_id);
            crate::services::logging::event(
                "error",
                "scan_failed",
                &[("root_id", root_id.to_string()), ("error", message.clone())],
            );
            let _ = app.scan_emit(
                "scan-failed",
                ScanFailed {
                    scan_id,
                    root_id,
                    message,
                },
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_pipeline(
    app: &impl ScanEmitter,
    database: &Database,
    root_id: i64,
    root: &Path,
    cover_cache_directory: &Path,
    scan_id: &str,
    workers: usize,
    cancelled: &AtomicBool,
    progress: &Arc<PipelineProgress>,
) -> Result<(), String> {
    if !root.is_dir() {
        return Err(format!(
            "Library root is not an accessible folder: {}",
            root.display()
        ));
    }
    let (candidate_sender, candidate_receiver) = bounded::<Vec<ScanCandidate>>(4);
    let (job_sender, job_receiver) = bounded::<ExtractionJob>(workers * 2);
    let (result_sender, result_receiver) = bounded::<ExtractionResult>(workers * 2);

    std::thread::scope(|scope| -> Result<(), String> {
        let discovery = scope.spawn(|| {
            discover_files(
                app,
                root_id,
                root,
                scan_id,
                cancelled,
                progress,
                candidate_sender,
            )
        });
        let detection = scope.spawn(|| {
            detect_changes(
                database,
                root_id,
                cancelled,
                progress,
                candidate_receiver,
                job_sender,
            )
        });
        let mut extraction_threads = Vec::with_capacity(workers);
        for _ in 0..workers {
            let receiver = job_receiver.clone();
            let sender = result_sender.clone();
            extraction_threads.push(scope.spawn(move || {
                extract_metadata(
                    app,
                    root_id,
                    scan_id,
                    cover_cache_directory,
                    progress,
                    receiver,
                    sender,
                )
            }));
        }
        drop(job_receiver);
        drop(result_sender);
        let write_result =
            write_metadata_batches(app, database, root_id, scan_id, progress, result_receiver);
        let discovery_result = discovery.join().map_err(panic_message)?;
        let detection_result = detection.join().map_err(panic_message)?;
        for worker in extraction_threads {
            worker.join().map_err(panic_message)?;
        }
        write_result?;
        discovery_result?;
        detection_result
    })
}

fn discover_files(
    app: &impl ScanEmitter,
    root_id: i64,
    root: &Path,
    scan_id: &str,
    cancelled: &AtomicBool,
    progress: &PipelineProgress,
    sender: Sender<Vec<ScanCandidate>>,
) -> Result<(), String> {
    let stage_started = Instant::now();
    let mut directories = vec![root.to_path_buf()];
    let mut batch = Vec::with_capacity(SCAN_BATCH_SIZE);
    let mut last_emit = Instant::now() - Duration::from_secs(1);
    while let Some(directory) = directories.pop() {
        if cancelled.load(Ordering::Acquire) {
            break;
        }
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if directory == root => {
                return Err(format!(
                    "Library root cannot be enumerated ({}): {error}",
                    root.display()
                ));
            }
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            if cancelled.load(Ordering::Acquire) {
                break;
            }
            let path = entry.path();
            let Ok(file_metadata) = entry.metadata() else {
                continue;
            };
            if file_metadata.is_dir() {
                if !is_hidden_or_system(&path, &file_metadata) {
                    directories.push(path);
                }
                continue;
            }
            if !file_metadata.is_file() || !is_epub(&path) {
                continue;
            }
            let parent_folder_path = path.parent().unwrap_or(root).to_string_lossy().into_owned();
            let file_name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let modified_time = file_metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|time| time.as_secs() as i64)
                .unwrap_or_default();
            let file_path = path.to_string_lossy().into_owned();
            batch.push(ScanCandidate {
                path,
                file_path: file_path.clone(),
                parent_folder_path,
                file_name,
                file_size: file_metadata.len() as i64,
                modified_time,
            });
            progress.discovered.fetch_add(1, Ordering::Relaxed);
            if last_emit.elapsed() >= Duration::from_millis(100) {
                emit_progress(app, scan_id, root_id, "discovery", progress, file_path);
                last_emit = Instant::now();
            }
            if batch.len() == SCAN_BATCH_SIZE {
                if sender.send(std::mem::take(&mut batch)).is_err() {
                    if cancelled.load(Ordering::Acquire) {
                        return Ok(());
                    }
                    return Err("Scan pipeline stopped.".into());
                }
                batch = Vec::with_capacity(SCAN_BATCH_SIZE);
            }
        }
    }
    if !batch.is_empty() && !cancelled.load(Ordering::Acquire) {
        if sender.send(batch).is_err() {
            if cancelled.load(Ordering::Acquire) {
                return Ok(());
            }
            return Err("Scan pipeline stopped.".into());
        }
    }
    progress
        .discovery_nanos
        .store(elapsed_nanos(stage_started), Ordering::Relaxed);
    Ok(())
}

fn detect_changes(
    database: &Database,
    root_id: i64,
    cancelled: &AtomicBool,
    progress: &PipelineProgress,
    receiver: Receiver<Vec<ScanCandidate>>,
    sender: Sender<ExtractionJob>,
) -> Result<(), String> {
    for candidates in receiver {
        if cancelled.load(Ordering::Acquire) {
            break;
        }
        let inputs = candidates
            .iter()
            .map(|book| NewBook {
                library_root_id: root_id,
                file_path: &book.file_path,
                parent_folder_path: &book.parent_folder_path,
                file_name: &book.file_name,
                file_size: book.file_size,
                modified_time: book.modified_time,
            })
            .collect::<Vec<_>>();
        let catalog_started = Instant::now();
        let upserts = database
            .upsert_scanned_books(&inputs)
            .map_err(|error| error.to_string())?;
        progress
            .catalog_nanos
            .fetch_add(elapsed_nanos(catalog_started), Ordering::Relaxed);
        for (candidate, upsert) in candidates.into_iter().zip(upserts) {
            if upsert.changed {
                progress.changed.fetch_add(1, Ordering::Relaxed);
                if sender
                    .send(ExtractionJob {
                        book_id: upsert.book_id,
                        candidate,
                    })
                    .is_err()
                {
                    break;
                }
            }
        }
    }
    Ok(())
}

fn extract_metadata(
    app: &impl ScanEmitter,
    root_id: i64,
    scan_id: &str,
    cover_cache_directory: &Path,
    progress: &PipelineProgress,
    receiver: Receiver<ExtractionJob>,
    sender: Sender<ExtractionResult>,
) {
    performance::start();
    for job in receiver {
        // Every received job was already committed as `pending`; finish it even
        // after cancellation so no partially staged row is mistaken for an
        // unchanged, fully extracted book on the next scan.
        let extraction_started = Instant::now();
        let metadata =
            metadata::extract_epub(&job.candidate.path, job.book_id, cover_cache_directory);
        progress
            .extraction_nanos
            .fetch_add(elapsed_nanos(extraction_started), Ordering::Relaxed);
        let extracted = progress.extracted.fetch_add(1, Ordering::Relaxed) + 1;
        if let Some(error) = &metadata.extraction_error {
            progress.failures.fetch_add(1, Ordering::Relaxed);
            crate::services::logging::event(
                "warn",
                "metadata_extraction_failed",
                &[
                    ("book_id", job.book_id.to_string()),
                    ("path", job.candidate.file_path.clone()),
                    ("error", error.clone()),
                ],
            );
        }
        if extracted == 1 || extracted % 16 == 0 {
            emit_progress(
                app,
                scan_id,
                root_id,
                "extraction",
                progress,
                job.candidate.file_path.clone(),
            );
        }
        if sender
            .send(ExtractionResult {
                book_id: job.book_id,
                file_path: job.candidate.file_path,
                metadata,
            })
            .is_err()
        {
            break;
        }
    }
    let timings = performance::finish();
    progress.metadata_parse_nanos.fetch_add(
        duration_nanos(timings.metadata_parsing()),
        Ordering::Relaxed,
    );
    progress
        .cover_nanos
        .fetch_add(duration_nanos(timings.cover_extraction), Ordering::Relaxed);
}

fn write_metadata_batches(
    app: &impl ScanEmitter,
    database: &Database,
    root_id: i64,
    scan_id: &str,
    progress: &PipelineProgress,
    receiver: Receiver<ExtractionResult>,
) -> Result<(), String> {
    performance::start();
    let mut batch = Vec::with_capacity(METADATA_BATCH_SIZE);
    let result = (|| {
        for result in receiver {
            batch.push(result);
            if batch.len() == METADATA_BATCH_SIZE {
                write_metadata_batch(app, database, root_id, scan_id, progress, &mut batch)?;
            }
        }
        write_metadata_batch(app, database, root_id, scan_id, progress, &mut batch)
    })();
    let timings = performance::finish();
    progress.reading_nanos.store(
        duration_nanos(timings.reading_derivation),
        Ordering::Relaxed,
    );
    progress.search_nanos.store(
        duration_nanos(timings.search_document_updates),
        Ordering::Relaxed,
    );
    result
}

fn write_metadata_batch(
    app: &impl ScanEmitter,
    database: &Database,
    root_id: i64,
    scan_id: &str,
    progress: &PipelineProgress,
    batch: &mut Vec<ExtractionResult>,
) -> Result<(), String> {
    if batch.is_empty() {
        return Ok(());
    }
    let refs = batch
        .iter()
        .map(|item| (item.book_id, &item.metadata))
        .collect::<Vec<_>>();
    let indexing_started = Instant::now();
    database
        .save_extracted_metadata_batch(&refs)
        .map_err(|error| error.to_string())?;
    progress
        .indexing_nanos
        .fetch_add(elapsed_nanos(indexing_started), Ordering::Relaxed);
    progress.indexed.fetch_add(batch.len(), Ordering::Relaxed);
    let current_path = batch
        .last()
        .map(|item| item.file_path.clone())
        .unwrap_or_default();
    emit_progress(app, scan_id, root_id, "indexing", progress, current_path);
    let _ = app.scan_emit("scan-catalog-updated", scan_id);
    batch.clear();
    Ok(())
}

fn emit_progress(
    app: &impl ScanEmitter,
    scan_id: &str,
    root_id: i64,
    stage: &'static str,
    progress: &PipelineProgress,
    current_path: String,
) {
    let _ = app.scan_emit(
        "scan-progress",
        ScanProgress {
            scan_id: scan_id.to_owned(),
            root_id,
            stage,
            discovered_count: progress.discovered.load(Ordering::Relaxed),
            changed_count: progress.changed.load(Ordering::Relaxed),
            extracted_count: progress.extracted.load(Ordering::Relaxed),
            indexed_count: progress.indexed.load(Ordering::Relaxed),
            current_path,
        },
    );
}

fn panic_message(_: Box<dyn std::any::Any + Send>) -> String {
    "A scan pipeline worker stopped unexpectedly.".into()
}

fn elapsed_nanos(started: Instant) -> u64 {
    started.elapsed().as_nanos().min(u64::MAX as u128) as u64
}

fn duration_nanos(duration: Duration) -> u64 {
    duration.as_nanos().min(u64::MAX as u128) as u64
}

fn nanos_millis(nanos: u64) -> String {
    format!("{:.3}", nanos as f64 / 1_000_000.0)
}

fn is_epub(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("epub"))
}

fn is_hidden_or_system(path: &Path, metadata: &fs::Metadata) -> bool {
    if path
        .file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with('.'))
    {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        let attributes = metadata.file_attributes();
        attributes & 0x2 != 0 || attributes & 0x4 != 0
    }
    #[cfg(not(windows))]
    {
        let _ = metadata;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_rejects_same_root_but_allows_different_roots() {
        let controller = ScanController::default();
        controller.start("first", 1).unwrap();
        assert!(controller.start("second", 1).is_err());
        assert!(controller.start("third", 2).is_ok());
        assert!(controller.cancel("first"));
        controller.finish("first");
        assert!(controller.start("replacement", 1).is_ok());
    }

    #[test]
    fn finds_case_insensitive_epubs() {
        assert!(is_epub(Path::new("日本語.EPUB")));
        assert!(!is_epub(Path::new("cover.jpg")));
    }
}

/// Exercises the production pipeline with a silent event sink; no real app data.
#[cfg(test)]
pub(crate) fn test_import(
    database: &Database,
    root_id: i64,
    root: &Path,
    cache: &Path,
) -> Result<(), String> {
    let app = SilentEvents;
    database.begin_scan(root_id).map_err(|e| e.to_string())?;
    run_pipeline(
        &app,
        database,
        root_id,
        root,
        cache,
        "api-test",
        2,
        &AtomicBool::new(false),
        &Arc::new(PipelineProgress::default()),
    )?;
    database
        .reconcile_completed_scan(root_id)
        .map_err(|e| e.to_string())?;
    Ok(())
}
