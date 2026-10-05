import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export type ImportedDictionary = {
  id: number; title: string; revision: string; enabled: boolean; priority: number;
  resultLimit?: number; termCount: number; attribution: string | null; warnings?: string[];
};
export type DictionaryImportReport = {
  manifest: { title: string; revision: string; format: number; sequenced: boolean; attribution: string | null };
  terms: number; tags: number; assets: number; metadata?: number; warnings: string[];
};
type ImportStatus = {
  running: boolean;
  progress: { phase: string; filesDone: number; filesTotal: number; terms: number; tags: number; metadata?: number };
  error?: string | null; report?: DictionaryImportReport | null;
};

/** Device-local import management for reader definitions and term metadata. */
export default function DictionaryManager({ resultLimits = true }: { resultLimits?: boolean } = {}) {
  const [dictionaries, setDictionaries] = useState<ImportedDictionary[]>([]);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<ImportStatus | null>(null);
  const [report, setReport] = useState<DictionaryImportReport | null>(null);
  const [error, setError] = useState("");
  const [remove, setRemove] = useState<ImportedDictionary | null>(null);
  const mounted = useRef(false);
  const generation = useRef(0);
  const control = "rounded-lg border border-current px-3 py-2 disabled:opacity-50";

  async function refresh() {
    const result = await invoke<ImportedDictionary[]>("dictionary_manage", { action: "list" });
    if (mounted.current) setDictionaries(result);
  }
  useEffect(() => {
    mounted.current = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const current = ++generation.current;
    const poll = async () => {
      try {
        const next = await invoke<ImportStatus>("dictionary_import_status");
        if (!mounted.current || generation.current !== current) return;
        setStatus(next);
        if (next.running) timer = setTimeout(() => void poll(), 500);
        else {
          if (next.error) setError(next.error);
          if (next.report) setReport(next.report);
          await refresh();
        }
      } catch (reason) {
        if (mounted.current && generation.current === current) setError(String(reason));
      }
    };
    void refresh().catch(reason => { if (mounted.current) setError(String(reason)); });
    void poll();
    return () => { mounted.current = false; clearTimeout(timer); };
  }, []);

  async function importZip(replace?: number) {
    setBusy(true); setError(""); setReport(null);
    const current = ++generation.current;
    let timer: ReturnType<typeof setTimeout> | undefined;
    // Poll during file selection/import and stop when the command settles.
    const poll = async () => {
      try {
        const next = await invoke<ImportStatus>("dictionary_import_status");
        if (mounted.current && generation.current === current) {
          setStatus(next);
          timer = setTimeout(() => void poll(), 500);
        }
      } catch (reason) {
        if (mounted.current && generation.current === current) setError(String(reason));
      }
    };
    timer = setTimeout(() => void poll(), 500);
    try {
      const result = await invoke<DictionaryImportReport | null>("dictionary_import", { replace: replace ?? null });
      if (mounted.current) setReport(result);
      await refresh();
    } catch (reason) {
      if (mounted.current) setError(String(reason));
    } finally {
      clearTimeout(timer);
      if (generation.current === current) ++generation.current;
      if (mounted.current) { setBusy(false); setStatus(null); }
    }
  }
  async function manage(action: "update" | "remove" | "limit", dictionary: ImportedDictionary, update = {}) {
    setBusy(true); setError("");
    try {
      await invoke("dictionary_manage", { action, id: dictionary.id, ...update, ...(action === "update" ? { enabled: dictionary.enabled, priority: dictionary.priority, ...update } : {}) });
      await refresh();
      if (mounted.current) setRemove(null);
    } catch (reason) { if (mounted.current) setError(String(reason)); }
    finally { if (mounted.current) setBusy(false); }
  }
  const disabled = busy || !!status?.running;
  return <section aria-label="Imported dictionaries" className="my-5 space-y-4 rounded-xl border border-current/20 p-5">
    <h2 className="text-xl font-semibold">Imported dictionaries</h2>
    <p>Import local Yomitan format-3 term, frequency or pitch-accent dictionary ZIPs. Imports stay on this device; the source ZIP stays untouched.</p>
    <p className="text-sm opacity-75">Enabled term dictionaries provide reader definitions, tags and local images. Frequency and pitch dictionaries add metadata to matching headwords; import a term dictionary to look up words. Unsupported features are reported during import.</p>
    <p className="text-sm opacity-75">Uncheck Enable to disable a dictionary while keeping its import. You can enable it again anytime.</p>
    <button className={control} disabled={disabled} onClick={() => void importZip()}>Import dictionary ZIP</button>
    {status?.running && <div role="status" aria-live="polite">
      <p>{status.progress.phase}: {status.progress.filesDone} / {status.progress.filesTotal} files · {status.progress.terms.toLocaleString()} terms · {(status.progress.metadata ?? 0).toLocaleString()} metadata entries</p>
      <button className={control} onClick={() => void invoke("cancel_dictionary_import").catch(reason => { if (mounted.current) setError(String(reason)); })}>Cancel import</button>
    </div>}
    {busy && !status?.running && <p role="status">Waiting for file selection or dictionary storage…</p>}
    {error && <p role="alert">{error}</p>}
    {report && <div role="status"><p>Imported {report.manifest.title}: {report.terms.toLocaleString()} terms, {(report.metadata ?? 0).toLocaleString()} metadata entries, {report.tags.toLocaleString()} tags.</p>{report.warnings.length > 0 && <ul>{report.warnings.map((warning, index) => <li key={index}>{warning}</li>)}</ul>}</div>}
    {dictionaries.length === 0 && <p>No imported dictionaries.</p>}
    {dictionaries.map(dictionary => <div key={dictionary.id} className="space-y-2 border-t border-current/20 pt-4">
      <h3 className="font-semibold">{dictionary.title}</h3>
      <p className="text-sm">Revision {dictionary.revision} · {dictionary.termCount.toLocaleString()} terms</p>
      {dictionary.attribution && <p className="whitespace-pre-wrap break-words text-sm">{dictionary.attribution}</p>}
      {!!dictionary.warnings?.length && <ul className="text-sm">{dictionary.warnings.map((warning, index) => <li key={index}>{warning}</li>)}</ul>}
      <div className="flex flex-wrap items-center gap-3">
        <label><input type="checkbox" checked={dictionary.enabled} disabled={disabled} onChange={event => void manage("update", dictionary, { enabled: event.target.checked })} /> Enable {dictionary.title}</label>
        <label>Priority for {dictionary.title} <input className="w-20 rounded border bg-transparent p-2" type="number" defaultValue={dictionary.priority} key={`${dictionary.id}:${dictionary.priority}`} disabled={disabled} onBlur={event => {
          const priority = Number(event.target.value);
          if (event.target.value !== "" && Number.isSafeInteger(priority) && priority !== dictionary.priority) void manage("update", dictionary, { priority });
          else event.target.value = String(dictionary.priority);
        }} /></label>
        {resultLimits && <label>Result limit for {dictionary.title} <input className="w-20 rounded border bg-transparent p-2" aria-label={`Result limit for ${dictionary.title}`} type="number" min={0} max={256} step={1} defaultValue={dictionary.resultLimit ?? 0} key={`${dictionary.id}:limit:${dictionary.resultLimit}`} disabled={disabled} onBlur={event => {
          const resultLimit = Number(event.target.value);
          if (event.target.value !== "" && Number.isSafeInteger(resultLimit) && resultLimit >= 0 && resultLimit <= 256 && resultLimit !== (dictionary.resultLimit ?? 0)) void manage("limit", dictionary, { resultLimit });
          else event.target.value = String(dictionary.resultLimit ?? 0);
        }} /> <span className="text-sm opacity-75">entries per lookup (0 = default)</span></label>}
        <button className={control} disabled={disabled} onClick={() => void importZip(dictionary.id)}>Replace {dictionary.title} from ZIP</button>
        <button className={control} disabled={disabled} onClick={() => setRemove(dictionary)}>Remove {dictionary.title}</button>
      </div>
    </div>)}
    {remove && <div role="alertdialog" aria-label={`Remove ${remove.title}`} className="space-y-3 border border-current p-4">
      <p>Remove the app-managed import of {remove.title}? Its source ZIP and saved passages/history stay untouched. You can import the ZIP again later.</p>
      <button className={control} disabled={disabled} onClick={() => void manage("remove", remove)}>Confirm removal</button>{" "}
      <button className={control} disabled={disabled} onClick={() => setRemove(null)}>Keep dictionary</button>
    </div>}
  </section>;
}
