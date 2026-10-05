# Dictionary improvements — implementation evidence

Phases 1–4 are implemented. Stage 5 hardening is implemented on 2026-10-05;
remaining platform/device acceptance is listed in its checkpoint below. Specific lookup
timings and numerical latency targets are optional by user instruction; none are
needed to proceed. This report does not establish physical-phone acceptance.

## Phase 3 — shared chunk lookup and exact batched stores

`chunk_lookup.rs` implements a new portable engine result alongside contract v1.
It searches multiple original text lengths containing the click, independently
of tokenizer boundaries, and retains original scalar offsets through NFKC and
hiragana/katakana variants (including halfwidth voiced kana). Independently
authored suffix rules cover ichidan/godan past, polite and negative forms,
i-adjectives, common suru/kuru forms, te-progressives, potential, ichidan
causative/passive chains and contracted `ちゃった`/`じゃった`. Reverse steps track
grammatical classes; inflected matches require compatible dictionary rules.
JMdict raw/expanded POS is adapted in the shared store. Empty or unknown rule
metadata supports exact matches but does not authorize arbitrary deinflections.
The cached tokenizer provides a bounded target hint on a miss; it cannot dictate
candidate boundaries. Unsupported forms can still miss.

Ranking chooses longer validated original spans, fewer deinflection steps,
direct normalization, exact term before reading, dictionary priority, score and
stable ties. Only results for the winning original span are returned, so a
single passage anchor never silently combines shorter spans. Results group by
headword and reading, preserve alternate readings, and deduplicate rows within
their source namespace. Provenance carries source kind/title and revision;
SQLite IDs remain explicitly ephemeral. Safe glossary JSON, tags, rules, score
and sequence remain available for Phase 4 rendering. Source/title/revision is
provenance, not a claim that an imported row is a portable saved-word identity.

`lookup_storage.rs` provides the exact same batched query implementation for
desktop catalog entries, the Android bundled entries table and imported format-3
terms. It uses normalized term/reading index seeks, never prefix hits. Enabled
sources and priorities are respected. `Storage` implements the engine store;
two stores can be composed through the shared tuple adapter to retain JMdict.
No database migration, live dictionary import or library-source access is needed.

Bounds: input <=16,000 scalars, local tokenizer window <=96 scalars, <=8 starts
within seven scalars before the click, original term <=32 scalars, <=512 generated
candidates/unique query keys, <=4 reverse steps, <=4096 queued states and <=4096
materialized rows. Keep one best candidate per row rather than allocating a
candidate/row cross product. Each SQLite store supports <=32 enabled sources,
<=12 rows per key/source/index seek and <=12 per-key term or reading hits after
priority ranking. Keys are <=512 bytes each / <=128 KiB per batch; serialized
materialized entry payloads are <=8 MiB per store. A composed pair may materialize
up to two such payload budgets, with <=4096 combined rows. Final response <=12
source rows. Source and payload overflows are explicit errors; matching buckets
beyond the per-key cap are deterministically limited. Within a single source,
the seek's first 12 row IDs are considered before score ranking; higher-scored
later duplicate senses are a remaining hardening limitation. Bound errors
propagate instead of appearing as successful empty lookups. No whole-book parse,
full-dictionary scan, network request or UI-thread work belongs to this engine.

Official [term-bank-v3 schema](https://github.com/yomidevs/yomitan/blob/master/ext/data/schemas/dictionary-term-bank-v3-schema.json)
and [translator](https://github.com/yomidevs/yomitan/blob/master/ext/js/language/translator.js)
were rechecked. No GPL upstream implementation/rule data was copied. Supported
subset remains Japanese format-3 term/tag dictionaries; dictionary-provided
custom transformations, exhaustive contractions/dialects, long-vowel expansion,
frequency/pitch and full Yomitan compatibility remain unsupported/deferred.

Phase 4 boundary: both current readers keep the Phase 1 adapter and v1 result
shape. The new grouped result API is callable from Rust with either platform's
existing SQLite connection and imported `Storage`; reader command selection,
highlighting, safe structured/local-asset rendering and portable imported-result
history resolution are not enabled by this stage. Existing reader behavior,
passage/history IDs and frontend management notices therefore remain unchanged.
No APK/package/version bump or physical-device acceptance is claimed.

### Phase 3 validation

- Shared Rust final regression: **28 passed, 1 ignored**, using
  `cargo test --offline --manifest-path crates/japanese-core/Cargo.toml --features yomitan --lib`.
  This includes the existing engine/importer regressions and new chunk/store tests.
- All 12 generated comparison cases match desired original spans, headwords and
  alternate readings against a shared synthetic dictionary with competing short
  terms. Seven prior span/ranking gaps improve (compound, past, polite past,
  negative past, contraction, adjective past and inline compound); five previous
  desired matches remain. This is a fixture accuracy result, not Yomitan parity.
- Coverage includes four auxiliary chains, godan subclass rejection, unknown
  compounds spanning tokenizer boundaries, voiced-halfwidth/non-BMP offsets,
  punctuation, deterministic grouping, priority/source-ID collisions, composed
  fallback errors and pathological candidate/depth/result caps.
- All three actual SQLite layouts use term/reading index probes. Query-plan
  assertions also reject final `SCAN e`; materialized bounded hit IDs and CROSS
  JOIN ordering prevent the optimizer from choosing a whole-entry-table scan.
  Tests cover source enable/priority, large homophone buckets, too many sources,
  row/payload overflow, exact-only matching and expanded/raw JMdict POS.
- A generated local format-3 ZIP imports into temporary `Storage`, reopens,
  matches `食べました` -> `食べる` over the entire original span with safe structured
  JSON and source/revision, then stops matching when disabled. This is also the
  brief host engine smoke check; no live app dictionary or private ZIP was needed.
- Desktop and Android **host** Rust library checks passed offline. The checks
  initially lacked Lindera assets in new Cargo build-cache directories; the
  existing cached IPADIC archive was reused in ignored build output and retries
  passed. No dictionary download, new dependency or lockfile change was needed.
- Changed new Rust files passed rustfmt and documentation passed diff/readback.
  No frontend change, APK/ARM64 build, native UI smoke or physical-device test
  was performed. Prior Phase 1 DOM evidence is reused for ruby exclusion,
  inline text and vertical DOM collection; the collector did not change.

The synthetic corpus is desired behavior, not observed Yomitan output; direct
same-dictionary/settings comparison, exhaustive grammatical coverage, rare
homophone/duplicate-sense ranking and device responsiveness remain unverified.
Desktop's existing single-column indexes can visit more rows inside an exact-key
bucket while filtering dictionary scope; imported composite indexes constrain
both key and source. This stage bounds input, outputs and candidate work but does
not claim a numerical latency guarantee for every existing database distribution.

Approximate elapsed work: 20–25 minutes development/audit/documentation and
2–3 minutes validation, overlapping checks counted once. The final shared pass
was repeated only after concrete failures/fixes and the added composition/POS
checks. No broad frontend suites or both-ABI packaging were run. Validation is
under the 50% budget; unrelated dirty/untracked work and source materials remain
preserved.

## Phase 2 — shared local Yomitan import

Implemented 2026-10-04. `crates/japanese-core/src/yomitan.rs` independently parses
format-3 ZIPs through `Read + Seek`; `dictionary_storage.rs` supplies the same
version-1 SQLite schema and transactional sink on both apps. No Yomitan code or
deinflection rules were copied. The official upstream index, term-bank-v3 and
[tag-bank-v3 schema](https://github.com/yomidevs/yomitan/blob/master/ext/data/schemas/dictionary-tag-bank-v3-schema.json)
were rechecked before implementation. Supported subset: format 3 (`format` or
`version`), eight-field term tuples, five-field tag tuples, string/structured/image
glossary data and dictionary-provided deinflection records. This is not universal
Yomitan compatibility. Other formats, legacy tagMeta, frequency/pitch, kanji and
audio are visibly unsupported/deferred. Unknown files/banks produce persisted
warnings; dictionaries with no term rows fail rather than appear usable.

The original index metadata, publisher attribution, structured glossary, term and
definition tags, rules, score, sequence and alternate reading rows are retained.
Empty headword/reading fields are valid format-3 strings and remain in storage;
later lookup must exclude empty matching keys. Definition data never becomes HTML.
Separate safe JSON permits a fixed element list, preserves text/local image paths,
and disables CSS, links, event attributes, scripts and remote resources. Unsupported
elements have placeholders and persisted management warnings. Raw presentation
fields remain available for Phase 4; safe JSON is not a complete renderer.

Storage is app-local `yomitan-v1/dictionaries.sqlite3`, separate from the existing
catalog and bundled JMdict. A single FULL-synchronous WAL transaction publishes
terms, tags, image BLOBs, metadata and counts together. Failure/cancellation/drop
rolls back the entire new import or replacement. Explicit replacement requires
the selected dictionary's title and preserves its ID, enabled state and priority.
New duplicate-title imports fail with a Replace instruction. Removal accepts only
an imported row ID and cascades generated rows/BLOBs; it never accepts a source path.
SQLite may retain freed pages after removal; no destructive vacuum or migration
of existing stores was performed. Images are isolated BLOBs, so archive filenames
never become filesystem writes or Windows case-alias paths. Ephemeral dictionary/
term IDs are not added to saved passage/history identities.

The shared `DictionaryManager` is available in desktop Settings and the reader
footer, and Android Settings. It supports multiple imports, persistent enable/
disable and priorities (-1000..1000), explicit replacement, removal confirmation,
progress, cancellation, attribution and retained warnings. It explains that imported
reader lookup/definition rendering belongs to Phases 3–4; JMdict remains the active
lookup source. Import operations run on blocking workers. The bounded shared job
state allows one import per app and 500 ms polling only during active work; UI
unmounts ignore stale responses. Management never executes SQL in JavaScript.

Windows uses a backend ZIP picker and opens the selected file read-only. Storage
and newly added library roots are checked for overlap, including existing storage
symlinks. Android uses `ACTION_OPEN_DOCUMENT` with read permission, reads a content
URI on its own worker, and copies at most 512 MB into a fixed app-private cache
file. Rust consumes that seekable file and removes only the managed temporary
copy after success/failure. No normal path is assumed for the provider document.
Cancel cannot dismiss the OS selector; returning from it after cancellation cannot
publish an import. Provider-copy progress is a waiting state rather than a byte
percentage. An interrupted process may leave that fixed cache file; the next
selection overwrites it. Live sources and credentials are never touched.

Bounds: ZIP <=512 MB, declared/actual expansion <=2 GB, <=100,000 archive entries,
bank <=64 MB, index <=1 MB, individual row <=1 MB, <=5 million term/tag rows,
definition nesting <=32, <=100,000 distinct image references. Banks stream raw
JSON rows before allocating a JSON tree, then flush <=128 rows or ~4 MB raw row
data per batch; a rejected oversized row's transient raw buffer is bank-bounded.
Statements are cached, normalized keys indexed, and no full dictionary is held
in memory. ZIP paths reject traversal, absolute/drive/ADS/backslash/reserved-name
paths, duplicate names and symlinks. Reads check actual entry size, CRC/EOF and
cancellation. PNG/JPEG/WebP assets are limited to 16 MB and 32 million pixels by
header inspection and stored atomically. Full pixel decoding/rendering must apply
limits again in Phase 4; header checks alone do not prove every compressed image
decodes. Total dictionary storage has expansion bounds per import, not a device-wide
quota; low-space failures roll back and show storage guidance.

### Phase 2 validation and remaining acceptance

- Shared importer/store: eight active tests passed (including existing JMdict)
  covering field preservation, unsafe paths, malformed deinflections, missing
  assets, sanitized content, local image BLOBs, cancellation at publication,
  failed replacement rollback, retained settings and cascading removal.
- The supplied Downloads ZIP was opened read-only and fully imported into an
  automatically deleted temporary database: 92,217 term rows, zero tag rows and
  2,598 images. Source length/mtime were unchanged and database reopen/list passed.
  One early rejection of schema-valid empty headwords was corrected and the full
  fixture then passed. No private data or assets were committed or uploaded.
- Desktop dictionary regressions: two passed, including the Phase 1 corpus.
  Android host bundled lookup/atomic install regression passed; both Rust command
  adapters compiled. An offline Lindera cache miss was resolved by copying the
  existing verified cached IPADIC tarball into the new ignored build output.
- Shared management UI: three tests passed for enable/priority payloads, explicit
  replacement failure, active cancellation and removal confirmation. Existing
  desktop/mobile reader regressions: nine tests passed. Desktop and Android
  TypeScript and targeted ESLint checks passed.
- Both production frontend builds passed with runner config loading (the existing
  >500 kB advisory remains). Android ARM64 release Kotlin compile passed using
  existing Gradle dependencies without rebuilding Rust/packaging another APK.
  Android dependency notices and locked ARM64 inventory were regenerated to include
  the importer/image dependencies. Source dictionaries' notices remain separate.

Real ZIP compatibility was exercised on the shared host implementation, not through
both running app pickers. Physical Android provider selection, low-storage/provider
faults and cancellation, and desktop native-picker smoke acceptance remain
unverified. No APK/installers were packaged, installed or version-bumped; no phone
data/signing keys or live catalog were changed. No imported dictionary was placed
in live app storage automatically. Performance/memory hardening and direct Yomitan
comparison remain Phase 5; peak memory and device throughput were not measured.

Approximate Phase 2 work: 20–25 minutes development/audit/documentation and 3–4
minutes validation, counting overlapping checks once. This is under the 50% budget.
The fixture and affected tests were repeated only after concrete failures or later
changes; broad unrelated suites and both-ABI builds were avoided.

The remaining sections record the historical Phase 1 boundary and its then-pending
import contract; Phase 2 implementation above supersedes those pending statements.

## Audit and implementation

| Concern | Before Phase 1 | Phase 1 result |
| --- | --- | --- |
| Tokenizer, cached IPADIC, NFKC and JMdict JSON importer | Shared `crates/japanese-core` | Retained; no reading/index changes |
| Ruby-free DOM extraction, scalar offsets, sentence and navigation | Shared `packages/reader-core` | Retained; added corpus DOM coverage |
| Surface → lemma → reading orchestration | Desktop React and mobile Rust independently implemented it | One shared Rust engine and store interface in `crates/japanese-core/src/lookup.rs` |
| Clicked-text desktop requests | Tokenizer IPC plus up to three query IPC requests | One `lookup_reader_text` IPC request on a blocking worker |
| Mobile lookup | Blocking worker and installation mutex, local read-only database | Same worker/mutex/storage with shared engine and compatible flattened response |
| Result types | Separate Rust/TypeScript types | Shared Rust entry/response and TypeScript contract; mobile history accepts the stable subset |
| Query storage | Two platform schemas and row mappings | Thin platform adapters retain their schemas and shared existing query predicate |

Desktop stores `dictionaries` and `dictionary_entries` in the app-local catalog;
entries join enabled dictionaries and use normalized term/reading indexes. It
retains a mutex-protected SQLite connection. Mobile provisions the bundled gzip
atomically into an app-local `jmdict-20260928-v2.sqlite3` with an `entries` table,
term/reading indexes and a NOCASE prefix index. It opens one read-only connection
per lookup and reuses it through the fallback sequence. `crates/dictionary-build`
generates that bundle with the existing shared JMdict importer. No live database,
schema, bundled data, signing key, source EPUB or selected archive was changed.

Existing query behavior remains exact term, exact reading, then prefix, ordered
by term length and row ID, limited to 12 results. The first non-empty fallback
query wins, including prefix hits; that can prevent a later lemma lookup. This is
deliberately retained for the baseline. Manual desktop queries and plain-text error
fallback retain their existing command. No new chunk generation/deinflection,
normalization variants, dictionary priority or ranking changes are included.
Token selection now converts the click to a UTF-8 byte position once and counts
the original Unicode prefix only for the selected token, avoiding repeated prefix
traversal for every earlier token while preserving scalar spans.

## Portable lookup contract v1

`LookupRequest { text, offset }` takes ruby-free original text with an offset in
Unicode scalar values, not UTF-8 bytes or UTF-16 code units. Both reader adapters
already convert DOM offsets into scalar positions. Maximum input is 16,000 scalar
values; counting stops at the bound, and empty/out-of-range requests fail before
storage is accessed. Desktop now has the same bound as mobile; its existing error
fallback remains available. DOM collection itself still collects the paragraph;
bounded collection around the click is later-stage work.

`LookupResponse { contractVersion: 1, target, matchedSpan, entries }` retains
surface/lemma/reading and reports original start/end scalar offsets, end exclusive.
The tokenizer supplies the span directly, so repeated text and non-BMP characters
do not require searching for the surface string. Entries retain term, reading,
plain definitions, part of speech, dictionary display name and ephemeral row ID.
Mobile adds its previous `elapsedMs` and `dictionaryBytes` fields outside the
flattened shared response for compatibility; these are not required benchmarks.

`DictionaryStore::query` supplies the enabled dictionary scope and the existing
bounded ordering. The shared engine makes at most three sequential queries and
propagates storage errors. It stops after a non-empty result. The cached tokenizer
is reused. These optimizations reduce duplicated orchestration and desktop IPC;
no measured speedup is claimed. Indexed candidate batching and further resource
reuse are later work, especially replacement of the OR/prefix query where needed.

CFIs, sentences, book versions, generation/stale guards, recording preferences,
saved passages and history remain platform-owned. Desktop row IDs remain local
inputs to its existing history resolver. Mobile keeps its established portable
JMdict content identity and never persists the new ephemeral row ID. New import
storage IDs must not replace portable history identity. Dictionary provenance will
need revision/content identity beyond display names in the later engine contract;
v1 currently handles only the existing bundled JMdict paths.

## Storage/import contract for Phase 2

Shared `dictionary.rs` now declares `DictionaryManifest`, `TermRecord`, `TagRecord`
and `DictionaryImportSink`. This is a contract, not a working Yomitan importer.
Term records preserve reading, definition/term tags, rules, numeric score,
sequence and structured glossary JSON. Glossary values are untrusted data and
must never be injected as HTML. No Yomitan source or rule data was copied.

The next importer should accept a `Read + Seek` archive rather than a source
filename. Windows opens a read-only selected file; Android uses its document
provider and, where necessary, an app-managed temporary seekable copy. Platform
adapters own selection and safe app-local paths, never library-root paths.

An isolated staging sink writes bounded term/tag batches and validated relative
asset paths with bounded streams. `publish(self)` happens only after complete
validation; dropping an unpublished sink discards only staging. Implementations
must leave existing imports usable on errors/cancellation. The shared importer
owns archive/path/schema limits, cancellation between batches and asset validation;
platform adapters own progress delivery and storage errors. Replacement/removal
operate only on managed published generations. No destructive catalog migration
is authorized by this contract. Versioned dictionary storage, persistent enable/
priority settings, limits, safe rendering and implementations remain Phase 2+.

## Official format review and local archive

Reviewed official upstream sources on 2026-10-04:

- [Index schema](https://github.com/yomidevs/yomitan/blob/master/ext/data/schemas/dictionary-index-schema.json).
- [Term-bank format 3 schema](https://github.com/yomidevs/yomitan/blob/master/ext/data/schemas/dictionary-term-bank-v3-schema.json).
- [Lookup translator](https://github.com/yomidevs/yomitan/blob/master/ext/js/language/translator.js).
- [Upstream license](https://github.com/yomidevs/yomitan/blob/master/LICENSE).

Format 3 term tuples have eight fields and may contain structured definitions,
images and dictionary-provided deinflection records. Upstream lookup considers
text variants and multiple lengths, batches unique terms, and filters matches by
rule conditions. The proposed importer must declare its supported subset and
report unsupported features explicitly. Phase 1 supports no Yomitan import;
format 3 term/tag banks are the next implementation target. Frequency/pitch,
kanji banks, audio and full UI parity are not Phase 1 features. Upstream GPLv3
code/rules are not reused; any later reuse requires a separate license review.

The user-selected Downloads archive `[Monolingual] 旺文社国語辞典 第十一版
(Recommended).zip` was inspected read-only, without extraction or copying into
the repository. Its manifest declares title 旺文社国語辞典 第十一版, format 3,
revision `OUKOKU11_1.6`, `sequenced: true`, and publisher copyright attribution.
The copyright notice is not a redistribution license; no private entries/assets
are included in the repository or sent to external services.

Inventory: 123,972,997 archive bytes; 141,796,225 declared expanded bytes; 10 term
banks plus index.json, 2,590 PNGs and 8 JPGs. The first term bank is 2,665,088
expanded bytes with 10,000 eight-field rows. A bounded first-64-row sample found
57 text and 7 structured-content glossary items. This establishes a useful fixture
with asset requirements, not whole-archive schema validation or successful import.

## Generated comparison baseline

`docs/dictionary-comparison-corpus.json` contains 12 original synthetic cases with
desired future headword/readings and original spans. Desired values are not
observed Yomitan output. Ruby, inline splits and vertical DOM text are additionally
checked through the actual shared text collector. DOM tests do not establish
WebView/physical vertical-reader layout acceptance.

The desktop regression imports a generated 10-word JMdict JSON fixture into a
temporary SQLite catalog (20 forms), runs the actual store adapter and shared
engine, and compares every ranked result with an independent pre-extraction
surface/lemma/reading sequence. All 12 cases preserve the previous result list.
The generated fixture deliberately includes both readings of 生物; unlike the
real importer limitation below, those are represented as separate fixture words.

| Case | Baseline surface / original span | Ranked fixture headword(s) / reading(s) | Gap relative to desired match |
| --- | --- | --- | --- |
| Compound | 学校 / [0,2) | 学校 / がっこう; 学校生活 / がっこうせいかつ | Longer compound is second; span excludes 生活 |
| Verb past | 読ん / [2,4) | 読む / よむ | Span excludes だ |
| Polite past | 食べ / [2,4) | 食べる / たべる | Span excludes ました |
| Negative past | 読ま / [3,5) | 読む / よむ | Span excludes なかった |
| Contraction | 読ん / [2,4) | 読む / よむ | Span excludes じゃった |
| Adjective past | 高かっ / [3,6) | 高い / たかい | Span excludes た |
| Halfwidth kana | ﾈｺ / [0,2) | ネコ / ねこ | Desired fixture match retained |
| Ambiguous reading | 生物 / [0,2) | 生物 / せいぶつ; 生物 / なまもの | Both fixture readings retained |
| Non-BMP repeated word | 猫 / [4,5) | 猫 / ねこ | Correct second-word offset |
| Ruby | 漢字 / [0,2) | 漢字 / かんじ | Ruby annotations excluded |
| Inline split | 学校 / [0,2) | 学校 / がっこう; 学校生活 / がっこうせいかつ | Same compound ranking/span gap |
| Vertical DOM | 漢字 / [0,2) | 漢字 / かんじ | Text/offset retained; physical layout not tested |

No generated case is an empty dictionary miss, but finding a headword does not
mean the whole inflected span is recognized. No parity percentage is claimed.
The current shared JMdict JSON importer chooses the first kana reading for each
word and flattens senses; Phase 1 preserves that behavior. It is insufficient as
a model for richer Yomitan imports or all alternate readings.

The one synthetic import observation was 6.37 ms including parsing and
transactional insertion. This is a tiny fixture, not large-dictionary throughput.
Peak process memory was not measured; declared archive expansion is not peak
memory. Real Yomitan import time/memory cannot be established before Phase 2.
Same-dictionary/settings comparison in Yomitan is pending: no extension run or
settings snapshot was available, and the app does not yet import this archive.
Missing lookup timings are explicitly not a blocker.

## Validation and remaining limits

- Shared Rust: final 7-test pass covering engine fallback order/stop, storage
  errors, oversized/invalid requests, original non-BMP/repeated-word spans,
  tokenizer/readings/normalization and JMdict import.
- Desktop Rust: 2 dictionary tests passed, including the 12-case compatibility
  corpus and disabled dictionary scope; test build compiled the new command.
- Android Rust host test: bundled lookup and atomic install passed; serialization
  checks preserve the flattened target/entries shape with shared contract fields.
- Reader/text/history frontend regression: 5 files, 18 tests passed. Desktop
  clicked lookup saves the same ruby-free sentence/CFI and records once; manual
  queries remain unanchored. Android tap/history regressions passed.
- Desktop and Android TypeScript checks passed.
- Desktop and Android production frontend builds passed with Vite's runner config
  loader. Both report the existing >500 kB JavaScript chunk advisory; bundle splitting
  is outside Phase 1. No lookup speed or size improvement is inferred from builds.

No APK was packaged, installed or tested on a phone/emulator. No physical-phone
timing, reader layout acceptance or large-dictionary import is claimed. Source
archives, source books, live app storage and unrelated dirty/untracked work were
preserved. No database migration, application version change or new notices are
needed for this extraction.

Approximate total: 10–15 minutes implementation/audit/documentation and about
1 minute validation (overlapping checks counted once). The final targeted Rust
passes were repeated after the token-selection optimization and serialization
assertions; frontend behavior checks were not repeated without a relevant change.
Diff/readback checks completed. Validation remains under the 50% budget.


## Phase 4 — reader integration (2026-10-05)

Implemented desktop and Android reader/manual adapters to the shared bounded chunk
engine on blocking workers. Desktop uses imported sources only and prompts for local
ZIP import/enable when none are enabled. Android composes imported and bundled JMdict
stores. Existing mouse modifier, touch glyph/edge priority, stale-result guards,
matched-span selection, sentence/CFI saving and version-checked history jumps remain.

Desktop dependency audit: catalog metadata reading/search uses cached Lindera/IPADIC
and does not depend on the legacy imported JMdict lookup UI. Legacy rows and DB
methods remain for old passage/history evidence; no destructive cleanup occurred.
Removed desktop legacy import/list/toggle/ensure-bundled command exposure and UI.
Passages save text/headword/reading/CFI, never imported entry IDs. Intentional history
stores source+revision identity and headword/reading; ambiguous desktop groups do not
claim resolved identity. Long identities use SHA-256 through the shared frontend
helper to preserve the existing 128-character synchronization contract. Existing
legacy and bundled identities remain readable, with existing source/version jump checks.

Shared React rendering constructs allowlisted structured trees (ruby, tables, lists,
details and plain containers) without HTML injection, styles, scripts or remote URLs.
Source groups, alternate readings and tags remain visible. Links are plain disabled
containers; unsupported elements and unavailable images display notices. Images use
validated app-private blobs as PNG/JPEG/WebP data URLs, only after final ranking:
16 references per entry, 1 MiB/image and 3 MiB total raw bytes; larger images remain
visible placeholders. No source archive is modified or uploaded.

Additive `term_metadata` table/index in version-1 imported storage supports frequency
and pitch banks and metadata-only archives. Existing stores gain the table without
rewriting terms; old imports require explicit replacement to add previously skipped
banks. Import rows remain streamed/batched/transactional with cancellation. Frequency
numbers, strings, display values and reading-qualified frequencies are supported;
pitch integer/H-L positions, tags, nasal/devoice data are displayed. IPA, kanji-only,
audio, remote content, custom CSS and full Yomitan parity remain unsupported.
Metadata uses exact indexed headwords with reading qualification, 64 rows/entry and
512 KiB/result. Ranking and term work retain Phase 3 caps. No per-candidate image decode.
Official metadata format verified against
https://github.com/yomidevs/yomitan/blob/master/ext/data/schemas/dictionary-term-meta-bank-v3-schema.json;
implementation remains independently authored (no upstream code/rule reuse).

Validation: shared Rust 29 active tests pass, including metadata-only source reopen,
reading-qualified enrichment, disabling, local-image payload, generated inflected
lookup/reopen and bounds. Supplied monolingual ZIP read-only full temporary import
passes, preserving file size/modification time. Desktop 4 reader tests and 4 Rust
history tests pass; Android 10 reader/history tests and bundled provisioning/chunk
regression pass. Shared renderer/manager 6 tests pass; portable long-ID SHA-256 regression plus desktop reader tests pass (8 tests). Final focused shared+Android
history pass reports 10 tests. Both TypeScript checks pass, desktop Rust check passes,
and both frontend production builds pass. Targeted ESLint has no errors (existing
Android effect-ref/shared Fast Refresh helper-export warnings remain).
Default esbuild config bundling hits sandbox parent-directory denial; Vite/Vitest
`--configLoader runner` succeeds. Android launcher Check with process-local execution
policy bypass reaches SDK/JDK discovery but adb fails creating its home directory;
no app data, signing keys or emulator configuration were changed.

Approximate checkpoint: 25 minutes development/integration/documentation, under
3 minutes aggregate validation wall time (overlaps counted once), below 50% budget.
No APK, Windows executable/installer or physical-device smoke was produced. No real
Yomitan JMdict ZIP was supplied/discovered; generated JMdict-like term fixtures cover
engine/persistence behavior but do not establish real-JMdict compatibility or live
reader acceptance. Interactive desktop restart/offline and old-history user acceptance,
Android document-provider import and physical reading acceptance remain unverified.
At this Phase 4 checkpoint, Phase 5 remained pending; see the later Stage 5 checkpoint. These acceptance checks are not claimed complete by builds/fixtures.


## Pinned Yomitan rules integration (2026-10-05)

User-authorized reuse replaces the independently authored Phase 3 suffix subset;
earlier no-upstream-reuse statements describe the historical implementation.
Pinned commit `77e200428902abf4fa48284df92da7af3dcb4162` supplies Japanese transforms,
helper factories and a reference transformer. Original sources and full GPL license
are retained in `crates/japanese-core/vendor/yomitan`; its README records hashes and
offline regeneration commands. GPL-3.0-or-later copyright/attribution/full-license
notices are available in desktop Settings and Android offline notices. Android's
notice generator reads the same files. Dictionary-data licenses remain separate.
No publication, account, remote dictionary lookup or source-book change occurred.

The offline generator expands the pinned factories into 889 suffix/whole-word rules
and 22 condition descriptors. It explicitly rejects nonliteral regex inputs.
Rust loads this compiled-in table once, resolves hierarchical condition flags,
indexes suffixes by final scalar and whole-word rules by exact input, and preserves
upstream chaining semantics: initial zero flags are unrestricted; subsequent steps
require overlapping input/output flags. Only dictionary-form flags qualify entries.
As upstream does, godan subclasses adapt to broad `v5` (and suru tags to `vs`);
this supersedes the former TMW subclass-specific rejection contract. Exact suffix
spelling and compatible dictionary entries still determine the actual match.

Both existing app adapters now return engine version 2 without storage/schema/API
migration. Original Unicode spans, kana/NFKC candidates, source provenance, ranking,
passages and history are retained. Expansion reserves exact original surface roots,
then rotates per-surface transformation frontiers so productive endings do not
consume all slots before longer spans. Limits remain 512 emitted candidates,
4096 pending/processed states, depth 4, 32-character terms, existing query/result
payload limits and batched indexed dictionary lookup. These deliberate bounds mean
TMW does not claim exhaustive equivalence to upstream's unbounded traversal.

Evidence is generated by executing the original pinned LanguageTransformer on 30
samples. It confirms `死んじゃえ -> 死ぬ` (2 steps), `信じらんない -> 信じる`
(3 steps), and 27 other ordinary grammatical forms. Exact expressive
`信っじらんない` has no upstream grammatical match; per user steering no special
small-tsu removal is implemented. TMW regression tests recover every one of the
29 positive forms at first/middle click offsets with complete original spans,
and verify spoken forms in the original sentence, noun/POS rejection, expressive
spelling rejection and bounded pathological input. The fixture is grammar evidence,
not a claim about identical UI/dictionary configurations or universal parity.

Validation: shared Rust full regression 34 passed/1 existing opt-in fixture ignored;
Android host Rust bundled provisioning/chunk adapter test passed (engine v2);
desktop Rust check and TypeScript passed; desktop and Android frontend production
builds passed with configLoader runner. Existing large-JS-chunk build warnings
remain. Prior Phase 4 import evidence is reused: importer/store is unaffected.
Generator/oracle completed successfully; desktop notices TypeScript/targeted ESLint
and Android notice exact readback passed. Approximate development/integration time
15 minutes, validation under 2 minutes (overlapping intervals counted once), below
the 50% budget. No executable/APK packaging or physical-device acceptance was
performed; installed binaries need a new build to receive this source change.


## Stage 5 — performance and compatibility hardening (2026-10-05)

Fixed a concrete duplicate-sense ranking defect: the imported adapter previously
limited each exact key/source bucket to its first 12 IDs before sorting scores.
Two additive covering indexes now order by normalized key, source, descending
score and ID. Term and reading seeks rank before the existing 12-row cap, without
scanning/sorting an entire homophone bucket. Older version-1 stores gain these
indexes on open; no terms, assets, settings, legacy rows or catalogs are removed.
Existing simple indexes are retained for compatibility. First open may take longer
while constructing the indexes; subsequent opens use a read-only schema check and avoid the index-creation write lock. Additional
index storage/import writes are the tradeoff for bounded score-correct seeks.

Reviewed existing bounded candidate/frontier/depth work, indexed batching, cached
rules/tokenizer, blocking platform adapters and ranked-only asset hydration. Kept
those bounds rather than raising caps. Added regression coverage for late high-score
senses through both term and reading indexes, old-store index upgrade preserving
ID/enable/priority/terms, corrupt ZIPs and oversized declared expansion rejected
before sink creation. Existing rollback/cancel, alternate-reading/ranking, payload,
source-count, portable-history and stale-reader regressions are reused.

Opt-in `large_multi_dictionary_import_reopens_offline` generates three independent
20,000-term dictionaries in an isolated temporary store, imports 60,000 total terms,
reopens and verifies the last term in all three sources offline. Host debug observation:
2.183 seconds including fixture construction/compression, about 27,488 terms/s;
checkpointed database 12,767,232 bytes. This is a generated host observation, not a
real-dictionary or Android-device throughput claim. Peak process memory was not
measured; database size is not memory. Existing supplied monolingual ZIP full-import
evidence is reused. This opt-in test is not part of normal regression runs.

Validation: final shared full regression 37 active tests passed (two opt-in tests excluded); opt-in 60k import/reopen passed.
Six frontend files / 21 tests passed, including safe renderer, dictionary management,
desktop passage integration and Android reader/history. Both TypeScript checks,
both frontend production builds and both host Rust checks passed. Existing >500 kB
bundle warnings remain. No live catalog/source files or phone app data were changed.

Remaining acceptance: large multi-dictionary import and peak memory on Android,
real imported JMdict ZIP, interactive desktop restart/offline/old-history checks,
physical document-provider import and phone reader/lookup responsiveness. Host
fixtures do not establish these. Full Yomitan parity, audio/IPA/kanji dictionaries
remain outside the supported subset. Stage 5 implementation is complete but final
device/real-dictionary acceptance remains open.


ARM64 package built with the established test-signing key; final package signature,
zipalign `-c -P 16 4` and aapt metadata pass: com.tmw.companion 0.7.1/code 7001,
min API 24/target 36, arm64-v8a. SHA-256:
`70dc0adc67b73c79b51a63c2e35eb1028d9c1d30ddb76d36b31cf9f5b62edd87`.
First link attempt was denied access to the NDK clang executable by the sandbox;
retry with approved tool access succeeded. Final rebuild includes the schema-check
optimization. Package was not installed; physical acceptance and 16KB runtime are
unverified. Windows executable/installer packaging was not performed.
Approximate checkpoint: 6 minutes development/audit/documentation and 4 minutes
validation/package work including retries/waits (overlaps counted once), under 50%.
No additional broad suites, second ABI build or device passes were performed.
