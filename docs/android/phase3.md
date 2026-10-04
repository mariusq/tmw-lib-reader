# Android Phase 3 — private desktop API and pairing

Implemented on 2026-10-04. Phase 3 only: opt-in desktop service and a minimal
Android connection panel. Phase 2 reader, tap lookup, dictionary queries, local
EPUB storage and Japanese formatting are preserved. Reader polish remains Phase
6; no mobile catalog import, download manager, cover cache or user-data sync was
added. Existing uncommitted work was retained; no commits/reset/cleanup occurred.

## Desktop service and safety

Desktop Settings → Private Android connection enables `127.0.0.1:47831`.
Every application start is disabled, including after reboot. Enabling does not
configure Tailscale, open firewall/router ports or select a source folder.
Disabling stops accepting connections, cancels active file work at chunk
boundaries and closes the current pairing window. Device grants survive desktop
restart; the service itself must be explicitly enabled again.

Network handlers run on bounded background threads (four active requests) and
use independent **read-only** SQLite connections. They do not run startup
migrations/index rebuilding, share the desktop connection mutex, derive readings
or write catalog metadata. Existing normalized Japanese/romaji substring search,
overrides, sorting and filters use `Database::browse_books` unchanged. A tiny
scanner event-sink adapter permits exercising the actual discovery/extraction/
index pipeline in tests without a GUI runtime; production emits the same events.

Only the public opaque book ID resolves a source or effective-cover path.
Unknown fields (including `path`) are rejected. No HTTP endpoint accepts a
filesystem path, filename, cache location, SQL, database export or root change.
JSON responses omit desktop filesystem paths. EPUB paths must canonicalize
inside their cataloged source root and have an EPUB extension; symlinks/junctions
that resolve outside that root are rejected. Files open read-only. Windows file
handles allow read sharing only while hashing/downloading, preventing concurrent
write/rename/delete of the opened publication. Downloads never hold a SQLite
lock. Device revocation and service-disable checks occur between hash/transfer
chunks. Bytes already delivered to an approved device cannot be recalled.

A desktop-generated 12-hex-character code lasts 120 seconds, permits at most five
attempts and authorizes exactly one device. Reissuing closes the old window.
Possession of this code is desktop approval; there is no automatic approval from
Tailscale identity alone. Maximum 16 paired devices. The phone receives a random
64-hex-character bearer credential once; the desktop stores **only SHA-256 token
hashes** in `companion-devices.sqlite3` under app-local data. This grant store is
separate from catalog backups and source folders. Revocation is durably committed
before success. Restarting the service retains grants; a revoked token remains
invalid. If a pairing response is lost, create a new code and revoke the stale
row in Settings. Tokens and pairing requests are not logged.

Android Settings contains Pair phone, Check connection, and Forget connection.
Requests run on a native worker, use platform certificate/hostname verification,
have 8-second connect/read timeouts, reject redirects, and permit only root
`https://<device>.<tailnet>.ts.net` addresses on port 443. There is no cleartext LAN
mode or certificate bypass. The bearer credential never crosses into JavaScript,
localStorage, IndexedDB, source content or WebView fetch. Android Keystore AES-GCM
encrypts its private preferences with a fresh IV on each save. Backup is disabled
for the app because the Keystore credential is installation-specific. Forgetting
removes the local connection record; desktop Revoke is required to invalidate the
grant. Existing reader/dictionary data is neither deleted nor migrated.

## Identity and content-version audit

Existing scanner identity is the catalog row for an exact stored source path.
An unchanged or changed rescan of the same path retains that row and overrides.
Moving/renaming creates a **new** row; the old row becomes unavailable on a
completed scan. Distinct editions are never merged by title/filename/EPUB
identifier. Root removal deletes its rows; subsequent insertion can reuse a
numeric SQLite ID. Existing `content_hash` is not populated by normal discovery;
size/modified time alone are insufficient as a strong content version.

Additive migration 15 introduces `companion_books`, a unique random 128-bit
public ID per local row, plus a catalog namespace ID in settings. An insert
trigger covers import batches immediately. Foreign-key cascade deletes mappings
with their rows, so a reused numeric ID receives a new public ID. Same-row
rescans/replacements retain identity. Catalog backups include these IDs and the
namespace; restoring a pre-15 backup creates mappings during the existing
migration path. Restoring a newer backup preserves them. Copying that backup to
another PC intentionally retains the catalog namespace. Pairing grants are not
included in catalog export. No destructive migration, source change or storage
cleanup is performed; the existing Export backup action remains available.

`GET .../content` hashes the complete open source read-only and returns
`sha256-<64 hex digits>` and exact byte length. This version identifies actual
bytes independently of paths, mtimes and book identity. `GET .../epub` requires
that exact `If-Match` version, rehashes the locked source, then streams a bounded
complete copy. A mismatch/missing version returns 412. Source disappearance
returns 404; detected change while hashing returns 409. Hashing is deliberately
on demand rather than repeated during every catalog page/import. It adds a full
read before download and does not yet cache hashes; HDD throughput for large
manga is unmeasured. Future Phase 4 clients must hash the downloaded copy before
acceptance and key reader anchors by content version, preserving Phase 2's
content-hash resume behavior. An invalid CFI must never migrate silently to a
replacement version.

## Protocol v1

Bearer authorization is required for every endpoint except the desktop-code
redemption endpoint. Grants allow read-only access to the current catalog;
there are no per-book grants. Desktop commands may revoke any device; `DELETE /v1/device` revokes only the
authenticated caller. There is no remote administrative device-list/revoke endpoint. Errors are bounded JSON with stable error
codes and no filesystem/SQL diagnostics. Unsupported version prefixes return
426 to authenticated clients; unauthorized requests always return 401.

| Method / route | Meaning |
| --- | --- |
| `POST /v1/pair` | `{ "code": "12 hex", "name": "device name" }`; returns protocolVersion, deviceId, token once (201) |
| `DELETE /v1/device` | Revoke only the authenticated caller, then reject its credential |
| `GET /v1/status` | Authorized connection probe; protocolVersion, connected, catalogId, maxPageSize |
| `POST /v1/catalog` | Paginated existing browse/search query; effective fields, opaque IDs, availability, cover presence, finished flag |
| `GET /v1/books/{id}/metadata` | Effective display fields, bounded tags, availability, byte-size hint, content-version endpoint |
| `GET /v1/books/{id}/content` | Strong contentVersion and byte length; no book bytes |
| `GET /v1/books/{id}/cover` | Authorized, bounded JPEG thumbnail; versioned ETag |
| `GET /v1/books/{id}/epub` | Read-only complete EPUB copy with required `If-Match` content version; version ETag |

Catalog request example (body fields match the desktop query):

```json
{"libraryRootId":null,"tagId":null,"collectionId":null,"needsMetadata":false,"hideDuplicateTitles":false,"readingStatus":null,"query":"猫","sort":"title","offset":0,"limit":50}
```

Responses have `protocolVersion: 1`, `items`, `nextOffset` and
`consistency: "live"`. A full last page can yield an extra empty request. These
are live offset pages, **not** a consistent snapshot or delta cursor: concurrent
imports/edits can shift rows between pages. No durable mobile catalog consumes
these pages in Phase 3. Phase 4 must finalize consistent snapshot/delta cursors,
revision counters and deletion/tombstone semantics before durable synchronization.
Currently deletion simply makes its opaque ID return 404; unavailability keeps
the metadata row and returns 404 for source access. No write/sync ordering,
idempotent user mutation or clock-based conflict semantics is enabled here.

Major compatibility changes use a new URL prefix; older supported routes must
retain their contract. Additive response fields may be ignored. Unknown request
fields are rejected. Android checks protocolVersion before storing a credential
or declaring a connection. Cover encoding changes must bump the ETag prefix;
EPUB version is always the actual SHA-256, independent of thumbnail encoding.

Bounds: HTTP/1.1 only, one request per connection, no chunked requests or
WebSocket/Range/multipart/compressed-body support, no CORS/browser Origin,
16 KiB headers, 8 KiB body, 1,024-byte target, five-second total request-read
deadline, four simultaneous handlers, five-second socket-write timeout and
120-second source hash/stream deadline. Catalog limit 1–100, offset 0–1,000,000,
query ≤512 UTF-8 bytes; approved sort values title/author/series/dateAdded/
modified/folder. Display strings truncate at 512 Unicode scalars; metadata tags
cap at 100. Device names ≤80 UTF-8 bytes without controls. SQLite lock wait is
750 ms. No request queue grows with library size. Busy responses are 503.
EPUB length ≤512,000,000 bytes. Content range/resume is future scope.

Mobile transport covers are generated on demand at ≤240×360, JPEG quality 75,
8-bit RGB (alpha discarded), no mobile persistent cache. Inputs cap at
16,000,000 encoded bytes, 8,192×8,192 decode dimensions and 64,000,000 decoder
allocation budget. Decode failures are 422. ETag is `cover-v1-<SHA-256 of JPEG>`.
Headers use no-store and nosniff. Encoding is bounded to four workers; size and
real-phone decode-performance tuning remain Phase 4, alongside caching policy.

## Private Tailscale setup

Use the same private tailnet on PC and phone. Enable MagicDNS and HTTPS
certificates in the Tailscale admin settings if needed; the certificate uses the
PC's `.ts.net` DNS name. Restrict tailnet grants/ACLs to your own approved phone
and PC HTTPS service. App pairing is an additional independent credential check.
Keep the desktop running, service enabled, PC awake, source HDD available, and
Tailscale connected. No background Windows service or wake-on-LAN is installed.
Sleep, closing the desktop, unavailable drives and disconnected Tailscale prevent
new requests; local Phase 2 reading/lookup still work offline.

```powershell
tailscale serve status
tailscale serve --bg --https=443 http://127.0.0.1:47831
# Copy the HTTPS .ts.net address printed by Serve into Android Settings.
# Stop only this Serve listener when no longer wanted:
tailscale serve --https=443 off
```

Inspect existing Serve configuration first; do not replace another service's
listener. Use the loopback HTTP reverse proxy target above, never a source
folder/file-share target. Do not enable Funnel or router forwarding. Serve is
private to the tailnet; `--bg` persists its proxy configuration across restarts,
but this application remains disabled until explicitly enabled on each desktop
start. Reference: [Tailscale Serve CLI](https://tailscale.com/docs/reference/tailscale-cli/serve)
(checked 2026-10-04). No Tailscale settings/certificates were changed in this task.

## Commands and validation

Results and artifact checks below are recorded for this implementation; reuse
Phase 1/2 physical-phone functional evidence instead of rerunning reader suites.

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo fmt --manifest-path apps/android/src-tauri/Cargo.toml
cargo test --offline --manifest-path src-tauri/Cargo.toml --lib -- --nocapture
& 'C:\Program Files\nodejs\npm.cmd' run lint
& 'C:\Program Files\nodejs\npm.cmd' run test
& 'C:\Program Files\nodejs\npm.cmd' run build
& .\apps\android\Android.ps1 -Action Build -TestSigning -Target @('x86_64','aarch64')
# Existing emulator only; preserve application data:
adb -s emulator-5554 install -r <x86-64 APK>
adb -s emulator-5554 shell am start -W -n com.tmw.companion/.MainActivity
apksigner verify --verbose <ARM64 APK>
zipalign -c -P 16 4 <ARM64 APK>
```

Completed results:

- Windows Rust regression: **41 passed**, seven existing performance tests
  intentionally ignored. Both earlier-schema progress migration tests passed
  after making migration 15 safely idempotent and updating their expected schema
  version. Existing import/cancellation, overrides, recovery, backup, search and
  source-safety tests remain green. The final self-revocation endpoint addition
  was covered by a targeted companion rerun: three tests passed.
- Frontend: **37 tests passed**, ESLint passed, Windows production frontend build
  passed. Both Android production frontend builds and signed APK builds passed.
  Existing large frontend chunk warnings remain; no reader bundling changes.
- Windows debug executable build passed. Windows installer/release packaging was
  not repeated; this checkpoint does not claim an installer acceptance pass.
- Real loopback HTTP tests cover disabled startup, wrong/missing authorization,
  one-use pairing codes, approval, persisted grants across service restart,
  desktop revocation and authenticated self-revocation, rejected caller paths,
  unknown IDs, out-of-root catalog paths, page bounds, effective override search,
  metadata, SHA-256 versions, missing/wrong If-Match, exact EPUB copy bytes,
  authorized/unauthorized bounded covers, numeric-ID reuse, same-row rescans and
  pre-/post-15 backup restore. Generated sources remain unchanged after import
  and download; no app-managed files appeared inside their source folder.
- The actual two-worker desktop import pipeline ran with **512 original generated
  EPUB fixtures**, alongside content-version, EPUB and cover requests and nine
  paginated Japanese search requests. Representative targeted observation: import
  **310 ms**, maximum concurrent search request **28 ms**. Earlier targeted run:
  312 ms / 29 ms. These small generated-fixture SSD/system-temp observations
  establish overlap and correctness, not HDD/real-manga throughput or UI frame
  latency. No source-library scan or benchmark expansion was necessary.
- Existing **TMW_Test**, API 36/x86-64, updated with `adb install -r`, using the
  same local development key. Release launches measured 413 ms initially,
  494 ms after the corrected text update, and 304 ms after the final restoration.
  These are `am start -W` activity timings, not WebView readiness benchmarks.
  Existing `reader-vertical.epub` proof copy reopened after updates, and the
  Private PC connection controls plus native Not paired status were verified.
  No reader typography/lookup acceptance work was repeated.
- Native Android instrumented test: **one passed** (135 ms test runtime), using
  separate `phase3-test-*` preferences. Actual Keystore AES-GCM encryption
  round-tripped after constructing a new native client, stored preferences did
  not contain the plaintext token, and HTTP, non-tailnet, path-bearing,
  user-info, query-bearing and alternate-port addresses were rejected.
  Only isolated test preferences were cleared. Production connection, reader,
  dictionary and WebView storage were preserved.
- Final ARM64 APK passed `apksigner verify` (v2 scheme),
  `zipalign -c -P 16 4`; all native ELF LOAD segments report `0x4000` alignment.
  Reused the existing SDK, AVD and key; no initialization, AVD recreation,
  uninstall, app-data clear or network-state change occurred.

Native-test commands (run from the repository root with the documented SDK/Java
session environment):

```powershell
$native = 'apps/android/src-tauri/gen/android'
& "$native/gradlew.bat" -p $native assembleX86_64Debug assembleX86_64DebugAndroidTest -x rustBuildX86_64Debug --no-configuration-cache --no-daemon
# Uses the existing release JNI staging; no extra ABI/native build.
adb -s emulator-5554 install -r "$native/app/build/outputs/apk/x86_64/debug/app-x86_64-debug.apk"
adb -s emulator-5554 install -r "$native/app/build/outputs/apk/androidTest/x86_64/debug/app-x86_64-debug-androidTest.apk"
adb -s emulator-5554 shell am instrument -w -e class com.tmw.companion.ConnectionStorageTest com.tmw.companion.test/androidx.test.runner.AndroidJUnitRunner
# Restore the final release wrapper in place after instrumentation:
adb -s emulator-5554 install -r "$native/app/build/outputs/apk/x86_64/release/app-x86_64-release.apk"
```

Concrete failed attempts and bounded repairs: restricted esbuild could not
resolve the parent directory, so the same frontend checks ran successfully in
an approved unrestricted shell. A GUI mock runtime pulled an incompatible
Windows test DLL entry point; replacing only the scanner's event sink avoided
GUI initialization and the production pipeline regression passed. Older-schema
fixtures retained the new identity table, so the additive migration was made
idempotent; no catalog data was removed. The first mobile smoke check found a
stale no-PC-connection sentence: it was corrected and the affected signed APKs
rebuilt once. Android test dependencies were initially absent from offline
Gradle caches; only the already-declared test dependencies were fetched. Release
instrumentation's old runner crashed on a stripped tracing class, so the test
uses a debug wrapper with the same production connection implementation/JNI.
Its first Activity fixture used the instrumentation thread; only test Activity
creation was corrected to the main thread and the small test APK rebuilt.
The final phone APK was unaffected by those test-wrapper repairs.

## Final artifacts and verified checkpoint

Version **0.3.0**, same local development-key signing as Phases 1/2; update in
place. Do not uninstall or clear application data. Normal release outputs:

| ABI | File below `apps/android/src-tauri/gen/android/app/build/outputs/apk/` | Exact bytes | SHA-256 |
| --- | --- | ---: | --- |
| ARM64 | `arm64/release/app-arm64-release.apk` | 88,753,767 | `2435a2def7c11215240d36e1006551facc09a252cbe7c6f901d15ee594d411b4` |
| x86-64 | `x86_64/release/app-x86_64-release.apk` | 89,386,972 | `4dc2e563be556215170801379a594b2e2dde3a0f960a2860fecfe34d726610c2` |

**Physical-phone HTTPS pairing/connection verified by the user on 2026-10-04.**
The desktop screenshot showed the service listening and an Android phone grant.
The user confirmed that Android Check connection reports
“PC connected · protocol 1”, establishing approved physical-phone connection
through the private HTTPS setup. No phone model, OS version or timings were
supplied for this verification. The user also confirmed physical-phone
revocation: after revoking the device, the phone reported “device unauthorized”.
Approved access and rejection after revocation are therefore verified on the
physical phone, alongside the automated desktop/self-revocation tests.
Earlier read-only Tailscale inspection found no Serve configuration/certificate
domains; that was the pre-setup state, not the current acceptance status. No
Tailscale settings were changed by the agent. Later phases remain unstarted.

Also unverified: minimum-API runtime, 16 KB-page physical runtime, real HDD/manga
transfer performance, long slow downloads/cancellation timing under a real
Tailscale proxy, hardware-backed Keystore characteristics on the phone,
TalkBack, and desktop GUI frame latency during network downloads. Existing
Phase 2 phone lookup evidence is reused. Live-offset pagination and on-demand
full-file hashing are explicitly documented limits to address before Phase 4.

Approximate task-time accounting: **22 minutes development/documentation,
10 minutes validation (~31% validation)**. Validation includes failed checks,
verification builds, both required signed APKs, test-wrapper packaging, emulator
operations, diagnostic checks and validation waiting. Overlapping work is
counted once: test/build runtime is assigned to validation even when
documentation progresses alongside it; development excludes those overlaps. No extra work was added to pad
the ratio, and no testing-budget exception was needed.

Source safety: `F:\tmw collection` and `testLibrary/` were neither read nor
written during this task. Only original generated temporary EPUBs were used for
API/import tests; no source EPUBs were bundled, changed, renamed, moved, deleted
or uploaded. Covers/test captures/keys/APKs/dictionaries remain ignored. No
Phase 4 or Phase 5 work was implemented; Phase 6 reader polish remains deferred.


## Phase 4 handoff

Phase 3 acceptance is complete for its authorized scope. The remaining platform
and performance observations above are limitations, not outstanding phone
pairing/revocation acceptance. No additional builds or checks were run for this
documentation handoff. See [Phase 4 handoff](phase4-handoff.md) for implementation
entry points, protocol decisions, mobile storage/cache requirements and the
copyable next-agent prompt. Record new implementation and validation in
`phase4.md`; preserve this report as Phase 3 evidence.
