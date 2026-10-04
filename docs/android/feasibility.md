# Android Phase 1: feasibility and shared-code audit

Historical Phase 1 evidence; current status is in `../../AndroidAgents.md`.

Date: 2026-10-04. The Android app lives in `apps/android/`; Windows remains in
root `src/` and `src-tauri/`. Both frontend packages use the root npm workspace
lockfile. Rust applications retain separate manifests, lockfiles, targets, and
Tauri configuration. No desktop catalog migration was introduced.

## Implementation and boundaries

| Area               | Finding and Phase 1 decision                                                                                                                                                                                                                                                                                                                                                |
| ------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Tokenizer/readings | Extracted the existing implementation to `crates/japanese-core`. Both apps use Lindera 5.3.0 with actual embedded IPADIC, NFKC normalization, deterministic kana handling, and the process-cached segmenter. The desktop wrapper preserves performance instrumentation.                                                                                                     |
| JMdict importer    | Extracted unchanged English-gloss/tag-expansion/form semantics into the same crate. Android compiles and exercises that importer with a synthetic JSON fixture. The complete JMdict dataset is deliberately deferred to Phase 2.                                                                                                                                            |
| Dictionary queries | Desktop queries use normalized term/reading matching, then prefix matches, ordered exact term, exact reading, prefix, shortest term, limited to 12. Reader fallback order is surface, lemma, reading. Preserve both orders in Phase 2; the Phase 1 shell is not a dictionary reader.                                                                                        |
| Reader text        | `dictionaryText.ts` excludes `rt`, `rp`, scripts and styles. It translates DOM UTF-16 offsets to Unicode code points across text nodes. `passageContext.ts` retains bounded sentence context. These are portable candidates; defer extraction until both apps actually consume them.                                                                                        |
| Reader UI          | `EpubReader.tsx` combines epub.js rendering with desktop commands, asset URLs, Alt/Ctrl click, keyboard shortcuts, preferences, and catalog progress/bookmarks. Share parsing utilities rather than mounting this desktop component wholesale on Android. Tap/scroll disambiguation, vertical text, popup accessibility and lifecycle behavior belong to subsequent stages. |
| SQLite             | Android compiles the same rusqlite 0.32.1 bundled SQLite dependency. Its smoke database is private `feasibility.sqlite3` under its own Tauri app-data directory, separate from the Windows catalog. Full mobile catalog/dictionary migrations follow in later stages. Never sync live databases.                                                                            |
| Desktop services   | Discovery, missing-file reconciliation, cover-cache selection, source-root administration, opener/dialog commands, catalog backup/restore, and Windows file metadata stay desktop-only. No scanner or broad asset-file scope is linked into the mobile shell.                                                                                                               |
| EPUB parser        | Rust ZIP/XML/cover handling is potentially portable but remains with desktop extraction until mobile requirements are known. Android EPUB reading uses epub.js in Phase 2. No EPUBs are included in this shell.                                                                                                                                                             |
| Identity/sync      | Desktop local book IDs and path-based discovery are not yet a cross-device identity contract. Source-size/time checks already protect saved anchors. Audit move/rename/replacement semantics before Phase 4; no sync protocol or identity migration in Phase 1.                                                                                                             |

The diagnostic invokes real inflection analysis (`食べ` → `食べる`), imports two
synthetic JMdict-format forms, and increments a transactional SQLite counter.
Work runs through `spawn_blocking` and a mutex to keep the UI responsive and
prevent concurrent diagnostic fixture writes. Temporary synthetic JSON is
removed after import. It has no network service, phone downloads, catalog sync,
reader, or production dictionary installation.

## Build environment and repeatable commands

Existing Java: `C:\Users\Marius\Desktop\android-tools\jdk-21.0.12.1+1`.
Existing SDK: `C:\Users\Marius\Desktop\android-tools\SDK`.
Added platform 36, build-tools 36.0.0, NDK 27.2.12479018, and Rust targets
`x86_64-linux-android` / `aarch64-linux-android`. Existing SDK license acceptance
was used; no license prompts were automatically answered. No permanent
environment variables were changed, and the existing `TMW_Test` device was
reused without wiping data.

Native project: Tauri CLI 2.12.0, Gradle 9.6.1, Android Gradle plugin 9.3.1,
Kotlin 2.2.10. Native scaffolding is tracked; generated Kotlin/Tauri bridge files,
machine-specific paths, Gradle caches, native libraries and APK outputs are ignored.
The checked-in Gradle configuration uses compile/target API 36, minimum API 24
(Android 7.0). Minimum API is a declared build floor, not a tested compatibility
claim. Runtime testing uses Android 16/API 36, x86-64 Google APIs image.
ARM64 is the physical-phone build; 32-bit ABIs are outside this checkpoint.

The native link step explicitly aligns ELF segments to 16 KB with NDK 27.
APK archive alignment is checked with `zipalign -c -P 16 4`. The existing
emulator uses 4 KB pages; 16 KB device runtime testing remains unverified.
See [Android page-size guidance](https://developer.android.com/guide/practices/page-sizes).

From the repository root:

```powershell
& 'C:\Program Files\nodejs\npm.cmd' ci
& 'C:\Program Files\nodejs\npm.cmd' run android:check
& 'C:\Program Files\nodejs\npm.cmd' run android:frontend
& .\apps\android\Android.ps1 -Action Build -TestSigning
```

`Android.ps1` accepts SDK/Java/NDK parameters and `-Target x86_64` or
`-Target aarch64`. Default Build produces both ABIs. It regenerates dependency
notices, builds the frontend, compiles the Rust library with embedded frontend
assets, copies each resulting `.so` into ignored JNI staging directories, and
packages with Gradle. It fails immediately on a failed frontend/Rust/Gradle step.

The normal `cargo tauri android build` compiled x86-64 Rust successfully but
failed while creating a symbolic link: this Windows machine does not allow
symlink creation. The supplied Build script avoids that requirement by copying
only build outputs and excluding Gradle's redundant Rust invocation. It does
not enable Windows Developer Mode or require a system-settings change.
`android:dev` uses the standard Tauri workflow and retains that symlink
prerequisite; use the release-profile Build script for this environment.

With `-TestSigning`, release-profile APKs use the local Android development
keystore. They are **feasibility builds**, not production-signed releases.
Without this switch, release APKs remain unsigned. Do not lose/change the test
key if installing updates over an existing feasibility installation. Production
signing, secure key backup, and update/recovery acceptance remain Phase 7.

Outputs:

```text
apps/android/src-tauri/gen/android/app/build/outputs/apk/x86_64/release/
apps/android/src-tauri/gen/android/app/build/outputs/apk/arm64/release/
```

## Licensing and offline assets

Lindera is MIT licensed. Its embedded IPADIC data carries NAIST/ICOT notices;
the full upstream `lindera-ipadic` NOTICE is included in the APK's local notices.
Tauri is MIT OR Apache-2.0; React and the Tauri JS API are MIT. rusqlite uses
MIT licensing and SQLite itself is public domain. Locked Rust dependency license
declarations, including build dependencies, are recorded in `rust-licenses.md`.
`Generate-Notices.ps1` includes available license/copyright/notice files from
those dependency packages and the frontend runtime packages. The notices load
from a bundled local text asset only when opened, avoiding a large initial JS
bundle and DOM tree. AndroidX/Material platform dependencies use Apache-2.0;
retain their notices in the final distribution audit.

Full JMdict definitions are not included or measured in this Phase 1 APK.
The supplied English export is version 3.6.2, dictionary date 2026-09-28,
SHA-256 `5f54504a62a7f45741e1bf6fd28f6e1e5add6405f2829748cc004a802839a3aa`.
The upstream exporter license text is preserved as `jmdict-export-license.txt`
(CC BY-SA 4.0). EDRDG requires attribution and ShareAlike for its dictionary
data/derivatives. Phase 2 must bundle attribution, source/version, modifications
made by database conversion, and applicable license notices with the actual
indexed definitions. Do not describe the complete JMdict dataset as MIT merely
because exporter tooling packages use MIT. Dictionary data remains separate
from the application-code license.

Sources verified on 2026-10-04:

- [Tauri mobile prerequisites](https://v2.tauri.app/start/prerequisites/)
- [Tauri Android distribution](https://v2.tauri.app/distribute/google-play/)
- [Lindera 5.3.0 license](https://github.com/lindera/lindera/blob/v5.3.0/LICENSE)
- [EDRDG dictionary licensing](https://www.edrdg.org/edrdg/licence.html)
- [jmdict-simplified export license](https://github.com/scriptin/jmdict-simplified/blob/master/LICENSE.txt)

## Validation status

Windows validation: lint and production frontend build passed; all 34 frontend
tests passed; 38 desktop Rust tests passed (seven performance tests remain
intentionally ignored). The three extracted shared Rust tests and the Android
host smoke test passed. Rust formatting and whitespace checks passed.

Emulator validation: `TMW_Test`, Android 16/API 36, x86-64, 320×640, 4 KB memory
pages. Installation, activity launch, actual tokenizer/importer/SQLite check,
airplane-mode operation with Wi-Fi disabled, forced process restart, and
in-place APK update passed. The counter progressed from 1 to 6 across these
checks, demonstrating persistence and update preservation. Networking was
restored to its original state afterward. Offline notices were loaded from the
bundled asset; the native disclosure was replaced with an explicit touch button
after emulator testing exposed inconsistent native-summary taps.

| Measurement                               | Observed result                                |
| ----------------------------------------- | ---------------------------------------------- |
| Cold activity launch, first install       | 768 ms (`am start -W`)                         |
| Cold activity launch, offline restart     | 739 ms                                         |
| Cold activity launch, after update        | 1,058 ms                                       |
| Subsequent cold activity launch           | 803 ms                                         |
| First tokenizer check                     | 1.1 ms; total probe 28.2 ms                    |
| Offline process-restart tokenizer         | 22.6 ms; total probe 43.6 ms                   |
| Warm same-process tokenizer               | <0.05 ms (display rounds to 0.0); total 8.1 ms |
| After-update tokenizer                    | 0.8 ms; total probe 14.1 ms                    |
| Installed app size, Android Settings      | 59.58 MB                                       |
| User data, Android Settings               | 8.99 MB                                        |
| Cache, Android Settings                   | 348 kB                                         |
| Total installed storage, Android Settings | 68.93 MB                                       |

These are individual observations, not averaged benchmarks. `am start -W`
measures the activity launch and does not establish full webview readiness.
Android Settings storage includes WebView/runtime data and uses rounded units;
the smoke SQLite file alone is not responsible for the 8.99 MB user-data size.
Final touch-button build activity launch: 1,164 ms while offline.

Final APKs (release profile, development-key signing):

| ABI    | File                     | Exact bytes | Decimal MB |
| ------ | ------------------------ | ----------: | ---------: |
| ARM64  | `app-arm64-release.apk`  |  57,383,483 |      57.38 |
| x86-64 | `app-x86_64-release.apk` |  57,999,408 |      58.00 |

SHA-256:

```text
ARM64: 72e77bc92934a05ec5489c3252e24dbdab445d996c5b9a25d422c97f5f6409bb
x86-64: 870a96cb8c276880d8644b5352d175220fea8ed8d390db8e73045faebb36e4ba
```

Both passed `apksigner verify` and `zipalign -c -P 16 4`; ARM64 ELF LOAD
segments report `0x4000` alignment. Final emulator update retained the counter
and passed offline tokenization/import/SQLite. The explicit notices button
opened by touch with Wi-Fi disabled; the locally bundled attribution/license
text was visually verified. UIAutomator did not expose the entire large license
text node, so that final content check used a screenshot instead of an XML
text assertion. The emulator's original networking state was restored.

No complete JMdict database, EPUB downloads, or cover cache is present in these
measurements. Physical ARM64 runtime/storage remains unmeasured.

On 2026-10-04 the user confirmed that the physical-phone test worked without
problems and felt very quick. Phase 1 acceptance is complete based on that
confirmation and the recorded emulator/build checks. Phone model, Android
version, exact timings and storage measurements were not supplied, so those
details remain unmeasured. At the Phase 1 checkpoint, Phase 2 had not been started.

The user confirmed this is a private personal-use application with no intended
publication. Keep licensing/source notices unobtrusive under About or Settings;
no terms-of-service acceptance screen or public-release process is needed for
this scope. Preserve upstream notices in the project and packaged dependencies.

Source safety: implementation and tests use app-private or test-temporary files.
No scan, cache generation, or source-library write was performed. No source EPUB
was transferred, changed, renamed, moved, or deleted. `testLibrary/` and
`jmdict-eng/` remain ignored, and the source library `F:\tmw collection` was not
accessed by the companion.


## Phase 2 checkpoint

See [Offline reader and bundled lookup proof](phase2.md) for the shared-reader
audit, indexed JMdict provisioning, validation, measurements and ARM64 handoff.
The user confirmed Phase 2 works on the phone and lookup is fast, while reporting
poor UI and Japanese formatting. Functional proof is complete; reader polish
remains outstanding for Phase 6. At this Phase 2 checkpoint, Phase 3 had not
started. It is now complete, including user-confirmed private HTTPS pairing and
revocation; see [Phase 3 evidence](phase3.md) and [Phase 4 handoff](phase4-handoff.md).
