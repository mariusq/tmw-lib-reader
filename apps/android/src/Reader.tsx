import { caretAt } from "../../../packages/reader-core/caret";
import { isLookupTap } from "./tap";
import { useEffect, useRef, useState } from "react";
import ePub, { type Rendition } from "epubjs";
import { invoke } from "@tauri-apps/api/core";
import { dictionaryTextAt } from "../../../packages/reader-core/dictionaryText";
import { sentenceAt } from "../../../packages/reader-core/passageContext";
import { readerRenditionOptions } from "../../../packages/reader-core/readerConfig";
import { turnReaderPage, type PageLocation } from "../../../packages/reader-core/readerNavigation";
import { localBook, type LocalBook } from "./localBook";
type Result = {
  target: { surface: string; lemma: string; reading: string | null };
  entries: { term: string; reading: string | null; definitions: string[] }[];
  elapsedMs: number;
  dictionaryBytes: number;
};
export default function Reader({
  selected,
  onLocalSelection,
}: {
  selected?: LocalBook;
  onLocalSelection?: () => void;
}) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<Rendition | null>(null);
  const generation = useRef(0);
  const dialog = useRef<HTMLDialogElement>(null);
  const [stored, setBook] = useState<LocalBook>();
  const book = selected ?? stored;
  const [error, setError] = useState("");
  const [result, setResult] = useState<Result>();
  const [sentence, setSentence] = useState("");
  const [status, setStatus] = useState(
    "Choose a local EPUB copy (up to 64 MB). It stays on this device.",
  );
  useEffect(() => {
    const back = () => dialog.current?.close();
    window.addEventListener("popstate", back);
    return () => window.removeEventListener("popstate", back);
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
    const initialCfi = localStorage.getItem(locationKey);
    const instance = ePub(book.bytes);
    async function start() {
      await instance.opened;
      if (disposed || !host.current) return;
      const rendition = instance.renderTo(host.current, readerRenditionOptions());
      view.current = rendition;
      rendition.hooks.content.register((contents: { document: Document }) => {
        const doc = contents.document;
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
          if ((e.target as Element).closest("a,button,input,rt,rp")) return;
          const caret = caretAt(doc, e.clientX, e.clientY);
          if (!caret) return;
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
          setSentence(sentenceAt(text, offset));
          setResult(undefined);
          setError("");
          if (!dialog.current?.open) {
            history.pushState({ tmwLookup: true }, "");
            dialog.current?.showModal();
          }
          void invoke<Result>("lookup_text", { text, offset })
            .then((r) => {
              if (generation.current === current && !disposed) setResult(r);
            })
            .catch((e) => {
              if (generation.current === current && !disposed) setError(String(e));
            });
        });
        doc.documentElement.style.colorScheme = "light";
      });
      rendition.on("relocated", (location: { start: { cfi: string } }) => {
        if (!disposed) localStorage.setItem(locationKey, location.start.cfi);
      });
      try {
        await rendition.display(initialCfi ?? undefined);
      } catch {
        await rendition.display();
      }
      if (!disposed) setStatus(book!.name);
    }
    void start().catch((e) => {
      if (!disposed) setError(`Could not open EPUB: ${String(e)}`);
    });
    return () => {
      disposed = true;
      view.current?.destroy();
      view.current = null;
      instance.destroy();
    };
  }, [book]);
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
  return (
    <section className="space-y-3">
      <label className="block">
        Open local EPUB
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
      <p aria-live="polite" className="text-sm">
        {status}
      </p>
      {error && (
        <p role="alert" className="text-red-300">
          {error}
        </p>
      )}
      <div ref={host} className="reader-host" />
      <nav className="flex justify-between">
        <button className="control" onClick={() => void turn("previous")}>
          Previous page
        </button>
        <button className="control" onClick={() => void turn("next")}>
          Next page
        </button>
      </nav>
      <dialog
        ref={dialog}
        aria-labelledby="lookup-title"
        onCancel={(event) => {
          event.preventDefault();
          history.back();
        }}
        onClose={() => {
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
            <p className="text-xs">
              {result.target.lemma} · {result.target.reading} · {result.elapsedMs.toFixed(1)} ms ·{" "}
              {(result.dictionaryBytes / 1e6).toFixed(1)} MB dictionary
            </p>
            {result.entries.length === 0 && <p>No local definition found.</p>}
            {result.entries.map((e, i) => (
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
