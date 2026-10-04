# Dictionary improvements: Windows and Android

## Scope and constraints

- Implement on both Windows and Android. Read root `AGENTS.md`, `AndroidAgents.md`, and `apps/android/AGENTS.md`. This plan does not authorize unrelated Android phases. For this dictionary work, user-imported Japanese-Japanese dictionaries are in scope despite the older Android deferral.
- Support local Yomitan-formatted dictionary ZIPs without requiring Yomitan, a browser extension, Python, a server, or network access. Imported dictionaries and lookup must work offline on each device; dictionary synchronization is out of scope.
- Minimize duplication. Audit and extend `crates/japanese-core/` and `packages/reader-core/`. Share importer, dictionary schema/query code, candidate generation, deinflection, normalization, ranking, and result types. Keep file selection, platform storage paths, commands, and mouse/touch UI in thin platform adapters. Do not reorganize unrelated application code.
- Preserve bundled JMdict fallback, catalog reading derivation, saved passages, lookup history, sentence context, EPUB CFIs, settings, and existing user data. Dictionary IDs must not become the durable identity of saved words/history.
- Source EPUBs, `testLibrary/`, and selected dictionary archives are read-only. Store imported data/assets outside all source-library roots. Never modify, rename, move, upload, or delete source material.
- Use additive/versioned changes where practical. Before destructive database migration or storage cleanup, explain impact and provide a backup path. Preserve upstream/data notices; review licenses before reusing Yomitan code or rule data.
- Follow the root validation budget: at most 50% of task time. Use targeted checks, reuse evidence, and avoid repeated full suites or both-ABI builds. Report skipped/unverified checks explicitly.

## Lookup and performance requirements

- Improve lookup using bounded text chunks around the clicked/tapped position, not a single tokenizer-selected word. Collect text across inline nodes while excluding ruby annotations; preserve original Unicode offsets, matched span, sentence, and CFI in horizontal and vertical EPUBs.
- Generate multiple candidate lengths and grammatical deinflections; validate candidates against dictionaries and compatible rule/part-of-speech metadata. Use Lindera as a hint/fallback, not a mandatory boundary. Rank exact and plausible longer matches deterministically; preserve alternate readings and ambiguity.
- Verify current official Yomitan format schemas and lookup behavior before implementation. Declare supported format versions/features; never claim universal compatibility. Preserve structured definitions, tags, scores, sequences, and rule metadata needed for later rendering/matching.
- Bound chunk size, candidate count, deinflection depth, query/result count, caches, archive expansion, and asset decoding. Batch indexed candidate queries; reuse tokenizer/database resources. No full-book parsing or full-dictionary scan per lookup.
- Run expensive import/lookup work off the UI thread. Debounce where appropriate and cancel or ignore stale requests. Import cancellation/failure must leave existing dictionaries usable; publish new imports atomically after validation.
- Treat archive paths and rich definitions as untrusted: reject path traversal, bound decompression, sanitize supported structured content, disable scripts and remote resource loading. Explain unsupported definition elements visibly rather than silently losing essential content.

## Implementation stages

Complete stages in order. At each boundary, record changes, targeted validation, comparative timings where relevant, and remaining limitations in `docs/dictionary-improvements.md` before proceeding.

### 1. Audit, baseline, and shared contract
- Map shared code versus duplicated desktop React/mobile Rust fallback logic and their different database layouts.
- Define one portable lookup request/result contract and dictionary storage/import interface; route both apps toward the same engine without behavior regression.
- Record warm/cold lookup latency and import time/memory for representative fixtures. Create a bounded Japanese comparison corpus covering compounds, inflections, contractions, kana variants, ambiguous readings, ruby, inline-node splits, and vertical text.
- Compare against Yomitan with identical dictionaries/settings where practical; record expected headword/reading, matched span, misses, ranking, and latency. Set measurable latency/memory targets from the baseline; do not invent parity percentages.

### 2. Shared Yomitan dictionary import
- Implement streaming/batched ZIP term-bank and tag-bank import into a versioned shared dictionary schema. Preserve structured glossary and matching metadata. Support multiple dictionaries, priorities, enable/disable, and safe replacement/removal of app-managed dictionaries.
- Add Windows file-picker and Android document-provider adapters; do not assume Android supplies a normal filesystem path. Show bounded progress/cancellation and actionable format/storage errors.
- Keep bundled JMdict usable and preserve existing user data. Validate real representative licensed/local dictionary fixtures on both apps; do not commit large/private archives.

### 3. Shared chunk-based lookup engine
- Implement bounded text-window candidate generation, multi-step deinflection, compatible dictionary matching, normalization variants, deterministic ranking/grouping, and indexed batched lookup.
- Move lookup orchestration out of separate platform implementations into shared Rust. Return original matched span plus headword/reading and dictionary provenance; maintain history ambiguity semantics.
- Check comparison-corpus accuracy and latency against Stage 1; document remaining gaps and cap worst-case pathological input work.

### 4. Reader integration and useful definition rendering
- Connect both readers to the shared engine; retain desktop modifier-click and mobile touch behavior, stale-response protection, selection/highlight, passage saving, and available history features.
- Share portable definition rendering/result logic where practical. Render common structured glossary elements and local assets safely; preserve dictionary grouping, tags, and alternate readings.
- Add frequency/pitch-accent term metadata support after core lookup works; distinguish unsupported formats/features explicitly. Full Yomitan UI, audio services, Anki, and kanji-only dictionaries are deferred.

### 5. Performance and compatibility hardening
- Measure large multi-dictionary import and warm/cold lookup on Windows and Android; compare chunk sizes and candidate limits. Record p50/p95 latency, import throughput, peak memory where measurable, and slow/ambiguous cases. Optimize only measured bottlenecks.
- Cover atomic import/rollback, cancellation, malformed archives, bounded expansion, migrations, alternate readings, deterministic ranking, stale results, and preserved passages/history with relevant regression checks.
- Run one final relevant regression/build pass and brief smoke test on Windows and the selected Android ABI/device within budget. Verify restart persistence and offline lookup; preserve Android signing/app data. Request a budget exception before additional necessary validation exceeds the limit.
- Acceptance: both platforms use the same engine/importer; supported Yomitan dictionaries import locally; chunk lookup improves corpus results over baseline without material responsiveness regression; source files and existing user data remain intact. Document measured results and unverified items.
