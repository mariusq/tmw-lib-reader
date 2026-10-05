# TMW EPUB Library — agent guide

This is the canonical guide for Windows and Android work. Read the safety rules,
then the sections relevant to the request. Update this file when behavior changes;
do not create new phase plans or handoff instruction files.

Detailed historical reports, commands, measurements and artifact hashes remain in
`docs/documentation-before-consolidation.zip`. Consult individual entries only when
needed. Archived plans are historical evidence, not instructions to restart work.
The dictionary plan stays in `dictionaryImprovementsAgents.md`; Phases 1–3's shared
lookup contracts, local format-3 Yomitan importer and bounded chunk/deinflection engine
are implemented, with evidence in `docs/dictionary-improvements.md`. Phase 4 reader integration
and rich rendering and Phase 5 hardening are implemented; the documented device/real-JMdict
acceptance checks remain pending;
its references to former Android instruction files mean this guide's Android sections.

## Mandatory boundaries

- Source EPUBs and directories are read-only. Never modify, rename, move, delete
  or upload them to third-party storage. The library is `F:\tmw collection`.
  Explicit user-authorized exception: desktop Book Details provides DELETE BOOK
  at the bottom with confirmation of source EPUB and catalog/user-data removal.
  It backs up the catalog and existing EPUB to app-data `backups/deleted-book-*`
  first, deletes only the selected regular EPUB, and refuses deletion during scans.
  Catalog deletion syncs to Android as deleted/unavailable; downloaded phone copies,
  progress and history text remain, while saved-passage deletions sync normally.
  This exception never authorizes agents to delete real books for testing.
  2026-10-05 deletion checkpoint: approximately 6 minutes development and 1 minute
  validation, including build-lock waiting. Desktop TypeScript, targeted lint,
  frontend build and catalog deletion/delta regression passed. Native executable
  packaging and interactive Windows/physical Android acceptance remain pending.
- `testLibrary/` is read-only and Git-ignored. Never commit it. Use generated,
  isolated temporary fixtures rather than changing real books for testing.
- Folder and filename conventions are inconsistent, never authoritative metadata.
  SQLite stores extracted metadata and user overrides; user edits always win.
- Preserve Japanese Unicode/NFKC support, tags, collections, reading progress,
  passages, lookup history and unavailable-book recovery.
- Never infer or save series assignments without explicit user action.
- Catalog databases stay in app-local data on the system drive. Desktop covers
  support a separate user-selected HDD cache, e.g. `F:\tmw-browser-cache\covers`.
- Cache directories must neither contain nor sit inside any source-library root.
  No databases, thumbnails, logs or other managed files belong in source folders.
- Cover deletion/regeneration must not lose durable catalog data or source files.
- Keep the app local-first and offline-capable. No accounts, analytics, cloud
  storage/sync, OCR or book-content full-text search.
- Network requests and external API integrations are allowed when relevant to the
  requested feature; preserve offline reading and lookup where already supported.
- Android library transfers use the authenticated private PC connection and read-only
  phone copies. Never public hosting, Funnel, router forwarding or folder sharing.
- This is private personal use: no app-store/publication or terms-acceptance work.
  Preserve dependency/dictionary notices under About/Settings; data licenses differ
  from application-code licenses.
- Preserve dirty/untracked and unrelated work. Prefer small reviewable checkpoints.
  Keep EPUBs, dictionaries, databases, credentials, keys, binaries and captures ignored.
- Before destructive migration or storage cleanup, explain impact and provide a
  backup path. Prefer additive/versioned changes; never exchange live SQLite files.

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
mandatory. Record approximate development/validation time at meaningful checkpoints.

## Current status

Windows original features and optimization phases 1–7 are complete. Android
phases 1–7, including 5.5 history sync, are implemented. Preserve their behavior;
do not restart phase plans or start new Android features without a request.

On 2026-10-04 the user confirmed physical-phone functionality through Phase 5.5,
including progress/note sync, pairing/revocation, resolved downloads and previous
updates retaining copies/progress/offline queues. Earlier pending lists for those
phases are superseded. Phone model, OS version and measured timings are unknown.

Phase 6 reader revisions and Phase 7 export picker still need physical acceptance.
Builds and fixtures do not establish device acceptance. Latest packaged Android
version is `com.tmw.companion` 0.7.1 / code 7001, ARM64, min API 24 / target 36.
The 2026-10-05 ARM64 package includes Show covers and the five reader palettes.
Signature, alignment and package metadata were verified; physical palette acceptance
remains pending.

Remaining work is request-only: the separate dictionary plan, reading activity,
automatic phone recovery import, correct journal compaction, phone metadata/tag/
collection editing, true EPUB streaming and global phone-history cross-book jumps.

## Architecture and tooling

Use Tauri v2, React/TypeScript/Vite, Rust, SQLite and Tailwind CSS. Rust handles
EPUB extraction where practical; epub.js handles reading. UI code never executes SQL.

| Area | Location |
| --- | --- |
| Windows frontend/backend | `src/`, `src-tauri/` |
| Android frontend/backend | `apps/android/src/`, `apps/android/src-tauri/` |
| Tracked native Android project | `apps/android/src-tauri/gen/android/` |
| Shared tokenizer/readings/JMdict importer | `crates/japanese-core/` |
| Dictionary provisioning | `crates/dictionary-build/` |
| Shared text/context/rendition/navigation | `packages/reader-core/` |

Both apps consume extracted shared implementations. Audit before extracting more;
keep platform gestures, commands, storage and migrations app-specific. Avoid
speculative repository reorganization. Use the root npm workspace lockfile;
Rust apps retain separate manifests, locks, targets and configuration.

Npm is healthy. For restricted per-user shim access use the system launcher below.
esbuild parent-directory failures are sandbox issues, not broken installations;
Vite `--configLoader runner` has worked. Select relevant checks, not every command.

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

Windows executable: `src-tauri/target/release/tmw-epub-library.exe`; installers:
`src-tauri/target/release/bundle/`. Plain cargo release can still use localhost.
Preserve running locked executables/live catalogs; use an alternate build filename
rather than forced shutdown solely for validation.

## Windows storage and performance contracts

`catalog.sqlite3` lives in Tauri app data for `com.tmw.epublibrary`, with WAL and
foreign keys. Local JSONL logs are `logs/tmw-library.jsonl`; never log EPUB contents.
Settings exports consistent standalone SQLite backups, including durable user state
but no EPUB bytes. Restore validates and writes a pre-restore backup in app-data
`backups/`; older catalogs migrate forward. Covers are separately regenerable.

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

Committed books appear during import. Cancellation safely preserves completed
batches and abandons reconciliation. Reject same-root concurrent scans; different
roots may run with serialized writes. Distinct discovery/extraction/indexing
progress must not cause repeated full-view refreshes; coalesce once per second.

Startup uses versioned index/completeness checks, not unconditional rebuilds.
Catalog edits update derived search documents transactionally. Manual rebuild is
progress-reporting/cancellable and rolls back atomically. Reuse the cached tokenizer;
empty values skip analysis and kana uses deterministic reading derivation.

Search uses normalized FTS5 trigrams for queries >=3 characters, short-query
fallback otherwise. Search metadata, filenames, folders and tags, never contents.
Readings/romaji are assistive and cannot overwrite display metadata. Preserve
query-plan coverage, bounded pages (backend <=200, desktop 80), 300 ms debounce,
stale-response guards, one next-page request, lazy covers and rendering containment.

Desktop covers: JPEG quality 82, <=320×480, aspect preserved, no upscaling,
transparent pixels white, input <=64 MiB. Encode a unique cache-local temporary
file, flush then atomically rename. Failures preserve metadata and previous valid
entries, clean temps and report per-book errors. Unchanged covers are not rewritten.

Structured timers cover discovery, writes, parsing, covers, readings, indexing
and reconciliation plus total/counts/failures/throughput. Nested or overlapping
worker timings must not be summed as wall time. Detailed baselines are archived.
At 10k rows, Phase 1 -> Phase 7: initial catalog 41545 -> 622.8 ms, unchanged
84.7 -> 34.9 ms, index rebuild 35602 -> 24760 ms, startup 53807 -> 6.1 ms.
These generated fixtures are not real HDD extraction throughput claims.

Opt-in Rust benchmarks: `performance_baseline` (single test thread),
`performance_phase6_queries_10000_and_100000_rows` (release),
`history_retention_benchmark`, `resume_large_catalog_benchmark` and
`smart_shelf_large_catalog_benchmark`; append `-- --ignored --nocapture`.
Preserve rollback, cancellation, index equivalence, unavailable recovery,
duplicate prevention, backup compatibility and interrupted-cache regressions.

## Reading and user-data behavior

Both readers provide a collapsible Go to chapter picker using the EPUB's own
table of contents (including nested entries and fragment anchors). Desktop places
it in the reader toolbar as a floating dropdown that does not resize the toolbar,
closing on selection, outside click or Escape; Android places it inside Reading controls. Desktop
theme selects and options use the active palette and native light/dark color scheme.
Navigation uses the existing rendition/progress pipeline; books without a TOC show
an empty state. External TOC destinations are not navigable.
Desktop reader controls share palette-aware borders, heights and focus states;
The desktop header shows title and estimated progress without the source-storage
explanation. Controls have hover hints; size buttons disable at supported limits
and history/passage buttons expose expanded state for accessibility.
the reader status selector uses a compact accessible label, while catalog status
controls retain their existing layout. Toolbar/footer actions wrap on narrow windows
and text size controls are grouped. 2026-10-05 design checkpoint: approximately
2 minutes implementation and 20 seconds validation; TypeScript, targeted lint,
reader/picker tests and frontend build passed. Running-app visual acceptance pending.
2026-10-05 chapter picker checkpoint: approximately 4 minutes implementation and
1 minute validation. Both TypeScript checks, both frontend builds and 12 targeted
tests passed; targeted lint has no errors and the existing Android cleanup warning.
APK packaging and physical-device chapter navigation remain unverified.

Continue Reading shows up to 12 recent unfinished/unpaused books. Recency updates
only after successful display. Removing from recent retains position/status;
reopening adds back. Do not infer percentages or completion from final-page visits.
Statuses are Unset, Want to read, Reading, Paused and Finished; opening promotes
only Unset/Want to read. Preserve explicit completion dates and Paused/Finished.
Smart shelves store versioned validated dynamic filters, never executable SQL.

Desktop reader uses local assets, SQLite CFI progress and local font/theme.
Clicked-text lookup on both platforms uses the bounded shared chunk/deinflection
engine in `crates/japanese-core/src/chunk_lookup.rs`, with Unicode scalar offsets
and end-exclusive original matched spans. Engine v2 uses all 889 pinned Yomitan
Japanese suffix/whole-word rules and 22 hierarchical conditions (commit
`77e200428902abf4fa48284df92da7af3dcb4162`), replacing the handwritten subset.
Vendored sources, offline generation/comparison scripts and GPL-3.0-or-later
notices are retained; desktop Settings and Android notices include attribution/full
GPL text. No expressive internal-small-tsu normalization is applied. Preserve
512 candidates, 4096 queued/processed states, depth 4, indexed rule reuse and
round-robin expansion across original spans. Generated upstream comparison checks
cover 29 forms, not universal parity or real-device acceptance.
Desktop reader/manual lookup uses enabled
imported Yomitan dictionaries exclusively; with no term source enabled, import/enable
ZIP dictionaries in Settings. Android combines imports with bundled offline JMdict.
Both use blocking workers and retain gesture/stale-response guards. Legacy desktop
JMdict rows remain for old saved-history resolution; metadata readings/search still
use the cached tokenizer independently. Legacy desktop import/lookup commands and UI
are removed; no catalog or legacy dictionary rows are deleted.

Imported dictionaries remain in app-local `yomitan-v1/dictionaries.sqlite3` version 1;
an additive indexed `term_metadata` table supports format-3 frequency/pitch banks,
including metadata-only sources. Old imports need explicit replacement to import
previously skipped metadata. No catalog/bundled dictionary migration occurs.
Shared safe React rendering preserves supported structured text, ruby, tables,
lists, tags, source grouping and alternate readings. Dictionary links are disabled;
unsupported elements/assets are visible placeholders. Ranked-result local images
are encoded as validated data URLs, capped at 1 MiB each/3 MiB total input bytes,
16 references per entry; metadata is capped at 64 rows per entry/512 KiB total.
Both Settings screens retain read-only ZIP selection, enable/priority, explicit
replacement and removal. Imports remain transactional and offline.

Desktop modifier lookup remains Alt by default or Ctrl; Android keeps touch/edge
priorities. Results preserve original scalar spans, sentence context and CFI.
Successful intentional history stores portable source/revision and headword/reading,
never imported numeric IDs; long source identities use shared SHA-256 identifiers
within existing v3 wire limits. Ambiguous desktop results remain unresolved.
Dictionary failures preserve existing usable stores. Phase 4 evidence and remaining
physical-device, real imported-JMdict and interactive restart checks are documented.

Passages preserve surface/headword/reading, sentence <=4000, note <=2000 and
version-checked CFI. Duplicate word/anchor saves reuse records without losing edits.
History records successful intentional lookups only, not failed/stale events;
manual queries do not inherit clicked context. Preserve ambiguity/reading identity,
10k retention, 50-row pages and independent recording/clear controls. Missing or
changed sources retain text but refuse unsafe jumps. Rescans/backups retain data.

Reader palettes on both platforms are Light, Warm paper, Sepia, Dusk and Dark,
stored device-locally as `tmw-reader-theme`. Existing desktop choices persist;
Android defaults to Warm paper; phone recovery exports include the choice. Palette changes apply in place without navigation;
fixed-layout EPUB content retains its original styling.

Show covers is device-local `tmw-show-covers`, default on; off displays a generic default cover instead of the actual image. Android skips cover
requests while hidden; toggling never regenerates or deletes cached covers. No flashcards/SRS/mastery/study queues;
the user uses Jiten separately. Saved words are lightweight passage bookmarks.

## Android build and update safeguards

SDK: `C:\Users\Marius\Desktop\android-tools\SDK`; JDK:
`C:\Users\Marius\Desktop\android-tools\jdk-21.0.12.1+1`; NDK 27.2.12479018.
Reuse `TMW_Test` (Android 16/API 36, x86-64, 4KB pages); discover actual adb serial.
Never recreate/wipe/init Android, uninstall, clear app data or replace signing keys.
Use `Android.ps1` for the Windows symlink restriction; choose `aarch64` for phone
or `x86_64` for emulator. Default builds both, so select only the needed ABI.

Established key: `%USERPROFILE%\.android\debug.keystore`, alias `androiddebugkey`.
Certificate SHA-256: `606342f7138cbc6eaee5b672b9d9961cc9ca70d2e3f4eacb53cbd4a47f543ca8`.
The TestSigning launcher refuses missing/mismatched keys; never regenerate to fix it.
Update in place with the package installer or `adb install -r`, no downgrade flags.
Back up the exact key to two separate encrypted offline locations and compare
file hashes; preserve alias/password/build details privately. Backup remains pending.

APK: `apps/android/src-tauri/gen/android/app/build/outputs/apk/arm64/release/app-arm64-release.apk`.
Verify selected APK with apksigner, zipalign and aapt. Alignment checks do not prove
16KB runtime. Existing native test wrappers can test Kotlin storage without a
second Rust ABI build, but do not validate new ARM64 frontend behavior.
Preserve dependency/data notices and `Generate-Notices.ps1`; inventory is
`docs/android/rust-licenses.txt`. IPADIC notices and JMdict attribution/ShareAlike
are separate obligations; check upstream licenses before new dictionary reuse.

## Android reader and storage

Bundled indexed JMdict is versioned and provisioned atomically on first use;
blocking worker/mutex, no UI-thread import. Preserve bundled fallback and provenance.
Manual proof EPUB/positions stay in IndexedDB/content-hash-keyed localStorage,
separate from native downloaded copies; never guess their catalog association.

Current reflowable reader is horizontal/LTR, preserves spine/ruby; fixed-layout
untouched. Keep Previous/Menu/Next bottom bar and blank 32px edge taps; text glyphs
always take lookup priority. Translate iframe coordinates to the visible pane.
Scroll/movement/long-press guards, stale lookups, dialog Back/focus, native insets,
CFI-preserving reflow and lifecycle checkpoints must survive changes.
Archive inflation uses a bounded worker; never silently fall back to UI-thread
inflation. DOM/layout remains WebView. Abrupt death cannot guarantee unacked saves.

Native storage: `files/tmw-mobile/{catalog.sqlite3,books/,covers/}`. Catalog pages
and checkpoints commit together; staged generations publish atomically. Interrupted
refresh retains the previous complete view; namespaces/copies/user state survive.
Browse pages 25, debounce 250 ms, stale guards; refresh on job completion only.
Downloads use one bounded worker, temp/length/SHA/ZIP-CRC validation/fsync/atomic
publication and crash recovery. Reader input <=64,000,000 bytes; server 512MB limit
is not a supported-reader promise. Removing copies never removes durable user data.

All standalone mobile thumbnails and temporary bytes share one LRU budget:
100,000,000 bytes default, adjustable/disableable. Visible-only bounded fetching,
two workers/24 queued, JPEG 75 <=240×360, atomic writes and cheap placeholders.
Embedded EPUB covers work offline; extracted copies count toward the same budget.
Clear/evict covers only, never books, catalog, credentials, user data or PC files.

## Private connection and synchronization

Service is opt-in, disabled each desktop start, loopback `127.0.0.1:47831`.
Private Tailscale Serve only; inspect existing configuration before changing it.
PC/app/drive/tailnet must be available for requests, not offline reading.
Phone accepts validated root HTTPS `.ts.net` addresses on 443; no redirects,
cleartext or certificate bypass. Credentials remain native Keystore AES-GCM,
never JavaScript/EPUB/logs. Forget local connection differs from desktop revocation.

Public book IDs/namespace are independent of numeric IDs and phone paths.
Same-path replacement keeps identity but changes actual SHA-256 content version;
rename creates new identity. Authorized ID-only routes reject caller paths and
escaping symlinks. Keep accepted Windows sockets blocking and transfers outside DB locks.

Preserve v1 pairing/download, v2 fenced keyset snapshot/delta and v3 user-sync
compatibility. Invalidate changed/expired snapshots rather than mix pages;
unknown strong versions remain null. Check protocol versions before applying pages
or removing queue acknowledgments; incompatibility retains all local data.

Local user writes and outgoing operations commit together. Server arrival/commit
order gives progress last-write-wins, including backwards movement, without clocks.
Device sequences and durable receipts make delivery ordered/idempotent. Passage
field patches retain absent fields, null clears; deletes are terminal tombstones.
Retain acknowledged overlays until pull cursor reaches server highWater.
Restore resets epochs/canonical state but retains pending operations and archives
old provisional acknowledgments. New anchors require verified matching content;
existing text edits/deletes remain possible without source availability.

History uses stable event identities, intentional repeats distinct from retries,
portable dictionary references rather than numeric entry IDs, terminal deletions
and device-local recording preferences. Disabled recording still accepts sync.
Clear removes known events, not unknown offline/future arrivals; retention is not
a bound on uncompact journal storage. Preserve history capability backfill.
Sync retries automatically while running; killed/suspended OS scheduling is not promised.

## Recovery and remaining acceptance

Settings exports unencrypted offline `tmw-phone-recovery` v1 ZIP: complete manifest,
JSON tables and reader settings. Includes durable user state, pending/rejected/
provisional operations and sync cursors; excludes EPUBs, covers, dictionaries and
credentials. Copy securely off-phone. No automatic restore UI; never blindly replay
old queues or apply anchors to different versions. Partial ZIPs are not valid backups.
Uninstall/data clear loses phone-only/unsynced data; Android automatic backup is disabled.

Pending: real-phone reader layout/edges/rotation/resume/offline position, export
picker/ZIP opening and encrypted key backup. Minimum API/16KB runtime, TalkBack,
large-book memory, battery, HDD/tailnet throughput and export fault injection remain
unverified. Historical fixtures/builds/user acceptance are archived; reuse evidence.

Reading activity remains request-only: optional daily/weekly focused active time,
conservative idle/sleep/background handling, no double counting, monotonic durations,
bounded persistence and calendar/timezone correctness. Clear/toggle preserves other
user state; include activity in backups. No proficiency scores/streaks/mandatory goals.

2026-10-05 reader palette checkpoint: approximately 5 minutes implementation and
3 minutes validation/package work (overlapping checks counted once). Both TypeScript
checks, frontend builds and 17 targeted tests passed; targeted lint has no errors
and one existing Android reader cleanup warning. No physical-device smoke test.


2026-10-05 dictionary Stage 5 hardening checkpoint: score-ranked covering indexes
fix late-sense truncation while retaining caps; existing stores upgrade additively.
37 shared tests and 21 focused frontend tests pass; generated 3-source/60k-term
host import/reopen passes. Both TypeScript/frontend builds and host Rust checks pass.
Final ARM64 0.7.1/code 7001 package rebuilt with signature/alignment/metadata verified.
Approximately 6 minutes development and 4 minutes validation/package work.
Physical dictionary acceptance, real imported JMdict, Android large-import memory
and interactive desktop restart/offline remain open; Windows executable not rebuilt.

2026-10-05 writing direction checkpoint: both readers provide device-local
`tmw-reader-writing-mode`: Horizontal left to right (`horizontal-tb`, default)
or Vertical right to left (`vertical-rl`, top-to-bottom glyphs, right-to-left columns).
Reflowable books use LTR paginated flow; mode changes recreate pagination at the
current CFI. Vertical controls use left/next and right/previous; horizontal uses
right/next and left/previous. Android blank-edge taps follow the selected page progression;
text lookup retains priority. Fixed-layout document styling stays intact.
Phone recovery includes the preference. Approximately 7 minutes development and
1 minute validation. TypeScript checks, targeted tests and frontend builds checked;
running-app visual/page-boundary acceptance and native EXE/APK packaging pending.

2026-10-05 correction: user intended vertical right to left. Left advances and
right retreats in vertical mode; horizontal retains right/next and left/previous.
The prior vertical-lr preference resolves to vertical-rl. Approximately 2 minutes
development and 30 seconds validation; native packaging/device acceptance pending.

2026-10-05 reader layout correction: reflowable renditions use a single page
(no automatic two-page spreads); desktop horizontal pages have a centered bounded
reading width. Reflowable illustrations fit the page with preserved aspect ratio.
Desktop footer groups dictionary controls and paired page navigation; horizontal
Previous/Next sit left/right, vertical Next/Previous sit left/right. Approximately
3 minutes implementation, under 1 minute validation. Native packaging and running
visual acceptance remain pending.

Layout checkpoint checks: both TypeScript checks, 14 targeted tests and desktop
frontend build passed. Updated desktop mock includes the spine presentation hook.

Both readers offer device-local Hide furigana (`tmw-reader-hide-furigana`, default off).
Reflowable EPUB ruby annotations (`rt`/`rp`) are hidden with CSS, preserving base text,
lookup offsets and CFIs; toggling recreates pagination at the current position. Fixed-layout
books retain publication styling. Phone recovery exports include this preference.

2026-10-05 hidden-cover/furigana checkpoint: approximately 4 minutes implementation and
1 minute validation. 13 focused tests, both TypeScript checks and both frontend builds
passed; Android reader tests repeated after the rendition-hook adjustment. Targeted lint
has no errors and the existing Android cleanup warning. Native EXE/APK packaging and
running-app visual/device acceptance remain pending.

2026-10-05 desktop library declutter: search, reading-status dropdown and sort share
one row. Additional filters collapse under Filters with an active count and clear action;
cover visibility is in General Settings beside Cover cache; the smart-shelf editor
is under Saved shelves. There is no Display collapsible in the library.
The metadata-attention view is the single attention control; the duplicate checkbox
and six-button status row are removed. Existing filters/shelf storage stay intact.
Approximately 4 minutes implementation and under 1 minute validation. TypeScript,
targeted lint, three library tests and frontend build checked. Native executable
packaging and running-app visual acceptance remain pending.

2026-10-05 cover-control relocation: Show covers moved from the library to General
Settings beside Cover cache, retaining the same device-local preference. Approximately
1 minute implementation and 15 seconds validation; TypeScript and library tests passed.
Native executable packaging and visual acceptance remain pending.

2026-10-05 dictionary management: desktop outside clicks (including EPUB frames)
close lookup; Android lookup backdrop taps close its dialog through the existing
Back/history lifecycle. Both management screens explain disabling without removal
and offer a device-local per-import result limit, 1–256 entries across all returned
reading groups per lookup; 0 retains existing engine caps. Limits are applied after
ranking and before image/metadata hydration. Dictionary storage upgrades additively
with a default-zero result_limit column; source ZIPs and catalog data are untouched.
Android bundled JMdict remains the offline fallback, outside imported-source controls.
Approximately 8 minutes development and 2 minutes validation. Both TypeScript and
Rust host checks, focused UI tests and frontend builds checked; existing Android
cleanup lint warning remains. Shared Rust limit fixture execution blocked by unavailable
Lindera dictionary build assets. Native packaging and desktop/phone visual acceptance
remain pending.

2026-10-05 desktop progress correction: the displayed percentage now uses epub.js
character-based CFI locations (150-character samples) across the whole linear book,
rather than equal chapter weights and current pagination. Indexing runs sequentially
in the background; percentage stays hidden until ready or if indexing fails.
Fixed-layout/image-only books omit the text percentage. Saved CFIs, reading status
and source EPUBs are unchanged. Approximately 3 minutes development and 30 seconds
validation: TypeScript, six focused tests and frontend build passed (runner config
loader used for sandbox restrictions). Native executable packaging and real-book
visual acceptance remain pending.

2026-10-05 desktop search rebuild freeze correction: Windows recorded AppHangB1;
manual rebuild previously ran synchronously on the window thread. The command now
awaits a blocking background worker, preserving progress, single-rebuild exclusion,
cancellation and atomic rollback. Worker join failures clear the active controller
and return an error. Approximately 3 minutes investigation/implementation and
30 seconds validation (including build-lock waiting); Rust compilation and the
focused cancellation/reading-alias/index-equivalence test passed. Full-library
interactive responsiveness remains unverified; standalone EXE not packaged.

2026-10-05 desktop romaji spacing correction: queries compact normalized romaji;
FTS retains spaced and joined spellings for title/author/series/filename/aliases.
Short-query fallback and title ranking use compact romaji too. Catalog migration
19 transactionally backfills FTS from existing derived readings without EPUB reads
or tokenizer reprocessing; user metadata/readings stay intact. Approximately
3 minutes implementation and 15 seconds validation. Two focused Rust tests passed,
covering spaced/joined queries, legacy-index backfill, rebuild and title ranking.
Running-app visual acceptance and standalone EXE packaging remain unverified.

2026-10-05 requested ARM64 APK rebuild: current Android frontend/native changes packaged
for com.tmw.companion 0.7.1/code 7001 using the established signing certificate.
Signature, 16KB ZIP alignment and ARM64/min API 24/target 36 metadata verified.
Approximately 4 minutes build work and under 1 minute package validation; no new
tests or device smoke test. Initial sandbox linker permission failure resolved
with toolchain access. Physical acceptance remains pending.

2026-10-05 Android dictionary/reopen correction: reproduced word-tap popup opening
on pointer-up then immediately closing on Android's retargeted click. Dismissal now
requires a gesture that began on the backdrop; gesture/stale-result/Back guards remain.
Reader menu includes Dictionary with manual offline search and import management.
Manual searches do not inherit book context or create passage/history records.
Failed last-download reopening discards its stale descriptor; warnings are dismissible
and clear on navigation or selection. No source books or durable user data are removed.
Eight focused frontend tests, Android TypeScript/build and targeted lint passed (one
existing cleanup warning). Updated TMW_Test in place with the established certificate;
final x86-64 compiled package verified actual taps, imported fixture plus bundled JMdict
results, intentional backdrop dismissal, manual search, and stale-download recovery.
Approximately 7 minutes development and 17 minutes investigation/validation/build work;
user explicitly authorized exceeding the validation budget for emulator troubleshooting.
Generated fixtures only; real imported JMdict/physical-phone acceptance and updated
ARM64 phone packaging remain pending. Emulator retains its generated proof EPUB/import.

2026-10-05 duplicate-filter correction: exact normalized-title collapsing remains
active with both candidate confidence levels, including books absent from the JSON.
Exact-title comparison considers candidate survivors to avoid suppressing the
available representative with an unavailable copy. Unchecked checkbox or Show all
copies disables both. Approximately 2 minutes development and 15 seconds validation;
focused Rust exact-title/candidate regressions passed. Native EXE remains unbuilt.


2026-10-05 mobile jump-back row: Android library front page shows the last five
successfully opened downloaded catalog books, newest first, independently of browse
filters. Recent IDs are bounded and stored per catalog namespace in native settings,
included in phone recovery exports. Successful rendition display records recency;
missing local copies are omitted and existing version-checked CFI resume is reused.
Show covers is honored. Windows changes from this request were removed.
Approximately 6 minutes implementation and 1 minute validation: Android TypeScript,
8 focused frontend tests, frontend build, Kotlin ARM64 release compilation and a
recent-order/current-generation SQL fixture passed. APK packaging and physical
phone visual acceptance remain pending.

2026-10-05 series proposal export: user requested title/author inference from the desktop catalog. Read-only SQLite/query-only metadata snapshot contains 17,412 books. Local book-series-proposals.json and matching text report propose 1,268 groups covering 8,226 records (687 high, 529 medium, 52 possible). These are review-only heuristics; no series assignments, catalog writes or EPUB access occurred. Scripts preserve original metadata, exclude missing authors, flag edition/copy ambiguity and leave unnumbered volumes unspecified. Different-title sequels and differing author credits may be missed. Approximately 5 minutes implementation and under 30 seconds validation; JSON membership/ID/count checks and sample review passed. No application behavior changed.
