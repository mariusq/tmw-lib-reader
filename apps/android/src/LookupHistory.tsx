import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import type { LocalBook } from "./localBook";
import { canJumpHistory, readerLookupResult, type HistoryPage, type HistoryRow, type LookupResult } from "./historyData";
import DictionaryGlossary from "../../../packages/reader-core/DictionaryGlossary";
import { mobileUser } from "./userData";

export default function LookupHistory({ book, onJump }: {
  book?: LocalBook;
  onJump?: (cfi: string) => void;
}) {
  const [query, setQuery] = useState("");
  const [offset, setOffset] = useState(0);
  const [revision, refresh] = useState(0);
  const [page, setPage] = useState<HistoryPage>();
  const [error, setError] = useState("");
  const [definition, setDefinition] = useState<LookupResult>();
  const [lookupWord, setLookupWord] = useState("");
  const generation = useRef(0);
  const lookupGeneration = useRef(0);
  useEffect(() => {
    const current = ++generation.current;
    let disposed = false;
    const timer = setTimeout(() => {
      void mobileUser<HistoryPage>({ action: "historyList", query, offset }).then((result) => {
        if (!disposed && current === generation.current) { setPage(result); setError(""); }
      }).catch((e) => { if (!disposed && current === generation.current) setError(String(e)); });
    }, 250);
    return () => { disposed = true; clearTimeout(timer); };
  }, [query, offset, revision]);
  useEffect(() => () => { lookupGeneration.current++; }, []);
  async function change(args: Record<string, unknown>) {
    try { await mobileUser(args); setOffset(0); refresh((v) => v + 1); }
    catch (e) { setError(String(e)); }
  }
  async function lookup(row: HistoryRow) {
    const current = ++lookupGeneration.current;
    const word = row.fields.headword ?? row.fields.surface;
    setLookupWord(word); setDefinition(undefined); setError("");
    try {
      const result = readerLookupResult(await invoke<LookupResult>("lookup_text", { text: word, offset: 0 }));
      if (lookupGeneration.current === current) setDefinition(result);
    } catch (e) { if (lookupGeneration.current === current) setError(String(e)); }
  }
  return <section className="space-y-3" aria-label="Lookup history">
    <h2 className="text-xl">Lookup history</h2>
    <p className="text-sm">Completed word taps are saved offline. Repeated taps are separate events. Definitions open from the installed dictionary.</p>
    <label className="block">Search word, reading or context
      <input className="block w-full bg-slate-800 p-2" aria-label="Search history" maxLength={256}
        value={query} onChange={(e) => { setQuery(e.target.value); setOffset(0); }} />
    </label>
    {page && <>
      <label className="block"><input type="checkbox" checked={page.settings.enabled}
        onChange={(e) => void change({ action: "historySettings", enabled: e.target.checked })} /> Record new word taps</label>
      <label className="block">Retain recent events on this phone
        <select className="bg-slate-800 p-2 ml-2" value={page.settings.retentionLimit}
          onChange={(e) => {
            const limit = Number(e.target.value);
            if (limit >= page.settings.retentionLimit || window.confirm("Lowering retention permanently deletes older known history across synced devices. Continue?"))
              void change({ action: "historySettings", retentionLimit: limit });
          }}>
          {[...new Set([100, 1000, 10000, page.settings.retentionLimit])].sort((a, b) => a - b)
            .map((n) => <option key={n} value={n}>{n.toLocaleString()}</option>)}
        </select>
      </label>
      <p className="text-sm">Disabling recording keeps existing and incoming history. Clear/delete sync permanently. Lowering retention deletes the oldest known events across synced devices.</p>
      <button className="control" onClick={() => {
        if (window.confirm("Delete all currently known history on this phone and sync those deletions? Offline events unknown to this phone can still arrive."))
          void change({ action: "historyClear" });
      }}>Clear known history</button>
      <button className="control" onClick={() => refresh((v) => v + 1)}>Refresh history</button>
      {page.rows.length === 0 && <p>No matching history.</p>}
      {page.rows.map((row) => <article key={`${row.ns}:${row.entityId}`} className="border-t border-slate-600 py-3 space-y-2">
        <h3 lang="ja">{row.fields.surface} · {row.fields.headword} {row.fields.reading}</h3>
        <p lang="ja" className="text-sm whitespace-pre-wrap">{row.fields.sentence}</p>
        <p className="text-xs break-all">{row.fields.dictionaryId || "Legacy dictionary reference"}</p>
        <button className="control" onClick={() => void lookup(row)}>Definition</button>
        {onJump && canJumpHistory(row, book) && <button className="control"
          onClick={() => onJump(row.fields.locationCfi)}>Go to lookup</button>}
        <button className="control" onClick={() => void change({ action: "historyDelete", ns: row.ns, entityId: row.entityId })}>Delete</button>
      </article>)}
      <nav className="flex justify-between">
        <button className="control" disabled={offset === 0} onClick={() => setOffset(Math.max(0, offset - 50))}>Previous history</button>
        <button className="control" disabled={page.next === null} onClick={() => setOffset(page.next ?? 0)}>Next history</button>
      </nav>
    </>}
    {error && <p role="alert">{error}</p>}
    {lookupWord && <section aria-label="History definition" className="border border-slate-600 p-3">
      <button className="control float-right" onClick={() => { lookupGeneration.current++; setLookupWord(""); }}>Close definition</button>
      <h3 lang="ja">{lookupWord}</h3>
      {!definition && <p>Looking up…</p>}
      {definition?.entries.length === 0 && <p>No definition in the installed dictionary. The original reference is retained.</p>}
      {definition?.groups ? definition.groups.map((group, i) => <article key={i}><h3>{group.term} {group.reading}</h3>{group.matches.map(({ entry }, index) => <section key={index}><h4>{entry.provenance.title}</h4><DictionaryGlossary entry={entry} /></section>)}</article>) : definition?.entries.map((entry, i) => <p key={i} lang="ja">{entry.term} {entry.reading}: {entry.definitions.join("; ")}</p>)}
    </section>}
  </section>;
}
