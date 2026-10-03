# Future reading and learning features

This document describes features for a future agent to implement. It is a backlog, not an instruction to implement everything immediately. Follow `AGENTS.md`, preserve existing behavior and tests, and prefer small, reviewable checkpoints.

## Product direction and boundaries

The user uses Jiten separately for vocabulary learning. Do not build a second vocabulary-management system: no flashcards, spaced repetition, word mastery states, study queues, or vocabulary goals. Jiten integration may be considered later, but API integration, authentication, synchronization, and network calls are out of scope here.

Saved words are lightweight bookmarks to useful passages, not vocabulary items to study. Lookup history is a reference aid, and reading activity describes reading habits rather than measuring Japanese proficiency.

All features must work offline. Store durable user data in the local SQLite catalog and include it in catalog backup/restore. Never modify source EPUBs or place app-managed files in source directories. Preserve saved data across rescans, unavailable files, and recovery. Any migration must preserve existing corrections, collections, tags, and reading progress; follow the existing backup requirements before destructive changes.

## 1. Lookup history and repeat encounters

### Intended behavior

Keep a local history of intentional dictionary lookups. In the popup, show a compact indication such as **Looked up 4 times** and optionally the last lookup date or previous book. Provide a searchable history with recent lookups and links back to their source passages when available.

History must remain distinct from saved passages: looking up a word does not automatically save a passage or create a learning task. Counts reflect lookups, not every occurrence of a word in a book.

### Implementation guidance

- Record successful, intentional lookups once per user action. Do not count tokenization attempts, fallback queries, popup rerenders, or dictionary retries as separate encounters.
- Retain the original surface form and normalize aggregation by an identified headword and reading where reliable. Preserve ambiguity rather than merging unrelated homographs.
- Support editable/manual dictionary queries; these may have no book anchor or sentence.
- Provide an enable/disable preference, clear-history action, and a documented bounded retention policy. Clearing history must not remove saved passages or reading activity.
- Keep storage and retrieval indexed and bounded. Do not scan EPUB contents to calculate encounter counts.

### Acceptance criteria

- Repeated intentional lookups show accurate counts across sessions and books.
- Inflected forms can aggregate under the same reliable headword while unrelated readings stay distinguishable.
- Internal lookup fallbacks do not inflate counts.
- Disabling or clearing history works independently of saved passages.

## 2. Reading activity

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

## 3. Reading statuses and smart shelves

### Intended behavior

Add explicit reading statuses: **Want to read**, **Reading**, **Finished**, and **Paused**, plus an unset state. Let users change status from book details and relevant reader controls. Opening an unset or want-to-read book may transition it to Reading after a successful open; never automatically override Paused or Finished. Do not infer Finished solely from visiting the final page.

Provide built-in shelves for these statuses and allow saved filter combinations, for example **Unfinished books by this author** or **Want to read with this tag**. Smart shelves are dynamic catalog queries, distinct from existing manually curated collections.

### Implementation guidance

- Store status independently of reader CFI/progress and extracted EPUB metadata. Rescans must never reset it.
- Record completion dates for explicit Finished actions so activity can show completions. Reopening a finished book must preserve its status unless the user changes it.
- Define the status transition rules and completion-date behavior, including marking a book unfinished or finished again.
- Initially support filters already available in the catalog plus reading status. Store a versioned, validated filter definition, not arbitrary SQL.
- Support naming, editing, deleting, and sorting smart shelves. Deleting a shelf removes only its saved filter, never books or reading data.
- Use indexed, paginated backend queries and existing stale-request protection. Shelves should reflect committed metadata, tag, and status changes without full-library reloads.

### Acceptance criteria

- Statuses survive restarts, rescans, unavailable-file recovery, and backup/restore.
- Explicit Paused and Finished states are not overwritten by opening a book.
- Smart shelves update when a matching book changes and preserve their filters across sessions.
- Deleting or editing a shelf cannot affect source files, collections, or book data.
- Status filtering and common saved filters remain responsive on large generated catalogs.

## Suggested implementation order and validation

Implement statuses first, then lookup history and reading activity. Reuse the existing saved-passage anchoring when adding history links.

At each checkpoint, document the final behavior and relevant checks. Add meaningful regression coverage for persistence, backup/restore, source safety, and the edge cases listed above. Reuse existing reader and catalog infrastructure rather than replacing it. Keep generated fixtures temporary and `testLibrary/` read-only and Git-ignored.
