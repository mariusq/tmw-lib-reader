# Future reading and learning features

This document describes features for a future agent to implement. It is a backlog, not an instruction to implement everything immediately. Follow `AGENTS.md`, preserve existing behavior and tests, and prefer small, reviewable checkpoints.

## Product direction and boundaries

The user uses Jiten separately for vocabulary learning. Do not build a second vocabulary-management system: no flashcards, spaced repetition, word mastery states, study queues, or vocabulary goals. Jiten integration may be considered later, but API integration, authentication, synchronization, and network calls are out of scope here.

Saved words are lightweight bookmarks to useful passages, not vocabulary items to study. Lookup history is a reference aid, and reading activity describes reading habits rather than measuring Japanese proficiency.

All features must work offline. Store durable user data in the local SQLite catalog and include it in catalog backup/restore. Never modify source EPUBs or place app-managed files in source directories. Preserve saved data across rescans, unavailable files, and recovery. Any migration must preserve existing corrections, collections, tags, and reading progress; follow the existing backup requirements before destructive changes.

## 1. Reading activity

### Intended behavior

Show daily and weekly active reading time, recently read books, and completed books. Keep the presentation simple and optional. Do not treat time, lookup frequency, or book completions as a proficiency score, and do not add mandatory streaks or targets.

Character counts may be a later extension only if they can be measured credibly. EPUB pagination changes with layout, so avoid presenting rendered page counts as comparable across books or devices.

### Implementation guidance

- Count time only while the reader is visible and the app is active. Use a documented idle timeout, with conservative behavior when attention cannot be inferred.
- Stop counting when the reader closes, the app loses focus, the machine sleeps, or the session becomes idle. Prevent overlapping readers or timers from double-counting.
- Persist periodically so crashes lose at most a bounded amount of activity. Use monotonic elapsed time for durations and timestamps for calendar grouping; handle midnight and timezone changes consistently.
- Offer activity tracking controls and a clear-activity action. Reading location and reading status must survive clearing activity.
- Store compact sessions or aggregates without tracking EPUB contents or logging sentences.

### Acceptance criteria

- Active reading records time across restarts without counting long idle or background periods.
- Sleep, focus changes, reopening the reader, and midnight boundaries do not inflate totals.
- Daily/weekly views remain fast with a long activity history.
- Tracking can be disabled, and clearing activity does not erase progress or saved passages.

## Suggested implementation order and validation

Reading statuses and smart shelves are implemented. Reading activity is the remaining feature.

At each checkpoint, document the final behavior and relevant checks. Add meaningful regression coverage for persistence, backup/restore, source safety, and the edge cases listed above. Reuse existing reader and catalog infrastructure rather than replacing it. Keep generated fixtures temporary and `testLibrary/` read-only and Git-ignored.
