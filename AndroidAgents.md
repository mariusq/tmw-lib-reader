# Android companion implementation instructions

This document governs explicitly requested Android companion work. It is a plan, not authorization to begin every implementation phase. Read it together with `AGENTS.md`; preserve the Windows app and its existing behavior and tests. Private networking and sync are explicitly allowed for this companion despite the desktop's original no-network rule. Do not add analytics, cloud library storage, or public hosting.

## Confirmed product decisions

Follow the canonical testing budget in `AGENTS.md`: validation must take at most 50% of task time.

- This is a private personal-use companion with no intended publication. Keep
  dependency and dictionary credits unobtrusive under About/Settings; do not add
  terms-of-service acceptance flows or app-store publication work. Preserve
  applicable upstream notices and keep install/update signing reliable.

- Build an installable Android APK with Tauri v2, React, TypeScript, Vite, Tailwind, and a Rust backend where practical.
- Keep the companion in this repository with separate application configuration and build outputs. Introduce shared packages/crates gradually; do not reorganize the working desktop app just to match a proposed layout.
- The PC owns the source library and authoritative catalog. The phone holds selected EPUB copies, a local SQLite catalog, user state, and pending offline changes.
- Bundle JMdict Japanese-English definitions for offline lookup. Reuse the existing `jmdict-eng` import semantics; verify attribution and redistribution requirements for both the data and supplied export. Japanese-Japanese dictionaries are future scope.
- Match desktop Japanese word parsing, normalization, tokenization, and lookup ordering. Adapt reader interaction to tap-to-lookup, preserving ruby exclusion, Unicode offsets, vertical text, and sentence context. Audit existing implementation before extracting shared code.
- Download a complete EPUB copy before opening it. A Read action may initiate the download. Reading and dictionary lookup must then work without the PC or network. True streaming is deferred.
- Do not add special rereading protection, furthest-position rules, or rereading conflict prompts. Use simple last-write-wins progress with a documented server ordering rule; preserve delivery order for queued device updates and make retries idempotent.
- Sync records through a narrow API; never exchange or share live SQLite files.
- Never modify, rename, move, delete, or upload source EPUBs to third-party storage. Explicit phone downloads may transfer read-only copies over the private connection. `testLibrary/` remains read-only and Git-ignored.

## Confirmed cover policy

Do not mirror the PC cover folder or bundle it in the APK. Catalog sync transfers metadata and versioned cover identifiers, not all cover images.

- Fetch small, bounded thumbnails on demand only for visible catalog items. Bound concurrent requests, decoding, and in-memory image retention; cancel obsolete requests.
- Default persistent cover cache limit: **100 MB (100,000,000 bytes)**. Allow adjustment or disabling persistent cover caching in settings. Provide a clear-cache action and display current usage.
- Evict least-recently-used thumbnails automatically to enforce the limit. All app-generated or downloaded standalone mobile cover thumbnails count toward this same limit, including thumbnails for downloaded books. Do not pin covers outside the budget or create a second unbounded cover cache.
- Use embedded covers from downloaded EPUB copies for offline display where available. Embedded image bytes remain part of the book download and do not count separately toward the cover-cache limit; extracted thumbnail copies do. Bound thumbnail generation and image decoding.
- Offline catalog items without a cached cover or local EPUB show inexpensive placeholders. When persistent caching is disabled, online covers may still display using bounded transient memory; local EPUB covers remain available offline.
- Keep the cover cache separate from downloaded books, catalog metadata, notes, and progress. Eviction, disabling caching, and clearing this cache must never delete those records, phone EPUB copies, PC cache files, or source EPUBs.
- Write cache files atomically and validate cached entries. Include temporary files in the storage budget, reject oversized entries, and recover abandoned temporary files so interrupted writes cannot grow storage without bounds. Existing source and storage-cleanup safeguards apply.
- Version cache keys when source cover inputs or thumbnail encoding change. Measure thumbnail dimensions, quality, transfer bytes, decode cost, and real cache usage before choosing encoding defaults. Do not send full-sized PC covers when a mobile thumbnail suffices.

## Architecture and data boundaries

### Repository layout

The Android companion lives in `apps/android/`, with its React frontend in
`apps/android/src/` and its independent Tauri configuration/Rust backend in
`apps/android/src-tauri/`. Android native scaffolding lives below
`apps/android/src-tauri/gen/android/`; build outputs remain ignored.
The Windows app stays in the existing root `src/` and `src-tauri/`, with its
existing root npm commands. Do not relocate it as part of Android feasibility.

Extract audited portable Rust into `crates/` and portable TypeScript into
`packages/` only when needed. Both applications must consume the same extracted
implementation; avoid divergent copies. Keep platform commands, UI interaction,
storage paths, and database migrations app-specific. Each app has its own
application identifier and SQLite database. The existing shared implementations are `crates/japanese-core/` and
`packages/reader-core/`. Share protocol types only after their semantics are
defined; avoid speculative abstractions.

- Share portable reader/lookup code and protocol types; keep Windows discovery, library management, and filesystem paths desktop-only.
- Phone paths, download status, credentials, cache configuration, and device settings remain local.
- PC-to-phone sync supplies catalog metadata, overrides, tags, collections, availability, and cover versions. Page large snapshots and apply subsequent deltas.
- Both directions support progress, bookmarks/saved passages, and notes. Add phone metadata/tag/collection editing only after core sync works; preserve user-override semantics.
- Establish stable catalog book IDs independent of phone paths, and distinguish identity from EPUB content version. Audit existing move/rename behavior before selecting an identity migration. Do not deduplicate different editions by filename or title.
- Define revision/cursor semantics, idempotent operations, tombstones, pagination, and schema/protocol compatibility. Do not rely on synchronized phone/PC clocks for ordering. Source replacement must not silently apply an invalid EPUB CFI.
- Removing a phone download retains catalog records, progress, and notes. PC source unavailability must not erase a usable downloaded copy. Define catalog deletion handling explicitly before enabling it over sync.
- A desktop service bound to loopback may be published privately via Tailscale Serve. Support a home-LAN transport only with explicit configuration and appropriate authentication. Never enable public Funnel, automatic router port forwarding, or broad filesystem sharing.
- Pair and revoke devices; store credentials securely. Serve EPUBs and covers by authorized catalog IDs, never arbitrary caller-provided paths. Validate inputs, bound requests, and keep database writes short. Network service remains opt-in.

## Current checkpoint and development environment

Phases 1 and 2 are complete as functional proofs, including user-confirmed
physical-phone operation. Phase 2 lookup felt fast; the user reported poor UI
and incorrect Japanese text formatting. Reader quality is not accepted as
finished. Preserve lookup performance and address typography, ruby, vertical
pagination, spacing and reader controls in Phase 6 unless separately authorized.
Phase 3 implementation is recorded in `docs/android/phase3.md`; the user confirmed physical-phone pairing and the private HTTPS connection on 2026-10-04. The user also confirmed physical-phone revocation with “device unauthorized”; revocation is additionally covered by API tests. Later phases have not started.

Use [feasibility.md](docs/android/feasibility.md) for the shared-code audit,
installed toolchain and build commands; [phase2.md](docs/android/phase2.md) records
reader extraction, dictionary provisioning, measurements and known limitations.
These are prior evidence, not instructions to rerun completed acceptance checks.
Use [phase3.md](docs/android/phase3.md) for the accepted private API, protocol
bounds, signing artifacts and phone pairing/revocation evidence.
[Phase 4 handoff](docs/android/phase4-handoff.md) records implementation entry
points, remaining protocol decisions, storage constraints and the next prompt.
The latest reported device grant was revoked; check connection state and use a
fresh pairing code when needed. Do not treat unauthorized access as a network fault.

Reuse `TMW_Test` (Android 16/API 36, x86-64). SDK:
`C:\Users\Marius\Desktop\android-tools\SDK`; Java:
`C:\Users\Marius\Desktop\android-tools\jdk-21.0.12.1+1`.
The ignored `Launch-Android.ps1` launcher can start the existing AVD; if absent,
use the installed emulator directly. Select the actual serial with `adb devices`
and update with `adb install -r`. Never recreate/wipe the AVD, clear app data,
change the existing signing key, or rerun Android initialization for routine work.
Use `apps/android/Android.ps1 -Action Build -TestSigning`; select `-Target x86_64`
for emulator builds or `-Target aarch64` for phone APKs when only one ABI is needed.
Record new phase evidence in its own `docs/android/phaseN.md`.

## Implementation stages

Implement sequentially when authorized. At each stage, run relevant checks, document measurements and limitations, and report completion before proceeding. Keep small reviewable checkpoints. Do not commit EPUB fixtures, generated dictionaries/databases, credentials, signing keys, or build artifacts.

### 1. Android feasibility and shared-code audit

Audit desktop reader, tokenizer, dictionary, SQLite, and platform-specific dependencies. Establish Android toolchain and release APK packaging without changing Windows behavior. Compile the actual tokenizer/dictionary dependencies for Android; measure APK and installed size rather than repeating estimates. Verify licensing and notices. Record supported Android versions and ABI choices based on the tested build.

Acceptance: APK installs and launches on a physical phone; Windows checks still pass; portable and desktop-only boundaries are documented.

### 2. Offline reader and bundled lookup proof

Provision a versioned, indexed JMdict database using a reproducible build step or bounded first-run installation. Avoid retaining redundant raw and imported data unnecessarily. Reuse desktop parsing and lookup behavior. Read a manually supplied local EPUB copy; implement tap lookup with touch/scroll/page-turn disambiguation and an accessible popup.

Acceptance: airplane-mode reading and lookup work for horizontal and vertical Japanese text, furigana, inflections, and words split across DOM elements. Record cold-start time, dictionary size, lookup latency, and memory use. Compare representative lookup results with desktop behavior.

### 3. Private desktop API and pairing

Add opt-in catalog pagination/search, authorized cover access, read-only EPUB download, and pairing/revocation endpoints. Reuse the existing catalog query behavior. Keep service tasks independent of desktop UI responsiveness. Document private Tailscale Serve setup and PC-awake requirements. Keep the
service disabled by default. Define versioned, bounded API responses and audit
catalog identity/content-version behavior before exposing book identifiers.
Add only the minimal Android pairing/connection UI needed to verify an approved
phone connection, with secure credential storage. Full mobile catalog import,
download management, cover caching and user-data sync remain Phases 4 and 5.

Acceptance: approved phone connects; unauthorized/revoked clients fail; caller-supplied paths cannot expose unrelated files; no source files change. Test representative requests alongside a desktop import.

### 4. Catalog, downloads, and mobile storage

Follow [the Phase 4 handoff](docs/android/phase4-handoff.md). Create mobile SQLite migrations and bounded, restartable catalog snapshot/delta import, plus local browse/search/filter/sort. Finalize consistent snapshot/revision cursors, complete metadata, versioned cover identifiers, namespace changes, and unavailable/deleted-book semantics before importing durable mobile state. Phase 3 live-offset pagination is not a durable snapshot or delta protocol. Preserve existing public IDs, source-version semantics, authorized ID-only access, native credential storage and v1 compatibility. Preserve the existing local EPUB and reading position when adding multiple downloads. Download books to private app storage using temporary files, integrity validation, atomic finalization, cancellation, and disk-space checks. Display download size/progress and local availability. Apply the finalized cover policy here.

Acceptance: browse a generated 10,000-row catalog without loading every cover; interrupted downloads never appear complete; downloaded books reopen offline. Verify the default 100 MB cover budget, least-recently-used eviction, adjusted limits, disabled caching, offline placeholders, embedded local covers, interrupted cache writes, and safe cache clearing. Removing copies/cache preserves user data. Measure browse latency and actual disk usage.

### 5. Progress and user-data sync

Persist progress/bookmarks/notes locally first and queue operations transactionally. Sync automatically when the private PC connection is available, with backoff, visible status, durable cursors, and idempotent retries. Apply the confirmed simple progress ordering without rereading protection. Define and test field-level rules for bookmark/note edits and deletion. Add metadata/tag/collection editing separately after those rules are documented.

Acceptance: offline updates survive process termination and reconnect; duplicate delivery creates no duplicate passages; phone and desktop converge after sync. Test simultaneous edits, interrupted batches, source replacement, unavailable files, deletion, and clock skew.

### 6. Android lifecycle and reader polish

Handle app suspension, process death, rotation, safe areas, system back, font sizing, vertical layout, and touch targets. Save progress before lifecycle transitions where possible. Recover downloaded books and queued changes without the network. Keep parsing, installation, download, and sync work off the UI thread.

Acceptance: real-phone testing confirms reliable resume, offline startup, responsive tap lookup, and consistent position saving. Document storage, battery, and memory observations.

### 7. Release, updates, and recovery

Produce a signed release APK and preserve the signing key outside the repository with a documented secure backup. Verify in-place updates preserve downloaded books and unsynced changes. Provide backup/export for durable phone user data; explain uninstall/data-clear consequences. Before destructive migrations or cleanup, explain impact and provide a backup path. Check protocol compatibility with older installed clients and reject unsupported versions clearly.

Acceptance: physical-device release install/update succeeds; Windows release checks remain green; offline reader and dictionary work; pairing revocation works; source-library contents remain unchanged. Record final APK/installed sizes with and without catalog/cache separately and update documentation.

## Deferred scope

Japanese-Japanese dictionaries, true EPUB streaming, cloud sync/accounts, public internet hosting, OCR, book-content full-text indexing, and special rereading protection are outside this implementation.

