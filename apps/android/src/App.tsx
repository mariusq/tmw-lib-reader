import ConnectionSettings from "./ConnectionSettings";
import Reader from "./Reader";
import Catalog from "./Catalog";
import type { LocalBook } from "./localBook";
import { useState } from "react";

import { invoke } from "@tauri-apps/api/core";
import noticesUrl from "./notices.txt?url";
import "./styles.css";

type ProbeReport = {
  architecture: string;
  surface: string;
  lemma: string;
  reading: string | null;
  tokenizerMs: number;
  importForms: number;
  sqliteVersion: string;
  persistedRuns: number;
  totalMs: number;
};

export default function App() {
  const [selected, setSelected] = useState<LocalBook>();
  const [libraryOpen, setLibraryOpen] = useState(true);
  const [report, setReport] = useState<ProbeReport | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [notices, setNotices] = useState("");
  const [noticesOpen, setNoticesOpen] = useState(false);

  async function check() {
    setBusy(true);
    setError("");
    try {
      setReport(await invoke<ProbeReport>("run_feasibility_probe"));
    } catch (error) {
      setError(String(error));
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="mx-auto max-w-lg space-y-2 px-3 py-3">
      <h1 className="text-xl font-semibold">TMW Companion</h1>

      <button className="control" onClick={() => setLibraryOpen(!libraryOpen)}>
        {libraryOpen ? "Open reader" : "Browse library"}
      </button>
      {libraryOpen ? (
        <Catalog
          onOpen={(book) => {
            setSelected(book);
            setLibraryOpen(false);
          }}
        />
      ) : (
        <Reader selected={selected} onLocalSelection={() => setSelected(undefined)} />
      )}
      <details>
        <summary className="control">About / Settings</summary>
        <ConnectionSettings />
        <section className="space-y-4 rounded-2xl bg-slate-800 p-5">
          <h2 className="text-xl font-medium">Offline dependency check</h2>
          <p>Test the shared Japanese tokenizer, JMdict importer, and local SQLite storage.</p>
          <p lang="ja" className="text-xl">
            猫を食べました。
          </p>
          <button
            onClick={check}
            disabled={busy}
            className="min-h-12 w-full rounded-xl bg-teal-300 px-4 font-semibold text-slate-950 disabled:opacity-50"
          >
            {busy ? "Checking…" : "Run check"}
          </button>
          <div aria-live="polite">
            {error && (
              <p role="alert" className="break-words text-red-300">
                {error}
              </p>
            )}
            {report && (
              <dl className="space-y-2">
                <div>
                  <dt className="text-teal-300">Passed · {report.architecture}</dt>
                  <dd>
                    食べ → {report.lemma} ({report.reading})
                  </dd>
                </div>
                <div>
                  <dt>Tokenizer</dt>
                  <dd>{report.tokenizerMs.toFixed(1)} ms</dd>
                </div>
                <div>
                  <dt>JMdict format import</dt>
                  <dd>{report.importForms} synthetic forms</dd>
                </div>
                <div>
                  <dt>SQLite {report.sqliteVersion}</dt>
                  <dd>{report.persistedRuns} saved runs</dd>
                </div>
                <div>
                  <dt>Total</dt>
                  <dd>{report.totalMs.toFixed(1)} ms</dd>
                </div>
              </dl>
            )}
          </div>
        </section>
        <p className="text-sm text-slate-400">
          Bundled JMdict English definitions work offline. Private PC pairing is available in
          Settings.
        </p>
        <section className="rounded-xl border border-slate-700 p-4">
          <button
            className="min-h-12 w-full cursor-pointer text-left"
            aria-expanded={noticesOpen}
            aria-controls="dependency-notices"
            onClick={() => {
              setNoticesOpen(!noticesOpen);
              if (!noticesOpen && !notices) {
                void fetch(noticesUrl)
                  .then((response) => {
                    if (!response.ok) throw new Error("Could not load notices");
                    return response.text();
                  })
                  .then(setNotices)
                  .catch(() =>
                    setNotices("Could not load bundled notices. Reopen the app to retry."),
                  );
              }
            }}
          >
            {noticesOpen ? "Hide" : "Show"} licenses and dictionary notices
          </button>
          {noticesOpen && (
            <pre
              id="dependency-notices"
              className="mt-3 whitespace-pre-wrap text-xs text-slate-300"
            >
              {notices || "Loading bundled notices…"}
            </pre>
          )}
        </section>
      </details>
    </main>
  );
}
