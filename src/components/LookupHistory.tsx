import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type Encounter = {
  id: number;
  surface: string;
  headword: string | null;
  reading: string | null;
  bookId: number | null;
  bookTitle: string | null;
  locationCfi: string;
  sentence: string;
  lookedUpAt: number;
  count: number;
  isAvailable: boolean;
};

export function LookupHistory({
  onJump,
}: {
  onJump: (bookId: number, cfi: string) => void | Promise<void>;
}) {
  const [rows, setRows] = useState<Encounter[]>([]);
  const [query, setQuery] = useState("");
  const [offset, setOffset] = useState(0);
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [revision, setRevision] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    let current = true;
    void invoke<string | null>("get_app_setting", { key: "lookup_history_enabled" })
      .then((value) => {
        if (current) setEnabled(value !== "false");
      })
      .catch((reason) => {
        if (current) setError(String(reason));
      });
    return () => {
      current = false;
    };
  }, []);
  useEffect(() => {
    let current = true;
    const timer = window.setTimeout(() => {
      void invoke<Encounter[]>("list_lookup_history", { query, offset })
        .then((result) => {
          if (current) {
            setRows(result);
            setError(null);
          }
        })
        .catch((reason) => {
          if (current) setError(String(reason));
        });
    }, 200);
    return () => {
      current = false;
      window.clearTimeout(timer);
    };
  }, [query, offset, revision]);
  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await action();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  };
  return (
    <section aria-label="Lookup history" className="w-full space-y-4">
      <h1 className="text-2xl font-semibold">Lookup history · 調べた言葉</h1>
      <p className="text-sm opacity-70">
        Your most recent 10,000 successful, intentional lookups. Counts cover retained history, not
        occurrences in books. Saved passages remain separate.
      </p>
      <div className="flex flex-wrap gap-3">
        <button
          className="reader-control"
          disabled={enabled === null || busy}
          aria-pressed={enabled === true}
          onClick={() =>
            void run(async () => {
              await invoke("set_app_setting", {
                key: "lookup_history_enabled",
                value: String(!enabled),
              });
              setEnabled(!enabled);
            })
          }
        >
          Tracking {enabled === null ? "…" : enabled ? "on" : "off"}
        </button>
        <button
          className="reader-control"
          disabled={busy}
          onClick={() => {
            if (
              window.confirm(
                "Clear lookup history? Saved passages and reading progress will be kept.",
              )
            )
              void run(async () => {
                await invoke("clear_lookup_history");
                setRows([]);
                setOffset(0);
                setRevision((value) => value + 1);
              });
          }}
        >
          Clear history
        </button>
      </div>
      <input
        aria-label="Search lookup history"
        className="control w-full"
        placeholder="Search surface, headword, or reading"
        maxLength={256}
        value={query}
        onChange={(event) => {
          setQuery(event.target.value);
          setOffset(0);
        }}
      />
      {error && (
        <p role="alert" className="text-red-400">
          {error}
        </p>
      )}
      {rows.length === 0 && <p className="opacity-60">No matching lookups.</p>}
      {rows.map((row) => (
        <article key={row.id} className="space-y-2 rounded-xl border border-current/15 p-4">
          <h2 className="font-semibold">
            {row.surface}
            {row.headword && row.headword !== row.surface ? ` · ${row.headword}` : ""} {row.reading}
          </h2>
          {!row.headword && (
            <p className="text-xs opacity-60">Ambiguous result · counts grouped by query only</p>
          )}
          <p className="text-xs opacity-60">
            Looked up {row.count} {row.count === 1 ? "time" : "times"} ·{" "}
            {new Date(row.lookedUpAt * 1000).toLocaleString()} · {row.bookTitle ?? "Manual query"}
          </p>
          {row.sentence && <p>{row.sentence}</p>}
          {row.bookId !== null && !row.isAvailable && (
            <p className="text-sm text-amber-400">Source unavailable; history retained.</p>
          )}
          {row.bookId !== null && row.locationCfi && (
            <button
              className="reader-control"
              disabled={busy || !row.isAvailable}
              onClick={() =>
                void run(async () => {
                  const cfi = await invoke<string>("get_lookup_history_location", { id: row.id });
                  await onJump(row.bookId!, cfi);
                })
              }
            >
              Jump to passage
            </button>
          )}
        </article>
      ))}
      <div className="flex gap-3">
        <button
          className="reader-control"
          disabled={offset === 0}
          onClick={() => setOffset((value) => Math.max(0, value - 50))}
        >
          Previous
        </button>
        <button
          className="reader-control"
          disabled={rows.length !== 50}
          onClick={() => setOffset((value) => value + 50)}
        >
          Next
        </button>
      </div>
    </section>
  );
}
