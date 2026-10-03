import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export type SavedPassage = {
  id: number; bookId: number; bookTitle: string; surface: string;
  headword: string | null; reading: string | null; sentence: string; note: string;
  locationCfi: string; isAvailable: boolean;
};

export function SavedPassages({ bookId, onJump }: {
  bookId?: number; onJump: (passage: SavedPassage, cfi: string) => void | Promise<void>;
}) {
  const [rows, setRows] = useState<SavedPassage[]>([]);
  const [offset, setOffset] = useState(0);
  const [hasMore, setHasMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);
  useEffect(() => {
    let current = true;
    void invoke<SavedPassage[]>("list_saved_passages", { bookId: bookId ?? null, offset })
      .then(result => { if (current) { setRows(result); setHasMore(result.length === 50); } })
      .catch(reason => { if (current) setError(String(reason)); });
    return () => { current = false; };
  }, [bookId, offset, revision]);
  const run = async (action: () => Promise<unknown>) => {
    setError(null);
    try { await action(); } catch (reason) { setError(String(reason)); }
  };
  return <section aria-label="Saved passages" className="w-full space-y-4">
    <h1 className="text-2xl font-semibold">Saved passages · 保存した文章</h1>
    <p className="text-sm opacity-70">Bookmarks to passages you want to return to. Notes and excerpts stay available offline.</p>
    {error && <p role="alert" className="text-red-400">{error}</p>}
    {rows.length === 0 && <p className="opacity-60">No saved passages yet. Use “Save passage” in the reader dictionary.</p>}
    {rows.map(row => <PassageCard key={row.id} passage={row}
      onSave={(sentence, note) => run(async () => { await invoke("edit_passage", { id: row.id, sentence, note }); setRevision(value => value + 1); })}
      onDelete={() => run(async () => { await invoke("delete_passage", { id: row.id }); setRevision(value => value + 1); })}
      onJump={() => run(async () => { const cfi = await invoke<string>("get_passage_location", { id: row.id }); await onJump(row, cfi); })} />)}
    <div className="flex gap-4"><button disabled={offset === 0} className="reader-control" onClick={() => setOffset(value => Math.max(0, value - 50))}>Previous</button><button disabled={!hasMore} className="reader-control" onClick={() => setOffset(value => value + 50)}>Next</button></div>
  </section>;
}

function PassageCard({ passage, onSave, onDelete, onJump }: {
  passage: SavedPassage; onSave: (sentence: string, note: string) => Promise<void>;
  onDelete: () => Promise<void>; onJump: () => Promise<void>;
}) {
  const [sentence, setSentence] = useState(passage.sentence);
  const [note, setNote] = useState(passage.note);
  const [busy, setBusy] = useState(false);
  const act = async (action: () => Promise<void>) => { setBusy(true); try { await action(); } finally { setBusy(false); } };
  return <article className="rounded-xl border border-current/15 p-4 space-y-3">
    <p className="text-sm opacity-60">{passage.bookTitle}</p>
    <h2 className="font-semibold">{passage.surface}{passage.headword && passage.headword !== passage.surface ? ` · ${passage.headword}` : ""} {passage.reading}</h2>
    <label className="block text-sm">Sentence<textarea aria-label={`Sentence for ${passage.surface}`} className="control mt-1 w-full" maxLength={4000} value={sentence} onChange={event => setSentence(event.target.value)} /></label>
    <label className="block text-sm">Note<textarea aria-label={`Note for ${passage.surface}`} className="control mt-1 w-full" maxLength={2000} value={note} onChange={event => setNote(event.target.value)} /></label>
    {!passage.isAvailable && <p className="text-sm text-amber-400">Source unavailable; your saved passage is retained.</p>}
    {!passage.locationCfi && <p className="text-sm opacity-60">No source anchor; use the excerpt as a reference.</p>}
    <div className="flex gap-3"><button disabled={busy} className="reader-control" onClick={() => void act(() => onSave(sentence, note))}>Save changes</button><button disabled={busy || !passage.isAvailable || !passage.locationCfi} className="reader-control" onClick={() => void act(onJump)}>Jump to passage</button><button disabled={busy} className="reader-control" onClick={() => { if (window.confirm("Delete this passage bookmark?")) void act(onDelete); }}>Delete</button></div>
  </article>;
}
