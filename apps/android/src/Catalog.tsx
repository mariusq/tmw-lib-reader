import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import type { LocalBook } from "./localBook";
import SavedPassages from "./SavedPassages";

type Book = {
  ns: string;
  id: string;
  title: string;
  creator: string;
  series: string;
  volume: string;
  bytes: number;
  available: boolean;
  deleted: boolean;
  coverVersion?: string;
  tags: { name: string }[];
  collections: { name: string }[];
  download?: { file: string; version: string; bytes: number };
};
type Page = { items: Book[]; next: number | null; ns: string; namespaces: string[] };
type Job = { busy: boolean; message: string; done?: number; total?: number };
let nextCoverToken = 0;
const mobile = <T,>(args: Record<string, unknown>) => invoke<T>("mobile_storage", { args });

function Cover({ book, generation, showCovers }: { book: Book; generation: number; showCovers: boolean }) {
  const host = useRef<HTMLDivElement>(null);
  const [data, setData] = useState("");
  useEffect(() => {
    if (!showCovers) return;
    let disposed = false;
    let token = "";
    const observer = new IntersectionObserver((entries) => {
      if (token) void mobile({ action: "coverCancel", token }).catch(() => {});
      if (!entries.some((e) => e.isIntersecting)) {
        token = "";
        setData("");
        return;
      }
      token = String(++nextCoverToken);
      const current = token;
      void mobile<{ data?: string }>({
        action: "cover",
        ns: book.ns,
        id: book.id,
        generation,
        token,
      })
        .then((r) => {
          if (!disposed && token === current) setData(r.data ?? "");
        })
        .catch(() => {
          if (!disposed && token === current) setData("");
        });
    });
    if (host.current) observer.observe(host.current);
    return () => {
      disposed = true;
      observer.disconnect();
      if (token) void mobile({ action: "coverCancel", token }).catch(() => {});
    };
  }, [book.ns, book.id, book.coverVersion, book.download?.version, generation, showCovers]);
  return (
    <div ref={host} className="book-cover">
      {showCovers && data ? (
        <img src={data} alt="" decoding="async" className="h-full w-full object-contain" />
      ) : (
        <span className="text-xs text-slate-500">文</span>
      )}
    </div>
  );
}

export default function Catalog({ onOpen }: { onOpen: (book: LocalBook) => void }) {
  const [showCovers, setShowCovers] = useState(() => localStorage.getItem("tmw-show-covers") !== "false");
  const [page, setPage] = useState<Page>({ items: [], next: null, ns: "", namespaces: [] });
  const [recentBooks, setRecentBooks] = useState<Book[]>([]);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState("title");
  const [filter, setFilter] = useState("");
  const [tag, setTag] = useState("");
  const [collection, setCollection] = useState("");
  const [ns, setNs] = useState("");
  const [offset, setOffset] = useState(0);
  const [refresh, setRefresh] = useState(0);
  const [job, setJob] = useState<Job>({ busy: false, message: "Local catalog" });
  const [error, setError] = useState("");
  const [cache, setCache] = useState({ budget: 100_000_000, usage: 0 });
  const [limit, setLimit] = useState("100");
  const generation = useRef(0);
  const [coverGeneration, setCoverGeneration] = useState(0);
  const reading = useRef(false);
  const pendingJob = useRef(false);
  const [transferBook, setTransferBook] = useState("");
  const [notesBook, setNotesBook] = useState<Book>();
  useEffect(() => {
    let disposed = false;
    void mobile<Page>({ action: "browse", recent: true, ...(ns ? { ns } : {}) })
      .then(result => { if (!disposed) setRecentBooks(result.items.filter(book => book.download).slice(0, 5)); })
      .catch(reason => { if (!disposed) setError(String(reason)); });
    return () => { disposed = true; };
  }, [ns, refresh]);
  useEffect(() => {
    const current = ++generation.current;
    let disposed = false;
    const timer = setTimeout(() => {
      void mobile<Page>({
        action: "browse",
        query,
        sort,
        filter,
        offset,
        tags: tag,
        collections: collection,
        ...(ns ? { ns } : {}),
      })
        .then(async (p) => {
          if (disposed || generation.current !== current) return;
          await mobile({ action: "coverGeneration", generation: current });
          if (disposed || generation.current !== current) return;
          setCoverGeneration(current);
          setPage(p);
          setError("");
        })
        .catch((e) => {
          if (!disposed && generation.current === current) setError(String(e));
        });
    }, 250);
    return () => {
      disposed = true;
      clearTimeout(timer);
    };
  }, [query, sort, filter, offset, tag, collection, ns, refresh]);
  useEffect(() => {
    let disposed = false;
    const timer = setInterval(() => {
      void mobile<Job>({ action: "status" })
        .then((j) => {
          if (disposed) return;
          setJob(j);
          if (pendingJob.current && !j.busy) setRefresh((v) => v + 1);
          pendingJob.current = j.busy;
        })
        .catch(() => {});
    }, 500);
    return () => {
      disposed = true;
      clearInterval(timer);
    };
  }, []);
  useEffect(() => {
    void mobile<typeof cache>({ action: "cacheSettings" })
      .then((c) => {
        setCache(c);
        setLimit(String(c.budget / 1e6));
      })
      .catch(() => {});
  }, [refresh]);
  async function run(args: Record<string, unknown>) {
    const background = args.action === "sync" || args.action === "download";
    try {
      setError("");
      if (background) {
        setTransferBook(args.action === "download" ? `${args.ns}:${args.id}` : "");
        setJob({ busy: true, message: "Starting" });
      }
      await mobile(args);
      if (background) {
        pendingJob.current = true;
      }
      setRefresh((v) => v + 1);
    } catch (e) {
      setError(String(e));
      if (background) setJob({ busy: false, message: String(e) });
    }
  }
  async function read(book: Book) {
    if (!book.download || reading.current) return;
    reading.current = true;
    try {
      const bytes = await invoke<ArrayBuffer>("read_mobile_book", { file: book.download.file });
      onOpen({
        name: book.title,
        file: book.download.file,
        bytes,
        id: book.download.version.slice(7),
        catalog: { ns: book.ns, id: book.id, version: book.download.version },
      });
    } catch (e) {
      setError(String(e));
    } finally {
      reading.current = false;
    }
  }
  return (
    <section className="space-y-3 border-b border-slate-700 pb-4">
      <section aria-labelledby="jump-back-heading" className="rounded-xl border border-slate-700 p-3">
        <h3 id="jump-back-heading" className="font-semibold">Jump back in</h3>
        <p className="mt-1 text-sm text-slate-400">Your last five opened books. Continue where you left off.</p>
        {recentBooks.length ? <div className="mt-3 flex gap-3 overflow-x-auto pb-2">
          {recentBooks.map(book => <button key={`${book.ns}:${book.id}`} className="w-28 shrink-0 text-left" aria-label={`Resume: ${book.title}`} onClick={() => void read(book)}>
            <Cover book={book} generation={coverGeneration} showCovers={showCovers} />
            <span className="mt-2 block truncate text-sm font-medium">{book.title}</span>
            <span className="block truncate text-xs text-slate-400">{book.creator}</span>
          </button>)}
        </div> : <p className="mt-3 text-sm text-slate-400">Open a downloaded book and it will appear here.</p>}
      </section>
      <p className="eyebrow">ON YOUR DEVICE</p>
      <div className="flex gap-2">
        <button
          className="control"
          disabled={job.busy}
          onClick={() => void run({ action: "sync" })}
        >
          Refresh from PC
        </button>
        {job.busy && (
          <button className="control" onClick={() => void run({ action: "cancel" })}>
            Cancel
          </button>
        )}
      </div>
      <p aria-live="polite">
        {job.message}{" "}
        {job.total
          ? `${((job.done ?? 0) / 1e6).toFixed(1)} / ${(job.total / 1e6).toFixed(1)} MB`
          : ""}
      </p>
      {error && (
        <p role="alert" className="text-red-300">
          {error}
        </p>
      )}
      <input
        aria-label="Search local catalog"
        placeholder="Title, author, Japanese reading or romaji"
        className="w-full bg-slate-800 p-3"
        value={query}
        onChange={(e) => {
          setQuery(e.target.value);
          setOffset(0);
        }}
      />
      <label className="flex min-h-11 w-fit cursor-pointer items-center gap-2">
        <input
          type="checkbox"
          className="accent-amber-400"
          checked={showCovers}
          onChange={(e) => {
            setShowCovers(e.target.checked);
            localStorage.setItem("tmw-show-covers", String(e.target.checked));
          }}
        />
        Show covers
      </label>
      <details className="catalog-filters">
        <summary>Filter & sort</summary>
        <div className="flex flex-wrap gap-2">
          <select
            aria-label="Sort"
            className="control bg-slate-800"
            value={sort}
            onChange={(e) => {
              setSort(e.target.value);
              setOffset(0);
            }}
          >
            {["title", "author", "series", "dateAdded", "modified"].map((s) => (
              <option key={s}>{s}</option>
            ))}
          </select>
          <select
            aria-label="Availability"
            className="control bg-slate-800"
            value={filter}
            onChange={(e) => {
              setFilter(e.target.value);
              setOffset(0);
            }}
          >
            {["", "downloaded", "available", "unavailable", "finished"].map((s) => (
              <option key={s} value={s}>
                {s || "All books"}
              </option>
            ))}
          </select>
          {page.namespaces.length > 1 && (
            <select
              aria-label="Catalog"
              className="control bg-slate-800"
              value={ns || page.ns}
              onChange={(e) => {
                setNs(e.target.value);
                setOffset(0);
              }}
            >
              {page.namespaces.map((s) => (
                <option key={s}>{s}</option>
              ))}
            </select>
          )}
          <input
            aria-label="Tag name"
            placeholder="Exact tag name"
            className="w-40 bg-slate-800 p-2"
            value={tag}
            onChange={(e) => {
              setTag(e.target.value);
              setOffset(0);
            }}
          />
          <input
            aria-label="Collection name"
            placeholder="Exact collection name"
            className="w-40 bg-slate-800 p-2"
            value={collection}
            onChange={(e) => {
              setCollection(e.target.value);
              setOffset(0);
            }}
          />
        </div>
      </details>
      {page.items.map((book) => (
        <article key={`${book.ns}:${book.id}`} className="book-card">
          <Cover book={book} generation={coverGeneration} showCovers={showCovers} />
          <div className="min-w-0 flex-1">
            <h3 lang="ja">{book.title}</h3>
            <p className="text-xs">
              {book.creator} · {book.series} {book.volume}
            </p>
            <p className="text-xs">
              {(book.bytes / 1e6).toFixed(1)} MB ·{" "}
              {book.download ? "On device" : book.available ? "PC copy" : "Source unavailable"}
              {book.deleted ? " · Removed from PC catalog" : ""}
            </p>
            <div className="book-actions">
              <button className="control" onClick={() => setNotesBook(book)}>
                Notes
              </button>
              {book.download ? (
                <>
                  <button className="control primary" onClick={() => void read(book)}>
                    Read offline
                  </button>
                  <button
                    className="control"
                    disabled={job.busy}
                    onClick={() => void run({ action: "remove", ns: book.ns, id: book.id })}
                  >
                    Remove copy
                  </button>
                </>
              ) : (
                <button
                  className="control"
                  disabled={job.busy || !book.available || book.deleted || book.bytes > 64_000_000}
                  onClick={() => void run({ action: "download", ns: book.ns, id: book.id })}
                >
                  Download
                </button>
              )}
            </div>
            {!book.download && (book.deleted || !book.available || book.bytes > 64_000_000) && (
              <p className="text-xs text-amber-300">
                {book.deleted || !book.available
                  ? "Download unavailable: the PC source is unavailable."
                  : "Download unavailable: this book exceeds the current 64 MB reader limit."}
              </p>
            )}
            {transferBook === `${book.ns}:${book.id}` && (
              <p role="status" className="text-xs">
                {job.message}
                {job.total
                  ? ` · ${((job.done ?? 0) / 1e6).toFixed(1)} / ${(job.total / 1e6).toFixed(1)} MB`
                  : ""}
              </p>
            )}
          </div>
        </article>
      ))}
      {!page.items.length && <p>No local results. Pair under Settings, then refresh.</p>}
      {notesBook && (
        <section className="rounded border border-slate-700 p-3">
          <p>{notesBook.title}</p>
          <button className="control" onClick={() => setNotesBook(undefined)}>
            Close notes
          </button>
          <SavedPassages
            key={`${notesBook.ns}:${notesBook.id}`}
            book={{ ns: notesBook.ns, id: notesBook.id, version: null }}
            revision={refresh}
            onJump={() => {}}
          />
        </section>
      )}
      <nav className="pagination">
        <button
          className="control"
          disabled={offset === 0}
          onClick={() => setOffset(Math.max(0, offset - 25))}
        >
          Previous
        </button>
        <span>
          {offset + 1}–{offset + page.items.length}
        </span>
        <button
          className="control"
          disabled={page.next === null}
          onClick={() => setOffset(page.next ?? offset)}
        >
          Next
        </button>
      </nav>
      <details>
        <summary className="control">Mobile cover storage</summary>
        <p>
          {(cache.usage / 1e6).toFixed(2)} MB used ·{" "}
          {cache.budget ? `${cache.budget / 1e6} MB LRU limit` : "Persistence disabled"}
        </p>
        <label>
          Limit in decimal MB (0 disables)
          <input
            type="number"
            min="0"
            max="1000"
            value={limit}
            className="m-2 w-24 bg-slate-800"
            onChange={(e) => setLimit(e.target.value)}
          />
        </label>
        <button
          className="control"
          onClick={() =>
            void run({ action: "cacheSettings", budget: Math.floor(Number(limit) * 1e6) })
          }
        >
          Apply
        </button>
        <button
          className="control"
          onClick={() => void run({ action: "cacheSettings", clear: true })}
        >
          Clear thumbnails
        </button>
        <p className="text-xs">
          Clearing removes only regenerable thumbnails. Books, catalog, positions, dictionary and
          pairing remain. Keep a device/app-data backup before uninstalling; export support is
          deferred.
        </p>
      </details>
      <p className="text-xs">
        Reader: up to 64 MB compressed, 128 MB expanded, 16 MB per ZIP entry. Larger publications
        require a future reader strategy.
      </p>
    </section>
  );
}
