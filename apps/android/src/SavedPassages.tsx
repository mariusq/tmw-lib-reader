import { useEffect, useState } from "react";
import { readUserState, saveUserData, type BookIdentity, type UserRecord } from "./userData";

function Passage({
  record,
  book,
  onChange,
  onJump,
}: {
  record: UserRecord;
  book: BookIdentity;
  onChange: () => void;
  onJump: (cfi: string) => void;
}) {
  const [note, setNote] = useState(record.fields.note ?? "");
  const [sentence, setSentence] = useState(record.fields.sentence ?? "");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  async function save(deleted = false) {
    setBusy(true);
    setError("");
    try {
      const fields = deleted
        ? {}
        : {
            ...(note !== (record.fields.note ?? "") ? { note } : {}),
            ...(sentence !== (record.fields.sentence ?? "") ? { sentence } : {}),
          };
      if (deleted || Object.keys(fields).length) {
        await saveUserData(
          { ...book, version: record.contentVersion },
          "passage",
          fields,
          record.entityId,
          deleted,
        );
        onChange();
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <article className="space-y-2 border-t border-slate-700 py-3">
      <p lang="ja">{record.fields.surface}</p>
      <label className="block">
        Excerpt
        <textarea
          aria-label="Saved excerpt"
          maxLength={4000}
          className="block w-full bg-slate-800 p-2"
          value={sentence}
          onChange={(e) => setSentence(e.target.value)}
        />
      </label>
      <label className="block">
        Note
        <textarea
          aria-label="Passage note"
          maxLength={2000}
          className="block w-full bg-slate-800 p-2"
          value={note}
          onChange={(e) => setNote(e.target.value)}
        />
      </label>
      {(!record.contentVersion || record.contentVersion !== book.version) && (
        <p className="text-sm">Saved for a different EPUB version; anchor is disabled.</p>
      )}
      <div className="flex flex-wrap gap-2">
        <button
          className="control"
          disabled={
            busy ||
            !book.version ||
            !record.contentVersion ||
            !record.fields.locationCfi ||
            record.contentVersion !== book.version
          }
          onClick={() => onJump(record.fields.locationCfi!)}
        >
          Go to passage
        </button>
        <button className="control" disabled={busy} onClick={() => void save()}>
          Save changes
        </button>
        <button className="control" disabled={busy} onClick={() => void save(true)}>
          Delete passage
        </button>
      </div>
      {error && <p role="alert">{error}</p>}
    </article>
  );
}

export default function SavedPassages({
  book,
  revision,
  onJump,
}: {
  book: BookIdentity;
  revision: number;
  onJump: (cfi: string) => void;
}) {
  const [records, setRecords] = useState<UserRecord[]>([]);
  const [refresh, setRefresh] = useState(0);
  const [error, setError] = useState("");
  const { ns, id, version } = book;
  const [offset, setOffset] = useState(0);
  const [next, setNext] = useState<number | null>(null);
  useEffect(() => {
    let disposed = false;
    void readUserState({ ns, id, version }, offset)
      .then((state) => {
        if (!disposed) {
          setRecords(state.passages.filter((p) => !p.deleted));
          setNext(state.next ?? null);
          setError("");
        }
      })
      .catch((e) => {
        if (!disposed) setError(String(e));
      });
    return () => {
      disposed = true;
    };
  }, [ns, id, version, revision, refresh, offset]);
  return (
    <details>
      <summary className="control" onClick={() => setRefresh((v) => v + 1)}>
        Saved passages and notes
      </summary>
      {error && <p role="alert">{error}</p>}
      {!records.length && <p>No saved passages. Save a bookmark or a lookup excerpt.</p>}
      {records.map((record) => (
        <Passage
          key={`${record.entityId}:${JSON.stringify(record.fields)}`}
          record={record}
          book={book}
          onChange={() => setRefresh((v) => v + 1)}
          onJump={onJump}
        />
      ))}
      <div className="flex gap-2">
        <button
          className="control"
          disabled={offset === 0}
          onClick={() => setOffset(Math.max(0, offset - 50))}
        >
          Previous notes
        </button>
        <button
          className="control"
          disabled={next === null}
          onClick={() => setOffset(next ?? offset)}
        >
          Next notes
        </button>
      </div>
    </details>
  );
}
