# Phase 4 handoff — catalog, downloads, and mobile storage

Prepared 2026-10-04. This is handoff guidance, not a Phase 4 implementation or
validation report. Phase 4 starts only when the user gives the next agent the
implementation request. Record that work in `phase4.md`, not in this file.

## Verified starting point

Phases 1–3 are complete for their authorized scope. The user confirmed physical
phone reading/fast lookup (Phases 1/2), approved pairing/private HTTPS connection
(`PC connected · protocol 1`), and revocation (`device unauthorized`, Phase 3).
The latest reported phone grant was revoked; re-pair using a fresh desktop code
if access is needed. An unauthorized response in that state is expected, not a
network defect. Do not reuse screenshot codes, expose credentials, reset app data
or replace keys to restore access. Inspect current connection state first.

The user configured private Tailscale connectivity successfully. Preserve that
setup: Serve only, never Funnel, router forwarding or source-folder sharing.
The desktop service still starts disabled and listens on loopback port 47831.
Enable it explicitly for tests and keep the PC awake/source drive available.
Use the configured HTTPS `.ts.net` address; do not substitute the numeric
Tailscale IP or loopback address in the phone UI. Do not reconfigure an existing
Serve listener just to repeat already accepted pairing evidence.

Japanese formatting and UI quality are known shortcomings, not accepted polish.
Preserve fast lookup and shared reader behavior; Phase 6 owns typography/ruby/
vertical-pagination/reader-control polish unless separately authorized.

## Read first and preserve

Read root `AGENTS.md`, `AndroidAgents.md`, `apps/android/AGENTS.md`,
`feasibility.md`, `phase2.md`, `phase3.md`, and this handoff. Inspect `git status`
and relevant diffs before editing. Much of Phases 1–3 and unrelated desktop work
is uncommitted/untracked; the current working tree is the starting point.
Do not reset, clean, stash away, relocate or overwrite it. No commit is required
just to begin the next phase. Never commit ignored fixtures, dictionaries,
credentials, keys, captures or APKs.

`F:\tmw collection` and `testLibrary/` are read-only source material. Phone
EPUB transfers are explicitly requested read-only copies over the private
connection. Generate validation fixtures in isolated temporary/app-managed
folders; do not alter actual sources to simulate replacements or cancellation.
Desktop covers must remain outside every library root; mobile cache maintenance
must never touch desktop covers or source directories.

Reuse `TMW_Test`, the existing SDK/Java, signing key and build script. Preserve
package identifier `com.tmw.companion`, Keystore alias/preferences, installed
reader/dictionary data and in-place update behavior. Phase 3 is version 0.3.0;
bump the next APK version deliberately, using the same key. Never initialize or
wipe Android again. Select the actual serial with `adb devices` and use
`adb install -r`. Use `-Target aarch64` for the final phone APK, and x86-64 only
when changed functionality needs an emulator build. Do not repeat both ABIs for
unrelated edits. Toolchain paths/commands and prior APK hashes are in the phase
reports. The Phase 3 native credential test runs in a debug wrapper with isolated
preferences; its release instrumentation runner had a stripped tracing-class
failure. Reuse that evidence unless credential handling changes.

## Implementation map and constraints to resolve

| Area | Existing implementation | Phase 4 consequence |
| --- | --- | --- |
| Desktop transport | `src-tauri/src/companion.rs`; opt-in background HTTP service, authorization, bounded requests, read-only query connections | Extend narrowly; preserve import/UI responsiveness, path isolation, revocation and disabled startup |
| Public identity | Migration 15, `src-tauri/src/db/companion.rs` | Namespace mobile records by catalog ID + public book ID; never use local numeric IDs/paths/title as cross-device identity |
| Catalog query | Existing `Database::browse_books`; v1 catalog exposes effective fields, flags and live offset pages | Live offsets are not a durable snapshot/delta cursor; finalize consistent pagination, revisions and deletion semantics before importing durably |
| Metadata completeness | v1 metadata has bounded tags; pages omit tag/collection membership, strong content versions and versioned cover descriptors | Audit and add the metadata/versions needed for Phase 4; do not claim current v1 already supplies a full mirror |
| Source version | `/content` hashes actual bytes; `/epub` requires matching If-Match and rehashes the open source | Stream to a bounded native temporary file, verify length + SHA-256, then atomically publish; size/mtime is only a hint |
| Covers | On-demand ≤240×360 RGB JPEG quality 75; version in `cover-v1-...` response ETag; no-store, no conditional 304 route | Establish versioned catalog cover identifiers and validation semantics; do not fetch every cover to discover versions or silently assume conditional GET works |
| Native connection | Android Rust `connection.rs` → Kotlin `ConnectionPlugin.kt`; only status/pair/check/forget today | Add bounded authorized catalog/file operations natively; credentials must never be returned to JS or reader content |
| Offline reader proof | `localBook.ts` keeps one IndexedDB copy; `Reader.tsx` restores content-hash-keyed CFI, accepts complete bytes | Integrate multiple native downloads without deleting the proof copy/positions; avoid full-book base64/JSON IPC or unbounded duplicate buffers |
| Reader limits | Phase 2 input limit is 64 MB; server EPUB limit is 512 MB; EPUB inflation depends on publication | Align explicit supported limits safely; do not promise the server maximum fits epub.js memory simply by increasing the input limit |
| Shared behavior | `crates/japanese-core`, `packages/reader-core`, Android `lookup.rs`, desktop query semantics | Keep both apps consuming the audited shared logic; retain lookup ordering/performance and bundled offline dictionary |

Existing public IDs survive same-path rescans/replacements and catalog backups.
A desktop rename/move creates a new row/public ID and leaves the old row
unavailable; root deletion removes mappings. Do not infer edition equivalence
or silently migrate reader anchors by title/filename. A restored catalog can
retain its namespace/IDs; an older backup may create new IDs. Define how the
phone handles namespace changes, revision rollback/restores, unavailable books,
deleted catalog rows and retained downloads before enabling durable deltas.

Maintain v1 pairing/status compatibility for the already-installed phone. Use
new versioned routes/contracts for breaking changes, or compatible additive
responses; v1 rejects unknown request fields. Bound pages, requests, snapshot
lifetime, queues, disk usage and database transactions. Do not exchange SQLite
files or keep a long write lock during file transfer. A desktop import must not
invalidate pagination silently. Interrupted catalog refreshes must not appear
complete or delete missing rows from an incomplete page sequence.

## Phase 4 scope and acceptance

Implement app-private mobile SQLite catalog/storage migrations; bounded,
restart-safe catalog snapshot/delta import; paginated/virtualized local browsing,
search/filter/sort; and selected EPUB download/remove operations with visible
size/progress/local availability. Complete downloads before opening. Use short
transactions, cancellation, disk-space checks, integrity validation and atomic
finalization. Recover bounded abandoned temporary files after process death.
Keep downloaded copies usable when the PC is offline or a source becomes
unavailable. Removing a copy retains metadata, positions and other durable user
records. Define catalog deletion handling explicitly; it must not silently erase
a usable phone EPUB or notes/progress.

Implement the complete cover policy in `AndroidAgents.md`: default
**100,000,000-byte** persistent budget, adjustable/disableable caching, usage
and clear action, automatic LRU eviction, only visible items fetched with bounded
concurrency/decoding/retention, canceled stale requests, cheap offline placeholders,
embedded covers from local EPUBs, and atomic validated cache writes. All standalone
mobile thumbnails, including extracted local-book thumbnails and temporary files,
share this budget. Embedded bytes inside downloaded EPUBs are not counted again.
No pinned/secondary unbounded cache. Disabled persistence still allows bounded
transient online display and embedded offline covers. Cleanup never deletes
catalog, downloaded EPUBs, progress, notes, pairing, dictionaries or PC/source
files. Before destructive migration/cleanup, explain the exact impact and provide
a backup path; design normal LRU eviction to affect only clearly owned regenerable
mobile cache entries. Measure encoding size/decode cost and actual usage.

Acceptance must include a generated 10,000-row mobile catalog without eager cover
loading; stale-response protection; interrupted refresh/download/cache writes;
hash mismatch/source-version change; restart and offline reopening; canceled
work; unavailable/deleted desktop records; copy removal preserving user data;
100 MB LRU enforcement, adjusted/disabled caching, embedded covers/placeholders,
and safe clear-cache behavior. Exercise representative authorized operations
alongside desktop importing and preserve revocation/path-isolation behavior.
Use original generated EPUBs for automated/emulator safety checks. Reuse existing
Phase 1–3 evidence instead of repeating unrelated acceptance suites.

The root testing budget applies: validation/builds/packaging/emulator checks and
validation waiting together ≤50% of task time, preferably less; count overlap
once, never pad development. Plan targeted checks, one relevant final regression
and build/package pass plus brief smoke test. Repeat only for concrete failures
or changes affecting the check. Ask for an explicit exception before a necessary
check would exceed the budget. Record exact commands/results, limitations,
measurements, APK hash/size, and approximate development/testing time in
`phase4.md`. Do not claim physical-phone Phase 4 acceptance before the user tests
it. Prepare a signed ARM64 APK using the existing key, then stop after Phase 4.

Not in scope: Phase 5 progress/bookmark/note sync, phone metadata/tag/collection
editing, Phase 6 reader polish, cloud services, accounts, analytics, public
hosting, OCR, full-text EPUB content indexing, true streaming or speculative
repository reorganization. Catalog metadata deltas in Phase 4 are distinct from
bidirectional user-data synchronization in Phase 5.

## Copyable implementation prompt

```text
Implement Android Phase 4 — Catalog, downloads, and mobile storage only.

Read AGENTS.md, AndroidAgents.md, apps/android/AGENTS.md,
docs/android/feasibility.md, docs/android/phase2.md, docs/android/phase3.md,
and docs/android/phase4-handoff.md. Inspect the existing dirty working tree
and preserve all uncommitted work and Windows behavior/tests.

Phases 1–3 passed physical-phone functional verification: offline reading and
fast lookup, private HTTPS pairing/connection, and revocation reporting
“device unauthorized”. The latest reported phone grant was revoked; inspect
current state and re-pair with a fresh code if needed. Preserve the working
private Tailscale Serve setup; never enable Funnel. Keep the desktop service
opt-in/disabled at startup. Preserve lookup and leave Japanese reader/UI polish
for Phase 6 unless separately authorized.

Implement mobile SQLite catalog/storage, bounded restart-safe snapshot/delta
import, responsive local catalog browsing/search/filter/sort, and selected EPUB
download/remove management. Finalize consistent pagination/revisions, metadata
completeness, namespace/identity, unavailable/deleted records and versioned cover
contracts first: Phase 3 live offsets are not durable sync cursors. Preserve v1
pairing/status compatibility. Use authorized public catalog IDs, never caller
filesystem paths, and reuse desktop query behavior. Keep native credentials out
of JavaScript and reader content.

Stream downloads into app-private temporary files; bound work/memory, check disk
space, support cancellation, verify exact length and SHA-256/If-Match version,
and finalize atomically. Open only complete copies offline; preserve the Phase 2
proof book/positions, existing dictionary and pairing. Keep copies usable when
PC sources become unavailable. Removing copies retains catalog/user records.
Resolve the 64 MB reader vs 512 MB server limit honestly without unbounded IPC
or memory. Never modify, rename, move, delete or upload source EPUBs.

Implement the full AndroidAgents.md cover policy: default 100,000,000-byte
persistent budget; adjustable/disabled caching; LRU eviction; usage/clear action;
visible-only bounded/cancellable fetch/decode/retention; versioned atomic validated
entries; temporary bytes and all standalone local-book thumbnails in the same
budget; embedded EPUB covers offline and cheap placeholders. Clearing/eviction
must preserve books, catalog, progress, notes, credentials and PC/source files.
Measure thumbnail encoding/transfer/decode cost and actual storage usage.

No Phase 5 user-data sync or phone metadata/tag/collection editing, and no Phase 6
reader polish. No cloud/accounts/analytics/public hosting or repository rewrite.
Source library F:\tmw collection and testLibrary are read-only. Keep credits
unobtrusive under About/Settings. Before destructive migrations or cleanup,
explain impact and provide a backup path.

Follow the root ≤50% testing-time budget, including verification builds, APK
packaging, emulator/manual checks and validation waiting. Reuse evidence; plan
targeted checks and one final relevant regression/build/smoke pass. Request an
explicit exception before exceeding the budget. Validate a generated 10,000-row
catalog, interrupted catalog/download/cache work, integrity/version changes,
restart/offline reopening, safe removal/clear, complete cover-budget behavior,
and representative requests during desktop import. Preserve authorization and
path isolation. Reuse TMW_Test and the existing signing key; preserve app data
with in-place updates. Prepare the signed ARM64 APK for my phone. Record work,
protocol/storage decisions, commands/results, limitations, measurements and
approximate development/testing time in docs/android/phase4.md. Stop after Phase 4.
```
