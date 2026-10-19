import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { activityNative } from "./readingActivity";
import { calendarDays, localDay, weekStart } from "../../../packages/reader-core/readingActivity";
import type { LocalBook } from "./localBook";
import { Cover, type Book } from "./Catalog";

type Day = { day: string; milliseconds: number; characters: number; lookups: number };
type Summary = { enabled: boolean; days: Day[]; books: (Day & { id: string; title: string })[]; finished: number; streak: number; longestStreak: number; syncMessage: string; resetPending: boolean; activitySync?: { busy: boolean; pending: number; rejected: number } };
const time = (ms: number) => { const minutes = Math.floor(ms / 60000); return minutes >= 60 ? `${Math.floor(minutes / 60)}h ${minutes % 60}m` : `${minutes}m`; };
const number = (value: number) => value.toLocaleString();
export default function ReadingHome({ onOpen, onLibrary }: { onOpen: (book: LocalBook) => void; onLibrary: (filter?: string) => void }) {
  const [summary, setSummary] = useState<Summary>();
  const [recent, setRecent] = useState<Book[]>([]);
  const [want, setWant] = useState<Book[]>([]);
  const [namespace, setNamespace] = useState("");
  const [metric, setMetric] = useState<"milliseconds" | "characters" | "lookups">(() => {
    const value = localStorage.getItem("tmw-home-metric"); return value === "characters" || value === "lookups" ? value : "milliseconds";
  });
  const [selected, setSelected] = useState(localDay(new Date()));
  const [revision, setRevision] = useState(0);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [confirmClear, setConfirmClear] = useState(false);
  const now = new Date(), dates = calendarDays(now, 84), week = weekStart(now), today = localDay(now);
  const showCovers = localStorage.getItem("tmw-show-covers") !== "false";
  useEffect(() => {
    void activityNative({ action: "coverGeneration", generation: 0 }).catch(() => {});
    void activityNative({ action: "activitySync" }).catch(() => {});
  }, []);
  useEffect(() => {
    let disposed = false;
    void Promise.all([
      activityNative<{ items: Book[]; ns: string }>({ action: "browse", recent: true }),
      activityNative<{ items: Book[] }>({ action: "browse", filter: "want" }),
    ]).then(([recent, want]) => { if (!disposed) { setRecent(recent.items.slice(0, 12)); setWant(want.items.slice(0, 6)); setNamespace(recent.ns); } }).catch(reason => { if (!disposed) setError(String(reason)); });
    return () => { disposed = true; };
  }, [revision]);
  useEffect(() => {
    let disposed = false;
    void activityNative<Summary>({ action: "activitySummary", ...(namespace ? { ns: namespace } : {}), startDay: localDay(dates[0]), endDay: today, selectedDay: selected, weekDay: localDay(week), weekStart: Math.floor(week.getTime() / 1000), nowSeconds: Math.floor(now.getTime() / 1000) + 1 })
      .then(value => { if (!disposed) { setSummary(value); setError(""); } }).catch(reason => { if (!disposed) setError(String(reason)); });
    return () => { disposed = true; };
  // Calendar boundaries change on the periodic refresh, including local midnight.
  }, [selected, namespace, revision, today]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => {
    const refresh = () => { if (document.visibilityState === "visible") setRevision(value => value + 1); };
    const timer = setInterval(refresh, 5000);
    window.addEventListener("tmw-catalog-updated", refresh); document.addEventListener("visibilitychange", refresh);
    return () => { clearInterval(timer); window.removeEventListener("tmw-catalog-updated", refresh); document.removeEventListener("visibilitychange", refresh); };
  }, []);
  async function change(clear = false) {
    setBusy(true);
    try {
      await activityNative({ action: clear ? "activityClear" : "activitySettings", ...(namespace ? { ns: namespace } : {}), ...(clear ? {} : { enabled: !summary?.enabled }) });
      window.dispatchEvent(new Event("tmw-activity-settings"));
      setConfirmClear(false); setRevision(value => value + 1);
      if (clear) void activityNative({ action: "activitySync" }).catch(() => {});
    } catch (reason) { setError(String(reason)); } finally { setBusy(false); }
  }
  async function open(book: Book) {
    if (!book.download || busy) return;
    setBusy(true);
    try {
      const bytes = await invoke<ArrayBuffer>("read_mobile_book", { file: book.download.file });
      onOpen({ name: book.title, bytes, file: book.download.file, id: book.download.version.slice(7), catalog: { ns: book.ns, id: book.id, version: book.download.version } });
    } catch (reason) { setError(String(reason)); } finally { setBusy(false); }
  }
  const row = (book: Book) => <article className="mobile-home-book" key={`${book.ns}:${book.id}`}>
    <Cover book={book} generation={0} showCovers={showCovers} />
    <div><h3 lang="ja">{book.title}</h3><p>{book.creator}</p>
      {book.download ? <button className="control" disabled={busy} onClick={() => void open(book)}>Resume</button> :
        <button className="control" disabled={!book.available || book.deleted || busy || book.bytes > 64000000} onClick={() => {
          setBusy(true); void activityNative({ action: "download", ns: book.ns, id: book.id }).then(() => onLibrary()).catch(reason => setError(String(reason))).finally(() => setBusy(false));
        }}>Download privately</button>}
    </div>
  </article>;
  const weekly = summary?.days.filter(day => day.day >= localDay(week)) ?? [];
  const totals = new Map(summary?.days.map(day => [day.day, day]) ?? []);
  const day = totals.get(selected);
  const primary = recent.find(book => book.download);
  const thresholds = metric === "milliseconds" ? [60000, 900000, 3600000] : metric === "characters" ? [1, 1000, 10000] : [1, 10, 50];
  const label = (value: number) => metric === "milliseconds" ? time(value) : number(value);
  const selectedDate = new Date(`${selected}T12:00:00`);
  const visibleWeek = weekStart(selectedDate);
  const weekDates = Array.from({ length: 7 }, (_, index) => new Date(visibleWeek.getFullYear(), visibleWeek.getMonth(), visibleWeek.getDate() + index));
  const shortDate = (date: Date) => date.toLocaleDateString(undefined, { month: "short", day: "numeric" });
  function moveWeek(offset: number) {
    const target = new Date(selectedDate.getFullYear(), selectedDate.getMonth(), selectedDate.getDate() + offset * 7);
    setSelected(localDay(new Date(Math.max(dates[0].getTime(), Math.min(now.getTime(), target.getTime())))));
  }
  return <section className="mobile-home">
    <h2>Home</h2>
    {error && <p role="alert">{error}</p>}
    <section className="mobile-home-panel">
      <div className="mobile-home-heading"><h2>Continue Reading</h2><button className="control" onClick={() => onLibrary()}>Library</button></div>
      {primary && <button className="control primary home-resume" disabled={busy} onClick={() => void open(primary)}>Resume · {primary.title}</button>}
      {recent.length ? recent.slice(0, 4).map(row) : <p>Open a library book to add it here.</p>}
      {recent.length > 4 && <button className="control" onClick={() => onLibrary()}>All recent books</button>}
    </section>
    <section className="mobile-home-panel mobile-activity-panel">
      <div className="mobile-home-heading"><h2>Reading activity</h2><select aria-label="Heatmap metric" value={metric} onChange={event => { const value = event.target.value as typeof metric; setMetric(value); localStorage.setItem("tmw-home-metric", value); }}>
        <option value="milliseconds">Reading time</option><option value="characters">Characters</option><option value="lookups">Words looked up</option>
      </select></div>
      <div className="mobile-week-heading">This week</div>
      <div className="mobile-week-stats">{[
        [time(weekly.reduce((sum, day) => sum + day.milliseconds, 0)), "Reading time"],
        [number(weekly.reduce((sum, day) => sum + day.characters, 0)), "Characters (est.)"],
        [number(weekly.reduce((sum, day) => sum + day.lookups, 0)), "Lookups"],
      ].map(([value, title]) => <div key={title}><strong>{value}</strong><span>{title}</span></div>)}</div>
      <div className="mobile-activity-facts">
        <span><b>{weekly.filter(day => day.milliseconds > 0).length}</b> reading days</span><span><b>{summary?.finished ?? 0}</b> marked finished</span>
        <span><b>{summary?.streak ?? 0}</b> <span>Current streak</span></span><span><b>{summary?.longestStreak ?? 0}</b> <span>Longest streak</span></span>
      </div>
      <div className="mobile-calendar-nav">
        <button className="control" aria-label="Previous week" disabled={localDay(visibleWeek) <= localDay(dates[0])} onClick={() => moveWeek(-1)}>‹</button>
        <span>{shortDate(weekDates[0])} – {shortDate(weekDates[6])}</span>
        <button className="control" disabled={selected === today} onClick={() => setSelected(today)}>Today</button>
        <button className="control" aria-label="Next week" disabled={localDay(weekDates[6]) >= today} onClick={() => moveWeek(1)}>›</button>
      </div>
      <div className="mobile-calendar-scroll" aria-label="Reading calendar, last 12 weeks">
        <div className="mobile-heatmap">{weekDates.map(date => {
          const key = localDay(date), value = totals.get(key)?.[metric] ?? 0;
          const level = value <= 0 ? 0 : value < thresholds[0] ? 1 : value < thresholds[1] ? 2 : value < thresholds[2] ? 3 : 4;
          return <div className="mobile-calendar-day" key={key}><span>{date.toLocaleDateString(undefined, { weekday: "short" })}</span><button className={`activity-cell activity-level-${level}`} disabled={key > today || key < localDay(dates[0])} aria-pressed={selected === key} aria-label={`${key}: ${label(value)}`} onClick={() => setSelected(key)}><span>{date.getDate()}</span></button></div>;
        })}</div>
      </div>
      <div className="mobile-day-detail" aria-live="polite"><h3>{selected === today ? "Today" : selectedDate.toLocaleDateString(undefined, { weekday: "short", month: "short", day: "numeric", year: "numeric" })}</h3><p>{time(day?.milliseconds ?? 0)} · {number(day?.characters ?? 0)} characters · {number(day?.lookups ?? 0)} lookups</p>
        {!!summary?.books.length && <details className="mobile-activity-details" key={selected}><summary>Books · {summary.books.length}</summary>{summary.books.map(book => <div key={book.id} className="home-day-book"><span lang="ja">{book.title}</span><small>{time(book.milliseconds)} · {number(book.characters)} characters · {number(book.lookups)} lookups</small></div>)}</details>}
      </div>
      {summary && !summary.enabled && <p className="mobile-activity-notice">Tracking paused</p>}
      {summary?.resetPending && <p className="mobile-activity-notice">Clear pending on PC</p>}
      <details className="mobile-activity-details mobile-tracking-controls"><summary>Tracking & sync{summary?.activitySync?.rejected ? ` · ${summary.activitySync.rejected} rejected` : summary?.activitySync?.pending ? ` · ${summary.activitySync.pending} pending` : ""}</summary>
      <small className="mobile-activity-sync">{summary?.syncMessage}{summary?.activitySync ? ` · ${summary.activitySync.pending} pending${summary.activitySync.rejected ? ` · ${summary.activitySync.rejected} rejected` : ""}` : ""}</small>
      <div className="mobile-tracking-actions"><button className="control" disabled={busy || !summary} onClick={() => void change()}>{summary?.enabled ? "Pause tracking" : "Resume tracking"}</button>
      <button className="control" disabled={busy || summary?.activitySync?.busy} onClick={() => {
        void activityNative({ action: "activitySync" }).then(() => setRevision(value => value + 1)).catch(reason => setError(String(reason)));
      }}>Sync activity now</button><button className="control mobile-clear-activity" disabled={busy} onClick={() => setConfirmClear(true)}>Clear activity</button></div>
      {confirmClear && <div role="alertdialog" aria-label="Clear reading activity" className="home-clear-confirm">
        <p>Clear activity on both devices? Old queued activity will be discarded. Books, downloads, positions, passages and lookup history are kept. Export phone data in Settings before clearing.</p>
        <button className="control" disabled={busy} onClick={() => void change(true)}>Clear reading activity</button><button className="control" onClick={() => setConfirmClear(false)}>Cancel</button>
      </div>}
      </details>
    </section>
    <section className="mobile-home-panel"><div className="mobile-home-heading"><h2>Want to read</h2><button className="control" onClick={() => onLibrary("want")}>View shelf</button></div>{want.length ? want.map(row) : <p>No books marked Want to read.</p>}</section>
  </section>;
}
