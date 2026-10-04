import { caretAt } from "../../../packages/reader-core/caret";
import { ReadingStatus } from "../../components/ReadingStatus";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import ePub, { type Book, type Rendition } from "epubjs";
import { dictionaryTextAt, japaneseWordAt } from "./dictionaryText";
import { readerRenditionOptions } from "./readerConfig";
import { turnReaderPage } from "./readerNavigation";
import { readerProgress, type ProgressLocation } from "./readerProgress";
import { useEffect, useRef, useState } from "react";
import { sentenceAt } from "./passageContext";
import { SavedPassages } from "../../components/SavedPassages";
import { LookupHistory } from "../../components/LookupHistory";

type ReaderBook = { id: number; filePath: string; title: string };
type Theme = "dark" | "light";
type DictionaryEntry = { id: number; term: string; reading: string | null; definitions: string[]; partOfSpeech: string[]; dictionaryName: string };
type DictionarySummary = { id: number; name: string; sourcePath: string; enabled: boolean; importedAt: number; entryCount: number };
type DictionaryTarget = { surface: string; lemma: string; reading: string | null };
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
    localStorage.getItem("tmw-reader-theme") === "light" ? "light" : "dark",
  );
  const [fontSize, setFontSize] = useState(
    () => Number(localStorage.getItem("tmw-reader-font-size")) || 110,
  );
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(true);
  const [finished, setFinished] = useState(false);
  const [statusRevision, setStatusRevision] = useState(0);
  const [progress, setProgress] = useState<number | null>(null);
  const [dictionaryOpen, setDictionaryOpen] = useState(false);
  const [dictionaryEnabled, setDictionaryEnabled] = useState(
    () => localStorage.getItem("tmw-dictionary-enabled") !== "false",
  );
  const [dictionaryQuery, setDictionaryQuery] = useState("");
  const [dictionaryEntries, setDictionaryEntries] = useState<DictionaryEntry[]>([]);
  const [dictionaryError, setDictionaryError] = useState<string | null>(null);
  const [dictionaryModifier, setDictionaryModifier] = useState<"alt" | "ctrl">(() =>
    localStorage.getItem("tmw-dictionary-modifier") === "ctrl" ? "ctrl" : "alt",
  );
  const [dictionaries, setDictionaries] = useState<DictionarySummary[]>([]);
  const [dictionaryStatus, setDictionaryStatus] = useState("Preparing bundled JMdict…");
  const [passagesOpen, setPassagesOpen] = useState(false);
  const [sentence, setSentence] = useState("");
  const [passageNote, setPassageNote] = useState("");
  const [passageMessage, setPassageMessage] = useState<string | null>(null);
  const [savingPassage, setSavingPassage] = useState(false);
  const [selection, setSelection] = useState<{ surface: string; cfi: string } | null>(null);
  const lookupGeneration = useRef(0);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [lookupCount, setLookupCount] = useState<number | null>(null);
  const [historyError, setHistoryError] = useState<string | null>(null);

  async function recordLookup(
    entries: DictionaryEntry[],
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
      const count = await invoke<number | null>("record_lookup_history", {
        request: {
          surface,
          entryId: reliable ? first.id : null,
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

  async function refreshDictionaries() {
    try { setDictionaries(await invoke<DictionarySummary[]>("list_dictionaries")); } catch { setDictionaries([]); }
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
      const entries = await invoke<DictionaryEntry[]>("lookup_dictionary", { query });
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
      const target = await invoke<DictionaryTarget>("tokenize_dictionary_target", { text, offset });
      if (generation !== lookupGeneration.current) return;
      setSelection({ surface: target.surface, cfi });
      setDictionaryQuery(target.surface);
      setDictionaryOpen(true);
      setDictionaryError(null);
      let entries = await invoke<DictionaryEntry[]>("lookup_dictionary", { query: target.surface });
      if (entries.length === 0 && target.lemma !== target.surface)
        entries = await invoke<DictionaryEntry[]>("lookup_dictionary", { query: target.lemma });
      if (entries.length === 0 && target.reading)
        entries = await invoke<DictionaryEntry[]>("lookup_dictionary", { query: target.reading });
      if (generation === lookupGeneration.current) {
        setDictionaryEntries(entries);
        await recordLookup(entries, target.surface, generation, {
          cfi,
          sentence: sentenceAt(text, offset),
        });
      }
    } catch {
      if (generation !== lookupGeneration.current) return;
      const surface = japaneseWordAt(text, offset);
      setDictionaryQuery(surface);
      setDictionaryOpen(true);
      try {
        const entries = await invoke<DictionaryEntry[]>("lookup_dictionary", { query: surface });
        if (generation === lookupGeneration.current) {
          setDictionaryEntries(entries);
          await recordLookup(entries, surface, generation, {
            cfi,
            sentence: sentenceAt(text, offset),
          });
        }
      } catch (reason) {
        if (generation === lookupGeneration.current) setDictionaryError(String(reason));
      }
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
    latestLocation.current = null;
    const open = async () => {
      try {
        const selected = await invoke<ReaderBook | null>("get_reader_book", { bookId });
        if (!selected) throw new Error("This book is no longer in the catalog.");
        if (disposed || !host.current) return;
        setProgress(null);
        setBusy(true);
        setReaderBook(selected);
        void (async () => { try { const count = await invoke<number>("ensure_bundled_dictionary", { rebuild: false }); setDictionaryStatus(`Ready · ${count.toLocaleString()} local entries`); } catch (reason) { setDictionaryStatus(`Unavailable: ${String(reason)}`); } finally { await refreshDictionaries(); } })();
        const savedLocation = await invoke<string | null>("get_reading_location", { bookId });
        if (disposed || !host.current) return;
        const instance = ePub(convertFileSrc(selected.filePath));
        book.current = instance;
        // Wait until epub.js has parsed the package before creating the rendition.
        // In particular, manga commonly declares a fixed (pre-paginated) layout,
        // SVG pages, spreads, and its own page progression direction. Forcing the
        // reflowable/RTL defaults here breaks those publications.
        await instance.opened;
        if (disposed || !host.current) return;
        fixedLayout.current = instance.packaging.metadata.layout === "pre-paginated";
        const view = instance.renderTo(host.current, readerRenditionOptions());
        rendition.current = view;
        const sectionIndices = instance.spine.spineItems
          .filter((section) => section.linear !== "no")
          .map((section) => section.index);
        view.on("relocated", (location) => {
          if (disposed) return;
          setProgress(readerProgress(location as ProgressLocation, sectionIndices));
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
        view.on("rendered", () => { applyAppearance(view, theme, fontSize, fixedLayout.current); attachDictionaryHandlers(view, dictionaryEnabled, dictionaryModifier, lookupTarget); });
        view.on("keydown", (event) =>
          handleReaderKey(event as KeyboardEvent, advanceRef.current, retreatRef.current, onClose),
        );
        applyAppearance(view, theme, fontSize, fixedLayout.current);
        await view.display(initialCfi ?? savedLocation ?? undefined);
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
    // Opening a book must only follow the book id; appearance changes are applied below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [bookId]);

  useEffect(() => {
    localStorage.setItem("tmw-reader-theme", theme);
    if (rendition.current) applyAppearance(rendition.current, theme, fontSize, fixedLayout.current);
  }, [theme, fontSize]);
  useEffect(() => {
    localStorage.setItem("tmw-reader-font-size", String(fontSize));
  }, [fontSize]);
  useEffect(() => { localStorage.setItem("tmw-dictionary-enabled", String(dictionaryEnabled)); void invoke("set_app_setting", { key: "reader_dictionary_enabled", value: String(dictionaryEnabled) }); }, [dictionaryEnabled]);
  useEffect(() => { localStorage.setItem("tmw-dictionary-modifier", dictionaryModifier); void invoke("set_app_setting", { key: "reader_dictionary_modifier", value: dictionaryModifier }); }, [dictionaryModifier]);
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      handleReaderKey(event, advanceRef.current, retreatRef.current, onClose);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);

  return (
    <main
      className={`fixed inset-0 z-50 flex flex-col ${theme === "dark" ? "bg-stone-950 text-stone-100" : "bg-stone-100 text-stone-900"}`}
      aria-label="EPUB reader"
    >
      <header
        className={`flex shrink-0 flex-wrap items-center justify-between gap-3 border-b px-4 py-3 ${theme === "dark" ? "border-white/10 bg-stone-900" : "border-stone-300 bg-white"}`}
      >
        <div className="min-w-0">
          <p className="truncate font-medium">{readerBook?.title ?? "Opening book…"}</p>
          <p className="text-xs opacity-60">
            Positions are saved locally. Source EPUB files are read-only.
          </p>
          {progress !== null && !busy && !error && (
            <p
              className="text-xs tabular-nums opacity-60"
              aria-label={`Book progress: approximately ${progress.toFixed(1)} percent`}
              title="Estimated from chapter and page position; chapters are weighted equally."
            >
              ≈ {progress.toFixed(1)}% read
            </p>
          )}
        </div>
        <div className="flex items-center gap-2">
          <button
            className="reader-control"
            onClick={() => {
              setHistoryOpen((value) => !value);
              setPassagesOpen(false);
            }}
          >
            Lookup history
          </button>
          <button className="reader-control" onClick={() => setPassagesOpen((value) => !value)}>
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
          {!busy && !error && <ReadingStatus key={statusRevision} bookId={bookId} onChanged={() => { void invoke<{status: string}>("get_reading_state", { bookId }).then(state => setFinished(state.status === "finished")); }} />}
          <button
            className="reader-control"
            onClick={() => setFontSize((size) => Math.max(FONT_MIN, size - 10))}
            aria-label="Decrease font size"
          >
            A−
          </button>
          <span className="w-10 text-center text-xs">{fontSize}%</span>
          <button
            className="reader-control"
            onClick={() => setFontSize((size) => Math.min(FONT_MAX, size + 10))}
            aria-label="Increase font size"
          >
            A+
          </button>
          <button
            className="reader-control"
            onClick={() => setTheme((value) => (value === "dark" ? "light" : "dark"))}
          >
            {theme === "dark" ? "Light" : "Dark"}
          </button>
          <button
            className="reader-control"
            onClick={() => setDictionaryEnabled((value) => !value)}
          >
            Dict {dictionaryEnabled ? "on" : "off"}
          </button>
          <button className="reader-control" onClick={onClose}>
            Close
          </button>
        </div>
      </header>
      <section className="relative min-h-0 flex-1">
        <div
          ref={host}
          className={`h-full w-full ${theme === "dark" ? "bg-stone-950" : "bg-stone-100"}`}
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
            className={`dictionary-popup ${theme === "dark" ? "bg-stone-900 text-stone-100" : "bg-white text-stone-900"}`}
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
                No enabled JMdict entry found. The term remains editable for a broader lookup.
              </p>
            )}
            {dictionaryEntries.map((entry) => (
              <article key={entry.id} className="mt-3 border-t border-current/15 pt-2">
                <div className="font-medium">
                  {entry.term}{" "}
                  {entry.reading && <span className="text-sm opacity-70">{entry.reading}</span>}
                </div>
                {entry.partOfSpeech.length > 0 && (
                  <div className="text-xs opacity-60">{entry.partOfSpeech.join(" · ")}</div>
                )}
                <ul className="mt-1 list-disc pl-5 text-sm">
                  {entry.definitions.slice(0, 8).map((definition, index) => (
                    <li key={index}>{definition}</li>
                  ))}
                </ul>
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
            className={`absolute inset-y-0 right-0 z-10 w-full max-w-xl overflow-y-auto p-5 ${theme === "dark" ? "bg-stone-900" : "bg-white"}`}
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
            className={`absolute inset-y-0 right-0 z-20 w-full max-w-xl overflow-y-auto p-5 ${theme === "dark" ? "bg-stone-900" : "bg-white"}`}
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
        className={`flex shrink-0 justify-between border-t p-3 ${theme === "dark" ? "border-white/10 bg-stone-900" : "border-stone-300 bg-white"}`}
      >
        <button
          className="reader-control"
          disabled={!!error || busy}
          onClick={() => void advance()}
        >
          ← 次へ
        </button>
        <span className="self-center text-xs opacity-60">← 次へ · → 前へ · Esc closes</span>
        <span
          className="self-center text-xs opacity-60"
          title="The bundled JMdict index is stored locally and can be safely rebuilt."
        >
          {dictionaryStatus}
        </span>
        <button
          className="reader-control"
          onClick={async () => {
            setDictionaryStatus("Rebuilding local index…");
            try {
              const count = await invoke<number>("ensure_bundled_dictionary", { rebuild: true });
              setDictionaryStatus(`Ready · ${count.toLocaleString()} local entries`);
              await refreshDictionaries();
            } catch (reason) {
              setDictionaryStatus(`Unavailable: ${String(reason)}`);
            }
          }}
        >
          Rebuild dictionary
        </button>
        <button
          className="reader-control"
          onClick={() => setDictionaryModifier((value) => (value === "alt" ? "ctrl" : "alt"))}
        >
          Trigger: {dictionaryModifier === "alt" ? "Alt-click" : "Ctrl-click"}
        </button>
        {dictionaries.map((dictionary) => (
          <button
            key={dictionary.id}
            className="reader-control"
            title={`${dictionary.entryCount.toLocaleString()} local entries`}
            onClick={async () => {
              await invoke("set_dictionary_enabled", {
                dictionaryId: dictionary.id,
                enabled: !dictionary.enabled,
              });
              await refreshDictionaries();
            }}
          >
            {dictionary.name}: {dictionary.enabled ? "on" : "off"}
          </button>
        ))}
        <button
          className="reader-control"
          disabled={!!error || busy}
          onClick={() => void retreat()}
        >
          前へ →
        </button>
      </footer>
    </main>
  );
}

function applyAppearance(rendition: Rendition, theme: Theme, fontSize: number, fixedLayout: boolean) {
  rendition.themes.default({
    html: {
      "background-color": theme === "dark" ? "#0c0a09 !important" : "#f5f5f4 !important",
    },
    body: fixedLayout ? {
      "background-color": theme === "dark" ? "#0c0a09 !important" : "#f5f5f4 !important",
      background: theme === "dark" ? "#0c0a09 !important" : "#f5f5f4 !important",
    } : {
      "background-color": theme === "dark" ? "#0c0a09 !important" : "#f5f5f4 !important",
      background: theme === "dark" ? "#0c0a09 !important" : "#f5f5f4 !important",
      color: theme === "dark" ? "#f5f5f4 !important" : "#1c1917 !important",
      "font-family": "Yu Gothic UI, Yu Gothic, Meiryo, serif",
      "line-height": "1.85",
    },
  });
  if (!fixedLayout) rendition.themes.fontSize(`${fontSize}%`);
}

function attachDictionaryHandlers(
  rendition: Rendition,
  enabled: boolean,
  modifier: "alt" | "ctrl",
  lookup: (text: string, offset: number, cfi: string) => Promise<void>,
) {
  if (!enabled) return;
  for (const content of rendition.getContents?.() ?? []) {
    const document = content.document;
    if (document.documentElement.dataset.tmwDictionaryBound === "true") continue;
    document.documentElement.dataset.tmwDictionaryBound = "true";
    document.addEventListener("click", (event) => {
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
) {
  const target = event.target as HTMLElement | null;
  if (target?.closest?.("input,textarea,select,[contenteditable='true']")) return;
  if (event.key === "Escape") {
    event.preventDefault();
    onClose();
    return;
  }
  // Japanese books advance toward the left. epub.js's `next` crosses spine items.
  if (event.key === "ArrowLeft") {
    event.preventDefault();
    void advance();
  }
  if (event.key === "ArrowRight") {
    event.preventDefault();
    void retreat();
  }
}
