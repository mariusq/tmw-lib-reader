import { READER_FURIGANA_KEY, applyReaderFurigana } from "../../../packages/reader-core/readerFurigana";
import { READER_WRITING_KEY, readerWriting } from "../../../packages/reader-core/readerWriting";
import { READER_THEME_KEY, readerThemes, readerTheme, applyReaderColors } from "../../../packages/reader-core/readerTheme";
import { caretAt } from "../../../packages/reader-core/caret";
import { edgePageTurn, isLookupTap } from "./tap";
import { useEffect, useRef, useState } from "react";
import { type Rendition } from "epubjs";
import { createReaderBook } from "./workerArchive";
import { invoke } from "@tauri-apps/api/core";
import { dictionaryTextAt } from "../../../packages/reader-core/dictionaryText";
import { sentenceAt } from "../../../packages/reader-core/passageContext";
import { readerRenditionOptions } from "../../../packages/reader-core/readerConfig";
import { turnReaderPage, type PageLocation } from "../../../packages/reader-core/readerNavigation";
import { localBook, type LocalBook } from "./localBook";
import { readUserState, saveUserData } from "./userData";
import SavedPassages from "./SavedPassages";
import LookupHistory from "./LookupHistory";
import { recordLookup, readerLookupResult, type LookupResult as Result } from "./historyData";
import DictionaryGlossary from "../../../packages/reader-core/DictionaryGlossary";
import DictionaryManager from "../../../packages/reader-core/DictionaryManager";
import { FONT_SIZE_KEY, readerFontSize, prepareReaderDocument } from "./readerPresentation";
import type { LookupResponse } from "../../../packages/reader-core/dictionaryLookup";
import ChapterPicker, { type Chapter } from "../../../packages/reader-core/ChapterPicker";
export default function Reader({
  selected,
  onLocalSelection,
  onExit,
}: {
  onExit?: () => void;
  selected?: LocalBook;
  onLocalSelection?: () => void;
}) {
  const [hideFurigana, setHideFurigana] = useState(() => localStorage.getItem(READER_FURIGANA_KEY) === "true");
  const [writing, setWriting] = useState(() => readerWriting(localStorage.getItem(READER_WRITING_KEY)));
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<Rendition | null>(null);
  const generation = useRef(0);
  const checkpoint = useRef<() => void>(() => {});
  const anchor = useRef("");
  const repaginating = useRef(false);
  const [fontSize, setFontSize] = useState(() =>
    readerFontSize(localStorage.getItem(FONT_SIZE_KEY)),
  );
  const fontSizeRef = useRef(fontSize);
  const [theme, setTheme] = useState(() => readerTheme(localStorage.getItem(READER_THEME_KEY), "paper"));
  const themeRef = useRef(theme);
  useEffect(() => {
    themeRef.current = theme;
    localStorage.setItem(READER_THEME_KEY, theme);
    const rendition = view.current;
    const fixed = rendition?.book?.packaging?.metadata?.layout === "pre-paginated";
    (rendition?.getContents?.() as unknown as Array<{ document: Document }> | undefined)?.forEach((content) => applyReaderColors(content.document, theme, fixed));
  }, [theme]);
  const dialog = useRef<HTMLDialogElement>(null);
  const backdropPress = useRef(false);
  const menu = useRef<HTMLDialogElement>(null);
  const swipe = useRef<{ x: number; y: number; id: number } | undefined>(undefined);
  function openMenu() {
    if (menu.current?.open) return;
    checkpoint.current();
    history.pushState({ ...history.state, tmwReaderMenu: true }, "");
    menu.current?.showModal();
  }
  const [stored, setBook] = useState<LocalBook>();
  const book = selected ?? stored;
  const [error, setError] = useState("");
  const [dictionaryOpen, setDictionaryOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [manualResult, setManualResult] = useState<Result>();
  const [manualError, setManualError] = useState("");
  const [manualBusy, setManualBusy] = useState(false);
  const manualGeneration = useRef(0);
  async function searchDictionary() {
    const text = query.trim();
    if (!text) return;
    const current = ++manualGeneration.current;
    setManualBusy(true); setManualError(""); setManualResult(undefined);
    try {
      const response = await invoke<LookupResponse & Result>("lookup_text", { text, offset: 0 });
      if (manualGeneration.current === current) setManualResult(readerLookupResult(response));
    } catch (reason) {
      if (manualGeneration.current === current) setManualError(String(reason));
    } finally {
      if (manualGeneration.current === current) setManualBusy(false);
    }
  }
  useEffect(() => () => { manualGeneration.current++; }, []);
  const [chapters, setChapters] = useState<Chapter[]>([]);
  const [chaptersReady, setChaptersReady] = useState(false);
  const [chapterPending, setChapterPending] = useState(false);
  const [result, setResult] = useState<Result>();
  const [sentence, setSentence] = useState("");
  const [lookupCfi, setLookupCfi] = useState("");
  const [passageNote, setPassageNote] = useState("");
  const [savedRevision, setSavedRevision] = useState(0);
  const [saveMessage, setSaveMessage] = useState("");
  const [saving, setSaving] = useState(false);
  const [status, setStatus] = useState(
    "Choose a local EPUB copy (up to 64 MB). It stays on this device.",
  );
  useEffect(() => {
    const back = () => {
      dialog.current?.close();
      menu.current?.close();
    };
    window.addEventListener("popstate", back);
    return () => window.removeEventListener("popstate", back);
  }, []);
  useEffect(() => {
    const save = () => checkpoint.current();
    const lifecycle = (event: Event) => {
      if ((event as CustomEvent).detail === "pause") save();
      else if ((event as CustomEvent).detail === "resume") {
        window.dispatchEvent(new Event("resize"));
      }
    };
    const visibility = () => {
      if (document.visibilityState === "hidden") save();
    };
    document.addEventListener("visibilitychange", visibility);
    window.addEventListener("pagehide", save);
    window.addEventListener("tmw-lifecycle", lifecycle);
    return () => {
      save();
      document.removeEventListener("visibilitychange", visibility);
      window.removeEventListener("pagehide", save);
      window.removeEventListener("tmw-lifecycle", lifecycle);
    };
  }, []);
  useEffect(() => {
    void localBook()
      .then(setBook)
      .catch((e) => setError(String(e)));
  }, []);
  useEffect(() => {
    if (!book || !host.current) return;
    let disposed = false;
    const locationKey = `tmw-local-cfi-${book.id ?? "legacy"}`;
    let initialCfi = localStorage.getItem(locationKey);
    const instance = createReaderBook(book.bytes);
    setChapters([]);
    setChaptersReady(false);
    setChapterPending(false);
    void instance.loaded?.navigation.then((navigation) => {
      if (!disposed) { setChapters(navigation.toc); setChaptersReady(true); }
    }).catch(() => { if (!disposed) setChaptersReady(true); });
    let lastSubmitted = "";
    const persist = (cfi: string) => {
      if (!cfi || cfi === lastSubmitted) return;
      anchor.current = cfi;
      localStorage.setItem(locationKey, cfi);
      lastSubmitted = cfi;
      if (book.catalog)
        void saveUserData(book.catalog, "progress", { locationCfi: cfi }).catch((e) => {
          // Permit retry on pause/resume without coalescing ordered native writes.
          if (lastSubmitted === cfi) lastSubmitted = "";
          if (!disposed) setError(`Progress save failed: ${String(e)}`);
        });
    };
    checkpoint.current = () => {
      if (repaginating.current) return;
      const location = view.current?.currentLocation() as unknown as PageLocation;
      persist(location?.start?.cfi ?? anchor.current);
    };
    instance.spine?.hooks.content.register((doc: Document) => {
      prepareReaderDocument(
        doc,
        fontSizeRef.current,
        instance.packaging?.metadata?.layout === "pre-paginated",
        writing,
      );
      applyReaderFurigana(doc, hideFurigana, instance.packaging?.metadata?.layout === "pre-paginated");
      applyReaderColors(doc, themeRef.current, instance.packaging?.metadata?.layout === "pre-paginated");
    });
    anchor.current = initialCfi ?? "";
    let observer: ResizeObserver | undefined;
    let resizeTimer: ReturnType<typeof setTimeout> | undefined;
    const resize = () => {
      checkpoint.current();
      const cfi = anchor.current;
      clearTimeout(resizeTimer);
      resizeTimer = setTimeout(() => {
        const rendition = view.current;
        const box = host.current?.getBoundingClientRect();
        if (disposed || !rendition || !box?.width || !box.height) return;
        repaginating.current = true;
        rendition.resize(Math.round(box.width), Math.round(box.height));
        void rendition
          .display(cfi || undefined)
          .catch((e) => {
            if (!disposed) setError(`Reader resize failed: ${String(e)}`);
          })
          .finally(() => {
            repaginating.current = false;
          });
      }, 160);
    };
    window.addEventListener("resize", resize);
    if (typeof ResizeObserver !== "undefined") {
      observer = new ResizeObserver(resize);
      observer.observe(host.current);
    }
    async function start() {
      if (book?.catalog) {
        try {
          const state = await readUserState(book.catalog);
          if (state.progress?.contentVersion === book.catalog.version && !state.progress.deleted)
            initialCfi = state.progress.fields.locationCfi ?? initialCfi;
        } catch (e) {
          if (!disposed) setError(`Local progress could not be loaded: ${String(e)}`);
        }
      }
      await instance.opened;
      if (disposed || !host.current) return;
      const rendition = instance.renderTo(host.current, { ...readerRenditionOptions(), ...(instance.packaging?.metadata?.layout !== "pre-paginated" ? { flow: "paginated", defaultDirection: "ltr", spread: "none" } : {}) });
      view.current = rendition;
      await rendition.started;
      if (disposed) return;
      if (instance.packaging?.metadata?.layout !== "pre-paginated") rendition.direction("ltr");
      if (instance.packaging?.metadata?.layout !== "pre-paginated")
        rendition.themes?.fontSize(`${fontSizeRef.current}px`);
      rendition.hooks.content.register(
        (contents: { document: Document; cfiFromRange: (range: Range) => string }) => {
          const doc = contents.document;
          applyReaderFurigana(doc, hideFurigana, instance.packaging?.metadata?.layout === "pre-paginated");
          let down: { x: number; y: number; time: number; id: number } | undefined;
          let moved = false;
          doc.addEventListener("pointerdown", (e) => {
            if (!e.isPrimary) {
              down = undefined;
              return;
            }
            down = { x: e.clientX, y: e.clientY, time: performance.now(), id: e.pointerId };
            moved = false;
          });
          doc.addEventListener("pointermove", (e) => {
            if (down && Math.hypot(e.clientX - down.x, e.clientY - down.y) > 10) moved = true;
          });
          doc.addEventListener(
            "scroll",
            () => {
              moved = true;
            },
            true,
          );
          doc.addEventListener("pointercancel", () => {
            down = undefined;
          });
          doc.addEventListener("pointerup", (e) => {
            const begin = down;
            down = undefined;
            if (
              !isLookupTap(
                begin,
                { x: e.clientX, y: e.clientY, time: performance.now(), id: e.pointerId },
                moved,
              )
            )
              return;
            if (disposed || (e.target as Node | null)?.nodeType !== 1) return;
            if ((e.target as Element).closest("a,button,input,textarea,select,rt,rp")) return;
            const caret = caretAt(doc, e.clientX, e.clientY);
            if (!caret) {
              // Chapter iframes can span many columns and be translated by epub.js.
              // Convert their local pointer coordinates to the visible reader pane.
              const pane = host.current?.getBoundingClientRect();
              const frame = doc.defaultView?.frameElement?.getBoundingClientRect();
              if (pane && begin) {
                const vertical = writing === "vertical-rl" && instance.packaging?.metadata?.layout !== "pre-paginated";
                const offset = (frame?.left ?? pane.left) - pane.left;
                const direction = edgePageTurn(begin.x + offset, e.clientX + offset, pane.width);
                if (direction) void turn(vertical ? (direction === "next" ? "previous" : "next") : direction);
              }
              return;
            }
            const node = caret.node;
            const root =
              node.parentElement?.closest("p,li,h1,h2,h3,blockquote,td") ?? node.parentElement;
            if (!root) return;
            const target = dictionaryTextAt(root, node, caret.offset);
            if (!target) return;
            // Bound tokenization around the tapped code point without changing its offset.
            const chars = [...target.text];
            const start = Math.max(0, target.offset - 4000);
            const text = chars.slice(start, start + 8000).join("");
            const offset = target.offset - start;
            const current = ++generation.current;
            const context = sentenceAt(text, offset);
            setSentence(context);
            let cfi = "";
            try {
              const range = doc.createRange();
              range.setStart(node, caret.offset);
              range.collapse(true);
              cfi = contents.cfiFromRange(range);
            } catch {
              /* A text excerpt can be retained without a jump anchor. */
            }
            setLookupCfi(cfi);
            setPassageNote("");
            setSaveMessage("");
            setResult(undefined);
            setError("");
            if (!dialog.current?.open) {
              history.pushState({ tmwLookup: true }, "");
              dialog.current?.showModal();
            }
            void invoke<LookupResponse & Result>("lookup_text", { text, offset })
              .then((response) => {
                const r = readerLookupResult(response);
                if (generation.current === current && !disposed) {
                  setResult(r);
                  // Display definitions first; persistence runs on the native local executor.
                  if (r.entries.length > 0) void recordLookup(book!, r, context, cfi).catch((e) => {
                    if (!disposed) setSaveMessage(`History save failed: ${String(e)}`);
                  });
                }
              })
              .catch((e) => {
                if (generation.current === current && !disposed) setError(String(e));
              });
          });
          applyReaderColors(doc, themeRef.current, instance.packaging?.metadata?.layout === "pre-paginated");
        },
      );
      rendition.on("relocated", (location: { start: { cfi: string } }) => {
        if (!disposed && !repaginating.current) persist(location.start.cfi);
      });
      try {
        await rendition.display(initialCfi ?? undefined);
      } catch {
        await rendition.display();
      }
      if (!disposed) {
        setStatus(book!.name);
        if (book?.catalog) void invoke("mobile_storage", { args: { action: "markOpened", ns: book.catalog.ns, id: book.catalog.id } }).catch(() => {});
      }
    }
    void start().catch((e) => {
      if (!disposed) setError(`Could not open EPUB: ${String(e)}`);
    });
    return () => {
      checkpoint.current();
      checkpoint.current = () => {};
      disposed = true;
      repaginating.current = false;
      generation.current++;
      observer?.disconnect();
      window.removeEventListener("resize", resize);
      clearTimeout(resizeTimer);
      view.current?.destroy();
      view.current = null;
      instance.destroy();
    };
  }, [book, writing, hideFurigana]);
  function changeFont(size: number) {
    checkpoint.current();
    const cfi = anchor.current;
    localStorage.setItem(FONT_SIZE_KEY, String(size));
    setFontSize(size);
    fontSizeRef.current = size;
    const rendition = view.current;
    if (!rendition || rendition.book?.packaging?.metadata?.layout === "pre-paginated") return;
    generation.current++;
    dialog.current?.close();
    repaginating.current = true;
    rendition.themes?.fontSize(`${size}px`);
    void rendition
      .display(cfi || undefined)
      .catch((e) => setError(String(e)))
      .finally(() => {
        repaginating.current = false;
      });
  }
  async function select(file?: File) {
    if (!file) return;
    try {
      if (!file.name.toLowerCase().endsWith(".epub") || file.size > 64_000_000)
        throw new Error("Select an EPUB smaller than 64 MB.");
      setStatus("Saving local copy…");
      const selected = { name: file.name, bytes: await file.arrayBuffer() };
      const digest = await crypto.subtle.digest("SHA-256", selected.bytes);
      const identified = {
        ...selected,
        id: Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join(
          "",
        ),
      };
      await localBook(identified);
      setBook(identified);
      onLocalSelection?.();
      setError("");
    } catch (e) {
      setError(String(e));
    }
  }
  async function turn(direction: "next" | "previous") {
    const rendition = view.current;
    if (!rendition) return;
    generation.current++;
    dialog.current?.close();
    try {
      await turnReaderPage(rendition, direction, {
        // epub.js's public declaration describes one endpoint, while the runtime returns start/end.
        location: () => rendition.currentLocation() as unknown as PageLocation,
        nextSection: async (index) => {
          const next = rendition.book.spine.get(index)?.next();
          if (next?.href && view.current === rendition) await rendition.display(next.href);
        },
      });
    } catch (e) {
      setError(String(e));
    }
  }
  async function savePassage(bookmark = false) {
    if (!book?.catalog || saving) return;
    setSaving(true);
    setSaveMessage("");
    try {
      const location = view.current?.currentLocation() as unknown as PageLocation;
      await saveUserData(book.catalog, "passage", {
        surface: bookmark ? "Bookmark" : (result?.target.surface ?? "Excerpt"),
        headword: bookmark ? null : (result?.target.lemma ?? null),
        reading: bookmark ? null : (result?.target.reading ?? null),
        sentence: bookmark ? "" : sentence,
        note: bookmark ? "" : passageNote,
        locationCfi: bookmark ? (location?.start?.cfi ?? "") : lookupCfi,
      });
      setSavedRevision((v) => v + 1);
      setSaveMessage("Saved on this device; sync queued.");
    } catch (e) {
      setSaveMessage(`Save failed: ${String(e)}`);
    } finally {
      setSaving(false);
    }
  }
  return (
    <section className="reader-screen" style={{ background: readerThemes[theme].background, color: readerThemes[theme].foreground }}>
      <div ref={host} className="reader-host" aria-label="Book pages" />
      <nav style={{ background: readerThemes[theme].background, color: readerThemes[theme].foreground }} className="reader-page-bar" aria-label="Page navigation">
        <button aria-label={writing === "vertical-rl" ? "Next page" : "Previous page"} disabled={!book} onClick={() => void turn(writing === "vertical-rl" ? "next" : "previous")}>
          {writing === "vertical-rl" ? "← Next" : "← Previous"}
        </button>
        <button
          className="reader-menu-handle"
          aria-label="Open reader menu"
          onClick={openMenu}
          onPointerDown={(e) => {
            swipe.current = { x: e.clientX, y: e.clientY, id: e.pointerId };
            e.currentTarget.setPointerCapture(e.pointerId);
          }}
          onPointerUp={(e) => {
            const begin = swipe.current;
            swipe.current = undefined;
            if (
              begin?.id === e.pointerId &&
              begin.y - e.clientY > 40 &&
              Math.abs(begin.x - e.clientX) < 60
            )
              openMenu();
          }}
          onPointerCancel={() => {
            swipe.current = undefined;
          }}
        >
          <span aria-hidden="true">☰</span>
          {book ? "Menu" : "Open a book"}
        </button>
        <button aria-label={writing === "vertical-rl" ? "Previous page" : "Next page"} disabled={!book} onClick={() => void turn(writing === "vertical-rl" ? "previous" : "next")}>
          {writing === "vertical-rl" ? "Previous →" : "Next →"}
        </button>
      </nav>
      {error && (
        <p className="reader-error" role="alert">
          {error}
        </p>
      )}
      <dialog
        ref={menu}
        className="reader-menu lookup"
        aria-labelledby="reader-menu-title"
        onCancel={(e) => {
          e.preventDefault();
          history.back();
        }}
        onClose={() => {
          if (history.state?.tmwReaderMenu) history.back();
        }}
      >
        <div className="reader-menu-heading">
          <h2 id="reader-menu-title">Reading controls</h2>
          <button className="control" onClick={() => menu.current?.close()}>
            Close menu
          </button>
        </div>
        <button
          className="control primary"
          onClick={() => {
            checkpoint.current();
            onExit?.();
          }}
        >
          Exit reader
        </button>
        <div className="reader-toolbar">
          <button className="control" aria-expanded={dictionaryOpen} onClick={() => setDictionaryOpen(!dictionaryOpen)}>Dictionary</button>
          {dictionaryOpen && <section aria-label="Reader dictionary">
            <p>Tap a word in the book for definitions. Search here to check your offline dictionaries.</p>
            <form onSubmit={(event) => { event.preventDefault(); void searchDictionary(); }}>
              <label>Japanese word<input aria-label="Japanese word" value={query} onChange={(event) => { manualGeneration.current++; setManualBusy(false); setManualResult(undefined); setQuery(event.target.value); }} /></label>
              <button className="control" disabled={manualBusy || !query.trim()}>{manualBusy ? "Looking up…" : "Look up"}</button>
            </form>
            {manualError && <p role="alert">{manualError}</p>}
            {manualResult?.entries.length === 0 && <p>No local definition found.</p>}
            {manualResult?.groups ? manualResult.groups.map((group, i) => <article key={i}><h3>{group.term} {group.reading}</h3>{group.matches.map(({ entry }, index) => <section key={index}><h4>{entry.provenance.title}</h4><DictionaryGlossary entry={entry} /></section>)}</article>) : manualResult?.entries.map((entry, i) => <article key={i}><h3>{entry.term} {entry.reading}</h3><p>{entry.definitions.join("; ")}</p></article>)}
            <DictionaryManager />
          </section>}
          <ChapterPicker chapters={chapters} ready={chaptersReady} pending={chapterPending}
            onJump={(href) => {
              const rendition = view.current;
              if (!rendition || chapterPending) return;
              checkpoint.current();
              generation.current++;
              dialog.current?.close();
              setChapterPending(true);
              void rendition.display(href).then(() => {
                if (view.current === rendition) { checkpoint.current(); menu.current?.close(); }
              }).catch((e) => {
                if (view.current === rendition) setError(`Could not open chapter: ${String(e)}`);
              }).finally(() => { if (view.current === rendition) setChapterPending(false); });
            }} />
          <label><input type="checkbox" checked={hideFurigana} onChange={(event) => { checkpoint.current(); localStorage.setItem(READER_FURIGANA_KEY, String(event.target.checked)); setHideFurigana(event.target.checked); }} /> Hide furigana</label>
          <label>Writing direction <select aria-label="Writing direction" value={writing} onChange={(event) => { checkpoint.current(); localStorage.setItem(READER_WRITING_KEY, event.target.value); setWriting(readerWriting(event.target.value)); }}><option value="horizontal-tb">Horizontal left to right</option><option value="vertical-rl">Vertical right to left</option></select></label>
          <label>Reading theme
            <select aria-label="Reading theme" value={theme} onChange={(event) => setTheme(readerTheme(event.target.value, "paper"))}>
              {Object.entries(readerThemes).map(([value, palette]) => <option key={value} value={value}>{palette.label}</option>)}
            </select>
          </label>
          <label className="reader-import block">
            Open EPUB
            <input
              aria-label="Open local EPUB"
              type="file"
              accept=".epub,application/epub+zip"
              onChange={(e) => {
                void select(e.target.files?.[0]);
                e.target.value = "";
              }}
              className="block w-full py-3"
            />
          </label>
          <div className="reader-font-controls" role="group" aria-label="Text size">
            <button
              className="control"
              aria-label="Smaller text"
              disabled={fontSize <= 16}
              onClick={() => changeFont(Math.max(16, fontSize - 2))}
            >
              A−
            </button>
            <output aria-live="polite">{fontSize} px</output>
            <button
              className="control"
              aria-label="Larger text"
              disabled={fontSize >= 32}
              onClick={() => changeFont(Math.min(32, fontSize + 2))}
            >
              A+
            </button>
          </div>
        </div>
        <p aria-live="polite" className="text-sm">
          {status}
        </p>
        {error && (
          <p role="alert" className="text-red-300">
            {error}
          </p>
        )}
        {book?.catalog && (
          <>
            <button className="control" disabled={saving} onClick={() => void savePassage(true)}>
              Bookmark this page
            </button>
            <p aria-live="polite">{saveMessage}</p>
            <SavedPassages
              key={`${book.catalog.ns}:${book.catalog.id}:${book.catalog.version}`}
              book={book.catalog}
              revision={savedRevision}
              onJump={(cfi) => {
                void view.current?.display(cfi).catch((e) => setError(String(e)));
              }}
            />
          </>
        )}
        {book && !book.catalog && (
          <p className="text-sm">
            This manually opened copy keeps its position locally. Open a catalog download to sync
            progress and notes.
          </p>
        )}
        <p aria-live="polite">{!book?.catalog && saveMessage}</p>
        <details>
          <summary className="control">Lookup history</summary>
          <LookupHistory
            book={book}
            onJump={(cfi) => {
              void view.current?.display(cfi).catch((e) => setError(String(e)));
            }}
          />
        </details>
      </dialog>
      <dialog
        onPointerDown={event => {
          const bounds = event.currentTarget.getBoundingClientRect();
          backdropPress.current = event.target === event.currentTarget &&
            (event.clientX < bounds.left || event.clientX > bounds.right || event.clientY < bounds.top || event.clientY > bounds.bottom);
        }}
        onPointerCancel={() => { backdropPress.current = false; }}
        onClick={event => {
          // Android retargets the opening word tap's click to the new modal backdrop.
          // Only a gesture that began on this backdrop may dismiss it.
          const beganOutside = backdropPress.current;
          backdropPress.current = false;
          if (!beganOutside || event.target !== event.currentTarget) return;
          const bounds = event.currentTarget.getBoundingClientRect();
          if (event.clientX < bounds.left || event.clientX > bounds.right || event.clientY < bounds.top || event.clientY > bounds.bottom) dialog.current?.close();
        }}
        ref={dialog}
        aria-labelledby="lookup-title"
        onCancel={(event) => {
          event.preventDefault();
          history.back();
        }}
        onClose={() => {
          backdropPress.current = false;
          generation.current++;
          if (history.state?.tmwLookup) history.back();
        }}
        className="lookup"
      >
        <button className="control float-right" onClick={() => dialog.current?.close()}>
          Close
        </button>
        <h2 id="lookup-title" className="text-xl">
          {result?.target.surface ?? "Looking up…"}
        </h2>
        <p lang="ja" className="my-3 text-sm">
          {sentence}
        </p>
        {error && <p role="alert">{error}</p>}
        {result && (
          <>
            {book?.catalog && (
              <div className="my-3">
                <label>
                  Note
                  <textarea
                    className="block w-full bg-slate-800 p-2"
                    maxLength={2000}
                    value={passageNote}
                    onChange={(e) => setPassageNote(e.target.value)}
                  />
                </label>
                <button className="control" disabled={saving} onClick={() => void savePassage()}>
                  Save excerpt
                </button>
                <p aria-live="polite">{saveMessage}</p>
              </div>
            )}
            <p className="text-sm" lang="ja">
              {result.target.lemma} · {result.target.reading}
            </p>
            {result.entries.length === 0 && <p>No local definition found.</p>}
            {result.groups ? result.groups.map((group, i) => <article key={i} className="border-t border-slate-600 py-3">
              <h3>{group.term} {group.reading}</h3>
              {group.matches.map(({ entry }, index) => <section key={index}>
                <h4>{entry.provenance.title}</h4><DictionaryGlossary entry={entry} />
              </section>)}
            </article>) : result.entries.map((e, i) => (
              <article key={i} className="border-t border-slate-600 py-3">
                <h3>
                  {e.term} {e.reading}
                </h3>
                <p>{e.definitions.join("; ")}</p>
              </article>
            ))}
          </>
        )}
      </dialog>
    </section>
  );
}
