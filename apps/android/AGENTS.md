# Android companion

This directory is the Android app. Follow the repository `AGENTS.md` together
with `../../AndroidAgents.md` for explicitly requested companion work. Preserve
the root Windows app and its tests. Implement only the Android stage authorized
by the user; later stages are a plan, not blanket authorization.

Follow the canonical testing budget in `../../AGENTS.md` (at most 50% validation).
Reuse completed Phase 1–3 evidence; do not rerun their acceptance work by default.
For Phase 4 read `../../docs/android/phase4-handoff.md` and `phase3.md` beside it.
Finalize the durable snapshot/delta protocol before mobile catalog import; the
existing live-offset API alone does not provide consistent snapshots or deltas.

Frontend: `src/`. Rust/config: `src-tauri/`. Tracked native configuration:
`src-tauri/gen/android/`. Use the root npm workspace lockfile. Shared portable
Rust lives in `../../crates/japanese-core/`; avoid duplicated implementations.
Shared reader utilities already live in `../../packages/reader-core/`; both apps
consume them. Audit the desktop implementation before any further extraction.

Use `Android.ps1 -Action Build -TestSigning` for local feasibility APKs on the
current Windows setup, which does not allow Tauri's normal symbolic links.
Preserve the generated native project's checked-in settings; do not run Android
init over it. Keep APKs, native libraries, machine paths, dictionaries, databases,
credentials, keystores, and testing captures ignored. Never touch source EPUBs.

Record tested ABIs, Android version, measurements and remaining acceptance work
in `../../docs/android/phaseN.md`. Toolchain/build instructions are in
`../../docs/android/feasibility.md`; Phase 2 evidence is in `phase2.md` beside it.
Phases 1/2 passed physical-phone functional verification; Phase 3 approved
private HTTPS pairing and rejection after revocation are also user-confirmed.
The latest reported grant was revoked: check state and re-pair if needed.
Preserve the existing emulator, signing key, app data, local EPUB and positions.
Japanese formatting and UI quality remain known shortcomings for Phase 6;
Phase 4 must preserve fast lookup. Do not implement Phase 5 user-data sync.
See the current checkpoint in AndroidAgents.md and record new work in phase4.md.
