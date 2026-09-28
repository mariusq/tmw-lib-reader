# TMW EPUB Library

This is a local-first Tauri desktop catalog. EPUB source folders are read-only: scanning and search never modify EPUBs or source directories.

## Search

Phase 6 uses SQLite FTS5 with the bundled SQLite trigram tokenizer for fast local substring matching across effective title, author, series, filename, folder path, and tags. Matching text is regenerated in `book_search_documents` and `book_search_fts`; originals remain unchanged. The app applies Unicode NFKC normalization, collapses whitespace, and uses case-insensitive matching where applicable, so `1巻` matches `１巻`. Very short queries use a normalized indexed-field fallback because trigram indexes require three characters. Results prioritize exact and prefix title matches over metadata, filename, and path matches.

The index is rebuilt locally at startup and refreshed after scanner metadata extraction. It contains no book contents and makes no network requests.

## Romaji and reading search

Phase 7 adds a local, regenerable reading index for effective titles, authors, series, and filenames. It uses [Lindera](https://github.com/lindera/lindera) with its embedded IPADIC dictionary, both distributed under MIT terms. The dictionary is compiled into the Windows application rather than downloaded at runtime, so indexing has no network dependency. Kana readings are converted using Hepburn-style romaji (`しんげき` → `shingeki`); width, case, hyphens, and repeated whitespace are normalized on input.

This intentionally assistive analyzer is conservative: unknown or ambiguous tokens do not change displayed metadata, and Japanese-text search still works normally. The embedded dictionary increases the Windows binary size, but is loaded once per process and reused for indexing. IPADIC readings can be imperfect for unusual names, neologisms, and stylized titles.

Readings are generated-only: the UI deliberately offers no per-book editing or aliases, which keeps the workflow suitable for very large libraries. All derived fields are rebuilt locally on startup, with no source EPUB changes.
