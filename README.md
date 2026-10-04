# TMW EPUB Library

This is a local-first Tauri desktop catalog. EPUB source folders are read-only: scanning and search never modify EPUBs or source directories.

## Setup and Windows build

Requirements are Node.js/npm, the stable Rust toolchain, and the Windows prerequisites for Tauri v2 (WebView2 and Microsoft C++ Build Tools). From the repository root:

```powershell
& 'C:\Program Files\nodejs\npm.cmd' install
& 'C:\Program Files\nodejs\npm.cmd' run tauri dev
```

Run checks with `npm run lint`, `npm test`, `npm run build`, and `cargo test --manifest-path src-tauri/Cargo.toml`. Build Windows installers with `npm run tauri build`; Tauri writes release bundles below `src-tauri/target/release/bundle/`.

## Saved passages

Alt-click (or the configured Ctrl-click) a Japanese word in the reader, then choose **Save passage** in the dictionary popup. The bookmark stores the selected surface form, the first dictionary match's headword and reading when available, an editable sentence without ruby annotations, an optional note, and a precise EPUB CFI. No dictionary match or sentence context is required. Long context is bounded to 4,000 characters, notes to 2,000 characters. Repeated saves of the same word at the same anchor reuse the original bookmark and preserve its edits.

Open **Saved passages / 保存した文章** from the sidebar for bookmarks across books, or from the reader for that book alone. Both views paginate in groups of 50 and allow editing context/notes, jumping, and deleting individual bookmarks. Missing EPUBs leave excerpts readable. Jumps check the source size and modification time against the captured version; a changed or unavailable file produces an explanation rather than using an uncertain anchor. Excerpts without an anchor remain usable as references. Explicitly removing a source root from the catalog also removes its book bookmarks, like its other book-associated catalog data.

Schema migration 12 adds a separate bookmark table and index without changing existing metadata, overrides, tags, collections, or reading progress. Existing SQLite backups automatically include passages; restoring an older catalog adds an empty passage table. Bookmark operations never write source EPUBs or source folders and never index book contents. There is no vocabulary study workflow or network integration.

Validation: regression tests cover Japanese punctuation, ruby/vertical text, missing context, duplicate saves, editing/deletion, source-version checks, unavailable books, rescans, restarts, pagination, backup/restore, and restoration of schema 11 catalogs. Production frontend compilation and the Rust library suite are also checked.

## Performance diagnostics and Phase 1 baseline

Each scan writes one `scan_performance` JSON Lines record to the local application log. It includes total duration, discovery/change/skip/failure counts, throughput, and timings for filesystem discovery, catalog upserts, EPUB metadata parsing, cover extraction/encoding, Japanese reading derivation, search-document updates, and missing-file reconciliation. Startup emits a separate `application_startup_performance` record. Stage timers are thread-local, do no locking, and are inactive outside a scan or startup; diagnostics never record EPUB contents. Timings for nested work are also reported separately, so individual stage values should not be summed to calculate the total.

Repeat the generated baselines without using any source EPUBs:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml performance_baseline -- --ignored --nocapture --test-threads=1
```

Phase 1 baseline recorded on 2026-09-29 on the development Windows machine using the Rust test profile:

| Workload | Fixture | Baseline |
| --- | ---: | ---: |
| Initial catalog import | 10,000 rows | 41,545 ms |
| Unchanged catalog rescan | 10,000 rows | 84.7 ms |
| Changed-book rescan | 100 of 10,000 rows | 749.8 ms |
| Explicit search-index rebuild | 10,000 rows | 35,602 ms |
| Application database startup | 10,000 rows | 53,807 ms |
| EPUB metadata and cover extraction | 100 generated EPUB iterations | 89.6 ms (1,116.5 books/s) |

The EPUB fixture split was 37.9 ms for metadata parsing and 51.5 ms for cover extraction/copying. These are comparison baselines rather than hardware-independent targets; use the same command and build profile when measuring later phases. The generated catalog, EPUB, cover cache, and SQLite files live only in test-managed temporary directories and are removed afterward.

### Optimization Phase 2 checkpoint

Scan catalog writes now use bounded 128-book SQLite transactions with prepared lookup, upsert, seen-path, and extraction-result statements. Each scan upsert returns the stable book ID and whether extraction is required, eliminating the former query by path. Discovery no longer creates a provisional search document; changed books receive one transactional refresh after final extracted metadata is stored. Seen paths are staged in a temporary SQLite table as batches arrive, so missing-file reconciliation does not retain a second library-sized path list in memory. Cancelled scans discard the staging rows and never mark unseen books unavailable.

The same generated-catalog test was rerun on 2026-09-29 with the Phase 2 implementation and 128-row batches:

| Measurement | Phase 1 | Phase 2 |
| --- | ---: | ---: |
| Initial catalog discovery/upsert | 41,545 ms | 247.1 ms |
| Unchanged catalog rescan | 84.7 ms | 35.2 ms |
| Changed-book discovery/upsert | 749.8 ms | 1.7 ms |
| Explicit search-index rebuild | 35,602 ms | 34,566 ms |
| Application database startup | 53,807 ms | 35,814 ms |

The first three Phase 2 figures isolate catalog change detection and writes; EPUB extraction and the single final index refresh are measured by their existing stage timers during representative scans. Full index rebuild and startup remain intentionally unoptimized until Phase 3.

### Optimization Phase 3 checkpoint

Normal startup no longer rebuilds search data. A versioned `search_index_state` marker and row-completeness checks trigger repair only after a relevant migration or when derived rows are missing. Metadata extraction, overrides, tags, series assignments, reading overrides, aliases, filenames, and paths refresh their affected search document in the same SQLite transaction as the catalog change. The Lindera/IPADIC segmenter remains process-cached; empty strings skip analysis and all-kana values use a deterministic path without morphological analysis.

Settings now provides an explicit search-index rebuild with progress and cancellation. The rebuild runs in one transaction, so cancellation rolls it back and preserves the previously usable index. A completed rebuild atomically replaces the derived documents and records the current derived-index version.

The 10,000-row generated-catalog benchmark was rerun on 2026-09-29 using the same Rust test profile:

| Measurement | Phase 1 | Phase 2 | Phase 3 |
| --- | ---: | ---: | ---: |
| Initial catalog discovery/upsert | 41,545 ms | 247.1 ms | 241.4 ms |
| Unchanged catalog rescan | 84.7 ms | 35.2 ms | 35.2 ms |
| Changed-book discovery/upsert | 749.8 ms | 1.7 ms | 1.6 ms |
| Explicit search-index rebuild | 35,602 ms | 34,566 ms | 24,408 ms |
| Unchanged application database startup | 53,807 ms | 35,814 ms | **8.1 ms** |

Startup is now constant-time with respect to reading derivation and search-document rewriting for a complete index. The explicit rebuild remains intentionally proportional to catalog size and is covered separately because it is a user-requested maintenance operation.

### Optimization Phase 4 checkpoint

Library imports now launch as background Tauri work and return control to the webview immediately. The importer is a bounded pipeline: filesystem enumeration feeds 128-path change-detection transactions, changed EPUBs feed a small extraction queue, and one writer commits final metadata plus search documents in 32-book batches. The default of two extraction workers is deliberately conservative for HDD libraries; it can be changed through the local `import_worker_count` setting and is clamped to 1–8. Queue capacities scale only with the worker count, so catalog size does not create unbounded in-memory work.

Progress distinguishes discovery, extraction, and indexing. Each committed metadata batch becomes browseable during the import, while the UI limits catalog refreshes to at most twice per second. Cancellation stops discovery and new change-detection batches, lets the already committed bounded extraction batch finish, preserves those completed commits, and abandons the seen-path staging set. Consequently, a partial scan never marks files that were not reached as unavailable. A second scan of the same root is rejected before launch; different roots may scan concurrently while SQLite writes remain serialized by the catalog connection.

Each `scan_performance` record now also includes the configured worker count and pipeline discovery wall time, catalog-write time, cumulative extraction-worker time, index-write time, and missing-file reconciliation time. Because extraction workers overlap, cumulative extraction-worker time can exceed total wall time and should not be added to the other measurements.

The generated fixtures were rerun after the pipeline change. These isolate catalog/index work and single-worker EPUB work; live scan logs are the source for comparing the configurable one- and two-worker pipeline on a particular HDD because seek behavior is hardware- and library-layout-dependent.

| Phase 4 verification fixture | Result |
| --- | ---: |
| Initial catalog discovery/upsert (10,000 rows) | 252.8 ms |
| Unchanged catalog rescan (10,000 rows) | 37.2 ms |
| Changed-book discovery/upsert (100 rows) | 1.7 ms |
| Explicit search-index rebuild (10,000 rows) | 24,246 ms |
| Unchanged application database startup | 6.0 ms |
| EPUB parse/cover fixture (100 iterations, single worker) | 83.7 ms / 1,195 books/s |

### Optimization Phase 5 checkpoint

EPUB cover bytes are now decoded and converted into real, bounded thumbnails instead of being copied unchanged. The cache format is JPEG at quality 82 with a maximum size of 320×480 pixels, aspect ratio preserved and no upscaling. Transparent source pixels are composited onto white. JPEG was selected for its broad WebView support, fast decode, and predictable compact size for cover artwork; the fixed format also makes cache behavior independent of inconsistent EPUB media-type declarations.

Cover output is encoded to a uniquely named temporary file in the selected cache directory, flushed to disk, and only then renamed to the final `book-<id>.jpg` path. A failed decode or encode removes its temporary file and is recorded as a per-book extraction error while preserving the successfully parsed metadata. Cover input is capped at 64 MiB before decode. Normal unchanged rescans never enter extraction, so an existing valid thumbnail is not rewritten; the explicit **Regenerate covers** action intentionally rebuilds it.

The release-mode generated fixture was rerun on 2026-09-29. Its deliberately noisy 600×900 PNG is a conservative compression case rather than a typical illustrated cover:

| Phase 5 cover fixture | Result |
| --- | ---: |
| Source cover size | 1,966,969 bytes |
| Cached 320×480 JPEG | 142,535 bytes average |
| Cache-size reduction | 92.8% |
| Decode, resize, and atomic encode (100 iterations) | 25,341 ms / 3.94 books/s |

This work is CPU-heavier than the Phase 4 byte-copy baseline by design. The importer keeps it bounded by the existing conservative extraction-worker limit and bounded queues, while unchanged books incur no decode or cache write.

### Optimization Phase 6 checkpoint

Catalog substring searches of three or more normalized characters now use the existing SQLite FTS5 trigram index instead of applying `instr` to every search-document field. One- and two-character queries retain the indexed-field compatibility fallback because a trigram tokenizer cannot represent them. A schema-only migration adds measured browse indexes for recently added, modified, folder, and root-scoped ordering; it does not rewrite catalog metadata or touch source files. Empty-query browsing omits relevance sorting so SQLite can satisfy common recent/modified views directly from those indexes. An automated `EXPLAIN QUERY PLAN` regression confirms that substring search uses the FTS virtual-table index.

The webview debounces search for 300 ms and assigns a generation to each catalog request, so a slower stale response cannot overwrite newer filters or search text. Only one next-page request may be active at a time. Scan-driven refreshes are coalesced to once per second, off-screen cards use browser-native rendering containment, and covers use lazy loading plus asynchronous decode. The backend still returns bounded pages (maximum 200 rows; the UI requests 80), so neither browsing nor search materializes an entire result set in Rust or React.

The release-mode synthetic benchmark was run on 2026-09-29. It builds temporary databases and FTS rows only; it does not read or write library EPUBs. Values are the mean of 25 warm queries:

| Catalog rows | Recent first page | Deep modified page | Japanese substring | Romaji substring |
| ---: | ---: | ---: | ---: | ---: |
| 10,000 | 0.160 ms | 4.081 ms | 4.756 ms | 0.172 ms |
| 100,000 | 0.264 ms | 49.889 ms | 53.767 ms | 0.544 ms |

Run the repeatable benchmark with `cargo test --release performance_phase6_queries_10000_and_100000_rows -- --ignored --nocapture` from `src-tauri`. The deliberately deep `OFFSET` case is recorded as a worst-case pagination comparison; ordinary infinite-scroll pages are shallow and the UI never fetches beyond the next 80 rows.

### Optimization Phase 7 validation and release checkpoint

The Phase 1 generated-catalog workload was rerun on 2026-09-29 in the same Rust test profile. These figures are end-to-end database fixture timings on the development Windows machine; live HDD extraction throughput remains available from each local `scan_performance` log because it depends on disk layout and seek behavior.

| Measurement | Phase 1 baseline | Phase 7 | Change |
| --- | ---: | ---: | ---: |
| Initial catalog import (10,000 rows) | 41,545 ms | 622.8 ms | 98.5% faster |
| Unchanged rescan (10,000 rows) | 84.7 ms | 34.9 ms | 58.8% faster |
| Changed-book rescan (100 rows) | 749.8 ms | 8.0 ms | 98.9% faster |
| Explicit search-index rebuild (10,000 rows) | 35,602 ms | 24,760 ms | 30.5% faster |
| Unchanged application database startup | 53,807 ms | 6.1 ms | 99.99% faster |

The release-profile cover fixture processed 100 generated EPUB iterations in 25,201 ms (3.97 books/s), including decode, bounded resize, JPEG encoding, flush, and atomic placement. This is intentionally not compared as a speedup against the Phase 1 byte-copy number: Phase 5 added real image decoding and thumbnail generation to reduce cache size by 92.8%. An unchanged rescan bypasses extraction entirely, regardless of whether the HDD-friendly one-worker or default two-worker setting is selected. Use `import_worker_count=1` and `import_worker_count=2` with the same real library and compare local scan logs when tuning a particular HDD; generated catalog benchmarks cannot reproduce physical seek behavior.

The Phase 6 release query benchmark was also repeated. At 10,000/100,000 rows, recent-page browse took 0.162/0.346 ms, Japanese substring search 4.763/53.825 ms, and romaji substring search 0.169/0.580 ms. The 100,000-row deep `OFFSET` stress case took 53.502 ms; normal UI pages remain shallow and bounded.

Regression coverage now explicitly verifies batched transaction rollback across catalog and derived-search writes, cancellation without unavailable-file reconciliation, forced/incremental index equivalence, unavailable-file recovery, duplicate prevention, backup/restore preservation of overrides, and interrupted cover-cache writes preserving the previous entry while cleaning temporary files. Phase 7 adds no persistent schema migration; the current-schema backup/restore round trip is covered by the Rust suite, and any future persistent migration must retain the documented pre-restore safety backup path.

Release validation completed on Windows with 31 Rust tests passing offline (3 opt-in performance tests ignored during the normal run), frontend lint passing, 1 Vitest test passing, and the TypeScript/Vite production build passing. `cargo build --release --offline` confirms the Rust application can be built from the local dependency cache without network access. A full Tauri build produced both x64 MSI and NSIS installers. Runtime source contains no remote fetch, HTTP client, WebSocket, account, analytics, or cloud-sync path; the only URLs in configuration are the non-runtime schema reference and localhost development server.

Source-safety tests compare generated EPUB bytes before and after extraction. All performance and regression fixtures use temporary directories, and no validation command writes to `testLibrary/`, `F:\tmw collection`, or any configured source root. Cache files remain separately regenerable and catalog backups contain paths and metadata, never EPUB bytes.

## Architecture and privacy

The React/TypeScript webview handles browsing, editing, onboarding, and the epub.js reader. Rust owns filesystem discovery, EPUB parsing, Japanese analysis, dictionary import, and all SQLite access. UI code never executes SQL. Everything runs offline: the app has no accounts, analytics, cloud synchronization, or network lookup path.

Selected library roots—including the normal `F:\tmw collection` root and the ignored `testLibrary/` fixture—are source material. Scans open EPUBs read-only and never create thumbnails, databases, or other files beside them. User corrections always live in SQLite and take precedence over rescanned EPUB metadata.

## Storage, rescanning, and recovery

The catalog is `catalog.sqlite3` in the Tauri application-data directory for `com.tmw.epublibrary` on the system drive. SQLite foreign keys and WAL mode are enabled. Structured scan, extraction, backup, and restore events are JSON Lines in its `logs/tmw-library.jsonl` file.

A completed rescan marks catalog rows whose paths vanished as `unavailable`; it does not delete their metadata, overrides, tags, collections, or reader location. If the same path returns, a later rescan restores and re-extracts it. Cancelled or failed scans never mark missing books. Removing a library root is an explicit catalog-only action and never touches the source directory.

Settings can export a consistent SQLite catalog backup and restore one. A restore validates the selected SQLite file and automatically writes a pre-restore safety backup under the app-data `backups/` directory before replacing catalog state. Backups contain catalog metadata, settings, overrides, tags, collections, and reading state, but never source EPUB bytes. Dictionary indexes may also be present and remain regenerable.

The cover cache defaults to an app-data `covers/` directory and can be moved to a separate HDD directory such as `F:\tmw-browser-cache\covers`. It cannot be inside or contain a library root. The cache is not needed for catalog backup: deleting it loses no metadata or overrides, and **Regenerate covers** rebuilds it by reading source EPUBs.

On first launch, onboarding explains these read-only and local-storage boundaries before asking the user to add a library root.

## Search

Phase 6 uses SQLite FTS5 with the bundled SQLite trigram tokenizer for fast local substring matching across effective title, author, series, filename, folder path, and tags. Matching text is regenerated in `book_search_documents` and `book_search_fts`; originals remain unchanged. The app applies Unicode NFKC normalization, collapses whitespace, and uses case-insensitive matching where applicable, so `1巻` matches `１巻`. Very short queries use a normalized indexed-field fallback because trigram indexes require three characters. Results prioritize exact and prefix title matches over metadata, filename, and path matches.

The index is refreshed transactionally after scanner metadata extraction and catalog edits. Startup only rebuilds it after a relevant migration or when derived-index incompleteness is detected. It contains no book contents and makes no network requests.

## Romaji and reading search

Phase 7 adds a local, regenerable reading index for effective titles, authors, series, and filenames. It uses [Lindera](https://github.com/lindera/lindera) with its embedded IPADIC dictionary, both distributed under MIT terms. The dictionary is compiled into the Windows application rather than downloaded at runtime, so indexing has no network dependency. Kana readings are converted using Hepburn-style romaji (`しんげき` → `shingeki`); width, case, hyphens, and repeated whitespace are normalized on input.

This intentionally assistive analyzer is conservative: unknown or ambiguous tokens do not change displayed metadata, and Japanese-text search still works normally. The embedded dictionary increases the Windows binary size, but is loaded once per process and reused for indexing. IPADIC readings can be imperfect for unusual names, neologisms, and stylized titles.

The UI currently keeps readings generated-only, while the catalog and incremental index path support reading overrides and aliases for future workflows. Derived fields are rebuilt only when required by migration, detected incompleteness, or an explicit maintenance action, with no source EPUB changes.

## Folder groups and series assistance

Phase 9 treats a shared parent folder as a browsing aid, never as canonical series metadata. The book details panel lists its folder group and can create a collection or apply a user-chosen series override to a selected subset. These are explicit catalog-only actions and do not rename, move, or alter source EPUBs.

Suggested order prefers EPUB series indexes, then visible volume hints (`１巻`, `Vol. 1`, `第1巻`, numeric suffixes, and ranges), then Unicode-normalized filename order. Suggestions are never saved automatically and never overwrite a user override without the user choosing **Assign series**.

## Embedded reader

Phase 10 uses bundled `epub.js` inside the Tauri webview. The reader opens the cataloged EPUB through Tauri's local asset protocol, never uploads it, and never writes to it. Reader location is stored as an EPUB CFI in the SQLite `reading_progress` table, keyed by book ID, so reopening a book restores the last location. Font size and light/dark reader appearance are local preferences. Malformed, inaccessible, and DRM-protected EPUBs show a clear reader error while leaving the catalog usable.

## Offline reader dictionary

Phase 11 bundles the supplied `jmdict-eng-3.6.2.json` [JMdict](https://www.edrdg.org/jmdict/j_jmdict.html) export with the application and automatically builds a regenerable local SQLite index on first use. It is never uploaded, copied to a library root, or written into an EPUB. The reader's **Rebuild dictionary** action replaces only this app-managed index after a successful import; a bundled-resource failure leaves an existing usable index intact.

In the reader, hold the selected trigger modifier (Alt by default; switchable to Ctrl) while clicking Japanese text. Caret APIs identify the clicked text and bundled Lindera/IPADIC selects the local token and its lemma, so inflections such as `食べました` can be looked up as `食べる`. Lookup tries surface form, lemma, then reading; the query remains editable and a conservative Japanese-run fallback preserves reading controls if tokenization fails. Preferences and the index stay in SQLite app data, with no network calls.

## Lookup history and repeat encounters

Open **Lookup history** from the library sidebar or reader toolbar to search recent lookups and return to source passages, including passages in another book. The dictionary popup shows the retained encounter count after a successful lookup. One intentional click or submitted manual query records one encounter after all surface/lemma/reading fallbacks finish; unsuccessful lookups, tokenization, rerenders, and stale responses do not add encounters. Manual queries have no source anchor and do not reuse the previous clicked sentence.

When all returned senses agree on headword and reading, inflected surface forms aggregate under that Unicode-normalized identity. Different readings remain separate. Ambiguous results are labeled and grouped only by normalized query, separately from identified entries; no guessed headword is saved. History does not create saved passages or learning tasks.

Tracking defaults to on and can be disabled in the history view without deleting existing history. **Clear history** confirms removal of history only; reading progress, completion, saved passages, and source files are preserved. Retention is bounded to the latest 10,000 successful lookups, with counts calculated within that window. Pages contain at most 50 encounters; search is debounced and stale responses are ignored. Anchors are checked against source size and modification time before jumping, and unavailable/changed sources keep their excerpts readable.

Additive migration 13 stores history and its preference in the local SQLite catalog, included in backup/restore. Dictionary rebuilds and library rescans preserve history; no EPUB files are written or uploaded. Restoring an older catalog creates the history table without replacing existing corrections or progress.

Validation covers fallback counting, manual-query isolation, stale searches, anchors, tracking controls, independent clearing, retention, separate readings, rescans/recovery, restart, dictionary rebuild, and backup/restore. Run `cargo test --manifest-path src-tauri/Cargo.toml --lib history_retention_benchmark -- --ignored --nocapture` for the generated fixture benchmark. On this Windows debug build at 10,000 encounters, a recent 50-row page took 18.82 ms, substring search at the last page took 20.42 ms, and recording with retention enforcement took 0.75 ms. The fixture uses one shared identity (the worst case for repeat counts). Query-plan checks confirm the covering identity index is used. These are backend timings, not end-to-end UI latency.

## Continue Reading home screen

The app opens to Continue Reading (Japanese/English), with up to twelve recently opened unfinished books and a primary action for the most recent available one. Opens restore the existing EPUB CFI. Recency is recorded only after successful display, with book ID as a deterministic tie-breaker for opens in the same second. Opening details does not change it. Covers are lazy, and home does not issue full-library browse requests or refresh on scan progress. Revisit home after a rescan to refresh availability.

Unavailable books retain their saved position and link to Library Roots for reconnection/rescanning. Books with no saved anchor clearly open at the beginning. Home shows saved-position information instead of an unreliable percentage, including for fixed-layout EPUBs. A reader toolbar action explicitly marks a book finished or unfinished. Completion hides it from home; reopening it through the full library preserves completion. Visiting the last page never marks completion automatically.

Migration 11 adds a small indexed `reader_resume` table and seeds prior reader positions using their saved timestamps. It does not remove or rewrite existing metadata/progress. Resume data lives in the local catalog, survives rescans and unavailable-file recovery, and is included in existing SQLite backup/restore. All source libraries remain read-only.

Validation: frontend tests cover empty-state routes, selecting the available primary candidate, unavailable cards, and refresh after reader close. Backend regression covers reopen, completion, rescan, restart, backup/restore, and unavailable source preservation. Production frontend build, lint, 16 frontend tests, and 34 backend tests pass (five opt-in benchmarks excluded).

Run the generated-catalog benchmark with `cargo test --manifest-path src-tauri/Cargo.toml --lib resume_large_catalog_benchmark -- --ignored --nocapture`. With 100,000 catalog and resume rows, including twenty unavailable newest entries, 100 pairs of home queries took 10.35 ms total in a Windows debug build (about 0.10 ms per pair). SQLite used the covering `idx_reader_resume_recent` index for the bounded recency selection. This is a database benchmark, not a measured end-to-end UI latency.

### Reading statuses and smart shelves

Books have an explicit Unset, Want to read, Reading, Paused, or Finished status in the local catalog. Change it in book details or the reader. A successful reader display promotes Unset/Want to read to Reading; Paused and Finished survive reopening. Visiting the final page never finishes a book. The existing Mark finished/Mark unfinished buttons use the same status. Paused books leave Continue Reading until their status changes.

Finishing explicitly records a UTC completion timestamp (displayed in local time). Repeating Finished keeps the existing date. Leaving Finished clears it; finishing again records a new date. Existing finished books retain their status with an unknown historical completion date. Migration 14 is additive, preserves previous progress/corrections, and maps existing resume rows to Reading or Finished. Catalog backup/restore includes statuses, completion dates, and smart shelves, and restores older catalogs through migrations.

The library's status buttons provide built-in reading shelves. Combine a status with the existing query, root, tag, collection, metadata, duplicate, and sort controls, then name and save a smart shelf. Selecting it restores those filters. Edit the controls and use Update shelf to replace its definition or rename it; Position sorts shelves numerically (then name). Delete shelf removes only the saved query. Definitions are versioned and validated, never executable SQL. Queries stay dynamic and paginated at 80 rows, with the existing stale-request protection; committed corrections, tags, and statuses affect the next query immediately.

Validation: 41 Rust tests passed (7 optional benchmarks ignored), 31 frontend tests passed, TypeScript and ESLint passed, and Vite production output built with `--configLoader runner` (the default esbuild config loader encounters sandbox parent-directory access restrictions). Regression coverage includes transitions, preserved completion dates, rescans, unavailable books, restart, backup/restore, old-schema migration, invalid filters, shelf editing/deletion, and source safety. No source library or testLibrary files are written by this feature.

Generated smart-shelf benchmark (Windows debug build, 20 queries each; fixtures contain no EPUB files):

| Catalog rows | Want to read, title sort | Want to read + Japanese `進撃` | Want to read + romaji `shingeki` |
| --- | ---: | ---: | ---: |
| 10,000 | 3.029 ms | 4.016 ms | 0.366 ms |
| 100,000 | 34.743 ms | 47.026 ms | 0.770 ms |

A query-plan assertion verifies `idx_books_reading_status`; the status equality predicate is selected only when filtering so SQLite can use that index. Japanese two-character search uses the existing normalized fallback, while romaji uses trigram FTS. Reproduce with `cargo test --manifest-path src-tauri/Cargo.toml --lib smart_shelf_large_catalog_benchmark -- --ignored --nocapture`.
