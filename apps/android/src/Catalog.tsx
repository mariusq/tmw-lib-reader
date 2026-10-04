import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import type { LocalBook } from "./localBook";

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

function Cover({ book, generation }: { book: Book; generation: number }) {
  const host = useRef<HTMLDivElement>(null);
  const [data, setData] = useState("");
  useEffect(() => {
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
  }, [book.ns, book.id, book.coverVersion, book.download?.version, generation]);
  return (
    <div ref={host} className="h-24 w-16 shrink-0 bg-slate-800">
      {data ? (
        <img src={data} alt="" decoding="async" className="h-full w-full object-contain" />
      ) : (
        <span className="text-xs text-slate-500">No cover</span>
      )}
    </div>
  );
}

export default function Catalog({ onOpen }: { onOpen: (book: LocalBook) => void }) {
  const [page, setPage] = useState<Page>({ items: [], next: null, ns: "", namespaces: [] });
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
    try {
      setError("");
      await mobile(args);
      if (args.action === "sync" || args.action === "download") {
        pendingJob.current = true;
        setJob({ busy: true, message: "Starting" });
      }
      setRefresh((v) => v + 1);
    } catch (e) {
      setError(String(e));
    }
  }
  async function read(book: Book) {
    if (!book.download || reading.current) return;
    reading.current = true;
    try {
      const bytes = await invoke<ArrayBuffer>("read_mobile_book", { file: book.download.file });
      onOpen({ name: book.title, bytes, id: book.download.version.slice(7) });
    } catch (e) {
      setError(String(e));
    } finally {
      reading.current = false;
    }
  }
  return (
    <section className="space-y-3 border-b border-slate-700 pb-4">
      <h2>Offline catalog</h2>
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
      {page.items.map((book) => (
        <article key={`${book.ns}:${book.id}`} className="flex gap-3 rounded bg-slate-900 p-2">
          <Cover book={book} generation={coverGeneration} />
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
            <div className="flex gap-2">
              {book.download ? (
                <>
                  <button className="control" onClick={() => void read(book)}>
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
          </div>
        </article>
      ))}
      {!page.items.length && <p>No local results. Pair under Settings, then refresh.</p>}
      <nav className="flex gap-3">
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
