import { useEffect, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { EpubReader } from "../features/reader/EpubReader";

type Override = {
  title: string | null;
  creator: string | null;
  seriesName: string | null;
  volumeLabel: string | null;
  coverPath: string | null;
  notes: string | null;
};
type Details = {
  book: {
    id: number;
    filePath: string;
    parentFolderPath: string;
    fileName: string;
    fileSize: number;
    modifiedTime: number;
    discoveredTitle: string | null;
    discoveredCreator: string | null;
    discoveredLanguage: string | null;
    discoveredIdentifier: string | null;
    discoveredSeries: string | null;
    discoveredSeriesIndex: string | null;
    discoveredCoverPath: string | null;
    extractionStatus: string;
    extractionError: string | null;
  };
  effectiveTitle: string;
  effectiveCreator: string;
  effectiveSeries: string;
  effectiveVolume: string;
  effectiveCoverPath: string | null;
  overrideValues: Override;
  tags: [number, string][];
};
type FolderGroup = {
  parentFolderPath: string;
  books: {
    id: number;
    effectiveTitle: string;
    effectiveVolume: string;
    fileName: string;
    suggestedVolume: number | null;
    hasSeriesOverride: boolean;
  }[];
};
const fields: { key: keyof Override; label: string; discovered: keyof Details["book"] }[] = [
  { key: "title", label: "Title", discovered: "discoveredTitle" },
  { key: "creator", label: "Author", discovered: "discoveredCreator" },
  { key: "seriesName", label: "Series", discovered: "discoveredSeries" },
  { key: "volumeLabel", label: "Volume", discovered: "discoveredSeriesIndex" },
];
const displayDate = (seconds: number) =>
  new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(
    seconds * 1000,
  );

export function BookDetailsPanel({
  bookId,
  onClose,
  onSaved,
}: {
  bookId: number;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [details, setDetails] = useState<Details | null>(null);
  const [values, setValues] = useState<Override | null>(null);
  const [tagText, setTagText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [folderGroup, setFolderGroup] = useState<FolderGroup | null>(null);
  const [groupSelection, setGroupSelection] = useState<number[]>([]);
  const [collectionName, setCollectionName] = useState("");
  const [seriesName, setSeriesName] = useState("");
  const [reading, setReading] = useState(false);
  const refresh = async () => {
    const [result, group] = await Promise.all([
      invoke<Details | null>("get_book_details", { bookId }),
      invoke<FolderGroup | null>("get_folder_group", { bookId }),
    ]);
    setDetails(result);
    setValues(result?.overrideValues ?? null);
    setTagText(result?.tags.map(([, name]) => name).join(", ") ?? "");
    setFolderGroup(group);
    setGroupSelection(group?.books.map((book) => book.id) ?? []);
    setSeriesName(result?.effectiveSeries ?? "");
  };
  useEffect(() => {
    // This is the initial Tauri detail-record synchronization for the selected book.
    // eslint-disable-next-line react-hooks/set-state-in-effect
    void refresh().catch((reason) => setError(String(reason)));
    // `refresh` is scoped to this selected record; repeating it on every render would loop.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [bookId]);
  if (!details || !values)
    return (
      <aside className="detail-panel">
        <button className="text-stone-400" onClick={onClose}>
          × Close
        </button>
        <p className="mt-6 text-stone-400">Loading book details…</p>
      </aside>
    );
  if (reading) return <EpubReader bookId={bookId} onClose={() => { setReading(false); onSaved(); }} />;
  const save = async () => {
    try {
      await invoke("save_book_overrides", { bookId, values });
      await invoke("set_book_tags", { bookId, tagNames: tagText.split(",") });
      await refresh();
      onSaved();
    } catch (reason) {
      setError(String(reason));
    }
  };
  const reset = async (field: string) => {
    const databaseField: Record<string, string> = {
      seriesName: "series_name",
      volumeLabel: "volume_label",
      coverPath: "cover_path",
    };
    await invoke("reset_book_override", { bookId, field: databaseField[field] ?? field });
    await refresh();
    onSaved();
  };
  const chooseCover = async () => {
    const path = await invoke<string | null>("choose_cover_replacement");
    if (path) setValues({ ...values, coverPath: path });
  };
  const toggleGroupBook = (id: number) =>
    setGroupSelection((current) =>
      current.includes(id) ? current.filter((bookId) => bookId !== id) : [...current, id],
    );
  const createCollection = async () => {
    try {
      await invoke("create_collection", {
        request: { name: collectionName, bookIds: groupSelection },
      });
      setCollectionName("");
      onSaved();
    } catch (reason) {
      setError(String(reason));
    }
  };
  const applySeries = async () => {
    try {
      await invoke("assign_series", { request: { seriesName, bookIds: groupSelection } });
      await refresh();
      onSaved();
    } catch (reason) {
      setError(String(reason));
    }
  };
  return (
    <aside className="detail-panel" aria-label="Book details">
      <div className="flex items-start justify-between gap-3">
        <div>
          <p className="text-xs font-semibold uppercase tracking-widest text-amber-300">
            Book details
          </p>
          <h1 className="mt-1 text-xl font-semibold">{details.effectiveTitle}</h1>
        </div>
        <button
          className="rounded p-2 text-stone-400 hover:bg-white/10"
          onClick={onClose}
          aria-label="Close details"
        >
          ×
        </button>
      </div>
      <div className="mt-5 grid grid-cols-[7rem_1fr] gap-4">
        <div className="aspect-[2/3] overflow-hidden rounded-lg bg-stone-800">
          {details.effectiveCoverPath ? (
            <img
              className="h-full w-full object-cover"
              src={convertFileSrc(details.effectiveCoverPath)}
              alt=""
            />
          ) : (
            <div className="grid h-full place-items-center text-2xl text-stone-500">本</div>
          )}
        </div>
        <div className="text-sm">
          <p className="text-stone-400">Effective metadata</p>
          <p className="mt-2">{details.effectiveCreator || "Unknown author"}</p>
          <p className="mt-1 text-amber-200">
            {[details.effectiveSeries, details.effectiveVolume].filter(Boolean).join(" · ")}
          </p>
          <button
            className="mt-4 text-amber-300 underline"
            onClick={() =>
              void invoke("open_book_folder", { path: details.book.parentFolderPath }).catch(
                (reason) => setError(String(reason)),
              )
            }
          >
            Open folder
          </button>
          <button className="mt-2 block text-amber-300 underline" onClick={() => setReading(true)}>
            Read book
          </button>
        </div>
      </div>
      <section className="mt-7">
        <div className="flex items-center justify-between">
          <h2 className="font-semibold">Your corrections</h2>
          <button
            className="text-xs text-amber-300 underline"
            onClick={() =>
              void invoke("reset_all_book_overrides", { bookId }).then(refresh).then(onSaved)
            }
          >
            Reset all overrides
          </button>
        </div>
        {fields.map(({ key, label, discovered }) => (
          <label key={key} className="mt-3 block text-sm">
            <span className="flex justify-between text-stone-300">
              {label}
              <button
                type="button"
                className="text-xs text-amber-300 underline"
                onClick={() => void reset(key)}
              >
                Reset
              </button>
            </span>
            <input
              className="control mt-1"
              value={values[key] ?? ""}
              placeholder={`Discovered: ${details.book[discovered] || "—"}`}
              onChange={(e) => setValues({ ...values, [key]: e.target.value || null })}
            />
          </label>
        ))}
        <label className="mt-3 block text-sm">
          Replacement cover{" "}
          <div className="mt-1 flex gap-2">
            <input
              className="control"
              value={values.coverPath ?? ""}
              readOnly
              placeholder="Discovered cover or none"
            />
            <button
              className="rounded border border-white/15 px-3"
              onClick={() => void chooseCover()}
            >
              Choose
            </button>
            <button
              className="text-xs text-amber-300 underline"
              onClick={() => void reset("cover_path")}
            >
              Reset
            </button>
          </div>
        </label>
        <label className="mt-3 block text-sm">
          Tags{" "}
          <input
            className="control mt-1"
            value={tagText}
            onChange={(e) => setTagText(e.target.value)}
            placeholder="Comma separated"
          />
        </label>
        <label className="mt-3 block text-sm">
          Notes{" "}
          <textarea
            className="control mt-1 min-h-24"
            value={values.notes ?? ""}
            onChange={(e) => setValues({ ...values, notes: e.target.value || null })}
          />
        </label>
        <button className="primary mt-4" onClick={() => void save()}>
          Save corrections
        </button>
      </section>
      {folderGroup && (
        <section className="mt-7 border-t border-white/10 pt-5 text-sm">
          <h2 className="font-semibold">Folder group</h2>
          <p className="mt-1 break-all text-xs text-stone-400">{folderGroup.parentFolderPath}</p>
          <p className="mt-2 text-xs text-stone-500">
            Volume numbers and this grouping are suggestions only. Nothing changes until you apply
            an action below.
          </p>
          <div className="mt-3 max-h-52 overflow-auto rounded border border-white/10">
            {folderGroup.books.map((book) => (
              <label
                key={book.id}
                className="flex cursor-pointer items-center gap-2 border-b border-white/10 px-3 py-2 last:border-0"
              >
                <input
                  type="checkbox"
                  checked={groupSelection.includes(book.id)}
                  onChange={() => toggleGroupBook(book.id)}
                />
                <span className="min-w-0 flex-1 truncate">{book.effectiveTitle}</span>
                <span className="shrink-0 text-xs text-amber-200">
                  {book.effectiveVolume ||
                    (book.suggestedVolume !== null
                      ? `suggested ${book.suggestedVolume}`
                      : "natural order")}
                </span>
                {book.hasSeriesOverride && (
                  <span className="shrink-0 text-xs text-stone-500">custom series</span>
                )}
              </label>
            ))}
          </div>
          <div className="mt-4 grid gap-3">
            <div className="flex gap-2">
              <input
                className="control"
                value={collectionName}
                onChange={(event) => setCollectionName(event.target.value)}
                placeholder="New collection name"
              />
              <button
                className="rounded border border-white/15 px-3"
                disabled={!collectionName.trim() || !groupSelection.length}
                onClick={() => void createCollection()}
              >
                Create collection
              </button>
            </div>
            <div className="flex gap-2">
              <input
                className="control"
                value={seriesName}
                onChange={(event) => setSeriesName(event.target.value)}
                placeholder="Series name"
              />
              <button
                className="rounded border border-amber-300/50 px-3 text-amber-200"
                disabled={!seriesName.trim() || !groupSelection.length}
                onClick={() => void applySeries()}
              >
                Assign series
              </button>
            </div>
          </div>
        </section>
      )}
      <section className="mt-7 border-t border-white/10 pt-5 text-sm">
        <h2 className="font-semibold">EPUB-discovered data</h2>
        <dl className="mt-3 grid gap-2 text-stone-400">
          <div>Title: {details.book.discoveredTitle || "—"}</div>
          <div>Author: {details.book.discoveredCreator || "—"}</div>
          <div>Language: {details.book.discoveredLanguage || "—"}</div>
          <div>Identifier: {details.book.discoveredIdentifier || "—"}</div>
          <div>
            Extraction: {details.book.extractionStatus}
            {details.book.extractionError ? ` — ${details.book.extractionError}` : ""}
          </div>
        </dl>
      </section>
      <section className="mt-5 border-t border-white/10 pt-5 text-xs text-stone-400">
        <p className="break-all">Source: {details.book.filePath}</p>
        <p className="mt-1">
          File: {details.book.fileName} · {(details.book.fileSize / 1024 / 1024).toFixed(1)} MB
        </p>
        <p className="mt-1">Modified: {displayDate(details.book.modifiedTime)}</p>
      </section>
      {error && (
        <p role="alert" className="mt-4 text-sm text-red-300">
          {error}
        </p>
      )}
    </aside>
  );
}
