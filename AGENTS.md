# TMW EPUB Library — agent guide

2026-10-19 desktop comfort pass: library and series grids use a responsive
auto-fill grid (min 9rem cards; ~6 columns at 1280px instead of 4) with a subtle
cover hover lift. Selection checkboxes appear on hover/focus or once any book is
selected (always on touch). Available books show a hover/focus ▶ Read pill that
opens the reader at the saved position without opening Book Details. Book Details
has a primary Read book button, dimmed backdrop, and Escape/backdrop close that
confirms before discarding unsaved corrections. Ctrl+F or / focuses library search
while browsing. A sidebar indicator shows scan progress away from Library Roots.
The batch-tag bar is localized; Home day detail shows a formatted date. Storage,
commands and reader behavior are unchanged. Approximately 15 minutes investigation/
implementation and 5 minutes validation: desktop TypeScript, targeted lint, 25
component tests and frontend build passed; mocked-backend browser preview checked
grid density, hover reveal, Ctrl+F, backdrop and Escape. Interactive Windows
acceptance and executable packaging remain pending; no commit.

2026-10-19 reading-position sync correction:

Desktop: progress saves now require deliberate navigation; layout relocations only
update the display anchor. Same-CFI token changes adopt silently; remote movement
follows automatically without a write when no deliberate local navigation is at risk.
Local navigation and atomic save rejection retain Go to synced position / Keep this
position. Choices serialize behind active polls; polls skip active choices. Re-pagination
retains the coordinator, queued retries and local navigation state. Rapid turns share
the in-flight navigation intent rather than being dropped. Once a session has navigated,
remote changes keep the explicit choice (a stale queued phone position cannot silently
undo deliberate backwards reading). 52 reader tests, desktop TypeScript
(tsconfig.app.json) and reader ESLint passed.
Approximately fifteen minutes investigation/implementation and two minutes validation/
source review. Interactive Windows acceptance and executable packaging remain pending.
No apps/, Rust, source EPUB, testLibrary, live catalog or app-data changes; no commit.

Android: only deliberate navigation dirties progress; opening/layout relocations
re-anchor without writes, including pause/resume cleanup. Catalog readers check
userPrepareOpen every ten seconds and on visible/foreground return, never per page
turn (a save must not wait on a network round trip offline). Checks do not overlap or
block page display; a turn made while a check is in flight is held until it answers.
Clean readers follow remote positions without writing; dirty navigation is held for
Go to synced position / Keep this position, retaining the latest pending CFI across
foreground reopens. Ordered saves, backwards reading, proof-copy storage and activity
tracking remain. 45 Android frontend tests and Android TypeScript passed; reader
ESLint has zero errors/two existing hook warnings.
Packaging: Windows custom-protocol release executable SHA-256
22a285077309f8c3e0b6c2a4466ba9b99da7ea0e4323bf571574a7a2ab8880bc. ARM64 0.7.2/code
7002 APK with the established certificate; signature, 16KB ZIP alignment and API 24/36
metadata verified. APK SHA-256:
e1f008ca01dc09ad606015ab918f3bbfb1e23c1d060a4b48f042dfbe31056913. About two minutes
packaging; no phone installation.
Approximately twenty minutes investigation/implementation and two minutes validation/
source review. Physical-phone acceptance and APK packaging remain pending. No native,
source EPUB, testLibrary, live catalog, signing-key or app-data changes; no commit.

2026-10-18 desktop sync conflict protection: reader writes now compare an opaque
saved-position token inside an immediate SQLite transaction. The token includes the
stored anchor/version, source metadata guards and catalog epoch; phone delivery,
source replacement and restore invalidate stale reader writes without a schema or
wire-protocol change. An open reader checks saved positions every five seconds and
on focus/visibility return, offering Go to synced position or Keep this position.
Without deliberate local navigation, synced changes now follow automatically
(2026-10-19); otherwise the choice remains. Keep rereads the latest state and checks
the token atomically when saving, so another intervening update remains protected.
Initial page display
and closing an unchanged reader do not write progress; writes serialize and coalesce
pending page turns, retain failed writes for retry while open and preserve deliberate
backwards reading. Phone durable user saves now trigger immediate bounded delivery
instead of waiting for the periodic retry. Existing offline queues/receipts remain.
Ten focused frontend regressions, one generated Rust phone-delivery/conflict/content
replacement regression, desktop TypeScript/frontend build and targeted lint passed.
ARM64 release reused the preceding frontend/Rust outputs; Kotlin/release packaging,
established certificate, API 24/36 metadata and 16KB ZIP alignment passed. APK SHA-256:
47ed51bf6d331929d7cd2b7b8c0cbf690d91480e09f587e62f19a30c86adbf2a.
No source EPUB access, live catalog writes, queue clearing or phone installation
occurred. Interactive Windows/physical-phone acceptance remains pending.
Approximately six minutes investigation/implementation and two minutes validation/
packaging, overlapping intervals counted once. Windows custom-protocol release
packaging also passed; both generated artifacts require user installation/restart.

2026-10-18 PC-to-phone reading-position correction: opening a downloaded catalog
book now waits for serialized user-sync delivery and canonical pulls before choosing
its version-matched position. The check has an eight-second wait limit; an unavailable
PC falls back to durable local progress with a visible warning. Native foreground
return after a pause reopens through the same check. Initial rendition display and
page-start rounding establish a checkpoint baseline rather than enqueueing a new
phone progress write, preventing a cached opening position from superseding newer
PC reading. Deliberate navigation, backwards movement and existing pending operations
retain their ordered last-write-wins behavior. Bounded incomplete sync passes no
longer claim completion. No queue clearing, source EPUB changes or live data writes
were used. Nine generated reader regressions and Android TypeScript/frontend build
passed; targeted lint has no errors and two existing reader hook warnings. A native
multi-page prepare-open regression was added but not executed (no emulator attached).
Physical-phone/private-transport acceptance remains pending.
Approximately six minutes investigation/implementation and four minutes validation/
packaging, including the sandbox Gradle-cache retry, with overlapping intervals
counted once. ARM64 0.7.2/code 7002 release packaging passed; established signing
certificate, API 24/36 metadata and 16KB ZIP alignment verified. No phone installation
was performed. APK SHA-256:
d40f31c1ceb42e957ff6556d34f4cac51d7d03019c0b3ab65565cc96e49fc5ca.

2026-10-14 activity HTTP 400 / existing-PC schema repair: read-only inspection of
the reported PC catalog found user_version 22 with the early activity schema,
missing activity_sync_state.cutoff_ms and activity_reset_receipts. Editing migration
022 had not upgraded already-opened catalogs. Additive migration 023 now supplies
those guards transactionally, preserving the epoch, measured activity and all other
data; it also accepts catalogs that already contain the guards. Storage failures
on the activity endpoint now report 503 rather than masquerading as invalid input.
The rebuilt Windows custom-protocol executable applies the repair on next startup;
no phone update or queue clearing is required. Approximately eight minutes
investigation/implementation and one minute validation/package work: nine focused
Rust activity checks passed, including early-schema upgrade with a generated
319-operation phone queue and authenticated HTTP delivery (the socket regression
required execution outside the sandbox). Existing production frontend output was
reused. No live catalog writes, source EPUB access or phone data changes were made
by the agent. Physical-phone retry and production startup acceptance remain pending.

2026-10-14 compact Android tracker: Home combines activity and weekly statistics in
one card. A Monday–Sunday day strip replaces the seven-row horizontal calendar;
previous/next week and Today retain the full 84-day history, with out-of-range/future
days disabled and 44px day targets retained. Time/characters/lookups use three compact
columns; reading days, explicit completions and both streaks use inline rows. Amber
activity shades match the app palette. Selected-day book breakdown and tracking/sync
actions expand on demand; paused/reset notices and pending/rejected badges remain
visible, and destructive clear still requires its existing confirmation. Storage,
tracking and private delivery contracts are unchanged.
Approximately eight minutes investigation/implementation and four minutes validation/
packaging, excluding approval waiting: two focused Home regressions, Android TypeScript,
targeted lint, frontend build and ARM64 release packaging passed. Tests used mocked
generated records only. Sandbox temporary-file/Gradle cache restrictions required
focused tests and the final Gradle step outside the sandbox. The browser could not
reach the local generated preview; visual/physical-phone acceptance remains unverified.
Updated 0.7.2/code 7002 APK retains the established certificate; ARM64, API 24/36 and
16KB ZIP alignment verified (not a 16KB runtime claim). No device installation, source
EPUB access or production-data writes occurred. APK SHA-256:
ec2491132774c27f844e02f820f658d86e92b63c7371b152cf7d1eae4fd7e1dc.

2026-10-14 activity-save/sync correction: fractional performance.now intervals could
credit 10,000ms inside 9,999ms of integer wall timestamps. The phone rejected these
with generic "Failed requirement", preventing durable recording and upload; fractional
boundaries could also fail desktop integer deserialization. Shared checkpoints now use
conservative integer boundaries and cap credit to their representable span. Native
storage also clamps older small-skew checkpoints before foreground clipping; invalid
large/sleep intervals remain rejected with a specific duration error. No rejected time
is inferred or backfilled, and saved activity/queues are retained without a migration.

"Sync saved data now" now starts activity delivery as well as progress/history delivery.
Home has "Sync activity now"; Home and Settings show activity pending/rejected counts
and its transport status separately. Exhausting a bounded delivery pass no longer reports
completion, and an unpaired device reports local-only activity. Android update is 0.7.2 /
code 7002 with the established signing identity; the Windows build includes the same
shared checkpoint fix. Focused clock/reader/Home/sync UI and isolated native storage/retry
regressions cover the skewed checkpoint through save, interruption, retry and canonical
replay. Physical-phone confirmation remains necessary; old unsaved checkpoints cannot be
reconstructed safely. Approximately seven minutes investigation/implementation and three
minutes validation/package work (overlapping intervals counted once): 18 focused frontend
checks and six isolated native storage/recovery instrumentation checks passed; both
TypeScript/frontend production builds, targeted lint and Windows custom-protocol/ARM64
release builds passed. APK 0.7.2/code 7002, ARM64, min/target API 24/36, established
certificate and 16KB ZIP alignment verified. Tests used generated in-memory fixtures on
TMW_Test's separate test package, never real books or production app data. The queued,
normalized checkpoint's request shape and duration were checked through interrupted
mock transport, retry and canonical replay; this does not establish physical PC/phone
transport acceptance. Source files, app data, signing keys and existing queues were kept.
APK SHA-256: d24b8dda0637093b9b8a5b437b50c00ac09afb504eb666508dce79cd0ae6544f;
Windows SHA-256: d7e314ee11939f08b23bc741702e259e280d5908f9e89fa11089b3ff8251492c.


2026-10-14 Android Home and combined activity: Android opens on compact Home with
Continue Reading/Resume, up to four recent rows (the library retains twelve), a
six-book Want to read shelf linking to the filtered library, weekly time/estimated
characters/successful lookups/reading days/explicit completions and all-history
current/longest streaks. The selectable time/characters/lookups calendar shows twelve
scrollable weeks with 44px day buttons and selected-day totals/books. Display settings
remain local. The next catalog refresh of an existing phone catalog performs one
atomic full metadata refresh to obtain explicit completion dates; dates are never inferred.

Phone tracking starts only after successful display, samples monotonic time at ten-second
checkpoints with a bounded six-write queue/retry and the shared thirty-minute idle allowance.
Native resumed/window-focus lifecycle plus visibility govern activity, never iframe blur.
Native foreground ranges additionally clip delayed WebView pause checkpoints. Sleep/clock
jumps are discarded. Deliberate successful forward turns require three seconds of focused
dwell; failed/stale turns, chapter jumps and reflow invalidate credit. Shared bounded scalar
indexes exclude ruby annotations, whitespace, scripts and hidden publication CSS. Fixed-layout,
image-only and unresolved pages omit estimates. Strong downloaded versions are validated;
coverage merges prevent reread/overlap credit. Successful intentional clicked/manual lookup
events are independent of history preferences, with stale/empty results excluded. Native
tracking pause applies to every metric. Proof-copy activity stays separate from catalog
identities and is never guessed into private sync.

Authenticated /v1/activity-sync is separate from existing catalog/user-sync protocols.
Additive desktop migration 022 and native activity tables retain immutable IDs, canonical
sequence/cursor, durable pending/rejected operations, coverage and reset generations.
PC timelines union overlapping device intervals across books; character coverage unions
per book/version/spine. Verified SHA-256 promotion preserves metadata-guarded coverage
without new credit. Measured desktop historical totals migrate as baselines, never invented
intervals or lookups. Uploads omit redundant titles; downloads are bounded by count and
encoded size. Canonical records are applied incrementally; only the pending overlay replays.
Older PCs leave activity queued. A confirmed clear is catalog-wide when delivered; native
session tokens and per-operation origin/reset identities prevent late results and old queues,
including never-synced multi-page queues, from resurrecting cleared activity. New activity
after an offline phone clear retains its reset identity across upload pages. Concurrent newer
PC resets supersede older queued clears. Restore changes the activity generation/cursor;
activity/queues/reset state are included in desktop backups and phone recovery ZIPs. Clear
preserves books, downloads, positions, passages and lookup history. Abrupt termination can
lose the final unacknowledged checkpoint. Desktop Home refreshes combined totals while visible.

Approximately thirty minutes implementation/investigation and eleven minutes validation/
packaging, including failed wrapper attempts and overlapping intervals counted once:
37 distinct focused frontend regressions, seven Rust activity/migration/backup checks,
one authenticated activity HTTP smoke check and six isolated Android storage/recovery
instrumentation checks passed. Native tests used a separate com.tmw.activityfixture package
on existing TMW_Test, in-memory fixtures and no second Rust ABI. The broader pre-existing
private API/import stress regression returned 503 on its v1 catalog request before activity
checks; it remains unverified rather than being reported as passed. Desktop/Android
TypeScript and frontend builds, targeted lint (no errors; existing reader dependency/cleanup
warnings), Windows custom-protocol release and established-key ARM64 release packaging
passed. ARM64 com.tmw.companion 0.7.1/code 7001, API 24/36, certificate and 16KB ZIP
alignment were verified. Final APK SHA-256:
14f02171e5748492e6a9672e6e664d27bedf9a5da74b81695b41be70b41c728e;
Windows executable SHA-256:
0736618b76ceed77abfc112229b2ad71aede5be19c9831641648d81f9555d4cf.
Physical-phone Home/touch layout, real 10–30 minute pages, lock/background/sleep,
private interrupted/offline sync and cross-device clear acceptance remain pending;
16KB runtime is not established by ZIP alignment. No live catalog, source EPUB, app data
clear/uninstall or signing-key replacement was used. Earlier phone-activity pending notes
are historical and superseded by this implementation.


2026-10-13 fullscreen Home cleanup: removed explanatory character/streak/calendar/idle
copy, retaining paused state and destructive-clear confirmation. Streak now sits in the
weekly stats panel; heatmap selector shares the calendar header with its accessible label
and bounded width. Home width/spacing and heatmap cells are tightened; recent-book rows
no longer stretch a single card across the panel. Approximately 2 minutes editing and
under 1 minute validation/package work: frontend and Windows release builds passed.
Interactive acceptance remains pending.

2026-10-13 compact Home: recent books use horizontal 64×96 cover/text rows; header
actions share the heading row. Home margins/panel spacing are reduced and weekly
metrics use a compact responsive grid. Library layouts and reader behavior are retained.
Approximately 2 minutes implementation and under 1 minute validation/package work:
TypeScript/frontend and Windows release builds passed; interactive acceptance pending.

2026-10-13 activity focus correction: DOM window blur can mean focus entered the EPUB
iframe rather than the application lost focus. Desktop reading activity now uses native
Tauri window focus with visibility guards and document-focus fallback for browser-only
contexts. Initial focus queries cannot override newer native focus events; subscriptions
are disposed safely. The 30-minute idle/sleep/overlap guards are retained. Approximately
3 minutes investigation/implementation and under 1 minute validation/package work:
focused activity/reader tests, TypeScript, targeted lint, frontend and Windows release
builds passed. Interactive WebView acceptance remains pending; missed historical time
is not inferred or changed. No live catalog or source EPUB was used for validation.

2026-10-13 slow-reading correction: desktop activity now allows 30 minutes without input
before idling, supporting 10+ minute pages. Hidden/unfocused/sleep guards are retained.
Home explains that characters update on forward page turns; lookups count successful
dictionary requests, not words read. Approximately 1 minute editing and under 1 minute
validation/package work: focused timer tests, frontend and Windows release builds passed.
Interactive Windows acceptance remains pending. Previously omitted time is not backfilled.

2026-10-13 tracker metrics: Home adds daily/weekly estimated unique characters covered,
successful lookup-event counts and current/longest reading streaks. Heatmap metric selection
switches time/characters/lookups. Characters require a deliberate successful forward turn
following at least three seconds of focused page dwell; ruby annotations, whitespace,
script/style and explicitly hidden text are excluded. Cached, bounded per-document Unicode
scalar indexes map visible CFI boundaries to spine offsets. Image/fixed-layout/unresolvable
pages, chapter jumps, failed/stale turns and repagination do not invent counts. Additive
catalog coverage ranges merge overlaps per book/content key/spine, making retries and
rereading idempotent. Cached strong content versions are used when available; otherwise
size/modified metadata guards apply. Huge chapters (>2M UTF-16 units/100k nodes) and more
than 4096 disjoint coverage ranges per book/content key omit further character estimates;
reading remains available. No source EPUB bytes or text are persisted by this tracker.
Lookup activity counts one successful intentional event (including repeats/manual queries)
in the existing lookup transaction, independently of the lookup-history recording toggle;
failed/stale requests do not count. Historical lookup counts are not backfilled and phone
lookup sync does not contribute. Tracking pause applies to all metrics. Confirmed Clear
activity clears time, characters, lookup totals and coverage/streaks while preserving books,
positions, passages and lookup history; catalog backups contain the new tables. A streak
requires at least 60 seconds of focused reading on consecutive local dates, can end yesterday
until today's recording, and is calculated across all recorded days rather than 52 weeks.
Approximately 12 minutes investigation/implementation and 2 minutes validation/package
work: 23 focused frontend tests, seven activity/history/migration Rust checks, desktop
TypeScript, targeted lint and production frontend/Windows custom-protocol release builds
passed. Interactive Windows acceptance and phone implementation/sync remain pending.
Validation used isolated generated fixtures, never the live catalog or source EPUBs.

2026-10-13 Home copy cleanup: removed motivational headings, taglines and encouragement
from Home/Continue Reading in English and Japanese; retained functional headings, dates,
empty-state instructions and tracking explanations. Approximately 1 minute editing and
under 1 minute validation/package work: frontend and Windows release builds checked.

2026-10-13 desktop opening page: Home now combines Continue Reading, a keyboard-accessible
52-week reading-time heatmap with selected-day book breakdown, Monday-to-today time/days/
explicitly finished counts, and a bounded six-book Want to read shelf with a full-shelf link.
English/Japanese labels, narrow-window stacking and Show covers preferences are retained.
Desktop reading time starts after successful display, uses monotonic elapsed time with
10-second checkpoints, pauses when unfocused/hidden or idle for 30 minutes, and discards
sleep/clock-jump gaps. Visible EPUB document listeners are released on spine changes.
An additive catalog migration stores daily per-book/device aggregates and a transactional
watermark preventing duplicate retries/overlapping windows. Activity is included in existing
catalog backups; pause and confirmed activity-only clear preserve other user state. Calendar
buckets use local dates and split midnight; no prior reading time is inferred. Shared calendar/
interval helpers and the device column prepare for phone work; Android activity UI/tracking and
activity synchronization are not implemented. Approximately 12 minutes investigation/
implementation and 3 minutes validation/package work: 23 distinct focused frontend tests,
activity backup/reopen/dedup and schema-migration Rust checks, desktop TypeScript, targeted
lint, frontend production build and Windows custom-protocol release build passed. Focused
frontend tests ran outside the sandbox due to its temporary-file rename restriction.
Interactive Windows acceptance remains pending; no source EPUB or live catalog was used
for validation. Abrupt termination can lose the last unacknowledged checkpoint.

2026-10-13 desktop usability: library search/status/sort have visible labels;
active search/status/root/tag/collection/duplicate filters have removable chips
and a reset action. Empty results offer reset or Add folder as applicable.
Grid/List preference persists device-locally as `tmw-library-layout`; active layout
and navigation are highlighted, keyboard controls have visible focus, and grid
titles wrap to two lines. Filters and Saved shelves share a compact row. Error
notifications can be dismissed. Existing catalog, series, reader and source-file
behavior is retained. Approximately 4 minutes investigation/implementation and
1.5 minutes validation/package work: 14 focused library/series/resume tests,
desktop TypeScript, targeted lint, production frontend and Windows custom-protocol
release builds passed. Focused tests required execution outside the sandbox after
its temporary-file rename restriction. Interactive Windows visual acceptance
remains pending; no live catalog or source EPUB was changed for validation.

2026-10-13 Continue Reading sync correction: accepted nonempty phone progress
adds desktop resume membership transactionally and promotes only Unset/Want to read.
Android canonical progress pulls and local reading update the existing namespace-scoped
recent list (now up to 12), retaining pending local reading across older pulls.
The phone lists PC-only recent books with an explicit private download action;
Paused/Finished catalog entries are excluded. Successful phone display refreshes
the list after its native write; sync completion refreshes the active phone catalog,
and desktop Continue Reading refreshes every five seconds while visible outside
the reader. Existing wire formats, CFI/version guards and source EPUBs are preserved.
Desktop recent removal remains local; subsequent synced reading can add it back.
Approximately 7 minutes investigation/implementation and 4 minutes validation/package
work (overlapping intervals counted once). Seven focused frontend tests, the desktop
ordered-sync/resume regression, Android TypeScript, both frontend builds, Windows
custom-protocol release and ARM64 Kotlin/release builds passed. The bounded pending
recent-book SQL was checked with isolated in-memory fixtures. Android instrumentation
regression is added but not executed; interactive Windows/physical-phone acceptance
remains pending. ARM64 0.7.1/code 7001 signature, alignment and package metadata
were verified against the established signing identity; no app data is cleared.

2026-10-13 Android kanji glyph correction: app CSS selects the Japanese WebView
font locale and uses Japanese font names followed by generic sans-serif rather
than system-ui. Reflowable EPUB documents set HTML/XML language to Japanese and
apply the Japanese locale/serif stack to descendants, preventing publication font
overrides from selecting Chinese Han glyphs. Source EPUBs, text, ruby and saved
CFIs remain intact; fixed-layout pages retain publication styling. Approximately
3 minutes investigation/implementation and 30 seconds validation: five focused
presentation tests, Android TypeScript and frontend production build passed.
APK packaging and physical-phone glyph acceptance remain pending.

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
version is `com.tmw.companion` 0.7.2 / code 7002, ARM64, min API 24 / target 36.
The 2026-10-05 ARM64 package includes Show covers and the five reader palettes.
Signature, alignment and package metadata were verified; physical palette acceptance
remains pending.

Remaining work is request-only: the separate dictionary plan, automatic phone recovery import, correct journal compaction, phone metadata/tag/
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

Desktop reading activity is implemented below; phone activity/sync remains request-only. Activity contracts: optional daily/weekly focused active time,
conservative idle/sleep/background handling, no double counting, monotonic durations,
bounded persistence and calendar/timezone correctness. Clear/toggle preserves other
user state; include activity in backups. No proficiency scores or mandatory goals. Reading streaks are explicitly requested and defined below.

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

2026-10-05 desktop series consolidation: device-local tmw-consolidate-series checkbox defaults off; General Settings tmw-series-confidence defaults high, supports high+medium or all including possible. Existing effective series labels take priority. Bundled review-only proposals use a connection-local temporary index keyed by path plus unchanged effective title/author; no durable metadata assignments or source writes. Filtering and duplicate hiding precede grouped bounded pagination; opening a series shows all members without the outer search/tag/root/status filters, retaining duplicate visibility and a Back action. Proposed series names are searchable at the selected confidence with consolidation on or off, including short-query fallback. Books/copies and progress remain separate. Candidate generation uses scripts/generate-series-candidates.py with local metadata reports, not EPUBs. Approximately 12 minutes implementation and 2 minutes validation, including failure repair/build-lock waiting. TypeScript, targeted lint, five library UI tests, series SQL regression and existing duplicate regressions passed; frontend build checked. Native EXE packaging and running-app visual acceptance remain pending.

2026-10-05 series confidence correction: Alya screenshot exposed that duplicate-copy notes alone downgraded strong numbered series to medium, excluding them at the default high threshold. Series inference now computes confidence before adding copy-ambiguity notes; copies remain separate and duplicate controls remain independent. Regenerated reports/index retain 1,268 groups/8,226 books: 920 high, 296 medium, 52 possible. Main Alya group has 35 retained records at high; bonus/missing-author works are not automatically added. Approximately 2 minutes investigation/implementation and 15 seconds validation: duplicate-only confidence assertions and both series Rust regressions passed. Updated bundled index requires backend rebuild/restart; running visual acceptance remains pending.

2026-10-05 series navigation rendering correction: library/series detail/Settings/root views now have distinct React keys. Returning from a series previously rendered its retained member rows as multiple cards with one shared series key before the next request completed, risking stale DOM across navigation. Visible series cards now deduplicate by series key during transitions; individual books/copies remain intact in detail view. Regression uses realistic seriesCount/key fields on detail rows and checks a single card immediately on Back plus unmounted series DOM in Settings. Approximately 2 minutes investigation/implementation and 20 seconds validation: five UI tests, TypeScript, targeted lint and frontend build passed. Running-app visual acceptance pending.

2026-10-05 series search visibility/index expansion: read-only live tokidoki diagnosis proved prior grouping returned 16 cards versus 27 ungrouped, with unmatched bonuses sorting before the core series. Series cards now sort before individual results and show a loaded series/individual count. JSON rebuild adds 284 same-author related-title-prefix members at medium membership confidence, preserving core group confidence; missing-author and different-author works remain separate. Index now has 8,510 members. Group representative prefers available strong core members and volume order before title ranking. Read-only actual backend tokidoki query with duplicate hiding/high+medium returns 10 cards (2 series, 8 individual); Alya has 17 visible members. Opt-in read-only diagnostic test uses TMW_SERIES_DIAGNOSTIC_CATALOG and never runs migrations or durable writes. Approximately 4 minutes investigation/implementation and 40 seconds validation: series regressions, live read-only query, five UI tests, TypeScript/lint and frontend build passed. Native packaging/running visual acceptance pending.

2026-10-05 desktop library navigation usability: series detail has a Library/series breadcrumb and Back restores the prior loaded paginated results, outer filters/search/sort and window scroll/focus without a redundant reload. Snapshot invalidates on navigation, catalog updates or book edits. Series cards have an above-cover badge and separate series/individual result counts; confidence can be changed beside the checkbox as well as Settings. Catalog requests/debounce show updating feedback. Series detail uses accessible title rows with a device-local tmw-series-group-copies toggle (default on), includes every copy regardless of outer duplicate hiding, and offers expandable matching-title copies with independent opening/status. Copy grouping conservatively requires NFKC/whitespace/case-normalized title, author and volume label, never volume number alone; editions/bonus titles/different authors stay separate. Available copies are representatives; no durable data changes. Counts are loaded-so-far when pagination remains. Bulk tag selection remains available on representative rows or each row with grouping off. Approximately 7 minutes implementation and 1 minute validation: seven focused UI/copy tests including cached Back/scroll, TypeScript, targeted lint and frontend build passed. Native EXE packaging and running-app visual acceptance pending.
2026-10-05 Android series checkpoints: authenticated v2 catalog responses add versioned
series memberships and a series revision; proposal identities use normalized-name/author
hashes rather than report ordinals. Android stores these in additive indexed columns.
A changed revision triggers a full staged refresh, retaining the published catalog until
completion. Older servers remain usable without inferred memberships. Grouping happens
before bounded 25-row pagination and remains available offline after refresh; no durable
series assignments or source EPUB changes occur.
Android provides device-local consolidation/confidence controls, searchable series names,
series cards and a member view with conservative expandable matching-title copies.
Individual downloads, progress and notes retain their identities. Back restores outer
search/page/scroll; the mounted library retains navigation across reader/Settings visits.
Preferences are included in phone recovery exports. Approximately 15 minutes development
and 5 minutes validation/package work: focused frontend tests, TypeScript, targeted lint,
portable-membership/protocol regressions and isolated actual native-SQL fixtures passed.
ARM64 0.7.1/code 7001 APK built with the established signing certificate; signature,
alignment and package metadata verified. Physical-phone navigation/offline acceptance
remains pending. Updated PC backend must be rebuilt/restarted and the phone catalog
refreshed once; Windows standalone EXE was not packaged in this checkpoint.

2026-10-05 desktop series layout correction: series detail now receives the library Grid/List preference, rendering responsive cover cards in Grid and compact rows in List. Expandable copies and individual selection/opening remain available in both layouts. Approximately 2 minutes implementation and 20 seconds validation: eight focused series/library tests, TypeScript and frontend build passed. Native EXE packaging and running-app visual acceptance remain pending.

2026-10-05 series strictness placement: desktop and Android browsing retain only the consolidation checkbox; confidence selection is exclusively in Settings. Existing stored preferences and grouping/search behavior are retained. Approximately 1 minute implementation and 15 seconds validation; both TypeScript checks passed. Native packaging remains pending.

2026-10-06 PC library-refresh 503 correction: v2 catalog requests now initialize the connection-local temporary series proposal index before the page transaction. Independent read-only API connections previously lacked this table, returning catalog_busy/503 while status and user sync succeeded. Persistent catalog/source files remain untouched by initialization. Generated metadata-only regression exercises actual open_api_reader snapshot/repeated refresh/delta and rejects persistent writes; both focused protocol tests passed. Approximately 3 minutes investigation/implementation and 20 seconds validation, including a corrected test compile error. Standalone Windows EXE packaging and physical-phone retry remain pending; the PC backend must be rebuilt/restarted to use the fix. No Android update required.


2026-10-06 Windows Settings navigation: replaced the long General page with eight
category pages: General, Library Roots, Covers, Dictionaries, Android connection,
Backup & restore, Maintenance and Licenses. Wide windows use a sticky category
sidebar; smaller windows use wrapping category buttons. General retains language,
series confidence and duplicate preferences; Show covers and cache controls move
to Covers, search-index rebuilding to Maintenance. Hidden pages stay mounted to
preserve imports/tool state while switching categories; inactive controls are excluded
from keyboard/accessibility navigation. Existing preferences, commands and safeguards
remain intact. Approximately 5 minutes development and 1 minute validation: desktop
TypeScript, targeted lint, six library/navigation tests and frontend production build
passed (runner config loader for sandbox restrictions). Standalone EXE packaging
and running-app visual acceptance remain pending.
2026-10-06 Android empty-catalog recovery: user reported a complete initial catalog
transfer followed by no local results; later refreshes staged zero records. Read-only
PC inspection confirmed 17,412 books and companion identities remained intact.
The original disappearance is not reproduced or conclusively diagnosed. Refresh now
requests a full snapshot when the active local generation is absent/empty despite a
saved cursor; empty deltas retain the current generation instead of republishing it.
Completion reports the published local book count. Downloads, pairing and durable
user state remain separate and untouched by metadata recovery. Generated isolated
native emulator regression verifies initial snapshot, unchanged delta and recovery
of a missing generation with copy/user records retained. ARM64 0.7.1/code 7001 APK
repackaged using existing Rust/frontend outputs; established signature, alignment
and metadata verified. Existing emulator release restored in place after testing.
Approximately 7 minutes investigation/implementation and 2 minutes validation/package
work; physical-phone recovery and the original trigger remain unverified.

2026-10-06 Android consolidation query correction: series/copy browsing previously
used correlated full filtered-catalog scans for counts and representative selection,
including sorting. Persisted consolidation repeats that expensive read on restart,
which can leave initial results empty while loading; this is distinct from confirmed
catalog deletion. Group counts and representatives now use grouped aggregate joins,
preserving downloaded/available/confidence/numeric-order/ID preference and API 24
compatibility without window functions. Ungrouped browsing skips grouping work.
Isolated native-SQL regression preserves confidence, copies, pagination, namespaces
and generation publication; 17,412 additional generated books returned a grouped
page in approximately 0.09 seconds on the host. No source books, live catalogs or
durable series assignments were modified. Physical-phone acceptance remains pending.
ARM64 0.7.1/code 7001 APK repackaged with existing native/frontend outputs;
Kotlin compilation, established signature, alignment and package metadata verified.
Approximately 4 minutes investigation/implementation and 1 minute validation/package
work; no phone installation or physical-device smoke test performed.
