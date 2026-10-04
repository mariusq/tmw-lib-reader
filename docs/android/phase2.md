# Android Phase 2 — offline reader and bundled lookup proof

Implemented for personal use on 2026-10-04. This checkpoint implements Phase 2
only; there is no PC service, pairing, catalog sync, or network download.
The user confirmed physical-phone operation and fast lookup. They reported poor
UI and incorrect Japanese formatting: this is a functional proof, not a polished
reader. Phone model, OS version and measured phone timings were not supplied.

## Shared-code audit and implementation

The desktop reader was audited before extraction. `packages/reader-core/`
contains the existing ruby-free text collection, UTF-16 to Unicode-code-point
offset conversion, bounded sentence context, metadata-preserving rendition
options, and serialized page navigation with RTL spine
recovery. Both apps import those same implementations. The desktop retains its
insertion-caret click behavior; Android additionally tests the tapped glyph's
range rectangles to avoid choosing the next character when tapping a glyph's
second half. Rectangles support either writing mode and reject margin taps.

`crates/japanese-core/` supplies the same cached IPADIC tokenizer and JMdict
importer. Token offsets now use Lindera's original byte positions converted to
code points, preserving whitespace and emoji offsets. Lookup normalizes NFKC
and whitespace identically and tries surface, lemma, then reading, as on the
desktop. Exact term results precede exact reading, then prefix results; shorter
terms precede longer ones, limited to 12. A shared row-ID tie break makes equal
length results reproducible across SQLite query plans. Prefix patterns are now
bound as a second parameter instead of concatenated inside SQL, preserving
matching semantics while allowing SQLite's prefix index optimization.

Android uses epub.js, respecting publication layout, flow and progression
metadata. A system file chooser reads a manually supplied EPUB copy. One copy
is retained transactionally in app-private WebView IndexedDB; the current CFI
is retained in localStorage. Positions are keyed by the EPUB content SHA-256, so a different replacement starts afresh while reselecting identical content resumes. Stale relocation callbacks cannot write a replacement book's position. No
source URI is retained and no source writes are requested. This intentionally
small proof is not the Phase 4 download/catalog implementation.

Short primary-pointer taps (at most 450 ms and 10 px movement) open a native
HTML modal dialog. Movement, scrolling, cancellation, long presses, links and
ruby annotation taps do not look up a word. Explicit page buttons provide
unambiguous navigation. The dialog has an accessible title, focus containment,
a touch-sized Close button and Android Back dismissal through WebView history.
Request generations prevent old lookup results from replacing newer results.
EPUB scripted content is disabled by epub.js's default sandbox policy; the
app's CSP excludes remote connections and image/font sources. No dictionary
query, reading text or source file is sent over a network or logged.

## Reproducible offline dictionary provisioning

`Android.ps1 -Action Build -TestSigning` verifies the supplied JSON's SHA-256,
runs `crates/dictionary-build/` using the shared importer, regenerates notices,
then builds both APKs with the existing SDK and development key. The build tool
disables the optional tokenizer feature: dictionary conversion does not need
IPADIC or a network download. Runtime builds keep that feature enabled.

Input: `jmdict-eng/jmdict-eng-3.6.2.json`, date 2026-09-28, SHA-256
`5f54504a62a7f45741e1bf6fd28f6e1e5add6405f2829748cc004a802839a3aa`.
Output: 499,285 forms in `jmdict-20260928-v2.sqlite3`:

| Asset | Bytes | SHA-256 |
| --- | ---: | --- |
| Indexed SQLite | 128,303,104 | `ddb2bfd6fc862ba234d4a856e023ceacfcd714359ef08c201bfc1bd12aab15d1` |
| Bundled gzip | 31,000,394 | `9424b8595976c9da89718e6f25c993b501b16e189b91735e3cb1f73fcfd571f8` |

The APK embeds only the compressed, already indexed database, not raw JMdict
JSON or EPUBs. First lookup streams decompression into one app-private temporary
file, flushes it, validates SQLite `quick_check`, closes it and atomically renames
it. Subsequent lookups open the versioned database read-only. Interrupted first
installation is retried by overwriting the same temporary file; no growing
collection of partial files is created. Tokenization and database work run on a
blocking worker, serialized by a mutex, outside the UI thread. There is no
destructive catalog migration or storage cleanup in this checkpoint.

The installed copy is necessary for random-access SQLite; the APK's compressed
asset remains available for offline installation. JSON is not retained on the
phone. Developer uncompressed/gzip outputs, fixtures, screenshots, targets and
APKs stay Git-ignored. Future data updates require an intentional source hash,
date, versioned asset name and notice update followed by an in-place APK update.

About / Settings loads bundled notices only on request. These include EDRDG
attribution, source/date/hash, conversion modifications, CC BY-SA 4.0 text,
IPADIC notices and frontend/Rust dependency notices. Dictionary data has its
own license; it is not relabeled as application code. Sources:
[EDRDG](https://www.edrdg.org/edrdg/licence.html) and
[jmdict-simplified](https://github.com/scriptin/jmdict-simplified/blob/master/LICENSE.txt).

## Completed validation

Toolchain and APK commands are maintained in [feasibility.md](feasibility.md).
`Verify-Dictionary.py` and `Generate-ReaderFixture.py` under `apps/android/`
provide the dictionary comparison and original generated reader fixture.
The following checks are historical evidence; choose new checks according to
the canonical testing budget in `../../AGENTS.md` rather than rerunning this list.

Validation passed: 37 frontend tests (the existing 34 plus glyph/tap tests),
38 Windows Rust tests with seven intentional benchmarks ignored, two Android
host tests, and four shared Japanese-core tests. Windows frontend build and
lint passed. The existing source-safety and catalog tests remain green.

`Verify-Dictionary.py` checks integrity, form count, matching result sequences
against the desktop enabled-dictionary join on the same data, and query plans
for 猫, 食べ, 食べる, 日本語, 学校, ねこ, half-width ﾈｺ, and a missing term.
On this host, the original concatenated-prefix query scanned all entries and
took 125–140 ms; the final bound-prefix query took 0.1–1.7 ms in individual
observations. Its plan uses `MULTI-INDEX OR`, the exact-term/reading indexes and
the case-insensitive prefix index, followed by a bounded-result sort. These
host measurements are separate from emulator measurements.

## Known limitations

- This proof keeps one selected EPUB, with a 64,000,000-byte input limit. EPUB
  ZIP inflation and epub.js memory depend on the publication; large manga,
  malformed archives, DRM and pathological ZIP inputs are not validated here.
  No manga/layout compatibility claim is based on the small generated fixture.
- Local copies and positions survive process restarts and in-place updates;
  uninstalling or clearing application data removes them. Durable exports and
  broader library storage remain later-phase work. A new selection replaces
  the single proof copy; it does not change the manually supplied source.
- Generated EPUB testing demonstrates horizontal/vertical text, ruby, emoji,
  inflections and split elements. Real-book typography and TalkBack interaction
  are not established by those fixtures. The user subsequently reported poor
  Japanese formatting on the phone; TalkBack remains unverified. No
  source-library EPUB was accessed during implementation.
- epub.js inspects the document root's writing mode for pagination. Vertical
  styles declared only on the body rendered text and permitted lookup but
  did not paginate correctly in the generated stress test. The verified
  vertical publication declares `vertical-rl` on `html` and `body`. A mixed
  horizontal/LTR-content publication with global RTL progression also produced
  inconsistent page order. This checkpoint preserves publication styles and
  records these epub.js compatibility limitations instead of overriding them.
- Mobile cover caching, bookmarks/notes, secure pairing and sync are deferred.
  No standalone cover cache is created by this proof.
- Minimum API 24 and 16 KB runtime compatibility remain declared/unverified as
  in Phase 1. Runtime tests use the existing Android 16/API 36 x86-64 emulator.

Phone installation uses the same local development key as Phase 1, version
0.2.0. Preserve that key and update in place without uninstalling or clearing data.
Reader layout needs later work; Phase 3 should preserve existing lookup behavior.

## Emulator measurements and final artifacts

Existing `TMW_Test`: Android 16/API 36, x86-64, 320×640, 4 KB pages.
Airplane mode enabled and Wi-Fi disabled for the reader/lookup checks; original
networking state restored afterward. Individual observations, not averages:

| Measurement | Result |
| --- | --- |
| Activity launch / offline restart / update launch | 340–504 ms |
| First dictionary installation plus lookup | 721.3 ms |
| Warm indexed ruby-base lookup | 0.8–1.6 ms |
| Warm vertical inflection lookup | 0.9 ms |
| Split-element inflection lookup | 1.0–17.0 ms |
| Lookup after process restart | 2.4 ms |
| First lookup after final APK update/process start | 43.6 ms |
| App process PSS / RSS after first install and reading | 133,959 / 269,840 KiB |
| WebView renderer PSS / RSS at the same checkpoint | 101,867 / 262,288 KiB |
| Android Settings app / user data / cache / total | 90.94 MB / 254 MB / 381 kB / 345 MB |

`am start -W` measures the activity launch, not complete WebView/EPUB readiness.
Lookup times measure backend tokenizer + installation/query work, excluding
queue wait, IPC delivery and dialog paint. PSS and RSS include mapped code and
shared WebView/font pages; the separate renderer must not be omitted. These
small generated EPUBs do not establish large-publication memory limits.

The emulator retains an earlier **116,318,208-byte test-only v1 dictionary**
alongside the final 128,303,104-byte v2 database. It was deliberately not deleted
or cleared. Therefore its 254 MB user-data observation is not the expected
storage of a Phase 1 phone upgraded directly to Phase 2: that phone installs
only v2, plus its selected EPUB and WebView state. The APK itself is about 89 MB;
the indexed definitions add 128.3 MB of private data on first lookup.

Final validation: separate horizontal/LTR and vertical/RTL publications open
offline; ruby bases and inflected words, including split nodes, return the
desktop-equivalent definitions. Both page directions were exercised, Android
Back dismisses lookup, swipe and 750 ms long press do not open it, and process
restart retains the selected EPUB and CFI. In-place APK updates preserved data.
Offline About/Settings attribution was visually verified. TalkBack remains
unverified. Generated captures and fixtures are in ignored `.test-results/`.

Only original generated test material was transferred to the emulator.
`F:\tmw collection` and `testLibrary/` were neither read nor written during this
implementation/test run; no source EPUB was modified, renamed, moved or deleted.

Final APKs, release profile, same local development-key signing as Phase 1:

| ABI | File | Bytes | SHA-256 |
| --- | --- | ---: | --- |
| ARM64 | `app-arm64-release.apk` | 88,722,019 | `8a2ac06ab2bb37d81a7930a791c98654e9fa2bdbc0c007ac71614a16b0e34b3f` |
| x86-64 | `app-x86_64-release.apk` | 89,350,272 | `eac23c563f0bd1064518ac2e8eda0e96456a3668486a97e8101752449e6452fc` |

Paths remain the Phase 1 documented ABI release-output directories. Both APKs
passed `apksigner verify` (v2 scheme) and `zipalign -c -P 16 4`. All ARM64 ELF
LOAD segments use `0x4000` alignment. ARM64 functional operation was subsequently confirmed by the user; 16 KB
runtime compatibility remains unverified. No Phase 3 implementation was started
during Phase 2. The subsequent [Phase 3 report](phase3.md) records completed
private API/pairing work and user-confirmed phone connection and revocation.
Use [Phase 4 handoff](phase4-handoff.md) for the next authorized implementation;
the APK hashes above are historical Phase 2 artifacts.

