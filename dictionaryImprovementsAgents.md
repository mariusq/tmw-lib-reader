# Dictionary improvements: Windows and Android

## Scope and constraints

- Implement on both Windows and Android. Read the canonical root `AGENTS.md`, including its Android sections (the former Android instruction files have been consolidated there). This plan does not authorize unrelated Android phases. For this dictionary work, user-imported Japanese-Japanese dictionaries are in scope despite the older Android deferral.
- Support local Yomitan-formatted dictionary ZIPs without requiring Yomitan, a browser extension, Python, a server, or network access. Imported dictionaries and lookup must work offline on each device; dictionary synchronization is out of scope.
- Minimize duplication. Audit and extend `crates/japanese-core/` and `packages/reader-core/`. Share importer, dictionary schema/query code, candidate generation, deinflection, normalization, ranking, and result types. Keep file selection, platform storage paths, commands, and mouse/touch UI in thin platform adapters. Do not reorganize unrelated application code.
- Preserve Android's bundled JMdict fallback, catalog reading derivation, saved passages, lookup history, sentence context, EPUB CFIs, settings, and existing user data. User-approved scope change on 2026-10-05: Phase 4 replaces desktop's legacy JMdict reader lookup with imported Yomitan dictionaries only, including user-imported JMdict. This supersedes the desktop fallback-preservation requirement; Android retains its bundled fallback. Dictionary IDs must not become the durable identity of saved words/history.
- Source EPUBs, `testLibrary/`, and selected dictionary archives are read-only. Store imported data/assets outside all source-library roots. Never modify, rename, move, upload, or delete source material.
- Use additive/versioned changes where practical. Before destructive database migration or storage cleanup, explain impact and provide a backup path. Preserve upstream/data notices; review licenses before reusing Yomitan code or rule data.
- Follow the root validation budget: at most 50% of task time. Use targeted checks, reuse evidence, and avoid repeated full suites or both-ABI builds. Report skipped/unverified checks explicitly.

## Lookup and performance requirements

- Specific dictionary lookup timings, latency benchmarks, and numerical latency targets are not required for any stage or acceptance. Optimize code using bounded work, indexed/batched queries, resource reuse, and avoidance of redundant work; use correctness checks and brief responsiveness smoke tests. Optional measurements may help investigate a concrete issue, but missing lookup timings must not block progress.
- Improve lookup using bounded text chunks around the clicked/tapped position, not a single tokenizer-selected word. Collect text across inline nodes while excluding ruby annotations; preserve original Unicode offsets, matched span, sentence, and CFI in horizontal and vertical EPUBs.
- Generate multiple candidate lengths and grammatical deinflections; validate candidates against dictionaries and compatible rule/part-of-speech metadata. Use Lindera as a hint/fallback, not a mandatory boundary. Rank exact and plausible longer matches deterministically; preserve alternate readings and ambiguity.
- Verify current official Yomitan format schemas and lookup behavior before implementation. Declare supported format versions/features; never claim universal compatibility. Preserve structured definitions, tags, scores, sequences, and rule metadata needed for later rendering/matching.
- Bound chunk size, candidate count, deinflection depth, query/result count, caches, archive expansion, and asset decoding. Batch indexed candidate queries; reuse tokenizer/database resources. No full-book parsing or full-dictionary scan per lookup.
- Run expensive import/lookup work off the UI thread. Debounce where appropriate and cancel or ignore stale requests. Import cancellation/failure must leave existing dictionaries usable; publish new imports atomically after validation.
- Treat archive paths and rich definitions as untrusted: reject path traversal, bound decompression, sanitize supported structured content, disable scripts and remote resource loading. Explain unsupported definition elements visibly rather than silently losing essential content.

## Implementation stages

Complete stages in order. At each boundary, record changes, targeted validation, optimization decisions, and remaining limitations in `docs/dictionary-improvements.md` before proceeding. Lookup timings are optional.

### 1. Audit, baseline, and shared contract
Implemented on 2026-10-04; audit, generated corpus baseline, validation, and remaining comparison/device limitations are recorded in `docs/dictionary-improvements.md`. Lookup timings are optional as requested. Phases 2–4 are implemented; Stage 5 hardening is implemented; documented platform/device acceptance gaps remain pending.

- Map shared code versus duplicated desktop React/mobile Rust fallback logic and their different database layouts.
- Define one portable lookup request/result contract and dictionary storage/import interface; route both apps toward the same engine without behavior regression.
- Audit lookup code for redundant work, resource reuse, indexed queries, and bounded execution. Record import time/memory for representative fixtures. Create a bounded Japanese comparison corpus covering compounds, inflections, contractions, kana variants, ambiguous readings, ruby, inline-node splits, and vertical text.
- Compare against Yomitan with identical dictionaries/settings where practical; record expected headword/reading, matched span, misses, and ranking. Identify practical code optimizations and memory bounds; no numerical lookup latency targets are needed. Do not invent parity percentages.

### 2. Shared Yomitan dictionary import
Implemented on 2026-10-04. Shared format-3 term/tag importer, transactional app-private storage, dictionary management and both picker adapters are implemented. The supplied local ZIP passed a full temporary import; Android native document-provider/physical-device acceptance remains unverified. Evidence and supported limits are in `docs/dictionary-improvements.md`. Imported dictionaries have shared engine lookup in Phase 3; reader integration remains Stage 4.
- Implement streaming/batched ZIP term-bank and tag-bank import into a versioned shared dictionary schema. Preserve structured glossary and matching metadata. Support multiple dictionaries, priorities, enable/disable, and safe replacement/removal of app-managed dictionaries.
- Add Windows file-picker and Android document-provider adapters; do not assume Android supplies a normal filesystem path. Show bounded progress/cancellation and actionable format/storage errors.
- Keep bundled JMdict usable and preserve existing user data. Validate real representative licensed/local dictionary fixtures on both apps; do not commit large/private archives.

### 3. Shared chunk-based lookup engine

Implemented on 2026-10-04 in `crates/japanese-core/src/chunk_lookup.rs` and `lookup_storage.rs`. Generated comparison-corpus, grammatical compatibility and indexed/bounded adapter evidence is recorded in `docs/dictionary-improvements.md`. Reader UI/history integration deliberately remains Stage 4; this stage does not claim Yomitan parity or physical-device acceptance.
- Implement bounded text-window candidate generation, multi-step deinflection, compatible dictionary matching, normalization variants, deterministic ranking/grouping, and indexed batched lookup.
- Move lookup orchestration out of separate platform implementations into shared Rust. Return original matched span plus headword/reading and dictionary provenance; maintain history ambiguity semantics.
- Check comparison-corpus accuracy against Stage 1 and briefly smoke-test responsiveness; document remaining gaps and cap worst-case pathological input work. Specific lookup timings are not required.

### 4. Reader integration and useful definition rendering

Implemented on 2026-10-05. Both readers use the shared engine; desktop is imported-only,
Android retains bundled JMdict. Shared safe rich rendering, local assets, frequency/pitch
metadata and portable history are integrated. Generated fixture/reopen/offline tests and
frontend production builds pass. Physical-device/APK, interactive desktop restart, and
real imported-JMdict acceptance remain unverified; see `docs/dictionary-improvements.md`.
The requirements below describe implemented behavior and remaining acceptance checks.
- Connect both readers to the shared engine; retain desktop modifier-click and mobile touch behavior, stale-response protection, selection/highlight, passage saving, and available history features.
- Desktop uses enabled imported Yomitan dictionaries exclusively for reader and manual dictionary lookup; users may import JMdict in that format. Remove the legacy desktop JMdict lookup/import UI and fallback only after the imported-dictionary reader path works. When no imported dictionaries are enabled, show a clear local ZIP import/enable prompt rather than silently falling back. Android keeps its bundled JMdict fallback alongside imported dictionaries.
- Audit desktop JMdict dependencies before removal, including catalog reading/search assistance and legacy saved-passage/history resolution. Preserve metadata reading derivation and existing durable user data; remove only dependencies made unnecessary by the new desktop lookup path. Do not delete legacy dictionary rows if existing passages/history still need them. Any destructive cleanup requires the canonical backup/impact procedure; removing the fallback does not authorize deleting user-imported archives or existing catalogs.
- Validate desktop lookup with an imported JMdict ZIP, another supported Yomitan dictionary, no dictionaries enabled, and restart/offline use. Confirm old passages/history remain readable and safe jumps still respect source/version checks. These remain acceptance checks following the implemented Phase 4 integration.
- Share portable definition rendering/result logic where practical. Render common structured glossary elements and local assets safely; preserve dictionary grouping, tags, and alternate readings.
- Add frequency/pitch-accent term metadata support after core lookup works; distinguish unsupported formats/features explicitly. Full Yomitan UI, audio services, Anki, and kanji-only dictionaries are deferred.

### 5. Performance and compatibility hardening

Implemented on 2026-10-05: indexed score ordering before bounded seeks, old-store
upgrade coverage, malformed/expansion guards and generated 60k multi-source
import/reopen observation. Final physical-device and real-dictionary acceptance
remain open; see the Stage 5 checkpoint in `docs/dictionary-improvements.md`.
- Measure large multi-dictionary import on Windows and Android; record import throughput and peak memory where measurable. Review lookup chunk sizes, candidate limits, query batching, caches, and resource reuse; optimize evident inefficiencies and check slow/ambiguous cases with brief responsiveness smoke tests. Lookup latency measurements, including warm/cold and p50/p95 timings, are optional and are not acceptance requirements.
- Cover atomic import/rollback, cancellation, malformed archives, bounded expansion, migrations, alternate readings, deterministic ranking, stale results, and preserved passages/history with relevant regression checks.
- Run one final relevant regression/build pass and brief smoke test on Windows and the selected Android ABI/device within budget. Verify restart persistence and offline lookup; preserve Android signing/app data. Request a budget exception before additional necessary validation exceeds the limit.
- Acceptance: both platforms use the same engine/importer; supported Yomitan dictionaries import locally; chunk lookup improves corpus results over baseline and remains responsive in brief smoke tests; source files and existing user data remain intact. Document correctness results, code optimizations, any optional measurements, and unverified items. Missing lookup timings do not block acceptance.
