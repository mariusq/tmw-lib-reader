# TMW EPUB Library — agent guide

This is the canonical guide for Windows and Android work. Read the safety rules,
then the sections relevant to the request. It records rules and contracts that
must survive changes; what each feature does for the user is described in
`FEATURES.md` (implemented features plus proposals).

Update this file when a contract changes and `FEATURES.md` when behavior
changes. Do not append dated changelog entries here: put the outcome, approximate
times and anything still unverified in the commit message or PR description, and
keep only the latest artifact hashes and open acceptance items under Current status.
Do not create new phase plans or handoff instruction files.

History: the dated checkpoint log that used to live here (timings, validation
details, older APK/EXE hashes) is in git history up to commit `a384bbe`. Older
detailed reports are in `docs/documentation-before-consolidation.zip`; archived
plans are evidence, not instructions to restart work. The dictionary plan stays in
`dictionaryImprovementsAgents.md` (Phases 1–5 implemented; evidence in
`docs/dictionary-improvements.md`); its references to former Android instruction
files mean this guide's Android sections.

## Mandatory boundaries

- Source EPUBs and directories are read-only. Never modify, rename, move, delete
  or upload them to third-party storage. The library is `F:\tmw collection`.
  User-authorized exception: desktop Book Details DELETE BOOK, with confirmation,
  first backs up the catalog and EPUB to app-data `backups/deleted-book-*`, deletes
  only the selected regular EPUB and refuses during scans. This never authorizes
  agents to delete real books for testing.
- `testLibrary/` is read-only and Git-ignored. Never commit it. Use generated,
  isolated temporary fixtures rather than real books or live catalogs.
- Folder and filename conventions are inconsistent, never authoritative metadata.
  SQLite stores extracted metadata and user overrides; user edits always win.
- Preserve Japanese Unicode/NFKC support, tags, collections, reading progress,
  passages, lookup history, activity and unavailable-book recovery.
- Never infer or save series assignments without explicit user action. Never infer
  completion or percentages from final-page visits.
- Catalog databases stay in app-local data on the system drive. Desktop covers
  support a separate user-selected HDD cache, e.g. `F:\tmw-browser-cache\covers`.
  Cache directories must neither contain nor sit inside any source-library root.
  No databases, thumbnails, logs or other managed files belong in source folders.
- Cover deletion/regeneration must not lose durable catalog data or source files.
- Local-first and offline-capable: no accounts, analytics, cloud storage/sync, OCR
  or book-content full-text search. No flashcards/SRS/mastery scores or mandatory
  goals (the user uses Jiten separately). Network requests and external APIs are
  allowed when relevant to a requested feature; preserve offline reading/lookup.
- Android transfers use only the authenticated private PC connection and read-only
  phone copies. Never public hosting, Funnel, router forwarding or folder sharing.
- Private personal use: no app-store/publication or terms-acceptance work. Preserve
  dependency/dictionary notices under About/Settings; data licenses differ from
  application-code licenses.
- Preserve dirty/untracked and unrelated work. Prefer small reviewable checkpoints.
  Keep EPUBs, dictionaries, databases, credentials, keys, binaries and captures ignored.
- Before destructive migration or storage cleanup, explain impact and provide a
  backup path. Prefer additive/versioned changes; never exchange live SQLite files.
  Never edit an already-shipped migration to fix a schema: existing catalogs have
  already applied it (see migration 023). Add a new migration instead.

## Testing budget

Validation must take at most 50% of total task time, preferably less. Count tests,
verification-only builds/APK packaging, emulator/manual checks and validation-only
waiting. Count overlapping intervals once; never pad development to meet the ratio.

Use targeted checks during implementation, then one relevant final regression,
build/package pass and brief smoke test. Repeat only for concrete failures or later
changes affecting a check. Reuse prior evidence; avoid repeated full suites,
both-ABI builds and unrelated broad emulator passes.

Documentation-only edits need a diff/readback check. If a necessary check would
exceed the budget, explain that specific remaining check and request an explicit
exception first. Report skipped/unverified work honestly. Source safety remains
mandatory. Record approximate development/validation time in the commit or PR.

Known sandbox quirks: Vitest temporary-file renames and some Gradle cache steps
fail inside the sandbox (run focused tests outside it); esbuild parent-directory
failures are sandbox issues, and Vite `--configLoader runner` works; authenticated
socket regressions need execution outside the sandbox.

## Current status

Windows original features, optimization phases 1–7 and Android phases 1–7
(including 5.5 history sync) are implemented. Preserve their behavior; do not
restart phase plans or start new features without a request. The user confirmed
physical-phone functionality through Phase 5.5 on 2026-10-04 (phone model, OS and
timings unknown).

Latest packages (2026-10-19, reading-position sync correction):
- Android `com.tmw.companion` 0.7.2 / code 7002, ARM64, min API 24 / target 36,
  established certificate, 16KB ZIP alignment verified. APK SHA-256
  `e1f008ca01dc09ad606015ab918f3bbfb1e23c1d060a4b48f042dfbe31056913`.
  Not installed on the phone by an agent.
- Windows custom-protocol release executable SHA-256
  `22a285077309f8c3e0b6c2a4466ba9b99da7ea0e4323bf571574a7a2ab8880bc`.
  The generated-cover and desktop-comfort changes merged afterwards are not in it.

Acceptance still pending (builds and fixtures do not establish it):
- Interactive Windows: generated covers, library comfort pass (grid density, hover
  Read pill, Ctrl+F), Settings categories, series navigation/layout, reader layout,
  palettes, writing modes, furigana, chapter picker, progress percentage, Home and
  activity in the running WebView, position-sync choices, dictionary restart/offline,
  full-library search-rebuild responsiveness.
- Physical phone: Phase 6 reader revisions and Phase 7 export picker/ZIP opening,
  reader layout/edges/rotation/resume/offline position, palettes, kanji glyphs,
  chapter navigation, series browsing offline, empty-catalog recovery, Home/tracker
  touch layout, real 10–30 minute pages, lock/background/sleep, private interrupted
  and offline sync, cross-device activity clear, PC-to-phone position handoff and
  dictionary with real imported JMdict.
- Unverified: minimum-API and 16KB-page runtime, TalkBack, large-book and large
  dictionary-import memory, battery, HDD/tailnet throughput, export fault injection,
  the pre-existing private API/import stress regression (returned 503 before its
  activity checks), and encrypted signing-key backup.

Request-only work: see "Already tracked" in `FEATURES.md`.

## Architecture and tooling

Use Tauri v2, React/TypeScript/Vite, Rust, SQLite and Tailwind CSS. Rust handles
EPUB extraction where practical; epub.js handles reading. UI code never executes SQL.

| Area | Location |
| --- | --- |
| Windows frontend/backend | `src/`, `src-tauri/` |
| Android frontend/backend | `apps/android/src/`, `apps/android/src-tauri/` |
| Tracked native Android project | `apps/android/src-tauri/gen/android/` |
| Shared tokenizer/readings/JMdict importer/chunk lookup | `crates/japanese-core/` |
| Dictionary provisioning | `crates/dictionary-build/` |
| Shared text/context/rendition/navigation/dictionary UI | `packages/reader-core/` |

Both apps consume extracted shared implementations. Audit before extracting more;
keep platform gestures, commands, storage and migrations app-specific. Avoid
speculative repository reorganization. Use the root npm workspace lockfile;
Rust apps retain separate manifests, locks, targets and configuration.

Npm is healthy. For restricted per-user shim access use the system launcher below.
Select relevant checks, not every command.

```powershell
& 'C:\Program Files\nodejs\npm.cmd' ci
& 'C:\Program Files\nodejs\npm.cmd' run tauri dev
& 'C:\Program Files\nodejs\npm.cmd' run lint
& 'C:\Program Files\nodejs\npm.cmd' test
& 'C:\Program Files\nodejs\npm.cmd' run build
cargo test --offline --manifest-path src-tauri/Cargo.toml --lib
# Build dist first; custom-protocol is required for a standalone executable:
cargo build --offline --manifest-path src-tauri/Cargo.toml --release --bin tmw-epub-library --features tauri/custom-protocol
& 'C:\Program Files\nodejs\npm.cmd' run tauri build
& 'C:\Program Files\nodejs\npm.cmd' exec vitest -- run apps/android/src
& 'C:\Program Files\nodejs\npm.cmd' exec tsc -- --noEmit -p apps/android/tsconfig.json
& 'C:\Program Files\nodejs\npm.cmd' exec eslint -- apps/android/src
& 'C:\Program Files\nodejs\npm.cmd' run android:check
& .\apps\android\Android.ps1 -Action Build -TestSigning -Target aarch64
```

Desktop TypeScript uses `tsconfig.app.json`. Windows executable:
`src-tauri/target/release/tmw-epub-library.exe`; installers:
`src-tauri/target/release/bundle/`. Plain cargo release can still use localhost.
Preserve running locked executables/live catalogs; use an alternate build filename
rather than forced shutdown solely for validation.

## Windows storage and performance contracts

`catalog.sqlite3` lives in Tauri app data for `com.tmw.epublibrary`, with WAL and
foreign keys. Local JSONL logs are `logs/tmw-library.jsonl`; never log EPUB contents.
Settings exports consistent standalone SQLite backups including all durable user
state (activity included) but no EPUB bytes. Restore validates and writes a
pre-restore backup in app-data `backups/`; older catalogs migrate forward. Covers
are separately regenerable.

Completed scans mark vanished paths unavailable while preserving user data;
returning same paths restore/re-extract. Failed/cancelled scans never reconcile
unseen files. Rename/move creates a new row and leaves the old unavailable.
Explicit root removal deletes associated catalog data, never source directories.

Preserve the bounded background pipeline: 128-path detection transactions,
reusable statements returning ID/changed state, temporary SQLite seen-path staging,
bounded extraction queues and serialized 32-book final metadata/index batches.
Default two extraction workers; `import_worker_count` is clamped to 1–8.
Unchanged rescans do no parsing, cover work, reading derivation or index rewrite.
Changed books get one final transactional index refresh, no provisional duplicate.
Committed books appear during import. Cancellation preserves completed batches and
abandons reconciliation. Reject same-root concurrent scans; different roots may run
with serialized writes. Coalesce progress-driven view refreshes to once per second.

Startup uses versioned index/completeness checks, not unconditional rebuilds.
Catalog edits update derived search documents transactionally. Manual rebuild runs
on a blocking background worker (never the window thread — that caused AppHangB1),
is progress-reporting/cancellable and rolls back atomically. Reuse the cached
tokenizer; empty values skip analysis and kana uses deterministic reading derivation.

Search uses normalized FTS5 trigrams for queries >=3 characters, short-query
fallback otherwise, over metadata, filenames, folders, tags and proposed series
names, never contents. Readings/romaji are assistive and cannot overwrite display
metadata; romaji is matched both spaced and compacted. Preserve query-plan
coverage, bounded pages (backend <=200, desktop 80), 300 ms debounce,
stale-response guards, one next-page request, lazy covers and rendering containment.

Desktop covers: JPEG quality 82, <=320×480, aspect preserved, no upscaling,
transparent pixels white, input <=64 MiB. Encode a unique cache-local temporary
file, flush then atomically rename. Failures preserve metadata and previous valid
entries, clean temps and report per-book errors. Unchanged covers are not rewritten.
Generated placeholder covers are render-only (no files, cache or storage).

Structured timers cover discovery, writes, parsing, covers, readings, indexing
and reconciliation plus total/counts/failures/throughput. Nested or overlapping
worker timings must not be summed as wall time. At 10k generated rows, Phase 1 ->
Phase 7: initial catalog 41545 -> 622.8 ms, unchanged 84.7 -> 34.9 ms, index
rebuild 35602 -> 24760 ms, startup 53807 -> 6.1 ms (not real HDD throughput).
Opt-in Rust benchmarks: `performance_baseline` (single test thread),
`performance_phase6_queries_10000_and_100000_rows` (release),
`history_retention_benchmark`, `resume_large_catalog_benchmark` and
`smart_shelf_large_catalog_benchmark`; append `-- --ignored --nocapture`.
Preserve rollback, cancellation, index equivalence, unavailable recovery,
duplicate prevention, backup compatibility and interrupted-cache regressions.

Series and duplicates: proposals come from bundled review-only JSON
(`scripts/generate-series-candidates.py`, metadata reports only, never EPUBs) loaded
into a connection-local temporary index keyed by path plus unchanged effective
title/author. Every connection that queries it — including read-only API
connections serving v2 catalog pages — must initialize it before its transaction.
Existing effective series labels win. Filtering and duplicate hiding precede grouped
bounded pagination. Copy grouping requires matching NFKC/whitespace/case-normalized
title, author and volume label, never volume number alone. Opt-in read-only
diagnostics use `TMW_SERIES_DIAGNOSTIC_CATALOG` and never migrate or write.

## Reading and user-data contracts

Statuses are Unset, Want to read, Reading, Paused and Finished; opening promotes
only Unset/Want to read. Preserve explicit completion dates and Paused/Finished.
Continue Reading holds up to 12 recent unfinished/unpaused books; recency updates
only after successful display; removing from recent retains position/status.
Smart shelves store versioned validated dynamic filters, never executable SQL.

Reader rendering: reflowable books use single-page LTR paginated flow; writing mode,
furigana and palette changes recreate or restyle at the current CFI without
navigation. Fixed-layout content retains publication styling. Device-local reader
keys: `tmw-reader-theme`, `tmw-reader-font-size`, `tmw-reader-writing-mode`
(legacy vertical-lr resolves to vertical-rl), `tmw-reader-hide-furigana`. Other
device-local keys: `tmw-app-theme` (desktop library scheme; never applied inside
`.desktop-reader`), `tmw-show-covers`, `tmw-library-layout`,
`tmw-consolidate-series`, `tmw-series-confidence`, `tmw-series-group-copies`.
Android recovery exports include its reader and series preferences.
Desktop progress percentage uses epub.js 150-character CFI locations; hidden until
indexed and for fixed-layout/image-only books.

Reading-position writes (both platforms):
- Only deliberate navigation dirties progress. Initial display, page-start
  rounding and layout relocation re-anchor without writing; closing an unchanged
  reader writes nothing.
- Desktop writes compare an opaque saved-position token (anchor/version, source
  metadata guards, catalog epoch) inside an immediate SQLite transaction. Writes
  serialize and coalesce; failed writes are retained for retry while open;
  deliberate backwards reading is preserved.
- Open readers check saved positions (desktop every 5 s, Android `userPrepareOpen`
  every 10 s, both on focus/visibility return; never per page turn). Checks never
  overlap; choices serialize behind polls. Clean readers follow remote positions
  without writing; dirty navigation gets Go to synced position / Keep this
  position, and once a session has navigated, remote changes keep that choice.
- Android opening of a downloaded catalog book waits (max 8 s) for serialized
  sync delivery and canonical pulls, then falls back to local progress with a
  visible warning.

Clicked-text lookup on both platforms uses the bounded shared chunk/deinflection
engine in `crates/japanese-core/src/chunk_lookup.rs` with Unicode scalar offsets
and end-exclusive original matched spans. Engine v2 uses the 889 pinned Yomitan
Japanese rules and 22 hierarchical conditions (commit
`77e200428902abf4fa48284df92da7af3dcb4162`); keep vendored sources, generation
scripts and GPL-3.0-or-later notices. No expressive small-tsu normalization.
Preserve 512 candidates, 4096 queued/processed states, depth 4, indexed rule reuse
and round-robin expansion. Desktop lookups use enabled imported Yomitan
dictionaries only; Android combines imports with bundled offline JMdict. Both use
blocking workers with gesture/stale-response guards. Legacy desktop JMdict rows
remain for old history resolution; never delete them.

Imported dictionaries live in app-local `yomitan-v1/dictionaries.sqlite3` v1 with
additive `term_metadata` and `result_limit` columns (0 = engine caps; 1–256 applied
after ranking, before hydration). Shared safe React rendering; dictionary links
disabled; images as validated data URLs capped at 1 MiB each/3 MiB total and 16 per
entry; metadata 64 rows per entry/512 KiB. Imports are transactional and offline;
failures preserve usable stores.

Passages: surface/headword/reading, sentence <=4000, note <=2000, version-checked
CFI; duplicate saves reuse records. History records successful intentional lookups
only with portable source/revision and headword/reading identities (long identities
as SHA-256 within v3 wire limits), never numeric IDs; ambiguous desktop results
stay unresolved. Preserve 10k retention, 50-row pages and independent
recording/clear controls. Missing or changed sources retain text but refuse unsafe
jumps.

Reading activity (both platforms):
- Starts after successful display; monotonic time with 10-second checkpoints using
  conservative integer boundaries capped to their representable span; 30-minute
  idle allowance; sleep/clock jumps discarded, never backfilled.
- Focus uses native window focus/lifecycle plus visibility, never DOM/iframe blur.
  Initial focus queries cannot override newer native events.
- Characters: deliberate successful forward turn after >=3 s focused dwell; ruby,
  whitespace, scripts and hidden text excluded; fixed-layout/image-only/unresolved
  pages, chapter jumps, failed/stale turns and reflow give no credit. Coverage
  ranges merge per book/content key/spine; chapters >2M UTF-16 units/100k nodes or
  >4096 ranges stop estimating. No EPUB text is persisted.
- Lookups count one successful intentional event, independent of history recording.
- Streak: >=60 s focused reading on consecutive local dates, across all history.
- Calendar buckets use local dates and split at midnight. Pause applies to every
  metric. Confirmed clear removes activity only and is catalog-wide when delivered.
- Activity sync is the separate authenticated `/v1/activity-sync`: immutable IDs,
  canonical sequence/cursor, durable pending/rejected operations, reset generations
  and per-operation origin/reset identities so old queues cannot resurrect cleared
  activity. Desktop migrations 022/023; storage failures return 503, not 400.

## Android build and update safeguards

SDK: `C:\Users\Marius\Desktop\android-tools\SDK`; JDK:
`C:\Users\Marius\Desktop\android-tools\jdk-21.0.12.1+1`; NDK 27.2.12479018.
Reuse `TMW_Test` (Android 16/API 36, x86-64, 4KB pages); discover actual adb serial.
Never recreate/wipe/init Android, uninstall, clear app data or replace signing keys.
Use `Android.ps1` for the Windows symlink restriction; choose `aarch64` for phone
or `x86_64` for emulator. Default builds both, so select only the needed ABI.
Native storage tests can use a separate fixture package (e.g.
`com.tmw.activityfixture`) on `TMW_Test` without a second Rust ABI build, but do not
validate new ARM64 frontend behavior.

Established key: `%USERPROFILE%\.android\debug.keystore`, alias `androiddebugkey`.
Certificate SHA-256: `606342f7138cbc6eaee5b672b9d9961cc9ca70d2e3f4eacb53cbd4a47f543ca8`.
The TestSigning launcher refuses missing/mismatched keys; never regenerate to fix it.
Update in place with the package installer or `adb install -r`, no downgrade flags.
Back up the exact key to two separate encrypted offline locations and compare
file hashes; preserve alias/password/build details privately. Backup remains pending.

APK: `apps/android/src-tauri/gen/android/app/build/outputs/apk/arm64/release/app-arm64-release.apk`.
Verify with apksigner, zipalign and aapt. Alignment checks do not prove 16KB runtime.
Preserve dependency/data notices and `Generate-Notices.ps1`; inventory is
`docs/android/rust-licenses.txt`. IPADIC notices and JMdict attribution/ShareAlike
are separate obligations; check upstream licenses before new dictionary reuse.

## Android reader and storage

Bundled indexed JMdict is versioned and provisioned atomically on first use;
blocking worker/mutex, no UI-thread import. Preserve bundled fallback and provenance.
Manual proof EPUB/positions stay in IndexedDB/content-hash-keyed localStorage,
separate from native downloaded copies and catalog activity; never guess their
catalog association.

Reflowable reader keeps Previous/Menu/Next bottom bar and blank 32px edge taps
following page progression; text glyphs always take lookup priority. Translate
iframe coordinates to the visible pane. Lookup dismissal requires a gesture that
began on the backdrop. Preserve scroll/movement/long-press guards, stale lookups,
dialog Back/focus, native insets, CFI-preserving reflow and lifecycle checkpoints.
App CSS and reflowable documents use the Japanese locale/font stack (not
system-ui) so publication fonts cannot select Chinese Han glyphs.
Archive inflation uses a bounded worker; never fall back to UI-thread inflation.
Abrupt death cannot guarantee unacked saves.

Native storage: `files/tmw-mobile/{catalog.sqlite3,books/,covers/}`. Catalog pages
and checkpoints commit together; staged generations publish atomically. Interrupted
refresh retains the previous complete view; namespaces/copies/user state survive.
An absent/empty active generation with a saved cursor requests a full snapshot;
empty deltas keep the current generation. A changed series revision triggers a full
staged refresh. Series grouping uses grouped aggregate joins (no window functions,
API 24) before bounded pagination; ungrouped browsing skips grouping work.
Browse pages 25, debounce 250 ms, stale guards; refresh on job completion only.
Downloads use one bounded worker, temp/length/SHA/ZIP-CRC validation/fsync/atomic
publication and crash recovery. Reader input <=64,000,000 bytes; server 512MB limit
is not a supported-reader promise. Removing copies never removes durable user data.

All standalone mobile thumbnails and temporary bytes share one LRU budget:
100,000,000 bytes default, adjustable/disableable. Visible-only bounded fetching,
two workers/24 queued, JPEG 75 <=240×360, atomic writes and cheap placeholders.
Clear/evict covers only, never books, catalog, credentials, user data or PC files.
Recent-book IDs are bounded per catalog namespace in native settings.

## Private connection and synchronization

Service starts with the desktop by default (app setting `companion_autostart`;
only "false" opts out; startup bind failures are logged, never fatal) and can be
disabled in Settings; loopback `127.0.0.1:47831`.
Private Tailscale Serve only; inspect existing configuration before changing it.
Phone accepts validated root HTTPS `.ts.net` addresses on 443; no redirects,
cleartext or certificate bypass. Credentials remain native Keystore AES-GCM,
never JavaScript/EPUB/logs. Forget local connection differs from desktop revocation.

Public book IDs/namespace are independent of numeric IDs and phone paths.
Same-path replacement keeps identity but changes actual SHA-256 content version;
rename creates new identity. Authorized ID-only routes reject caller paths and
escaping symlinks. Keep accepted Windows sockets blocking and transfers outside DB locks.

Preserve v1 pairing/download, v2 fenced keyset snapshot/delta (with versioned series
memberships and revision) and v3 user-sync compatibility; activity sync is separate.
Invalidate changed/expired snapshots rather than mix pages; unknown strong versions
remain null. Check protocol versions before applying pages or removing queue
acknowledgments; incompatibility retains all local data. Older servers stay usable
without inferred memberships; older PCs leave activity queued.

Local user writes and outgoing operations commit together. Server arrival/commit
order gives progress last-write-wins, including backwards movement, without clocks.
Device sequences and durable receipts make delivery ordered/idempotent; phone
durable user saves trigger immediate bounded delivery. Accepted nonempty phone
progress adds desktop resume membership transactionally. Passage field patches
retain absent fields, null clears; deletes are terminal tombstones. Retain
acknowledged overlays until pull cursor reaches server highWater. Restore resets
epochs/canonical state but retains pending operations and archives old provisional
acknowledgments. New anchors require verified matching content. Bounded incomplete
sync passes never claim completion.

History uses stable event identities, intentional repeats distinct from retries,
portable dictionary references, terminal deletions and device-local recording
preferences. Disabled recording still accepts sync. Clear removes known events,
not unknown offline/future arrivals; retention is not a bound on uncompacted journal
storage. Catalog deletion syncs as deleted/unavailable; phone copies, progress and
history text remain. Sync retries while running; killed/suspended OS scheduling is
not promised.

## Recovery

Phone Settings exports an unencrypted offline `tmw-phone-recovery` v1 ZIP: complete
manifest, JSON tables, reader/series settings, activity, pending/rejected/provisional
operations and sync cursors; excludes EPUBs, covers, dictionaries and credentials.
Copy securely off-phone. No automatic restore UI; never blindly replay old queues or
apply anchors to different versions. Partial ZIPs are not valid backups.
Uninstall/data clear loses phone-only/unsynced data; Android automatic backup is
disabled.
