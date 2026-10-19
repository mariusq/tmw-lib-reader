# TMW EPUB Library features

What the desktop (Windows) app and the Android companion do today, followed by
proposals that have not been started. The rules and storage contracts behind
these features are in `AGENTS.md`; update this file whenever behavior changes.

# Part 1 — Implemented features

## Library (desktop)

- **Library roots and scanning:** add folders as library roots. Background scans
  show books as they are committed and can be cancelled. A sidebar indicator shows
  scan progress while you are on other pages. Books whose files disappear become
  unavailable instead of being deleted, and they come back if the file returns.
- **Browsing:** Grid or List layout (remembered per device). The grid is a
  responsive auto-fill grid (about 6 columns at 1280px), and its titles wrap to two
  lines. Covers lift slightly on hover. An available book shows a ▶ Read pill on
  hover or focus, which opens the reader at the saved position.
- **Search and filters:** search, reading status and sort share one labelled row.
  Search covers title, author, series, filenames, folders and tags, with Japanese,
  kana-reading and romaji matching (spaced or joined), but never book contents.
  Ctrl+F or `/` focuses search. More filters (root, tag, collection, duplicates,
  metadata attention) are under Filters, which shows how many are active. Each
  active filter appears as a removable chip, and there is a Reset action. Empty
  results offer to reset filters or add a folder.
- **Saved shelves:** smart shelves with saved filters.
- **Batch tagging:** select several books (checkboxes appear on hover, or stay
  once any book is selected) to tag them together.
- **Book Details:** shows metadata, and edits always win over extracted values. It
  has a primary Read book button. Unsaved corrections are confirmed before
  Escape or a backdrop click discards them. DELETE BOOK at the bottom, with
  confirmation, backs up the catalog and the EPUB, then removes the EPUB and its
  catalog data.
- **Covers:** extracted covers are cached on a user-selected drive. Settings →
  Covers controls whether covers are shown. A hidden, missing or broken cover is
  replaced by a generated cover: a stable colour gradient per title with the title
  text (written vertically for Japanese titles).
- **Duplicates:** exact normalized-title duplicates and candidate copies can be
  collapsed, keeping an available copy as the representative. Show all copies
  turns this off.

## Series

- **Series consolidation** (off by default): the library shows one card per series
  instead of one per volume. Groups come from explicit series labels or from
  bundled review-only proposals. Settings sets how strict the proposals are: high,
  high+medium, or all. Proposals are never saved as series assignments.
- **Series detail:** a breadcrumb leads back to the library, and Back restores the
  previous results, filters and scroll position. Detail follows the Grid/List
  setting and includes every copy. Matching-title copies can be grouped and
  expanded, and each copy opens and keeps its status independently.
- Proposed series names are searchable, and series cards are listed before
  individual books.

## Home (desktop)

- **Continue Reading:** up to 12 recent unfinished books. Removing a book from the
  list keeps its position. Reading a book on the phone adds it here.
- **Reading tracker:** a 52-week heatmap that can show time, characters or
  lookups. Selecting a day shows that day's per-book breakdown. Weekly totals
  cover time, estimated characters, lookups, reading days and finished books.
  Current and longest streaks are shown. Tracking can be paused, and a confirmed
  clear removes activity only.
- **Want to read shelf:** six books, with a link to the full filtered list.

## Reader (both platforms)

- Paginated single-page reading. Writing direction is Horizontal left to right
  or Vertical right to left; page-turn controls follow the direction.
- Five palettes: Light, Warm paper, Sepia, Dusk and Dark. Font size controls.
  Hide furigana option. Illustrations fit the page.
- A Go to chapter picker built from the book's own table of contents, including
  nested entries.
- Desktop shows a character-based progress percentage.
- **Reading statuses:** Unset, Want to read, Reading, Paused, Finished. Explicit
  completion dates are kept.
- **Position sync:** positions follow automatically between PC and phone. If you
  have navigated locally and the other device has moved too, the reader offers Go
  to synced position or Keep this position.

## Dictionary and words

- Clicking or tapping text looks it up, with Yomitan-style deinflection. Desktop
  uses Alt or Ctrl plus click. Results show the original matched text, sentence
  context, rich glossaries, frequency and pitch data, and images.
- **Yomitan dictionaries:** import ZIP dictionaries, then enable, reorder, replace
  or remove them, and set a per-dictionary result limit. Android also includes
  bundled offline JMdict. Manual dictionary search is available.
- **Saved passages:** lightweight bookmarks of a word and its sentence, with a
  note, that jump back to the place in the book.
- **Lookup history:** records successful lookups. Recording can be turned off and
  history can be cleared separately.

## Android companion

- Pairs privately with the PC over Tailscale Serve. Browses the full catalog
  offline, with search, filters and series consolidation, and downloads read-only
  copies.
- Home shows Continue Reading/Resume, recent books (including books read only on
  the PC, with a download action), the Want to read shelf, and a compact weekly
  tracker with a day strip, metrics and streaks.
- Progress, passages, history and reading activity sync both ways with offline
  queues. Sync saved data now and Sync activity now trigger delivery manually.
- A thumbnail cache with a size limit, and a recovery ZIP export.

## Settings (desktop)

Settings has eight categories: General, Library Roots, Covers, Dictionaries,
Android connection, Backup & restore (standalone SQLite backups, restore with
an automatic pre-restore backup), Maintenance (search-index rebuild) and Licenses.
The interface is available in English and Japanese.

---

# Part 2 — Proposals

2026-10-19. These are proposals only. Nothing here has been started, and each one
needs an explicit request before work begins (see AGENTS.md, "Current status").
Every proposal stays within the AGENTS.md boundaries: local-first, source EPUBs
read-only, no accounts or cloud, no searching inside book contents, no flashcards
or SRS, and no mandatory goals. Before writing any proposal, I checked the current
code to confirm the feature does not already exist.

Effort: **S** = under ~1 hour of agent work, **M** = a few hours,
**L** = multi-session. Ordered roughly by value-for-effort.

---

## 1. Reader typography controls (S, both platforms)

**Today:** the reflowable font stack and `line-height: 1.85` are hard-coded in
`src/features/reader/EpubReader.tsx` (~line 745) and
`apps/android/src/readerPresentation.ts`. Only font size is adjustable.

**Proposal:** add device-local preferences beside the existing
`tmw-reader-font-size`:
- `tmw-reader-font` — Gothic (sans) / Mincho (serif) / Publisher default.
- `tmw-reader-line-height` — 1.5 / 1.75 / 2.0 / 2.25.
- `tmw-reader-margin` — narrow / normal / wide (page padding, and the bounded
  horizontal reading width on desktop).

**Notes:** use the same "recreate pagination at the current CFI" path as the
writing mode and Hide furigana settings. Fixed-layout books keep the
publication's styling. Keep the Android Japanese locale and font ordering from
the kanji glyph fix. Add the new preferences to phone recovery exports.

## 2. "Next in series" handoff (S–M, desktop first)

**Today:** series membership exists (explicit series labels plus bundled
proposals), but finishing a volume leaves the reader at a dead end.

**Proposal:**
- On the reader's last page, and in Book Details for a book marked Finished,
  show a **Next volume** card. It finds the next member of the same effective
  series, ordered by volume number. Only the user's explicit series or a
  proposal at the selected confidence counts, and nothing is ever saved as a
  series assignment.
- Continue Reading on Home can show "Up next: <title>" for a recently finished
  series volume whose successor is still Unset or Want to read.

**Notes:** this only reads existing series data, so no migration is needed.
Never infer completion from a final-page visit; showing the card is not the
same as marking the book Finished.

## 3. Saved-passage and lookup-history export (S, desktop)

**Today:** passages and lookup history can only be viewed in the app (the user
reviews vocabulary separately in Jiten).

**Proposal:** add an Export button to Saved passages and Lookup history. It
writes a UTF-8 CSV or TSV file (with BOM, so Excel opens it correctly) with
these columns: surface, headword, reading, sentence, note, book title, date.
Filters cover one book, all books, or a date range. Use the Tauri save dialog,
so nothing is written into source folders.

**Notes:** this is a one-way export, not an SRS. A TSV whose columns Jiten or
Anki can import also serves people who study elsewhere, without adding any
study features to the app.

## 4. Estimated time left in chapter/book (S–M, both platforms)

**Today:** the app already has character-based progress locations and stores
measured characters covered against active reading time.

**Proposal:** show "~12 min left in chapter · ~4 h left in book" in the reader
footer. The estimate uses the user's own characters-per-minute rate, as a median
over the last N recorded days. Hide it until there is enough history (for
example, 30 minutes of measured reading), and for fixed-layout or image-only
books.

**Notes:** this is display-only. Remaining characters come from the existing
150-character location index, and the rate comes from existing activity tables.
It never creates new tracking data.

## 5. Distraction-free / fullscreen reading (S, desktop)

**Proposal:** F11 (or a toolbar button) puts the Tauri window into fullscreen.
The reader toolbar and footer auto-hide and reappear when the pointer reaches
the top or bottom edge, or on any keypress. Escape exits. The Alt/Ctrl lookup
modifier and its priority are unchanged.

**Notes:** reading-activity focus handling already uses native window focus, so
fullscreen should not affect time tracking. Check this anyway.

## 6. Personal rating and short review (S–M, desktop + sync)

**Proposal:** an optional 1–5 rating and private note on a book in Book Details,
which the library can sort and filter by ("Rated 4+", "Finished, unrated").
Smart shelves gain a rating filter.

**Notes:** this needs an additive catalog migration (user data, so it must be
included in backups). Syncing to the phone is optional, as a later v3 user-sync
field. Doing it desktop-only first is fine.

## 7. Optional daily reading target (S, both platforms)

**Proposal:** an opt-in target, off by default, such as "20 minutes" or
"5,000 characters a day". Home shows a quiet progress ring for today, and the
heatmap can outline days that met the target.

**Notes:** AGENTS.md rules out *mandatory* goals. This one stays off unless the
user enables it, with no nagging or notifications, and it is derived purely from
existing activity totals. The setting is a device-local preference.

## 8. Library statistics page (M, desktop)

**Proposal:** a Stats view built from data the app already stores. It shows
books finished per month or year (from explicit completion dates), total reading
time and characters per month, the most-read authors and series, and how the
library splits across statuses. Each view can be exported to CSV.

**Notes:** read-only queries only. It can extend Home's existing activity
aggregation instead of adding new tables.

## 9. Desktop reader keyboard map and help overlay (S)

**Proposal:** add more keyboard shortcuts:
- `PageUp`/`PageDown` and `Space` for page turns.
- `T` to open the table of contents.
- `F` to toggle furigana.
- `D` for manual dictionary search.
- `B` to save the current sentence as a passage.

Pressing `?` shows an overlay listing every shortcut. Page-turn keys follow the
writing mode, matching the existing vertical-rl rule.

## 10. Lookup "seen before" indicator (M, both platforms)

**Proposal:** when the dictionary popup opens for a headword the user has looked
up before, show "Looked up 3× · last in <book>". That entry links to the
existing lookup history.

**Notes:** this uses existing portable history identities (source/revision plus
headword/reading), so ambiguous results stay unresolved instead of guessed. It
helps people notice repeated lookups without turning into an SRS.

---

## Already tracked

Planned earlier and done only when requested:
- automatic phone recovery import
- correct journal compaction
- phone metadata/tag/collection editing
- true EPUB streaming
- global cross-book jumps from phone history

## Deliberately excluded

The following were considered and left out because they conflict with
AGENTS.md: full-text search of book content, cloud sync or accounts,
flashcards/SRS/mastery scores, OCR, and anything that writes into source
folders.
