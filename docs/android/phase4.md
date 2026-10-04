# Android Phase 4 — catalog, downloads, and mobile storage

Implemented 2026-10-04 for the private companion. Phase 5 user-data sync and
Phase 6 typography/reader polish are not implemented. Existing dirty/untracked
work was the starting point; no reset, stash, clean, commit, source-library scan,
Android initialization, dictionary deletion, key change, or app-data clear occurred.

## Protocol and desktop storage

Migration 16 is additive: a transactional revision counter, coalesced changed-ID
journal, and mutation triggers cover books, overrides, readings/search documents,
tags, collections and membership. Existing namespace/public IDs remain unchanged.
The existing catalog Export backup remains the backup path before an upgrade;
`Database::backup_to` includes the new tables. Restore preserves public identity
but rotates the journal epoch, including same-revision restores. Old-schema and
current backup/restore tests remain covered. No destructive migration runs.

V1 pairing/status/catalog/file routes remain supported. New contracts:

* `POST /v2/catalog`: `{epoch?, revision?, expires?, since:0, after:"", delta:false}`.
  Returns protocolVersion 2, catalogId, epoch, revision, expires (server seconds),
  up to 50 items and next public-ID keyset cursor (null at completion).
* A snapshot starts without revision/epoch. Subsequent pages send all returned
  fence fields; each page uses a short SQLite read transaction. Catalog writes
  between pages return 409 `restart_snapshot`, never inconsistent live offsets.
  Cursors expire after 15 minutes. There is no growing server snapshot queue or
  long-lived SQLite read/write lock. The process epoch changes after
  restart, deliberately requiring a fresh snapshot.
* Deltas set `delta:true`, `since` to the last committed revision and reuse its
  epoch. First-page revision is omitted to capture the current fence; subsequent
  pages use that fence. The journal returns each changed ID once, in ID order,
  with final metadata or `{id,deleted:true}`. Tombstones older than the retained
  deletion window (100,000 revisions) are compacted on deletion; older clients
  must restart a full snapshot. No client-clock ordering is used.
* `GET /v2/books/{publicId}/cover` requires If-Match equal to the catalog's
  coverVersion. Changed inputs reject with 412, checked before and after encoding.
  The byte ETag remains `cover-v1-<SHA256>` and is verified natively. No conditional
  304 behavior is assumed. Cover descriptors hash encoding version, effective
  cache path, byte size and nanosecond modification stamp, without exposing the
  path or reading every image/EPUB during catalog pages. External edits that
  deliberately preserve all these input attributes are not detected; normal
  app-managed atomic cover replacement changes the descriptor.

Items include effective title/creator/series/volume, discovered values and field
overrides, language/identifier, notes, readings/aliases, normalized Japanese and
romaji search fields, tags/collections with membership, root ID, filename,
availability/extraction/reading status, size/modified/added hints and cover version.
Search data contains the desktop's normalized matching terms, including folder
terms; these are metadata, never accepted as source paths by an API.
Book scalar text fields are limited to 64 KiB and pages to 1,000,000 bytes; oversized
metadata fails rather than silently publishing a truncated mirror. Request bounds,
authorization, revocation and ID-only source isolation remain in effect.

`contentVersion:null` explicitly means not yet hashed; its version endpoint is
included. Size/mtime are never accepted as strong versions. Selected downloads
resolve actual SHA-256 using the existing `/v1/.../content` endpoint and request
`/v1/.../epub` with matching If-Match. This avoids hashing an entire large library
just to browse it. Same-path replacements keep identity but get different actual
content versions; rename/move retains the existing new-ID/unavailable-old-row rule.

## Mobile SQLite and durable copies

Native Android storage lives under app-private
`files/tmw-mobile/{catalog.sqlite3,books/,covers/}`. This is distinct from the
Tauri dictionary directory and WebView IndexedDB/localStorage. Schema version 1
creates fresh catalog/staging/download/pending/cache/settings tables and indexes;
it does not migrate/delete any Phase 1–3 data. WAL is enabled. Platform SQLite
JSON functions are used for membership/status filters; runtime evidence is API 36,
not a minimum-API compatibility claim.

Catalog pages and checkpoints commit together. Incomplete stage rows never appear
as a completed catalog. Refresh resumes the exact persisted fence/cursor; expiration
or revision changes discard only the regenerable stage, retry at most three times,
then leave the previous complete view available. A complete stage builds an
invisible generation in 250-record transactions. One pointer transaction publishes
it; superseded extracted generations are cleaned in bounded batches. Interrupted
generation creation is replayed idempotently. Failed refreshes do not infer missing
rows or erase downloads. Switching namespace retains archived catalogs selectable
locally, their copies and content-keyed positions.

Snapshot-missing/deleted records retain their previous metadata with deleted and
unavailable flags. Deltas apply explicit tombstones. Neither deletion nor source
unavailability removes a usable device copy. Copy removal affects only its download
record and owned EPUB file; catalog, proof book, positions, pairing and dictionary
remain. Notes/progress sync, phone metadata edits, and a new user-data model are
deferred to Phase 5. Existing hash-keyed localStorage positions remain intact.

Downloads run on one bounded native background worker, independently of browsing
and the two cover workers. Native HTTPS reuses Keystore credentials without ever
returning tokens to JS. Redirects/compression are disabled; private root `.ts.net`
validation and certificate checks are preserved. Transfers use a 64 KiB buffer,
one fixed `.part` file, 32 MB free-space reserve, exact response length/version,
180-second transfer deadline, cancellation/socket disconnect, exact SHA-256, EPUB
container/entry count/length/CRC validation, fsync, then same-directory atomic rename.
Pending publication records recover crashes between rename and database commit;
old copies are preserved until validated replacement publication. Owned abandoned
temporary files recover on native storage initialization. No source file is opened
for writing, uploaded, renamed, moved or removed.

Complete copies reach epub.js through a Rust binary IPC response using a strictly
validated owned hash filename, never full-book base64/JSON or caller paths. The
extra React `ArrayBuffer.slice` was removed. Input remains at most 64,000,000 bytes;
download validation permits at most 4,096 ZIP entries, 16,000,000 expanded bytes
per entry and 128,000,000 total. Each entry is read through bounded buffers with
CRC validation before publication. This is not a promise that 512 MB manga fit
epub.js: JSZip/DOM/images and bridge copies still cost memory. The existing manual
proof import retains its Phase 2 64 MB input behavior; it does not have the native
download ZIP preflight. Real large-publication memory/compatibility remains unmeasured.

The local view reads 25 rows at a time, with title/author/series/added/modified sort,
availability/downloaded/finished filters and exact tag/collection filters. Search
is debounced 250 ms, uses shared Rust normalization/romaji normalization and the
desktop-derived Japanese matching document. Generations ignore stale responses;
job completion refreshes the view once rather than every progress update. Very fast
jobs also refresh. Read offline switches directly to the reader; the legacy local
proof remains available through Open reader. About/Settings retains credits.

## Cover policy and cleanup boundaries

Default budget is exactly 100,000,000 bytes (decimal 100 MB), adjustable 0–1,000 MB;
0 disables persistence. Settings display actual directory usage and clear action.
All standalone mobile thumbnails use one owned cover directory and one LRU table,
including extracted downloaded-book covers and temporary files. No pinned covers,
secondary cache, PC cache mirror, or eagerly downloaded cover set exists.

IntersectionObserver requests only visible rows. Two native workers and a 24-entry
bounded queue limit fetch/decode work; per-request cancellation and page generations
disconnect obsolete sockets and reject obsolete results. Leaving visibility clears
the JS image string; at most the 25-row page can retain thumbnails. Missing/offline
covers show cheap placeholders. Embedded EPUB3 `cover-image` and EPUB2 cover-meta
images are sampled and reduced offline; container/OPF parsing rejects DTD/external
entities and bounds XML/image sizes. Embedded image bytes remain part of the book,
but every generated standalone JPEG uses the same cache budget. Disabled persistence
still supports bounded transient online or embedded-offline images.

Transport/generated thumbnails are at most 240×360, JPEG quality 75; alpha is
discarded. Remote bytes cap at 262,144, dimensions/type/decode are validated, and
remote byte hashes are checked. Local extraction caps encoded input at 8 MB and
dimensions at 8,192, sampled down before bounded resize. Writes reserve temporary
bytes by evicting LRU entries before fsync/rename. Startup removes owned `.part`
files and orphan cache entries. Cache keys include namespace, book ID, source/cover
version and encoder version. Clear/disable/eviction never traverse outside covers/.

Cleanup impact: only regenerable thumbnails and superseded extracted catalog
generations are discarded. Durable backup source path is the private app directory
`/data/user/0/com.tmw.companion/` (including WebView data, `files/tmw-mobile/books/`
and catalog); do not back up just covers/. Existing Android backup remains disabled
for Keystore-bound pairing. A user-facing durable export is Phase 7, not added here.
No cleanup touches PC cache/source directories, dictionary, pairing, proof EPUB,
positions, notes or current downloaded copies. Uninstall/data-clear still loses
private data and is never used for updates.

## Validation and measured evidence

Generated fixtures only. Native test creates its own `cache/phase4-fixture-*` root,
uses a deterministic fake private transport, and removes only that isolated root.
Production native credentials and WebView/dictionary data are not touched by tests.

* Native API 36/x86-64 test: 10,000-row bounded sync interrupted after 100 rows,
  no premature publication, restart/resume, local 25-row romaji browse, no eager
  cover files; complete download, bad SHA-256, 412/source change, cancellation,
  tombstone/unavailable delta, retained local copy, offline recreation and pending
  rename recovery; removal preserves catalog/user-state fixture.
* Cover test: generated embedded cover validates bounded output; actual logical
  cache-file usage goes 120 MB → 80 MB by LRU under default 100 MB, then 40 MB
  adjusted limit, then 0 disabled. Embedded cover still displays transiently with
  persistence disabled. Clear preserves EPUB/catalog/user-state fixture; abandoned
  cover/download temporary files recover after recreation. Sparse generated files
  exercise byte budgeting; these are not representative real cover compression.
* Native final affected test passed: **6.568 seconds**, after the atomic-generation
  browse-fence and same-file replacement changes. Generated 10,000-row resumed
  sync: **3,577.611 ms**; first 25-row `neko` browse **7.1903 ms**; generated uniform
  800×1,200 embedded PNG → **1,279-byte** bounded JPEG, decode/resize/encode
  **24.2866 ms**. 210 fixture transport calls. Earlier passing observations were
  3,989.9457 ms / 7.7752 ms / 24.6048 ms (208 calls). These are individual emulator
  observations, not real tailnet/HDD throughput or natural-cover size ratios.
* Frontend regression: **38 tests passed**, including debounced stale-response
  protection and no cover request for hidden rows. Final affected test rerun passed.
  ESLint passed after correcting reader state derivation; Windows frontend build
  passed with the existing chunk-size warning.
* Windows Rust final regression run: 41 passed, seven ignored benchmarks, one stale
  schema-version expectation failed. That assertion was updated to 16 and its
  targeted rerun passed. The final affected companion/protocol group: **4 passed**.
  Existing rollback, overrides, unavailable recovery, source safety, old/current
  backup tests are covered by this evidence; no repeated broad suite was needed.
  Windows debug executable build passed earlier. The last rebuild after adding
  raw reading overrides and the restore-epoch assertion passed the affected Rust
  test, but executable linking was blocked by Windows holding
  `target/debug/tmw-epub-library.exe` open. The running desktop was preserved.
  A bounded repair succeeded with `cargo rustc --offline --manifest-path
  src-tauri/Cargo.toml --bin tmw-epub-library -- -C extra-filename=-phase4`.
  The latest debug executable was copied from
  `target/debug/deps/tmw_epub_library-phase4.exe` to
  `target/debug/tmw-epub-library-phase4.exe`; the running original stayed untouched.
  Close the current desktop normally before using this alternate debug executable
  with the usual Vite dev server (`npm run dev`, port 1420), or use the usual
  `npm run tauri -- dev` workflow to rebuild/relaunch the standard filename.
  Installer packaging was not repeated; the alternate executable was not launched
  against the user's live catalog during validation.
* Actual desktop two-worker import with 512 original generated EPUBs overlapped
  v1 search and v2 snapshot requests: **390 ms**, six overlapped requests, maximum
  **85 ms**. Earlier v1-only observations this task were 348–367 ms / 26–30 ms.
  Phase 3 baseline was 310 ms / 28 ms. V2 transfers 50 complete metadata records
  rather than ten minimal v1 search rows; concurrent builds also ran. These are
  system-temp observations, not an HDD performance regression conclusion.
* API tests preserve wrong/missing/revoked authorization, unknown-ID/path rejection,
  v1 compatibility, v2 authorization/unknown-field rejection, fixed revision pages,
  transaction rollback, and coalesced deletion tombstones.

Concrete repairs: trigger conflict handling uses explicit UPSERT rather than
inherited outer conflict policy; Android Harmony lacks the Xerces DTD flag, so
bounded preflight/independent entity rejection replaces it; SQLite pooled readers
cannot reliably read writer `changes()`, so cleanup uses executeUpdateDelete;
schema assertions moved 15→16; Android TypeScript assertions use native Vitest
types; late fast-job/view-navigation fixes required the affected APK asset rebuild.
Restricted Gradle/ADB/Tailscale inspection used the existing user caches in approved
shells; no toolchain reinstall or networking reconfiguration occurred.

Commands from repository root (system npm launcher used):

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo fmt --manifest-path apps/android/src-tauri/Cargo.toml
cargo test --offline --manifest-path src-tauri/Cargo.toml --lib -- --nocapture
cargo test --offline --manifest-path src-tauri/Cargo.toml --lib restoring_previous_schema_adds_passages_without_changing_progress
cargo test --offline --manifest-path src-tauri/Cargo.toml --lib companion -- --nocapture
cargo build --offline --manifest-path src-tauri/Cargo.toml
& 'C:\Program Files\nodejs\npm.cmd' run test
& 'C:\Program Files\nodejs\npm.cmd' run lint
& 'C:\Program Files\nodejs\npm.cmd' run build
& 'C:\Program Files\nodejs\npm.cmd' exec vitest -- run apps/android/src/Catalog.test.tsx
$env:GRADLE_USER_HOME='C:\Users\Marius\.gradle'
# Existing SDK/Java session paths from feasibility.md.
$native='apps/android/src-tauri/gen/android'
& "$native/gradlew.bat" -p $native assembleX86_64Debug assembleX86_64DebugAndroidTest -x rustBuildX86_64Debug --offline --no-daemon --no-configuration-cache
adb -s emulator-5554 install -r "$native/app/build/outputs/apk/x86_64/debug/app-x86_64-debug.apk"
adb -s emulator-5554 install -r "$native/app/build/outputs/apk/androidTest/x86_64/debug/app-x86_64-debug-androidTest.apk"
adb -s emulator-5554 shell am instrument -w -e class com.tmw.companion.MobileStorageTest com.tmw.companion.test/androidx.test.runner.AndroidJUnitRunner
# ARM64 phone artifact; x86-64 needed once for new binary/native IPC smoke.
& .\apps\android\Android.ps1 -Action Build -TestSigning -Target @('aarch64','x86_64')
```

## Artifacts, private connection, and remaining observations

Version **0.4.0**, Android versionCode **4000**, existing development-key signing,
same `com.tmw.companion`. Update in place; never uninstall or clear app data.
Final release-profile artifacts (same existing development key):

| ABI | Path under `apps/android/src-tauri/gen/android/app/build/outputs/apk/` | Bytes | SHA-256 |
| --- | --- | ---: | --- |
| ARM64 | `arm64/release/app-arm64-release.apk` | 88,821,615 | `7b348df403672a959721cd07bdc2de285a82282a4c1e5ba83cdd56e679093606` |
| x86-64 | `x86_64/release/app-x86_64-release.apk` | 89,460,812 | `47f80303366c928c711c9340507d4e860ffd675efb72fb29003ec558b44b5fbb` |

ARM64 `apksigner verify --verbose` passed (v2 scheme), and
`zipalign -c -P 16 4` passed. The first verifier shell lacked JAVA_HOME; the
corrected existing-Java invocation passed. Final APK build reused existing SDK,
AVD, key and JNI configuration. Affected packages were rebuilt after the late UI
fixes; the final browse-fence change required only Kotlin/wrapper packaging,
reusing final Rust/frontend binaries. No additional ABI/native dependency rebuild
was performed for that wrapper change.

Final in-place emulator smoke: release 0.4.0 catalog rendered without native-command
errors; default cover budget 100,000,000, actual usage 0. Existing
`reader-vertical.epub` reopened from the preserved WebView proof store. Activity
launch 543 ms, final restoration 520 ms (`am start -W`, not WebView readiness).
An existing original generated 2,774-byte horizontal fixture was temporarily
placed at a previously nonexistent owned hash filename to test the actual binary
bridge in the existing debug wrapper: **ArrayBuffer**, exact bytes and SHA-256
`90f96ab7e804cb32bb159f6c5c38c45741d9ecfff82aa07ac681c45e3c2af822` matched,
and `../../outside.epub` was rejected. Sanitized native connection status reported
**Not paired** on the emulator. No credential was returned/read/logged in JS.
Only that generated file and temporary ADB debug forwarding were removed, then
the final signed release was restored with `adb install -r`. Production reader,
positions, dictionaries, preferences and signing identity were retained.

Additional smoke commands used the documented SDK's adb/apksigner/zipalign:

```powershell
adb -s emulator-5554 install -r <final-x86-64-release-apk>
adb -s emulator-5554 shell am start -W -n com.tmw.companion/.MainActivity
apksigner verify --verbose <final-arm64-apk>
zipalign -c -P 16 4 <final-arm64-apk>
Get-FileHash <final-arm64-apk> -Algorithm SHA256
git diff --check
```

Approximate task accounting: **44 minutes development/documentation,
17 minutes testing/builds/packaging/smoke and validation waiting (~28% validation)**.
Failed attempts and concrete repair reruns are included. Overlaps count once:
active validation build/test runtime is assigned to validation even while writing
documentation, and excluded from development. No padding, broad emulator/reader
acceptance rerun, or testing-budget exception was used. Phase 4 stops here.

Read-only inspection found only emulator-5554 attached, and existing private
Serve `https://desktop-jevl1vm.tail755cbb.ts.net` proxying loopback port 47831.
Serve was not changed, Funnel was never enabled, and service startup remains
disabled. Physical phone is not attached; its latest reported grant is revoked.
Install the updated APK, run the updated desktop and explicitly enable its service,
then obtain a fresh code and re-pair through that same private HTTPS hostname.
Prior physical-phone pairing/revocation and offline dictionary evidence is reused.

Phase 4 physical-phone acceptance is **pending user testing**. Real tailnet slow
transfer/mid-stream cancellation latency, PC HDD/source drive behavior, natural
cover compression/phone decode timing, large manga memory, minimum API, 16 KB
physical runtime, accessibility and sustained import UI frame timings are not
claimed. Native fake transport and desktop real loopback are complementary tests,
not a phone end-to-end HTTPS proof. Continuous catalog writes can require refresh
restart; wait for desktop import to settle after retry exhaustion. No live SQLite
file exchange, new user-data sync, cloud storage, analytics or reader polish exists.

Source safety: actual `F:\tmw collection` and `testLibrary/` were not accessed or
altered. Generated desktop tests assert byte preservation and no source-managed
cache files. No source EPUB is bundled or uploaded; downloads transfer approved
read-only copies over the private service. Generated data, keys and APKs stay ignored.
