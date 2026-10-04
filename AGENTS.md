Build a local-first Windows desktop EPUB-library browser. Do not modify, rename, move, upload, or delete any EPUBs in the user’s source library.

## User-directed testing budget

For future work, testing and validation must consume no more than 50% of total
task time: at most a 50/50 development-to-testing split, preferably less testing.
Count test execution, verification-only builds/APK packaging, emulator/manual
checks, and validation-only waiting toward this budget. Track approximate time
without double-counting overlapping work; do not pad development to meet the ratio.

Use targeted checks during implementation, then one final relevant regression
run, build/package pass, and brief smoke test. Reuse existing evidence. Repeat
checks only for a concrete failure or a subsequent change affecting that check;
avoid repeated full suites, both-ABI rebuilds, and broad emulator passes for
unrelated small fixes. Documentation-only edits need a diff/readback check.

Plan required acceptance checks within this budget. If a necessary check would
exceed it, explain the specific remaining check and request an explicit budget
exception before expanding validation. Report unverified items honestly; never
claim skipped checks passed. Source-file safety remains mandatory.

Use:
- Tauri v2
- React + TypeScript + Vite
- Rust backend
- SQLite database stored in the app’s local data directory
- Tailwind CSS for styling
- EPUB parsing in Rust where practical; use epub.js in the React webview for reading

The app is for a very large, messy Japanese EPUB library. Folder and filename conventions are inconsistent and must never be treated as authoritative metadata.

## Location of library

The location of the library is F:\tmw collection

## Cover cache location

Keep the SQLite catalog in the app's local data directory on the system drive. Extracted EPUB covers are a regenerable cache and must support a user-selected cache directory, so they can be stored on the HDD (for example, `F:\tmw-browser-cache\covers`) rather than consuming substantial space on C:.

The cover cache directory must be separate from every source-library root. Do not create cache files, thumbnails, or any other app-managed files inside `F:\tmw collection` or inside any user-selected source-library directory. Deleting the cover cache must be safe: it may require regeneration from the source EPUBs, but must not lose catalog metadata, overrides, or source files.

## Local test library

`testLibrary/` contains local EPUBs for manual testing. Treat it as read-only source material: never modify, rename, move, upload, or delete its contents. It is deliberately Git-ignored and must not be committed.

## Local tooling note

Npm is installed and healthy on Windows. In restricted workspace shells, the per-user npm shim may fail because it cannot access the user-level npm directory; use the system launcher at `C:\Program Files\nodejs\npm.cmd` when that occurs. Do not treat that sandbox-specific failure as an npm installation problem.

Core principles:
1. The filesystem is read-only source material.
2. The SQLite catalog contains extracted metadata plus user overrides.
3. User edits always win over rescanned metadata.
4. Support Japanese text end-to-end, including Unicode normalization.
5. Optimize for browsing and finding books, not an initial full-text index of book contents.
6. Store bulky, regenerable cover thumbnails in a separate user-configurable cache location; never in a source-library directory.

The original feature phases are complete. Preserve their behavior and tests. Continue with the optimization phases below unless the user explicitly requests otherwise.

Implement in this order. At the end of every numbered phase, run the relevant checks, record comparative timings where applicable, and report what was completed before proceeding.

## Optimization Phase 1 — Establish performance baselines

1. Add structured timing around:
   - filesystem discovery
   - catalog upserts
   - EPUB metadata parsing
   - cover extraction and encoding
   - Japanese reading derivation
   - search-document updates
   - missing-file reconciliation
2. Record total duration, books discovered, books changed, books skipped, extraction failures, and throughput.
3. Create repeatable benchmarks or ignored performance tests for a generated large catalog fixture without committing source EPUBs.
4. Measure initial import, unchanged rescan, changed-book rescan, search-index rebuild, and application startup separately.
5. Keep logs local and never include EPUB contents.

Acceptance criteria:
- A developer can identify which stages dominate an import from local timing output.
- Baselines exist for at least 10,000 catalog rows and a representative EPUB fixture set.
- Performance instrumentation does not materially slow normal scans when verbose diagnostics are disabled.

## Optimization Phase 2 — Batch catalog writes and eliminate duplicate work

1. Replace per-book autocommit operations with explicit, bounded SQLite transactions.
2. Prepare and reuse statements for scan lookups, inserts, updates, and extraction-result writes.
3. Return the book ID and changed state from the scan upsert; do not query the row again by path.
4. Do not build a search document before metadata extraction when it will immediately be replaced.
5. Refresh each changed book's search document once, after final metadata is available.
6. Batch missing-file reconciliation without retaining an unnecessarily large duplicate path representation in memory.
7. Preserve user overrides, tags, collections, reading progress, and unavailable-book recovery behavior.

Acceptance criteria:
- Initial import uses bounded transactions rather than one commit per database operation.
- A changed book receives one final search-index refresh during import.
- An unchanged rescan performs no EPUB parsing, cover extraction, reading derivation, or search-index rewrite.
- Existing database, scanning, override, and recovery tests continue to pass.

## Optimization Phase 3 — Incremental search indexing and fast startup

1. Remove the unconditional full search-index rebuild from normal database startup.
2. Track the derived-index schema/version and rebuild only after a relevant migration, explicit maintenance action, or detected corruption/incompleteness.
3. Update search documents transactionally when discovered metadata, overrides, readings, aliases, filenames, paths, or tags change.
4. Cache or reuse the Japanese tokenizer where supported; do not repeatedly initialize dictionaries per field or per book.
5. Avoid morphological analysis for empty values and use the deterministic kana path where analysis is unnecessary.
6. Provide a cancellable, progress-reporting manual index rebuild.

Acceptance criteria:
- Opening an unchanged large catalog does not rewrite search tables or run reading derivation for every book.
- Incremental updates remain searchable immediately after commit.
- A forced rebuild produces results equivalent to a clean index.
- Startup and rebuild timings are covered by benchmarks.

## Optimization Phase 4 — Pipelined background importing

1. Separate scanning into stages:
   - fast filesystem enumeration and change detection
   - bounded parallel EPUB metadata/cover extraction
   - serialized batched database/index writes
2. Run imports as background Tauri tasks so the UI remains responsive and the initiating command does not block until the whole library is complete.
3. Use a bounded worker count suitable for HDD-backed libraries; make it configurable or choose it conservatively from measured results.
4. Apply backpressure with bounded queues so a very large library cannot cause unbounded memory growth.
5. Preserve cancellation across all stages and commit already completed batches safely.
6. Emit throttled progress with distinct discovery, extraction, and indexing states.
7. Prevent concurrent scans of the same root and define safe behavior for scans of different roots.

Acceptance criteria:
- Newly discovered books begin appearing before the complete import finishes.
- Browsing and cancellation remain responsive during a large import.
- Extraction concurrency improves throughput without uncontrolled disk seeking or SQLite contention.
- Cancelling leaves the catalog consistent and never marks unseen files unavailable from a partial scan.

## Optimization Phase 5 — Cover-cache efficiency

1. Decode extracted cover images and generate actual bounded thumbnails instead of copying arbitrary original cover bytes unchanged.
2. Choose and document thumbnail dimensions, quality, color handling, and output format based on measured size and decode performance.
3. Avoid rewriting an existing valid thumbnail when the source book and extraction inputs are unchanged.
4. Write cache files atomically through a temporary file in the cache directory, then rename them into place.
5. Keep cover work bounded and outside every source-library root.
6. Treat decode failures as per-book extraction errors without blocking metadata import.

Acceptance criteria:
- Cached covers have bounded dimensions and substantially lower typical disk usage than source images.
- Interrupted writes cannot leave a valid-looking partial thumbnail.
- Regeneration and cleanup remain safe because covers are fully regenerable.
- Source EPUBs and source directories remain byte-for-byte unchanged.

## Optimization Phase 6 — Large-library query and UI tuning

1. Profile browse, filter, sort, and Japanese/romaji substring search against at least 10,000 and preferably 100,000 generated catalog rows.
2. Use query plans to verify useful indexes and remove redundant indexes only after measurement.
3. Ensure pagination or virtualization does not trigger unnecessary full-result materialization.
4. Debounce searches and cancel or ignore stale frontend requests.
5. Avoid unnecessary full library reloads after progress events or small catalog changes.
6. Keep cover loading lazy and bounded; placeholders must remain cheap.

Acceptance criteria:
- Common browse and search interactions remain responsive on the large fixture.
- Stale searches cannot overwrite newer results.
- Import progress does not cause repeated expensive full-view refreshes.
- Query-plan and timing evidence is documented for important catalog operations.

## Optimization Phase 7 — Validation and release hardening

1. Compare all performance measurements with the Phase 1 baseline and document the results in the README.
2. Add regression tests for transaction rollback, cancellation, incremental indexing, unavailable files, duplicate prevention, and cache-write interruption.
3. Test initial import and unchanged rescan using both SSD-like and HDD-friendly concurrency settings where practical.
4. Verify database backup/import compatibility before any migration that changes persistent catalog data.
5. Verify Windows release builds and offline operation.
6. Confirm that no optimization writes to, renames, moves, uploads, or deletes source EPUBs or source directories.

Final acceptance criteria:
- Initial import throughput is materially improved over the recorded baseline.
- An unchanged rescan is dominated by filesystem enumeration and performs no redundant extraction or indexing.
- Application startup time no longer scales with a full search-index rebuild.
- The UI remains responsive throughout import, search, cancellation, and cover loading.
- Manual corrections and all existing catalog behavior survive optimization unchanged.

Important implementation rules:
- Prefer small, reviewable commits or checkpoints.
- Do not introduce cloud sync, user accounts, analytics, or network calls.
- Do not implement OCR or book-content full-text search in the first version.
- Do not infer or overwrite series metadata without an explicit user action.
- Before each destructive database migration or storage cleanup action, explain the impact and provide a backup path.
