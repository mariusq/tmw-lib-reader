import { useEffect, useState } from "react";
import { GeneratedCover } from "./GeneratedCover";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { calendarDays, localDay, weekStart } from "../../packages/reader-core/readingActivity";
import { ContinueReading } from "./ContinueReading";
import { EpubReader } from "../features/reader/EpubReader";

type Activity = {
  enabled: boolean;
  days: { day: string; milliseconds: number; characters: number; lookups: number }[];
  books: { id: number; title: string; milliseconds: number; characters: number; lookups?: number }[];
  finished: number;
  streak: number;
  longestStreak: number;
};
type NextBook = {
  id: number;
  effectiveTitle: string;
  effectiveCreator: string;
  effectiveCoverPath: string | null;
  isAvailable: boolean;
};
function activityTime(milliseconds: number, ja: boolean): string {
  if (milliseconds > 0 && milliseconds < 60000) return ja ? "1分未満" : "<1 min";
  const minutes = Math.floor(milliseconds / 60000);
  return minutes >= 60
    ? ja
      ? `${Math.floor(minutes / 60)}時間 ${minutes % 60}分`
      : `${Math.floor(minutes / 60)}h ${minutes % 60}m`
    : `${minutes}${ja ? "分" : " min"}`;
}
export function ReadingHome({
  language,
  showCovers,
  onLibrary,
  onRoots,
  onWant,
}: {
  language: "ja" | "en";
  showCovers: boolean;
  onLibrary: () => void;
  onRoots: () => void;
  onWant: () => void;
}) {
  const ja = language === "ja";
  const [now, setNow] = useState(() => new Date());
  const dates = calendarDays(now);
  const today = localDay(now),
    first = localDay(dates[0]),
    monday = weekStart(now),
    mondayKey = localDay(monday);
  const [selected, setSelected] = useState(today);
  const [metric, setMetric] = useState<"time" | "characters" | "lookups">("time");
  const [activity, setActivity] = useState<Activity | null>(null);
  const [next, setNext] = useState<NextBook[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);
  const [pending, setPending] = useState(false);
  const [confirmClear, setConfirmClear] = useState(false);
  const [readerId, setReaderId] = useState<number | null>(null);
  const [loadedDay, setLoadedDay] = useState<string | null>(null);
  const dayLoading = loadedDay !== selected;
  useEffect(() => {
    const timer = setInterval(() => setNow(new Date()), 60000);
    return () => clearInterval(timer);
  }, []);
  useEffect(() => {
    const timer = setInterval(() => {
      if (readerId === null && document.visibilityState === "visible") setRevision(value => value + 1);
    }, 5000);
    return () => clearInterval(timer);
  }, [readerId]);
  useEffect(() => {
    let disposed = false;
    invoke<NextBook[]>("browse_books", {
      request: {
        readingStatus: "want",
        query: "",
        sort: "dateAdded",
        offset: 0,
        limit: 6,
        needsMetadata: false,
        hideDuplicateTitles: false,
        duplicateFiltering: "off",
      },
    })
      .then((books) => {
        if (!disposed) setNext(books);
      })
      .catch((reason) => {
        if (!disposed) setError(String(reason));
      });
    return () => {
      disposed = true;
    };
  }, [revision]);
  useEffect(() => {
    let disposed = false;
    invoke<Activity>("get_reading_activity", {
      startDay: first,
      endDay: today,
      selectedDay: selected,
      weekStart: Math.floor(monday.getTime() / 1000),
    })
      .then((result) => {
        if (!disposed) {
          setActivity(result);
          setError(null);
        }
      })
      .catch((reason) => {
        if (!disposed) setError(String(reason));
      })
      .finally(() => {
        if (!disposed) setLoadedDay(selected);
      });
    return () => {
      disposed = true;
    };
    // Calendar boundaries, rather than every minute, determine the request.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [first, today, selected, mondayKey, revision]);
  async function changeActivity(clear = false) {
    setPending(true);
    try {
      if (clear) await invoke("clear_reading_activity");
      else
        await invoke("set_app_setting", {
          key: "reading_activity_enabled",
          value: String(!activity?.enabled),
        });
      setConfirmClear(false);
      setRevision((value) => value + 1);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setPending(false);
    }
  }
  const valueOf = (day: Activity["days"][number]) =>
    metric === "time"
      ? day.milliseconds
      : metric === "characters"
        ? (day.characters ?? 0)
        : (day.lookups ?? 0);
  const totals = new Map(activity?.days.map((day) => [day.day, valueOf(day)]) ?? []);
  const selectedActivity = activity?.days.find((day) => day.day === selected);
  const number = (value: number) => new Intl.NumberFormat(ja ? "ja-JP" : "en").format(value);
  const metricLabel = (value: number) =>
    metric === "time"
      ? activityTime(value, ja)
      : `${number(value)} ${metric === "characters" ? (ja ? "文字" : "characters") : ja ? "回の検索" : "lookups"}`;
  const weekly = activity?.days.filter((day) => day.day >= mondayKey) ?? [];
  const padding = (dates[0].getDay() + 6) % 7;
  const dateLabel = (date: Date) =>
    new Intl.DateTimeFormat(ja ? "ja-JP" : "en", { dateStyle: "long" }).format(date);
  const thresholds =
    metric === "time"
      ? [15 * 60000, 30 * 60000, 60 * 60000]
      : metric === "characters"
        ? [500, 1000, 2000]
        : [3, 6, 10];
  const level = (value: number) =>
    value === 0
      ? 0
      : value < thresholds[0]
        ? 1
        : value < thresholds[1]
          ? 2
          : value < thresholds[2]
            ? 3
            : 4;
  return (
    <section className="reading-home">
      <header className="home-welcome">
        <div>
          <h1>{ja ? "ホーム" : "Home"}</h1>
        </div>
        <p className="text-sm text-stone-400">{dateLabel(now)}</p>
      </header>
      {error && (
        <div
          role="alert"
          className="flex items-center justify-between gap-3 rounded-xl border border-red-400/30 p-3 text-red-200"
        >
          {error}
          <button
            onClick={() => {
              setError(null);
              setRevision((value) => value + 1);
            }}
          >
            {ja ? "再試行" : "Retry"}
          </button>
        </div>
      )}
      <div className="home-panel">
        <ContinueReading
          language={language}
          showCovers={showCovers}
          compact
          onRefresh={() => setRevision((value) => value + 1)}
          onLibrary={onLibrary}
          onRoots={onRoots}
        />
      </div>
      <div className="home-activity-layout">
        <section
          className="home-panel min-w-0"
          aria-label={ja ? "読書カレンダー" : "Reading calendar"}
        >
          <div className="home-section-heading">
            <div>
              <h2>{ja ? "読書カレンダー" : "Reading calendar"}</h2>
            </div>
            <div className="home-calendar-controls">
              {" "}
              <label className="home-metric-control text-sm text-stone-300">
                <span className="sr-only">{ja ? "表示する記録" : "Heatmap metric"}</span>
                <select
                  className="control"
                  style={{ width: "auto", maxWidth: "100%" }}
                  value={metric}
                  onChange={(event) => setMetric(event.target.value as typeof metric)}
                >
                  <option value="time">{ja ? "読書時間" : "Reading time"}</option>
                  <option value="characters">
                    {ja ? "読んだ文字数（推定）" : "Characters read (estimated)"}
                  </option>
                  <option value="lookups">{ja ? "単語の検索回数" : "Words looked up"}</option>
                </select>
              </label>
              <span className="home-pill">{ja ? "過去52週間" : "Past 52 weeks"}</span>
            </div>
          </div>
          {!activity ? (
            <p role="status">{ja ? "読み込み中…" : "Loading activity…"}</p>
          ) : (
            <>
              <div className="home-calendar-scroll">
                <div className="home-calendar">
                  <div className="home-calendar-labels" aria-hidden="true">
                    {(ja
                      ? ["月", "", "水", "", "金", "", "日"]
                      : ["Mon", "", "Wed", "", "Fri", "", "Sun"]
                    ).map((label, i) => (
                      <span key={i}>{label}</span>
                    ))}
                  </div>
                  <div>
                    <div className="home-months" aria-hidden="true">
                      {Array.from(
                        { length: Math.ceil((dates.length + padding) / 7) },
                        (_, column) => {
                          const date = dates[Math.max(0, column * 7 - padding)];
                          return (
                            <span key={column}>
                              {date && (column === 0 || date.getDate() <= 7)
                                ? new Intl.DateTimeFormat(ja ? "ja-JP" : "en", {
                                    month: "short",
                                  }).format(date)
                                : ""}
                            </span>
                          );
                        },
                      )}
                    </div>
                    <div
                      className="home-heatmap"
                      aria-label={ja ? "日ごとの読書記録" : "Daily reading activity"}
                    >
                      {Array.from({ length: padding }, (_, i) => (
                        <span key={`pad-${i}`} />
                      ))}
                      {dates.map((date, index) => {
                        const day = localDay(date),
                          ms = totals.get(day) ?? 0;
                        const label = `${dateLabel(date)}: ${metricLabel(ms)}`;
                        return (
                          <button
                            key={day}
                            className={`heatmap-day heatmap-level-${level(ms)}`}
                            aria-label={label}
                            title={label}
                            aria-pressed={selected === day}
                            tabIndex={selected === day ? 0 : -1}
                            onClick={() => setSelected(day)}
                            onKeyDown={(event) => {
                              const offset = (
                                {
                                  ArrowUp: -1,
                                  ArrowDown: 1,
                                  ArrowLeft: -7,
                                  ArrowRight: 7,
                                  Home: -index,
                                  End: dates.length - index - 1,
                                } as Record<string, number>
                              )[event.key];
                              if (offset === undefined) return;
                              event.preventDefault();
                              const target = Math.max(
                                0,
                                Math.min(dates.length - 1, index + offset),
                              );
                              setSelected(localDay(dates[target]));
                              const buttons =
                                event.currentTarget.parentElement?.querySelectorAll<HTMLButtonElement>(
                                  "button",
                                );
                              buttons?.[target].focus();
                            }}
                          />
                        );
                      })}
                    </div>
                  </div>
                </div>
              </div>
              <div className="mt-3 flex items-center justify-between gap-3 text-xs text-stone-400">
                <span>{ja ? "PC・スマートフォン" : "PC + phone"}</span>
                <div className="flex items-center gap-1.5">
                  <span>{ja ? "少" : "Less"}</span>
                  {[0, 1, 2, 3, 4].map((value) => (
                    <span
                      key={value}
                      className={`heatmap-day heatmap-level-${value}`}
                      title={
                        value === 0
                          ? metricLabel(0)
                          : value === 4
                            ? `${metricLabel(thresholds[2])}+`
                            : `<${metricLabel(thresholds[value - 1])}`
                      }
                    />
                  ))}
                  <span>{ja ? "多" : "More"}</span>
                </div>
              </div>
              <div className="home-day-detail" aria-live="polite" aria-busy={dayLoading}>
                <h3 className="text-sm font-medium">
                  {dateLabel(new Date(`${selected}T00:00:00`))} · {activityTime(selectedActivity?.milliseconds ?? 0, ja)}
                </h3>
                <p className="mt-2 text-xs text-stone-400">
                  {ja ? "読んだ文字数（推定）" : "Characters read (estimated)"}:{" "}
                  {number(selectedActivity?.characters ?? 0)} ·{" "}
                  {ja ? "単語の検索回数" : "Words looked up"}:{" "}
                  {number(selectedActivity?.lookups ?? 0)}
                </p>
                {dayLoading ? (
                  <p className="text-sm text-stone-400">{ja ? "読み込み中…" : "Loading day…"}</p>
                ) : activity.books.length === 0 ? (
                  <p className="mt-2 text-sm text-stone-400">
                    {ja
                      ? "この日の記録はまだありません。"
                      : "No recorded reading for this day yet."}
                  </p>
                ) : (
                  <ul className="mt-2 space-y-2">
                    {activity.books.map((book) => (
                      <li key={book.id} className="flex justify-between gap-4 text-sm">
                        <span className="min-w-0 break-words">{book.title}</span>
                        <span className="shrink-0 text-amber-200">
                          {activityTime(book.milliseconds, ja)} · {number(book.characters ?? 0)}{" "}
                          {ja ? "文字" : "characters"} · {number(book.lookups ?? 0)} {ja ? "回の検索" : "lookups"}
                        </span>
                      </li>
                    ))}
                  </ul>
                )}
              </div>
              <div className="mt-4 flex flex-wrap items-center gap-3 text-xs">
                <button
                  disabled={pending}
                  className="home-text-button"
                  onClick={() => void changeActivity()}
                >
                  {activity.enabled
                    ? ja
                      ? "記録を一時停止"
                      : "Pause tracking"
                    : ja
                      ? "記録を再開"
                      : "Resume tracking"}
                </button>
                {!activity.enabled && (
                  <span className="text-stone-500">
                    {ja ? "記録は一時停止中" : "Tracking is paused"}
                  </span>
                )}
                <button
                  disabled={pending}
                  className="home-text-button ml-auto"
                  onClick={() => setConfirmClear(true)}
                >
                  {ja ? "記録を消去" : "Clear activity"}
                </button>
              </div>
              {confirmClear && (
                <div className="mt-3 rounded-lg border border-white/15 p-3 text-sm">
                  <p>
                    {ja
                      ? "読書時間・文字数・検索回数・連続読書日数を消去します。本・読書位置・ノートは保持されます。カタログのバックアップから復元できます。"
                      : "Clear reading activity on both devices (time, character counts, lookup counts, and streaks)? Old queued activity will be discarded. Books, positions, and notes are kept. Before clearing, export a catalog backup in Settings → Backup & restore to a folder of your choice."}
                  </p>
                  <div className="mt-3 flex gap-4">
                    <button
                      disabled={pending}
                      className="text-red-300"
                      onClick={() => void changeActivity(true)}
                    >
                      {ja ? "消去する" : "Clear reading activity"}
                    </button>
                    <button onClick={() => setConfirmClear(false)}>
                      {ja ? "キャンセル" : "Cancel"}
                    </button>
                  </div>
                </div>
              )}
            </>
          )}
        </section>
        <aside className="home-panel home-week">
          <p className="home-eyebrow">{ja ? "月曜日から今日まで" : "MONDAY TO TODAY"}</p>
          <h2>{ja ? "今週の読書" : "This week"}</h2>
          <div className="home-week-stat">
            <strong>
              {activity
                ? activityTime(
                    weekly.reduce((sum, day) => sum + day.milliseconds, 0),
                    ja,
                  )
                : "—"}
            </strong>
            <span>{ja ? "読書時間" : "Time spent reading"}</span>
          </div>
          <div className="home-week-stat">
            <strong>
              {activity ? number(weekly.reduce((sum, day) => sum + (day.characters ?? 0), 0)) : "—"}
            </strong>
            <span>{ja ? "読んだ文字数（推定）" : "Characters read (estimated)"}</span>
          </div>
          <div className="home-week-stat">
            <strong>
              {activity ? number(weekly.reduce((sum, day) => sum + (day.lookups ?? 0), 0)) : "—"}
            </strong>
            <span>{ja ? "単語の検索回数" : "Words looked up"}</span>
          </div>
          <div className="home-week-stat">
            <strong>
              {activity ? weekly.filter((day) => day.milliseconds > 0).length : "—"}
              <small> / 7</small>
            </strong>
            <span>{ja ? "読書した日" : "Days with reading"}</span>
          </div>
          <div className="home-week-stat">
            <strong>{activity?.finished ?? "—"}</strong>
            <span>{ja ? "読了した本" : "Books marked finished"}</span>
          </div>
          <section className="home-streak" aria-label={ja ? "連続読書日数" : "Reading streak"}>
            <div>
              <h2>{ja ? "連続読書日数" : "Reading streak"}</h2>
            </div>
            <div>
              <strong className="text-2xl text-amber-200">{activity?.streak ?? "—"}</strong>
              <span className="ml-2 text-sm text-stone-400">
                {ja ? "日 · 現在" : "days · current"}
              </span>
            </div>
            <div>
              <strong className="text-2xl text-amber-200">{activity?.longestStreak ?? "—"}</strong>
              <span className="ml-2 text-sm text-stone-400">
                {ja ? "日 · 最長" : "days · longest"}
              </span>
            </div>
          </section>
        </aside>
      </div>
      <section className="home-panel">
        <div className="home-section-heading">
          <div>
            <h2>{ja ? "読みたい本" : "Want to read"}</h2>
          </div>
          <button className="home-text-button text-sm" onClick={onWant}>
            {ja ? "すべて見る →" : "View shelf →"}
          </button>
        </div>
        {next.length === 0 ? (
          <p className="text-sm text-stone-400">
            {ja
              ? "ライブラリで「読みたい」に設定した本がここに表示されます。"
              : "Books marked Want to read in your library appear here."}
          </p>
        ) : (
          <div className="home-next-grid">
            {next.map((book) => (
              <button
                key={book.id}
                className="home-next-book"
                disabled={!book.isAvailable}
                onClick={() => setReaderId(book.id)}
                aria-label={`${ja ? "開く" : "Open"}: ${book.effectiveTitle}`}
              >
                <NextCover path={showCovers ? book.effectiveCoverPath : null} title={book.effectiveTitle} />
                <div className="min-w-0">
                  <h3 className="line-clamp-2 text-sm font-medium">{book.effectiveTitle}</h3>
                  <p className="mt-1 truncate text-xs text-stone-400">{book.effectiveCreator}</p>
                  <p className="mt-3 text-xs text-amber-200">
                    {book.isAvailable
                      ? ja
                        ? "読み始める →"
                        : "Start reading →"
                      : ja
                        ? "ソースが見つかりません"
                        : "Source unavailable"}
                  </p>
                </div>
              </button>
            ))}
          </div>
        )}
      </section>
      {readerId !== null && (
        <EpubReader
          bookId={readerId}
          onClose={() => {
            setReaderId(null);
            setRevision((value) => value + 1);
          }}
        />
      )}
    </section>
  );
}
function NextCover({ path, title }: { path: string | null; title: string }) {
  const [broken, setBroken] = useState(false);
  return (
    <div className="home-next-cover">
      {path && !broken ? (
        <img
          src={convertFileSrc(path)}
          alt=""
          loading="lazy"
          className="h-full w-full object-cover"
          onError={() => setBroken(true)}
        />
      ) : (
        <GeneratedCover title={title} />
      )}
    </div>
  );
}
