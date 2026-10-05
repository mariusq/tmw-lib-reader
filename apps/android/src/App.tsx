import ConnectionSettings from "./ConnectionSettings";
import RecoverySettings from "./RecoverySettings";
import DictionaryManager from "../../../packages/reader-core/DictionaryManager";
import Reader from "./Reader";
import Catalog from "./Catalog";
import UserSyncStatus from "./UserSyncStatus";
import LookupHistory from "./LookupHistory";
import type { LocalBook } from "./localBook";
import { useEffect, useRef, useState } from "react";

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
  const [error, setError] = useState("");
  const [selected, setSelected] = useState<LocalBook>();
  type Tab = "library" | "reader" | "history" | "settings";
  const [tab, setTab] = useState<Tab>("library");
  const tabRef = useRef<Tab>("library");
  function navigate(next: Tab) {
    setError("");
    if (next === tabRef.current) return;
    if (next === "library") {
      history.back();
      return;
    }
    if (tabRef.current === "library") history.pushState({ tmwTab: next }, "");
    else history.replaceState({ tmwTab: next }, "");
    tabRef.current = next;
    setTab(next);
    localStorage.setItem("tmw-screen", next);
  }
  useEffect(() => {
    const back = () => {
      if (history.state?.tmwLookup || history.state?.tmwReaderMenu) return;
      const next = (history.state?.tmwTab ?? "library") as Tab;
      tabRef.current = next;
      setTab(next);
      localStorage.setItem("tmw-screen", next);
    };
    window.addEventListener("popstate", back);
    let disposed = false;
    if (localStorage.getItem("tmw-screen") === "reader") {
      try {
        const saved = JSON.parse(localStorage.getItem("tmw-last-download") ?? "null") as Omit<
          LocalBook,
          "bytes"
        > | null;
        if (saved?.file && saved.catalog)
          void invoke<ArrayBuffer>("read_mobile_book", { file: saved.file })
            .then((bytes) => {
              if (!disposed) {
                setSelected({ ...saved, bytes });
                history.pushState({ tmwTab: "reader" }, "");
                tabRef.current = "reader";
                setTab("reader");
              }
            })
            .catch(() => {
              if (!disposed) {
                localStorage.removeItem("tmw-last-download");
                localStorage.setItem("tmw-screen", "library");
                setError(
                  "The previous download could not be reopened. Choose a book from your library.",
                );
              }
            });
        else {
          history.pushState({ tmwTab: "reader" }, "");
          tabRef.current = "reader";
          queueMicrotask(() => {
            if (!disposed) setTab("reader");
          });
        }
      } catch {
        localStorage.removeItem("tmw-last-download");
      }
    }
    return () => {
      disposed = true;
      window.removeEventListener("popstate", back);
    };
  }, []);
  const [report, setReport] = useState<ProbeReport | null>(null);

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
    <main className={`app-shell ${tab === "reader" ? "reading" : ""}`}>
      <header className="app-header">
        <div className="brand-mark" aria-hidden="true">
          文
        </div>
        <div>
          <p className="eyebrow">YOUR READING COMPANION</p>
          <h1>TMW Library</h1>
        </div>
        <span className="offline-badge">Offline ready</span>
      </header>
      <div className="screen-content">
        {error && tab !== "settings" && <div role="alert"><p>{error}</p><button onClick={() => setError("")}>Dismiss</button></div>}
        {tab === "library" && (
          <>
            <div className="screen-heading">
              <h2>Your library</h2>
              <p>Japanese books, wherever you are.</p>
            </div>
            <Catalog
              onOpen={(book) => {
                setError("");
                setSelected(book);
                const descriptor = {
                  name: book.name,
                  id: book.id,
                  file: book.file,
                  catalog: book.catalog,
                };
                localStorage.setItem("tmw-last-download", JSON.stringify(descriptor));
                navigate("reader");
              }}
            />
          </>
        )}
        {tab === "reader" && (
          <Reader
            onExit={() => history.go(history.state?.tmwReaderMenu ? -2 : -1)}
            selected={selected}
            onLocalSelection={() => {
              setError("");
              setSelected(undefined);
              localStorage.removeItem("tmw-last-download");
            }}
          />
        )}
        {tab === "history" && <LookupHistory />}
        <div hidden={tab !== "settings"}>
          <div className="screen-heading">
            <h2>Settings</h2>
            <p>Your connection and saved data.</p>
          </div>
          <UserSyncStatus />
          {error && <p role="alert">{error}</p>}
          <section className="settings-panel">
            <ConnectionSettings />
            <RecoverySettings />
            {tab === "settings" && <DictionaryManager />}
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
          </section>
        </div>
      </div>
      <nav className="bottom-nav" aria-label="Main navigation">
        {(["library", "reader", "history", "settings"] as Tab[]).map((item, i) => (
          <button
            key={item}
            aria-current={tab === item ? "page" : undefined}
            onClick={() => navigate(item)}
          >
            <span aria-hidden="true">{["▤", "本", "◷", "⚙"][i]}</span>
            {item === "library" ? "Library" : item[0].toUpperCase() + item.slice(1)}
          </button>
        ))}
      </nav>
    </main>
  );
}
