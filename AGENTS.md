Build a local-first Windows desktop EPUB-library browser. Do not modify, rename, move, upload, or delete any EPUBs in the user’s source library.

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

Implement in this order. At the end of every numbered phase, run the relevant checks and report what was completed before proceeding.

Implementation status: Phases 1 through 5 are complete. Begin further work with Phase 6 unless the user explicitly requests otherwise.

## Phase 1 — Project setup

1. Create a Tauri v2 project using React, TypeScript, and Vite.
2. Add Tailwind CSS.
3. Establish this structure:
   - src/components
   - src/features/library
   - src/features/books
   - src/features/reader
   - src/lib
   - src-tauri/src
   - src-tauri/src/db
   - src-tauri/src/services
   - src-tauri/src/models
4. Add linting, formatting, and a test setup.
5. Create a simple responsive application shell:
   - sidebar
   - main content area
   - empty state
6. Use a clean dark-mode-capable visual design with good Japanese font fallback support.

Acceptance criteria:
- The desktop application launches.
- The React UI renders inside Tauri.
- There are no TypeScript or Rust compile errors.

## Phase 2 — SQLite catalog and migrations

Use SQLite in Tauri’s application data directory. Enable foreign keys and WAL mode.

Create migrations and database access functions for these tables:

library_roots:
- id
- path (unique)
- display_name
- added_at
- last_scanned_at

books:
- id
- library_root_id
- file_path (unique)
- parent_folder_path
- file_name
- file_size
- modified_time
- content_hash nullable
- discovered_title nullable
- discovered_creator nullable
- discovered_language nullable
- discovered_identifier nullable
- discovered_series nullable
- discovered_series_index nullable
- discovered_cover_path nullable
- extraction_status
- extraction_error nullable
- created_at
- updated_at

book_overrides:
- book_id (unique)
- title nullable
- creator nullable
- series_name nullable
- volume_label nullable
- cover_path nullable
- notes nullable
- updated_at

tags:
- id
- name (unique)

book_tags:
- book_id
- tag_id

collections:
- id
- name
- created_at

collection_books:
- collection_id
- book_id
- sort_order nullable

app_settings:
- key
- value

Also create useful indexes for:
- books.library_root_id
- books.parent_folder_path
- books.discovered_title
- books.discovered_creator
- tags.name

Implement a database abstraction layer; UI code must not directly execute SQL.

Acceptance criteria:
- Migrations run automatically on app startup.
- The database can insert and retrieve a library root and a book.
- Add Rust tests for migration and basic CRUD behavior.

## Phase 3 — Library root selection and scanning

Implement a Tauri command to let the user choose a folder using the native folder picker.

When a root is added:
1. Save it in library_roots.
2. Recursively discover files with .epub extension, case-insensitively.
3. Do not assume that “book folders” have a consistent layout.
4. Store every EPUB as an individual book.
5. Record its full file path, filename, parent folder path, file size, and modification time.
6. Skip files that have not changed since their previous scan.
7. Do not read entire EPUB files into memory unnecessarily.
8. Report scan progress to the frontend via Tauri events:
   - scan started
   - current path or count
   - scan completed
   - scan failed
9. Allow cancellation.
10. Do not scan hidden/system directories when reasonably identifiable.

Add a “Library Roots” settings screen with:
- add root
- rescan root
- remove root from the catalog only
- last scan time
- number of discovered books

Acceptance criteria:
- A test fixture with nested Japanese folders and multiple EPUBs is discovered correctly.
- Rescanning does not create duplicate book rows.
- Removing a root removes catalog records but never touches source files.

## Phase 4 — EPUB metadata and cover extraction

For each new or changed EPUB:
1. Open it as a ZIP archive.
2. Locate container.xml.
3. Find the OPF package document.
4. Extract, when present:
   - title
   - creator/author
   - language
   - identifier
   - series metadata
   - series index
5. Locate and extract the cover image where possible.
6. Save extracted cover thumbnails in the configured, separate cover-cache directory, keyed safely by book ID or content hash. Default to an app-managed cache location, but allow the user to choose an HDD location. Do not store this cache inside a source-library root.
7. Never write anything into the EPUB itself.
8. Record extraction errors per book without aborting the scan.

Metadata fallbacks:
- If title is absent, use filename without extension.
- If creator is absent, leave it blank.
- If a series cannot be found, do not invent one from the filename yet.

Use Japanese-safe string handling. Create a search-normalization function that:
- applies Unicode NFKC normalization
- normalizes full-width and half-width digits
- trims repeated whitespace
- preserves original display text
- is only used for matching, never as the displayed title unless explicitly requested

Acceptance criteria:
- The app extracts title and cover from common EPUB 2 and EPUB 3 fixtures.
- A malformed EPUB is shown as a catalog item with an understandable error state.
- Source EPUBs remain byte-for-byte unchanged.

## Phase 5 — Browsing interface

Create the primary library screen.

Sidebar:
- All Books
- Library Roots
- Recently Added
- Untitled / metadata needs attention
- Tags
- Collections
- Settings

Main view:
- cover grid as the default
- compact list view toggle
- each book card shows cover, effective title, effective author, series, and volume when available
- placeholder cover for missing images
- virtualized rendering for large result sets
- sort by title, author, series, date added, file modified date, and folder
- filter by library root, tag, collection, and “needs metadata”

Define “effective” metadata as:
- override value when it exists
- otherwise discovered EPUB metadata
- otherwise a safe fallback such as the filename

Acceptance criteria:
- The UI remains usable with at least 10,000 catalog records.
- Missing metadata or covers never crash the grid.
- Japanese text displays correctly.

## Phase 6 — Search and Japanese-friendly matching

Implement instant catalog search over:
- effective title
- discovered title
- effective author
- discovered creator
- effective series
- filename
- parent folder path
- tags

Requirements:
1. Search original and NFKC-normalized forms.
2. Support substring matching suitable for Japanese titles.
3. Make matching case-insensitive where meaningful.
4. Rank exact title matches above filename and path matches.
5. Debounce input.
6. Keep search local and private.
7. Do not implement full book-content indexing yet.

Use SQLite FTS5 with a trigram tokenizer if supported by the selected SQLite integration. If it is not available, use indexed normalized columns plus a carefully designed fallback search approach. Document the choice in the README.

Acceptance criteria:
- A query using `1巻` can locate a title stored as `１巻`, and vice versa.
- Searching a Japanese substring returns relevant books.
- Search results return quickly on a large catalog fixture.

## Phase 7 — Romaji and Japanese-reading search

Extend the local catalog search so romaji queries can find Japanese metadata, including kanji where a reading can be derived. This is an assistive search index, not canonical metadata.

Requirements:
1. Keep all processing offline and bundled with the desktop application; do not send titles, paths, or EPUB contents to any network service.
2. Add migration-backed, regenerable derived search fields for kana readings and normalized romaji forms of searchable metadata (effective title, author, series, filename, and aliases where applicable). Keep original display strings unchanged.
3. Use a Rust-compatible Japanese morphological analyzer and dictionary to derive readings from kanji where practical. Prefer a maintained, distributable Rust implementation with an embedded or app-bundled dictionary; evaluate the resulting binary size, Windows packaging, license, and scan/indexing performance before committing to a library.
4. Convert derived kana readings and romaji queries using one consistent, documented romanization scheme. Normalize case, Unicode width, whitespace, and common romaji input variants before matching.
5. For kana-only source text, generate romaji deterministically without morphological analysis. For kanji and mixed text, use best-effort morphological readings. Never invent or display a reading as authoritative metadata.
6. Index derived fields locally and update them when discovered metadata or user overrides change. A failure to derive a reading must not block cataloging, rescanning, or ordinary Japanese search.
7. Search both Japanese text and derived reading/romaji fields. Rank exact Japanese-title matches first, then exact/prefix reading matches, then broader romaji, filename, and path matches.
8. Add editable per-book reading and search-alias overrides. These overrides take precedence over derived readings, persist across rescans, and allow correction of names, unusual kanji readings, and stylized titles.
9. Clearly label generated readings versus user-provided reading/alias overrides in the book details editor. Resetting an override restores the generated reading, not a guessed canonical value.

Document the selected analyzer/dictionary, its licensing and distribution approach, romanization behavior, known limitations (especially ambiguous or unusual kanji readings), and search-index rebuild behavior in the README.

Acceptance criteria:
- A romaji query such as `shingeki no kyojin` can locate `進撃の巨人` when the bundled analyzer derives that reading.
- A romaji query can locate kana-only and mixed kana/kanji titles without network access.
- A user-supplied reading or alias corrects a deliberately unusual title and survives rescans and app restarts.
- Failure or ambiguity in analysis never changes displayed metadata and never prevents a book from appearing in search by its original Japanese text.
- Automated tests cover kana conversion, representative kanji reading derivation, input normalization, ranking, and override precedence.

## Phase 8 — Book details and manual correction

Add a book detail panel or page with:
- large cover
- effective metadata
- raw discovered metadata
- source path
- parent folder
- file information
- tags
- extraction status/errors
- “Open folder”
- “Read book”

Add editable overrides for:
- title
- author
- series name
- volume label
- cover replacement
- tags
- notes

Clearly distinguish:
- EPUB-discovered data
- user overrides
- filename/path fallback data

Provide:
- reset one override to discovered metadata
- reset all overrides for one book
- batch tagging for selected books

Acceptance criteria:
- A title correction survives rescans.
- Resetting an override restores the correct discovered/fallback value.
- No edit writes into the original EPUB.

## Phase 9 — Folder-based grouping and series assistance

Do not automatically treat folders as canonical series names. Instead, add optional suggestions.

For books sharing the same parent folder:
- show a “Folder group” section in the details view
- allow the user to create a collection or assign a series from selected books
- suggest likely volume order from:
  1. EPUB series index
  2. visible volume patterns in title/filename
  3. natural sort of filenames

Implement volume-pattern detection for common forms, including:
- 1巻 / ２巻
- Vol. 1
- 第1巻
- bare numeric suffixes
- ranges such as 1-2

Treat detected values as suggestions only. Never silently change user metadata.

Acceptance criteria:
- A folder holding `１巻` and `２巻` can be selected and assigned to one series manually.
- Suggestions do not overwrite overrides.
- Natural sorting handles full-width Japanese numerals and ASCII numerals sensibly.

## Phase 10 — Embedded reader

Implement a reader view using epub.js.

Requirements:
- Open an EPUB selected from the local catalog.
- Support next/previous page.
- Remember reading location locally per book.
- Support font-size controls and light/dark themes.
- Provide a clear fallback error if a DRM-protected or malformed EPUB cannot be opened.
- Keep the reader isolated from catalog scanning.

Acceptance criteria:
- A normal EPUB opens from the details page.
- Closing and reopening restores the previous reading position.
- The reader does not require uploading the book anywhere.

## Phase 11 — Reader pop-up dictionary

Add an offline pop-up dictionary to the embedded reader for quick Japanese lookups. This is a personal-use feature: optimize for the owner's local workflow rather than multi-user setup or cloud synchronization.

Requirements:
1. Allow a user to hover with a configurable modifier key and click/keyboard-select Japanese text in the epub.js reader to open a dictionary pop-up.
2. Select sensible Japanese word boundaries, including inflected forms, using a bundled offline tokenizer where practical. Always allow the user to adjust the selected text before looking it up.
3. Show the matched term, reading, definitions, and part of speech. Where installed dictionary data provides them, show pitch, frequency, and kanji details.
4. Keep dictionary data local. Do not send selected text, EPUB contents, titles, paths, or lookup history to a network service.
5. Support importing and enabling/disabling personal dictionary archives in a documented format. Prefer compatibility with Yomitan-format dictionaries when practical; do not assume an imported dictionary's metadata is canonical book metadata.
6. Store dictionary configuration and imported index data in app-managed storage, separate from source-library roots and the cover cache. Never write into an EPUB or its source directory.
7. Make lookups non-blocking: an unavailable dictionary, malformed entry, or tokenizer failure must leave the reader usable and fall back to a plain-text lookup where possible.
8. Provide reader settings for enabling the pop-up, trigger behavior, pop-up size, and active dictionaries.

Acceptance criteria:
- Hovering or selecting a common Japanese word shows a local definition without a network request.
- A user-imported dictionary remains available after an app restart.
- The reader remains responsive while looking up text and can still open malformed or unsupported dictionary data with a clear error.
- Source EPUBs and source-library directories remain unchanged.

## Phase 12 — Reliability, performance, and polish

1. Add structured logging for scan/extraction failures.
2. Add a recovery path for missing files:
   - mark unavailable
   - do not delete metadata automatically
   - allow a rescan to restore it if the path returns
3. Add database backup/export and import:
   - export catalog metadata and overrides
   - never export source EPUBs unless a future feature explicitly adds that
   - keep the cover cache separate and regenerable; document that it is not required for catalog backup
4. Add an onboarding flow:
   - explain that the app reads the selected folders
   - explicitly state it does not alter book files
5. Add a README covering:
   - setup
   - architecture
   - privacy model
   - database location
   - rescanning behavior
   - Japanese normalization/search behavior
   - cover-cache location, its separation from source libraries, and safe regeneration/cleanup behavior
6. Build a Windows release artifact.

Final acceptance criteria:
- The application works fully offline.
- It never modifies source EPUBs or source directories.
- It can scan deeply nested Japanese folder structures.
- It remains responsive with a very large library.
- Manual corrections persist across rescans and app restarts.
- The project has automated tests for the database, scanning, normalization, metadata fallback, romaji/reading search, and volume-detection logic.

Important implementation rules:
- Prefer small, reviewable commits or checkpoints.
- Do not introduce cloud sync, user accounts, analytics, or network calls.
- Do not implement OCR or book-content full-text search in the first version.
- Do not infer or overwrite series metadata without an explicit user action.
- Before each destructive database migration or storage cleanup action, explain the impact and provide a backup path.
