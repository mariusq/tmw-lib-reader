import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState, type ReactNode } from "react";
import type { LocalBook } from "./localBook";
import SavedPassages from "./SavedPassages";
import { readSeriesConfidence, type SeriesConfidence } from "./seriesPreferenceStorage";

export type Book = {
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
  seriesCard?: { key: string; name: string; count: number };
  copyGroup?: { key: string; count: number };
};
type Page = { items: Book[]; next: number | null; ns: string; namespaces: string[]; seriesReady?: boolean };
type Job = { busy: boolean; message: string; done?: number; total?: number };
let nextCoverToken = 0;
const mobile = <T,>(args: Record<string, unknown>) => invoke<T>("mobile_storage", { args });

export function Cover({ book, generation, showCovers }: { book: Book; generation: number; showCovers: boolean }) {
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

function CopyPicker({ book, seriesKey, confidence, refresh, renderBook }: { book: Book; seriesKey: string; confidence: SeriesConfidence; refresh: number; renderBook: (book: Book) => ReactNode }) {
  const [open, setOpen] = useState(false);
  const [page, setPage] = useState<Page>();
  const [offset, setOffset] = useState(0);
  const [error, setError] = useState("");
  useEffect(() => {
    if (!open) return;
    let disposed = false;
    void mobile<Page>({ action: "browse", ns: book.ns, selectedSeries: seriesKey, selectedCopy: book.copyGroup?.key, seriesConfidence: confidence, groupCopies: false, offset })
      .then(result => { if (!disposed) { setPage(result); setError(""); } })
      .catch(reason => { if (!disposed) setError(String(reason)); });
    return () => { disposed = true; };
  }, [book.ns, book.copyGroup?.key, confidence, seriesKey, offset, open, refresh]);
  return <details className="series-copy-picker" onToggle={event => setOpen(event.currentTarget.open)}><summary>Show {book.copyGroup?.count} copies</summary><p className="text-xs text-slate-400">Matching title, author and volume. Each copy keeps its own download, progress and bookmarks.</p>{open && <div>{error ? <p role="alert">{error}</p> : page ? page.items.map(renderBook) : <p role="status">Loading copies…</p>}{page && <nav className="pagination"><button className="control" disabled={!offset} onClick={() => setOffset(Math.max(0,offset-25))}>Previous copies</button><button className="control" disabled={page.next===null} onClick={() => setOffset(page.next ?? offset)}>Next copies</button></nav>}</div>}</details>;
}

export default function Catalog({ onOpen, active = true, homeFilter }: { onOpen: (book: LocalBook) => void; active?: boolean; homeFilter?: { filter: string; revision: number } }) {
  const [consolidateSeries, setConsolidateSeries] = useState(() => localStorage.getItem("tmw-consolidate-series") === "true");
  const [seriesConfidence, changeConfidence] = useState<SeriesConfidence>(readSeriesConfidence);
  const [groupCopies, setGroupCopies] = useState(() => localStorage.getItem("tmw-series-group-copies") !== "false");
  const [selectedSeries, setSelectedSeries] = useState<{ key: string; name: string; ns: string }>();
  const seriesRef = useRef(selectedSeries);
  const [loading, setLoading] = useState(false);
  const returnView = useRef<{ page: Page; offset: number; scrollY: number; refresh: number; confidence: SeriesConfidence } | undefined>(undefined);
  const restorePending = useRef(false);
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
    if (!homeFilter) return;
    let disposed = false;
    queueMicrotask(() => {
      if (disposed) return;
      setFilter(homeFilter.filter); setQuery(""); setTag(""); setCollection(""); setOffset(0); setSelectedSeries(undefined); seriesRef.current = undefined;
    });
    return () => { disposed = true; };
  }, [homeFilter]);
  useEffect(() => {
    const update = () => changeConfidence(readSeriesConfidence());
    const back = () => {
      if (!seriesRef.current || history.state?.tmwSeries === seriesRef.current.key || history.state?.tmwTab === "reader" || history.state?.tmwTab === "settings" || history.state?.tmwTab === "history") return;
      seriesRef.current = undefined; restorePending.current = true;
      setSelectedSeries(undefined); setNotesBook(undefined); setOffset(returnView.current?.offset ?? 0);
      if (returnView.current) setPage(returnView.current.page);
    };
    window.addEventListener("tmw-series-preferences", update);
    window.addEventListener("popstate", back);
    return () => { window.removeEventListener("tmw-series-preferences", update); window.removeEventListener("popstate", back); };
  }, []);
  function openSeries(book: Book) {
    if (!book.seriesCard) return;
    returnView.current = { page, offset, scrollY: window.scrollY, refresh, confidence: seriesConfidence };
    const selection = { key: book.seriesCard.key, name: book.seriesCard.name, ns: book.ns };
    setLoading(true); seriesRef.current = selection; setSelectedSeries(selection); setOffset(0); setNotesBook(undefined);
    setPage(current => ({ ...current, items: [], next: null }));
    history.pushState({ ...history.state, tmwTab: "library", tmwSeries: selection.key }, "");
    window.scrollTo({ top: 0, behavior: "instant" });
  }
  useEffect(() => {
    if (!active) return;
    let disposed = false;
    void mobile<Page>({ action: "browse", recent: true, ...(ns ? { ns } : {}) })
      .then(result => { if (!disposed) setRecentBooks(result.items.slice(0, 12)); })
      .catch(reason => { if (!disposed) setError(String(reason)); });
    return () => { disposed = true; };
  }, [ns, refresh, active]);
  useEffect(() => {
    const update = () => setRefresh(value => value + 1);
    window.addEventListener("tmw-reader-opened", update);
    return () => window.removeEventListener("tmw-reader-opened", update);
  }, []);
  useEffect(() => {
    if (!active) return;
    let disposed = false;
    let previous: number | undefined;
    const timer = setInterval(() => {
      void mobile<{ lastSuccess: number }>({ action: "userSyncStatus" }).then(status => {
        if (disposed) return;
        if (previous !== undefined && status.lastSuccess !== previous) setRefresh(value => value + 1);
        previous = status.lastSuccess;
      }).catch(() => {});
    }, 3000);
    return () => { disposed = true; clearInterval(timer); };
  }, [active]);
  useEffect(() => {
    if (!active) return;
    if (!selectedSeries && restorePending.current) {
      restorePending.current = false;
      const saved = returnView.current;
      if (saved && saved.refresh === refresh && saved.confidence === seriesConfidence) {
        requestAnimationFrame(() => { setLoading(false); window.scrollTo({ top: saved.scrollY, behavior: "instant" }); });
        return;
      }
    }
    const current = ++generation.current;
    let disposed = false;
    const timer = setTimeout(() => {
      setLoading(true);
      void mobile<Page>({
        action: "browse",
        query: selectedSeries ? "" : query,
        sort,
        filter: selectedSeries ? "" : filter,
        offset,
        tags: selectedSeries ? "" : tag,
        collections: selectedSeries ? "" : collection,
        ...((selectedSeries?.ns || ns) ? { ns: selectedSeries?.ns || ns } : {}),
        consolidateSeries, seriesConfidence, selectedSeries: selectedSeries?.key ?? "", groupCopies,
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
        }).finally(() => { if (!disposed && generation.current === current) setLoading(false); });
    }, 250);
    return () => {
      disposed = true;
      clearTimeout(timer);
    };
  }, [query, sort, filter, offset, tag, collection, ns, refresh, active, consolidateSeries, seriesConfidence, selectedSeries, groupCopies]);
  useEffect(() => {
    if (!active) return;
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
  }, [active]);
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
  function renderBook(book: Book, includeCopyGroup = true): ReactNode {
    return (
        <article key={`${book.ns}:${book.id}`} className="book-card">
          <Cover book={book} generation={coverGeneration} showCovers={showCovers && active} />
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
          {includeCopyGroup && selectedSeries && book.copyGroup && <CopyPicker book={book} seriesKey={selectedSeries.key} confidence={seriesConfidence} refresh={refresh} renderBook={copy => renderBook(copy, false)} />}
        </article>
);
  }
  return (
    <section className="space-y-3 border-b border-slate-700 pb-4" aria-busy={loading}>
      {!selectedSeries && <section aria-labelledby="jump-back-heading" className="rounded-xl border border-slate-700 p-3">
        <h3 id="jump-back-heading" className="font-semibold">Jump back in</h3>
        <p className="mt-1 text-sm text-slate-400">Recent reading on your phone and PC. Updates appear after syncing.</p>
        {recentBooks.length ? <div className="mt-3 flex gap-3 overflow-x-auto pb-2">
          {recentBooks.map(book => <button key={`${book.ns}:${book.id}`} className="w-28 shrink-0 text-left" disabled={!book.download && (job.busy || !book.available || book.deleted || book.bytes > 64_000_000)} aria-label={`${book.download ? "Resume" : "Download to resume"}: ${book.title}`} onClick={() => book.download ? void read(book) : void run({ action: "download", ns: book.ns, id: book.id })}>
            <Cover book={book} generation={coverGeneration} showCovers={showCovers && active} />
            <span className="mt-2 block truncate text-sm font-medium">{book.title}</span>
            <span className="block truncate text-xs text-slate-400">{book.creator}</span>
            {!book.download && <span className="block text-xs text-slate-400">{book.available && !book.deleted ? "Download to resume" : "Source unavailable"}</span>}
          </button>)}
        </div> : <p className="mt-3 text-sm text-slate-400">Open a downloaded book and it will appear here.</p>}
      </section>}
      {selectedSeries && <div key={`series-heading:${selectedSeries.key}`} className="series-heading"><nav aria-label="Breadcrumb"><button className="control" onClick={() => history.back()}>← Back to library</button><span aria-hidden="true"> / </span><span aria-current="page">{selectedSeries.name}</span></nav><h3>{selectedSeries.name}</h3><label className="flex min-h-11 items-center gap-2"><input type="checkbox" checked={groupCopies} onChange={event => {setGroupCopies(event.target.checked);localStorage.setItem("tmw-series-group-copies",String(event.target.checked));setOffset(0);}} />Group matching copies</label><p className="text-xs text-slate-400">All books in this series, including PC copies. Downloaded books remain readable offline.</p></div>}
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
      {!selectedSeries && <>
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
        <label className="flex min-h-11 items-center gap-2"><input type="checkbox" checked={consolidateSeries} onChange={event => {setConsolidateSeries(event.target.checked);localStorage.setItem("tmw-consolidate-series",String(event.target.checked));setOffset(0);}} />Consolidate by series</label>
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
            {["", "downloaded", "available", "unavailable", "want", "finished"].map((s) => (
              <option key={s} value={s}>
                {s === "want" ? "Want to read" : s || "All books"}
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
      </>}
      {consolidateSeries && !selectedSeries && page.seriesReady===false && <p className="text-sm text-amber-200">Refresh from an updated PC to load inferred series groups. Existing series labels still work.</p>}
      <p role="status" aria-live="polite">{loading ? "Updating results…" : selectedSeries ? `${page.items.length} title rows on this page` : consolidateSeries ? `${page.items.filter(book => book.seriesCard).length} series · ${page.items.filter(book => !book.seriesCard).length} individual books on this page` : `${page.items.length} books on this page`}</p>
      <div key={selectedSeries ? `series-results:${selectedSeries.key}` : "series-library"} className="space-y-3">
      {page.items.map(book => !selectedSeries && book.seriesCard ? <article key={`series:${book.ns}:${book.seriesCard.key}`} className="book-card series-card"><button className="series-card-button" onClick={() => openSeries(book)}><span className="series-badge">Series</span><Cover book={book} generation={coverGeneration} showCovers={showCovers && active} /><span className="min-w-0"><span className="block font-semibold">{book.seriesCard.name}</span><span className="mt-1 block text-xs text-slate-400">{book.creator}</span><span className="mt-2 block text-sm text-amber-200">{book.seriesCard.count} books · View series →</span></span></button></article> : renderBook(book))}
      </div>
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
