import { READER_FURIGANA_KEY, applyReaderFurigana } from "../../../packages/reader-core/readerFurigana";
import { READER_WRITING_KEY, readerWriting, applyReaderWriting } from "../../../packages/reader-core/readerWriting";
import { READER_THEME_KEY, readerThemes, readerTheme, applyReaderColors, type ReaderTheme } from "../../../packages/reader-core/readerTheme";
import { caretAt } from "../../../packages/reader-core/caret";
import { ReadingStatus } from "../../components/ReadingStatus";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import ePub, { type Book, type Rendition } from "epubjs";
import { dictionaryTextAt, japaneseWordAt } from "./dictionaryText";
import { readerRenditionOptions } from "./readerConfig";
import { turnReaderPage } from "./readerNavigation";
import { readerProgress, type ProgressLocation } from "./readerProgress";
import { useEffect, useRef, useState, type CSSProperties } from "react";
import { sentenceAt } from "./passageContext";
import { SavedPassages } from "../../components/SavedPassages";
import { LookupHistory } from "../../components/LookupHistory";
import { portableDictionaryId } from "../../../packages/reader-core/dictionaryLookup";
import type { ChunkDictionaryEntry, ChunkLookupResponse } from "../../../packages/reader-core/dictionaryLookup";
import DictionaryGlossary from "../../../packages/reader-core/DictionaryGlossary";
import DictionaryManager from "../../../packages/reader-core/DictionaryManager";
import ChapterPicker, { type Chapter } from "../../../packages/reader-core/ChapterPicker";

type ReaderBook = { id: number; filePath: string; title: string };
type Theme = ReaderTheme;
const FONT_MIN = 80;
const FONT_MAX = 180;

export function EpubReader({
  bookId,
  onClose,
  initialCfi,
}: {
  bookId: number;
  onClose: () => void;
  initialCfi?: string;
}) {
  const [target, setTarget] = useState<{
    sourceBookId: number;
    bookId: number;
    cfi: string;
  } | null>(null);
  const current = target?.sourceBookId === bookId ? target : null;
  return (
    <ReaderSession
      key={current?.bookId ?? bookId}
      bookId={current?.bookId ?? bookId}
      initialCfi={current?.cfi ?? initialCfi}
      onClose={onClose}
      onHistoryJump={(nextBookId, cfi) =>
        setTarget({ sourceBookId: bookId, bookId: nextBookId, cfi })
      }
    />
  );
}

function ReaderSession({
  bookId,
  onClose,
  initialCfi,
  onHistoryJump,
}: {
  bookId: number;
  onClose: () => void;
  initialCfi?: string;
  onHistoryJump: (bookId: number, cfi: string) => void;
}) {
  const [hideFurigana, setHideFurigana] = useState(() => localStorage.getItem(READER_FURIGANA_KEY) === "true");
  const [writing, setWriting] = useState(() => readerWriting(localStorage.getItem(READER_WRITING_KEY)));
  const host = useRef<HTMLDivElement>(null);
  const rendition = useRef<Rendition | null>(null);
  const book = useRef<Book | null>(null);
  const fixedLayout = useRef(false);
  const saveTimer = useRef<number | null>(null);
  const latestLocation = useRef<string | null>(null);
  const advanceRef = useRef<() => Promise<void>>(async () => {});
  const retreatRef = useRef<() => Promise<void>>(async () => {});
  const [readerBook, setReaderBook] = useState<ReaderBook | null>(null);
  const [theme, setTheme] = useState<Theme>(() =>
    readerTheme(localStorage.getItem(READER_THEME_KEY), "dark"),
  );
  const [fontSize, setFontSize] = useState(
    () => Number(localStorage.getItem("tmw-reader-font-size")) || 110,
  );
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(true);
  const [chapters, setChapters] = useState<Chapter[]>([]);
  const [chaptersReady, setChaptersReady] = useState(false);
  const [chapterPending, setChapterPending] = useState(false);
  const [finished, setFinished] = useState(false);
  const [statusRevision, setStatusRevision] = useState(0);
  const [progress, setProgress] = useState<number | null>(null);
  const [dictionaryOpen, setDictionaryOpen] = useState(false);
  const [dictionaryManagerOpen, setDictionaryManagerOpen] = useState(false);
  const [dictionaryEnabled, setDictionaryEnabled] = useState(
    () => localStorage.getItem("tmw-dictionary-enabled") !== "false",
  );
  const [dictionaryQuery, setDictionaryQuery] = useState("");
  const [dictionaryEntries, setDictionaryEntries] = useState<ChunkDictionaryEntry[]>([]);
  const [dictionaryError, setDictionaryError] = useState<string | null>(null);
  const [dictionaryModifier, setDictionaryModifier] = useState<"alt" | "ctrl">(() =>
    localStorage.getItem("tmw-dictionary-modifier") === "ctrl" ? "ctrl" : "alt",
  );
  const [passagesOpen, setPassagesOpen] = useState(false);
  const [sentence, setSentence] = useState("");
  const [passageNote, setPassageNote] = useState("");
  const [passageMessage, setPassageMessage] = useState<string | null>(null);
  const [savingPassage, setSavingPassage] = useState(false);
  const [selection, setSelection] = useState<{ surface: string; cfi: string } | null>(null);
  const lookupGeneration = useRef(0);
  const dictionaryPopup = useRef<HTMLElement | null>(null);
  const dismissDictionary = () => { lookupGeneration.current++; setDictionaryOpen(false); };
  useEffect(() => {
    if (!dictionaryOpen) return;
    const outside = (event: PointerEvent) => {
      if (!dictionaryPopup.current?.contains(event.target as Node)) { lookupGeneration.current++; setDictionaryOpen(false); }
    };
    document.addEventListener("pointerdown", outside, true);
    return () => document.removeEventListener("pointerdown", outside, true);
  }, [dictionaryOpen]);

  const [historyOpen, setHistoryOpen] = useState(false);
  const [lookupCount, setLookupCount] = useState<number | null>(null);
  const [historyError, setHistoryError] = useState<string | null>(null);

  async function recordLookup(
    entries: ChunkDictionaryEntry[],
    surface: string,
    generation: number,
    anchor?: { cfi: string; sentence: string },
  ) {
    if (entries.length === 0 || generation !== lookupGeneration.current) return;
    // Multiple senses can share an identity. Different headwords/readings remain unresolved.
    const first = entries[0];
    const reliable = entries.every(
      (entry) => entry.term === first.term && entry.reading === first.reading,
    );
    try {
      const dictionaryId = reliable ? await portableDictionaryId(first.provenance) : null;
      if (generation !== lookupGeneration.current) return;
      const count = await invoke<number | null>("record_lookup_history", {
        request: {
          surface,
          entryId: null,
          headword: reliable ? first.term : null,
          reading: reliable ? first.reading : null,
          dictionaryId,
          bookId: anchor ? bookId : null,
          locationCfi: anchor?.cfi ?? "",
          sentence: anchor?.sentence ?? "",
        },
      });
      if (generation === lookupGeneration.current) setLookupCount(count);
    } catch (reason) {
      if (generation === lookupGeneration.current)
        setHistoryError(`Could not record lookup history: ${String(reason)}`);
    }
  }

  async function lookupDictionary(query: string) {
    const generation = ++lookupGeneration.current;
    setDictionaryQuery(query);
    setDictionaryOpen(true);
    setDictionaryError(null);
    setSelection(null);
    setLookupCount(null);
    setHistoryError(null);
    setDictionaryEntries([]);
    try {
      const response = await invoke<ChunkLookupResponse>("lookup_dictionary", { query });
      const entries = response.groups.flatMap(group => group.matches.map(match => match.entry));
      if (generation === lookupGeneration.current) {
        setDictionaryEntries(entries);
        await recordLookup(entries, query, generation);
      }
    } catch (reason) {
      if (generation === lookupGeneration.current) {
        setDictionaryEntries([]);
        setDictionaryError(`Dictionary lookup failed: ${String(reason)}`);
      }
    }
  }

  async function lookupTarget(text: string, offset: number, cfi: string) {
    const generation = ++lookupGeneration.current;
    setLookupCount(null);
    setHistoryError(null);
    setSelection({ surface: japaneseWordAt(text, offset), cfi });
    setSentence(sentenceAt(text, offset));
    setPassageNote("");
    setPassageMessage(null);
    setDictionaryEntries([]);
    try {
      const response = await invoke<ChunkLookupResponse>("lookup_reader_text", {
        request: { text, offset },
      });
      const { target } = response;
      const entries = response.groups.flatMap(group => group.matches.map(match => match.entry));
      if (generation !== lookupGeneration.current) return;
      setSelection({ surface: target.surface, cfi });
      setDictionaryQuery(target.surface);
      setDictionaryOpen(true);
      setDictionaryError(null);
      if (generation === lookupGeneration.current) {
        setDictionaryEntries(entries);
        await recordLookup(entries, target.surface, generation, {
          cfi,
          sentence: sentenceAt(text, offset),
        });
      }
    } catch (reason) {
      if (generation !== lookupGeneration.current) return;
      setDictionaryOpen(true);
      setDictionaryQuery(japaneseWordAt(text, offset));
      setDictionaryError(String(reason));
    }
  }

  async function savePassage() {
    const target = selection;
    if (!target) return;
    setSavingPassage(true); setPassageMessage(null);
    try {
      const entry = dictionaryEntries[0];
      await invoke("save_passage", { request: { bookId, surface: target.surface, headword: entry?.term ?? null, reading: entry?.reading ?? null, sentence, note: passageNote, locationCfi: target.cfi } });
      setPassageMessage("Passage saved. Duplicate saves keep your existing bookmark and edits.");
    } catch (reason) { setPassageMessage(`Could not save passage: ${String(reason)}`); }
    finally { setSavingPassage(false); }
  }

  async function advance() {
    const view = rendition.current;
    if (!view) return;
    try {
      await turnReaderPage(view, "next", {
        location: () => view.currentLocation(),
        nextSection: async (index) => {
          if (rendition.current !== view) return;
          const next = book.current?.spine.get(index)?.next();
          if (next?.href) await view.display(next.href);
        },
      });
    } catch (reason) {
      setError(`Could not turn the page: ${String(reason)}`);
    }
  }

  async function retreat() {
    try {
      if (rendition.current) await turnReaderPage(rendition.current, "previous");
    } catch (reason) {
      setError(`Could not turn the page: ${String(reason)}`);
    }
  }
  useEffect(() => {
    advanceRef.current = advance;
    retreatRef.current = retreat;
  });

  useEffect(() => {
    let disposed = false;
    const resumeCfi = latestLocation.current;
    latestLocation.current = null;
    const open = async () => {
      try {
        const selected = await invoke<ReaderBook | null>("get_reader_book", { bookId });
        if (!selected) throw new Error("This book is no longer in the catalog.");
        if (disposed || !host.current) return;
        setProgress(null);
        setBusy(true);
        setReaderBook(selected);
        const savedLocation = await invoke<string | null>("get_reading_location", { bookId });
        if (disposed || !host.current) return;
        const instance = ePub(convertFileSrc(selected.filePath));
        book.current = instance;
        void instance.loaded?.navigation.then((navigation) => {
          if (!disposed) { setChapters(navigation.toc); setChaptersReady(true); }
        }).catch(() => { if (!disposed) setChaptersReady(true); });
        // Wait until epub.js has parsed the package before creating the rendition.
        // In particular, manga commonly declares a fixed (pre-paginated) layout,
        // SVG pages, spreads, and its own page progression direction. Forcing the
        // reflowable/RTL defaults here breaks those publications.
        await instance.opened;
        if (disposed || !host.current) return;
        fixedLayout.current = instance.packaging.metadata.layout === "pre-paginated";
        instance.spine.hooks.content.register((doc: Document) => {
          applyReaderWriting(doc, writing, fixedLayout.current);
          applyReaderFurigana(doc, hideFurigana, fixedLayout.current);
        });
        host.current.style.maxWidth = !fixedLayout.current && writing === "horizontal-tb" ? "64rem" : "none";
        const view = instance.renderTo(host.current, { ...readerRenditionOptions(), ...(!fixedLayout.current ? { flow: "paginated", defaultDirection: "ltr", spread: "none" } : {}) });
        rendition.current = view;
        let characterIndexReady = false;
        const updateProgress = (location: ProgressLocation) => {
          setProgress(characterIndexReady ? readerProgress(location, instance.locations) : null);
        };
        // Opening and saving CFIs need not wait for the sequential text index.
        if (!fixedLayout.current && instance.locations) {
          void instance.locations.generate(150).then((positions) => {
            if (disposed) return;
            characterIndexReady = positions.length > 1;
            updateProgress((view.currentLocation() as ProgressLocation) ?? {});
          }).catch(() => { /* Reading remains available if indexing fails. */ });
        }
        view.on("relocated", (location) => {
          if (disposed) return;
          updateProgress(location as ProgressLocation);
          const position = location as { start?: { cfi?: string } };
          const cfi = position.start?.cfi;
          if (!cfi) return;
          latestLocation.current = cfi;
          if (saveTimer.current) window.clearTimeout(saveTimer.current);
          saveTimer.current = window.setTimeout(() => {
            void invoke("save_reading_location", { bookId, locationCfi: cfi }).catch((reason) =>
              setError(`Could not save reading position: ${String(reason)}`),
            );
          }, 700);
        });
        view.on("rendered", () => { applyAppearance(view, theme, fontSize, fixedLayout.current); attachDictionaryHandlers(view, dictionaryEnabled, dictionaryModifier, lookupTarget, dismissDictionary); });
        view.on("keydown", (event) =>
          handleReaderKey(event as KeyboardEvent, advanceRef.current, retreatRef.current, onClose, writing),
        );
        applyAppearance(view, theme, fontSize, fixedLayout.current);
        await view.display(resumeCfi ?? initialCfi ?? savedLocation ?? undefined);
        if (disposed) return;
        const completed = await invoke<boolean>("record_reader_open", { bookId });
        if (!disposed) { setFinished(completed); setBusy(false); }
      } catch (reason) {
        if (!disposed) {
          setError(
            `This EPUB could not be opened. It may be malformed, DRM-protected, or no longer available. ${String(reason)}`,
          );
          setBusy(false);
        }
      }
    };
    void open();
    return () => {
      disposed = true;
      if (saveTimer.current) window.clearTimeout(saveTimer.current);
      if (latestLocation.current) {
        void invoke("save_reading_location", { bookId, locationCfi: latestLocation.current });
      }
      rendition.current?.destroy();
      book.current?.destroy();
      rendition.current = null;
      book.current = null;
      fixedLayout.current = false;
    };
    // Writing mode recreates pagination at the saved CFI; other appearance changes apply below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [bookId, writing, hideFurigana]);

  useEffect(() => {
    localStorage.setItem(READER_THEME_KEY, theme);
    if (rendition.current) applyAppearance(rendition.current, theme, fontSize, fixedLayout.current);
  }, [theme, fontSize]);
  useEffect(() => {
    localStorage.setItem("tmw-reader-font-size", String(fontSize));
  }, [fontSize]);
  useEffect(() => { localStorage.setItem("tmw-dictionary-enabled", String(dictionaryEnabled)); void invoke("set_app_setting", { key: "reader_dictionary_enabled", value: String(dictionaryEnabled) }); }, [dictionaryEnabled]);
  useEffect(() => { localStorage.setItem("tmw-dictionary-modifier", dictionaryModifier); void invoke("set_app_setting", { key: "reader_dictionary_modifier", value: dictionaryModifier }); }, [dictionaryModifier]);
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      handleReaderKey(event, advanceRef.current, retreatRef.current, onClose, writing);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose, writing]);

  return (
    <main
      style={{ background: readerThemes[theme].background, color: readerThemes[theme].foreground,
        colorScheme: readerThemes[theme].dark ? "dark" : "light",
        "--reader-background": readerThemes[theme].background,
        "--reader-foreground": readerThemes[theme].foreground } as CSSProperties}
      className={`desktop-reader fixed inset-0 z-50 flex flex-col ${readerThemes[theme].dark ? "bg-stone-950 text-stone-100" : "bg-stone-100 text-stone-900"}`}
      aria-label="EPUB reader"
    >
      <header
        style={{ background: readerThemes[theme].background }}
        className={`flex shrink-0 flex-wrap items-center justify-between gap-3 border-b px-4 py-3 ${readerThemes[theme].dark ? "border-white/10 bg-stone-900" : "border-stone-300 bg-white"}`}
      >
        <div className="min-w-0 flex-1 basis-64">
          <p className="truncate font-medium" title={readerBook?.title}>{readerBook?.title ?? "Opening book…"}</p>
          {progress !== null && !busy && !error && (
            <p
              className="text-xs tabular-nums opacity-60"
              aria-label={`Book progress: approximately ${progress.toFixed(1)} percent`}
              title="Estimated from character position across the whole book."
            >
              ≈ {progress.toFixed(1)}% read
            </p>
          )}
        </div>
        <div className="reader-toolbar-actions flex flex-wrap items-center gap-2">
          <ChapterPicker overlay background={readerThemes[theme].background} chapters={chapters} ready={chaptersReady} pending={busy || chapterPending}
            onJump={(href) => {
              const view = rendition.current;
              if (!view || chapterPending) return;
              lookupGeneration.current++;
              setDictionaryOpen(false);
              setChapterPending(true);
              void view.display(href).catch((reason) => {
                if (rendition.current === view) setError(`Could not open chapter: ${String(reason)}`);
              }).finally(() => { if (rendition.current === view) setChapterPending(false); });
            }} />
          <button
            className="reader-control"
            aria-expanded={historyOpen}
            title="Show lookup history"
            onClick={() => {
              setHistoryOpen((value) => !value);
              setPassagesOpen(false);
            }}
          >
            Lookup history
          </button>
          <button className="reader-control" aria-expanded={passagesOpen} title="Show saved passages" onClick={() => setPassagesOpen((value) => !value)}>
            Saved passages
          </button>
          <button
            className="reader-control"
            disabled={busy}
            aria-pressed={finished}
            onClick={() => {
              void invoke("set_reader_finished", { bookId, finished: !finished })
                .then(() => { setFinished(!finished); setStatusRevision(value => value + 1); })
                .catch((reason) => setError(String(reason)));
            }}
          >
            {finished ? "Finished · Mark unfinished" : "Mark finished"}
          </button>
          {!busy && !error && <ReadingStatus compact key={statusRevision} bookId={bookId} onChanged={() => { void invoke<{status: string}>("get_reading_state", { bookId }).then(state => setFinished(state.status === "finished")); }} />}
          <div className="reader-font-group flex items-center" role="group" aria-label="Text size">
          <button
            className="reader-control"
            onClick={() => setFontSize((size) => Math.max(FONT_MIN, size - 10))}
            aria-label="Decrease font size"
            title="Decrease text size"
            disabled={fontSize <= FONT_MIN}
          >
            A−
          </button>
          <span className="w-12 text-center text-xs tabular-nums">{fontSize}%</span>
          <button
            className="reader-control"
            onClick={() => setFontSize((size) => Math.min(FONT_MAX, size + 10))}
            aria-label="Increase font size"
            title="Increase text size"
            disabled={fontSize >= FONT_MAX}
          >
            A+
          </button>
          </div>
          <label className="reader-control flex items-center gap-2"><input type="checkbox" checked={hideFurigana} onChange={(event) => { localStorage.setItem(READER_FURIGANA_KEY, String(event.target.checked)); setHideFurigana(event.target.checked); }} />Hide furigana</label>
          <select className="reader-control" aria-label="Writing direction" value={writing} onChange={(event) => { localStorage.setItem(READER_WRITING_KEY, event.target.value); setWriting(readerWriting(event.target.value)); }}><option value="horizontal-tb">Horizontal left to right</option><option value="vertical-rl">Vertical right to left</option></select>
          <select className="reader-control" aria-label="Reading theme" value={theme}
            title="Reading theme"
            style={{ colorScheme: readerThemes[theme].dark ? "dark" : "light", backgroundColor: readerThemes[theme].background, color: readerThemes[theme].foreground }}
            onChange={(event) => setTheme(readerTheme(event.target.value, "dark"))}>
            {Object.entries(readerThemes).map(([value, palette]) =>
              <option key={value} value={value} style={{ backgroundColor: readerThemes[theme].background, color: readerThemes[theme].foreground }}>{palette.label}</option>)}
          </select>
          <button
            className="reader-control"
            aria-pressed={dictionaryEnabled}
            title={`Word lookup: ${dictionaryModifier === "alt" ? "Alt" : "Ctrl"}-click text`}
            onClick={() => setDictionaryEnabled((value) => !value)}
          >
            Dictionary {dictionaryEnabled ? "on" : "off"}
          </button>
          <button className="reader-control" title="Close reader (Esc)" onClick={onClose}>
            Close
          </button>
        </div>
      </header>
      <section className="relative min-h-0 flex-1">
        <div
          ref={host}
          style={{ background: readerThemes[theme].background }}
          className={`h-full w-full mx-auto ${readerThemes[theme].dark ? "bg-stone-950" : "bg-stone-100"}`}
        />
        {busy && !error && (
          <p className="absolute inset-0 grid place-items-center">Opening EPUB…</p>
        )}
        {error && (
          <div className="absolute inset-0 grid place-items-center p-6">
            <div className="max-w-lg rounded-xl border border-red-400/30 bg-red-950/30 p-5 text-center text-red-100">
              <p>{error}</p>
              <button className="mt-4 rounded border border-red-200/50 px-4 py-2" onClick={onClose}>
                Return to library
              </button>
            </div>
          </div>
        )}
        {dictionaryOpen && (
          <aside
            style={{ background: readerThemes[theme].background, color: readerThemes[theme].foreground }}
            className={`dictionary-popup ${readerThemes[theme].dark ? "bg-stone-900 text-stone-100" : "bg-white text-stone-900"}`}
            ref={dictionaryPopup}
            aria-label="Offline dictionary"
          >
            <div className="flex items-center gap-2">
              <input
                className="dictionary-input"
                value={dictionaryQuery}
                onChange={(event) => setDictionaryQuery(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") void lookupDictionary(dictionaryQuery);
                }}
                autoFocus
              />
              <button
                className="reader-control"
                onClick={() => void lookupDictionary(dictionaryQuery)}
              >
                Look up
              </button>
              <button className="reader-control" onClick={() => setDictionaryOpen(false)}>
                ×
              </button>
            </div>
            {dictionaryError && <p className="mt-3 text-sm text-red-400">{dictionaryError}</p>}
            {lookupCount !== null && (
              <p className="mt-2 text-xs opacity-70">
                Looked up {lookupCount} {lookupCount === 1 ? "time" : "times"} in retained history
              </p>
            )}
            {historyError && (
              <p role="alert" className="mt-2 text-sm text-amber-400">
                {historyError}
              </p>
            )}
            {!dictionaryError && dictionaryEntries.length === 0 && (
              <p className="mt-3 text-sm opacity-70">
                No imported dictionary match found. Import or enable a local Yomitan ZIP in Manage dictionaries.
              </p>
            )}
            {dictionaryEntries.map((entry) => (
              <article key={`${entry.provenance.source}:${entry.id}`} className="mt-3 border-t border-current/15 pt-2">
                <div className="font-medium">{entry.term} <span className="text-sm opacity-70">{entry.reading}</span></div>
                <div className="text-xs opacity-60">{entry.provenance.title}</div>
                <DictionaryGlossary entry={entry} />
              </article>
            ))}
            {selection && (
              <div className="mt-4 space-y-2 border-t border-current/15 pt-3">
                <label className="block text-sm">
                  Sentence
                  <textarea
                    className="dictionary-input w-full"
                    maxLength={4000}
                    value={sentence}
                    onChange={(event) => setSentence(event.target.value)}
                  />
                </label>
                <label className="block text-sm">
                  Optional note
                  <input
                    className="dictionary-input w-full"
                    maxLength={2000}
                    value={passageNote}
                    onChange={(event) => setPassageNote(event.target.value)}
                  />
                </label>
                <button
                  className="reader-control"
                  disabled={savingPassage}
                  onClick={() => void savePassage()}
                >
                  {savingPassage ? "Saving…" : "Save passage"}
                </button>
                {passageMessage && (
                  <p role="status" className="text-sm">
                    {passageMessage}
                  </p>
                )}
              </div>
            )}
          </aside>
        )}
        {passagesOpen && (
          <aside
            style={{ background: readerThemes[theme].background }}
            className={`absolute inset-y-0 right-0 z-10 w-full max-w-xl overflow-y-auto p-5 ${readerThemes[theme].dark ? "bg-stone-900" : "bg-white"}`}
          >
            <button className="reader-control mb-4" onClick={() => setPassagesOpen(false)}>
              Close passages
            </button>
            <SavedPassages
              bookId={bookId}
              onJump={async (_passage, cfi) => {
                if (!rendition.current) throw new Error("Reader is not ready.");
                await rendition.current.display(cfi);
                setPassagesOpen(false);
              }}
            />
          </aside>
        )}
        {historyOpen && (
          <aside
            style={{ background: readerThemes[theme].background }}
            className={`absolute inset-y-0 right-0 z-20 w-full max-w-xl overflow-y-auto p-5 ${readerThemes[theme].dark ? "bg-stone-900" : "bg-white"}`}
          >
            <button className="reader-control mb-4" onClick={() => setHistoryOpen(false)}>
              Close history
            </button>
            <LookupHistory
              onJump={async (targetBookId, cfi) => {
                if (targetBookId !== bookId) {
                  onHistoryJump(targetBookId, cfi);
                  return;
                }
                if (!rendition.current) throw new Error("Reader is not ready.");
                await rendition.current.display(cfi);
                setHistoryOpen(false);
              }}
            />
          </aside>
        )}
      </section>
      <footer
        style={{ background: readerThemes[theme].background }}
        className={`flex shrink-0 justify-between border-t p-3 ${readerThemes[theme].dark ? "border-white/10 bg-stone-900" : "border-stone-300 bg-white"}`}
      >
        <div className="flex flex-wrap items-center gap-2">
          <button className="reader-control" onClick={() => setDictionaryManagerOpen(value => !value)} aria-expanded={dictionaryManagerOpen}>Manage dictionaries</button>
          <button className="reader-control" onClick={() => setDictionaryModifier((value) => (value === "alt" ? "ctrl" : "alt"))}>
            Trigger: {dictionaryModifier === "alt" ? "Alt-click" : "Ctrl-click"}
          </button>
        </div>
        <nav className="flex items-center gap-2" aria-label="Page navigation">
          <button className="reader-control" aria-label={writing === "vertical-rl" ? "Next page" : "Previous page"} disabled={!!error || busy}
            onClick={() => void (writing === "vertical-rl" ? advance() : retreat())}>
            {writing === "vertical-rl" ? "← Next" : "← Previous"}
          </button>
          <button className="reader-control" aria-label={writing === "vertical-rl" ? "Previous page" : "Next page"} disabled={!!error || busy}
            onClick={() => void (writing === "vertical-rl" ? retreat() : advance())}>
            {writing === "vertical-rl" ? "Previous →" : "Next →"}
          </button>
        </nav>
      </footer>
      {dictionaryManagerOpen && <aside className="absolute inset-4 z-50 overflow-auto rounded-xl bg-stone-900 p-4 text-stone-100" aria-label="Dictionary management">
        <button className="reader-control" onClick={() => setDictionaryManagerOpen(false)}>Close dictionary management</button>
        <DictionaryManager />
      </aside>}
    </main>
  );
}

function applyAppearance(rendition: Rendition, theme: Theme, fontSize: number, fixedLayout: boolean) {
  rendition.themes.default({
    html: {
      "background-color": `${readerThemes[theme].background} !important`,
    },
    body: fixedLayout ? {
      "background-color": `${readerThemes[theme].background} !important`,
      background: `${readerThemes[theme].background} !important`,
    } : {
      "background-color": `${readerThemes[theme].background} !important`,
      background: `${readerThemes[theme].background} !important`,
      color: `${readerThemes[theme].foreground} !important`,
      "font-family": "Yu Gothic UI, Yu Gothic, Meiryo, serif",
      "line-height": "1.85",
    },
  });
  (rendition.getContents?.() as unknown as Array<{ document: Document }> | undefined)?.forEach((content) => applyReaderColors(content.document, theme, fixedLayout));
  if (!fixedLayout) rendition.themes.fontSize(`${fontSize}%`);
}

function attachDictionaryHandlers(
  rendition: Rendition,
  enabled: boolean,
  modifier: "alt" | "ctrl",
  lookup: (text: string, offset: number, cfi: string) => Promise<void>,
  dismiss: () => void,
) {
  for (const content of rendition.getContents?.() ?? []) {
    const document = content.document;
    if (document.documentElement.dataset.tmwDictionaryBound === "true") continue;
    document.documentElement.dataset.tmwDictionaryBound = "true";
    document.addEventListener("pointerdown", dismiss, true);
    document.addEventListener("click", (event) => {
      if (!enabled) return;
      if (!(modifier === "alt" ? event.altKey : event.ctrlKey)) return;
      const point = caretAt(document, event.clientX, event.clientY, false);
      if (!point) return;
      const block = (point.node.parentElement?.closest("p,div,li,td,h1,h2,h3") ?? point.node.parentElement);
      const target = block ? dictionaryTextAt(block, point.node, point.offset) : null;
      const raw = target?.text ?? point.node.data;
      const offset = target?.offset ?? [...point.node.data.slice(0, point.offset)].length;
      if (japaneseWordAt(raw, offset)) {
        event.preventDefault();
        let cfi = "";
        try { const range = document.createRange(); range.setStart(point.node, point.offset); range.collapse(true); cfi = content.cfiFromRange(range); } catch { /* Excerpts can be saved without an anchor. */ }
        void lookup(raw, offset, cfi);
      }
    }, true);
  }
}

function handleReaderKey(
  event: KeyboardEvent,
  advance: () => Promise<void>,
  retreat: () => Promise<void>,
  onClose: () => void,
  writing: string = "horizontal-tb",
) {
  const target = event.target as HTMLElement | null;
  if (target?.closest?.("input,textarea,select,[contenteditable='true']")) return;
  if (event.key === "Escape") {
    event.preventDefault();
    onClose();
    return;
  }
  // Match the pagination axis chosen by epub.js for the selected writing mode.
  if (event.key === (writing === "vertical-rl" ? "ArrowLeft" : "ArrowRight")) {
    event.preventDefault();
    void advance();
  }
  if (event.key === (writing === "vertical-rl" ? "ArrowRight" : "ArrowLeft")) {
    event.preventDefault();
    void retreat();
  }
}
