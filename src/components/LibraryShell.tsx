import { YomitanNotices } from "./YomitanNotices";
import { CompanionSettings } from "./CompanionSettings";
import DictionaryManager from "../../packages/reader-core/DictionaryManager";
import { SmartShelves, type ShelfFilter } from "./SmartShelves";
import { readingStatuses } from "../features/reader/readingStatuses";
import { useCallback, useEffect, useRef, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { BookDetailsPanel } from "./BookDetailsPanel";
import { ContinueReading } from "./ContinueReading";
import { BackupControls } from "./BackupControls";
import { SavedPassages } from "./SavedPassages";
import { LookupHistory } from "./LookupHistory";
import { EpubReader } from "../features/reader/EpubReader";

const navigation = [
  ["home", "続きを読む", "Continue Reading"],
  ["all", "ライブラリ", "Library"],
  ["passages", "読書ノート", "Reading Notes"],
  ["collections", "コレクション", "Collections"],
  ["settings", "設定", "Settings"],
] as const;
const libraryTabs = [["all", "すべての本", "All Books"], ["recent", "最近追加", "Recently Added"], ["attention", "要確認のメタデータ", "Metadata Needs Attention"]] as const;
const noteTabs = [["passages", "保存した文章", "Saved passages"], ["history", "調べた言葉", "Lookup history"]] as const;
const organizationTabs = [["collections", "コレクション", "Collections"], ["tags", "タグ", "Tags"]] as const;
const settingsTabs = [["settings", "一般", "General"], ["roots", "ライブラリルート", "Library Roots"]] as const;
const words = {
  library: ["ライブラリ", "Library"], grid: ["グリッド", "Grid"], list: ["リスト", "List"],
  unknownAuthor: ["著者不明", "Unknown author"], sort: ["並べ替え", "Sort"], rootFilter: ["ライブラリルートで絞り込み", "Filter by library root"], tagFilter: ["タグで絞り込み", "Filter by tag"], collectionFilter: ["コレクションで絞り込み", "Filter by collection"],
  allRoots: ["すべてのルート", "All roots"], allTags: ["すべてのタグ", "All tags"], allCollections: ["すべてのコレクション", "All collections"], attentionOnly: ["要確認のみ", "Needs attention only"], hideDuplicateTitles: ["同名の重複を隠す", "Hide duplicate titles"],
  titleSort: ["タイトル順", "Title"], authorSort: ["著者順", "Author"], seriesSort: ["シリーズ順", "Series"], addedSort: ["追加日順", "Date added"], modifiedSort: ["更新日順", "File modified"], folderSort: ["フォルダー順", "Folder"],
  search: ["タイトル・著者・フォルダーを検索", "Search title, author, folder…"],
  noMatches: ["条件に一致する本はありません。", "No books match these filters."], addFolderPrompt: ["フォルダーを追加して EPUB ライブラリを読み込みましょう。", "Add a folder to start cataloging your EPUB library."],
  addFolder: ["フォルダーを追加", "Add folder"], readOnly: ["選んだフォルダーは読み取り専用でスキャンします。", "Selected folders are scanned read-only."], scanning: ["スキャン中", "Scanning"], cancel: ["キャンセル", "Cancel"], rescan: ["再スキャン", "Rescan"], removeCatalog: ["カタログから削除", "Remove from catalog"], noRoots: ["まだライブラリルートがありません。", "No library roots yet."],
  coverCache: ["カバーキャッシュ", "Cover cache"], cacheNote: ["EPUB は変更しません。抽出したカバーだけを、ライブラリとは別のフォルダーへ保存します。", "EPUB files are never changed. Extracted covers are saved separately from your library."], currentLocation: ["現在の場所", "Current location"], loading: ["読み込み中…", "Loading…"], changeLocation: ["保存先を変更", "Change location"], regenerateCovers: ["カバーキャッシュを再生成", "Regenerate cover cache"], cacheSafe: ["このキャッシュは再生成できます。削除してもカタログや元の EPUB は失われません。", "This cache can be regenerated. Deleting it never removes catalog data or source EPUBs."], language: ["言語", "Language"], booksShown: ["冊を表示", "books shown"],
} as const;
const PAGE_SIZE = 80;
type Root = { id: number; path: string; displayName: string; lastScannedAt: number | null; bookCount: number };
type Book = { id: number; fileName: string; parentFolderPath: string; effectiveTitle: string; effectiveCreator: string; effectiveSeries: string; effectiveVolume: string; effectiveCoverPath: string | null; isAvailable: boolean; isFinished: boolean };
type Choice = [number, string]; type Sort = "title" | "author" | "series" | "dateAdded" | "modified" | "folder";
type Progress = { scanId: string; rootId?: number; stage?: "discovery" | "extraction" | "indexing"; discoveredCount: number; changedCount: number; extractedCount?: number; indexedCount?: number; currentPath: string };
type IndexProgress = { rebuildId: string; completed: number; total: number };
const time = (value: number | null, language: "ja" | "en") => value ? new Intl.DateTimeFormat(language === "ja" ? "ja-JP" : "en", { dateStyle: "medium", timeStyle: "short" }).format(new Date(value * 1000)) : language === "ja" ? "未スキャン" : "Not scanned";

function FinishedMark({ language }: { language: "ja" | "en" }) {
  const label = language === "ja" ? "読了" : "Finished";
  return <span role="img" aria-label={label} title={label} className="inline-flex size-6 shrink-0 items-center justify-center rounded-full bg-emerald-400 text-sm font-bold text-stone-950">✓</span>;
}

function Cover({ book, language, showCovers }: { book: Book; language: "ja" | "en"; showCovers: boolean }) {
  const [broken, setBroken] = useState(false);
  if (showCovers && book.effectiveCoverPath && !broken) return <img className="h-full w-full object-cover" src={convertFileSrc(book.effectiveCoverPath)} alt="" loading="lazy" decoding="async" onError={() => setBroken(true)} />;
  return <div className="grid h-full w-full place-items-center bg-gradient-to-br from-amber-300/30 to-stone-800 text-3xl text-amber-100/70">{language === "ja" ? "本" : "Book"}</div>;
}

export function LibraryShell() {
  const [duplicateFiltering, setDuplicateFiltering] = useState(() => {
    const saved = localStorage.getItem("tmw-duplicate-filtering");
    return saved === "off" || saved === "all" ? saved : "high";
  });
  const [showCovers, setShowCovers] = useState(() => localStorage.getItem("tmw-show-covers") !== "false");
  const [readingStatus, setReadingStatus] = useState<string>();
  const [passageReader, setPassageReader] = useState<{ bookId: number; cfi: string } | null>(null);
  const [roots, setRoots] = useState<Root[]>([]), [books, setBooks] = useState<Book[]>([]), [tags, setTags] = useState<Choice[]>([]), [collections, setCollections] = useState<Choice[]>([]);
  const [language, setLanguage] = useState<"ja" | "en">(() => localStorage.getItem("tmw-interface-language") === "en" ? "en" : "ja");
  const [active, setActive] = useState("home"), [layout, setLayout] = useState<"grid" | "list">("grid"), [sort, setSort] = useState<Sort>("title"), [rootId, setRootId] = useState<number>(), [tagId, setTagId] = useState<number>(), [collectionId, setCollectionId] = useState<number>(), [needsMetadata, setNeedsMetadata] = useState(false), [hideDuplicateTitles, setHideDuplicateTitles] = useState(false), [searchInput, setSearchInput] = useState(""), [query, setQuery] = useState("");
  const [progress, setProgress] = useState<Progress | null>(null), [error, setError] = useState<string | null>(null), [cache, setCache] = useState<string | null>(null), [hasMore, setHasMore] = useState(false);
  const [indexProgress, setIndexProgress] = useState<IndexProgress | null>(null);
  const [detailBookId, setDetailBookId] = useState<number | null>(null), [selectedBookIds, setSelectedBookIds] = useState<number[]>([]), [batchTags, setBatchTags] = useState("");
  const sentinel = useRef<HTMLDivElement>(null);
  const catalogRefreshTimer = useRef<number | null>(null);
  const loadGeneration = useRef(0);
  const pageLoading = useRef(false);
  const t = (key: keyof typeof words) => words[key][language === "ja" ? 0 : 1];
  const switchLanguage = () => setLanguage(current => { const next = current === "ja" ? "en" : "ja"; localStorage.setItem("tmw-interface-language", next); return next; });
  const refreshRoots = useCallback(async () => setRoots(await invoke<Root[]>("list_library_roots")), []);
  const load = useCallback(async (offset = 0) => {
    if (!["all", "recent", "attention", "collections", "tags"].includes(active)) { ++loadGeneration.current; return; }
    if (offset > 0 && pageLoading.current) return;
    if (offset > 0) pageLoading.current = true;
    const generation = offset === 0 ? ++loadGeneration.current : loadGeneration.current;
    try {
      const rows = await invoke<Book[]>("browse_books", { request: { readingStatus, libraryRootId: rootId, tagId, collectionId, needsMetadata, hideDuplicateTitles, duplicateFiltering: hideDuplicateTitles ? duplicateFiltering : "off", query, sort, offset, limit: PAGE_SIZE } });
      if (generation !== loadGeneration.current) return;
      setBooks(old => offset ? [...old, ...rows] : rows); setHasMore(rows.length === PAGE_SIZE);
    } finally {
      if (offset > 0) pageLoading.current = false;
    }
  }, [active, collectionId, duplicateFiltering, hideDuplicateTitles, needsMetadata, query, readingStatus, rootId, sort, tagId]);
  // These asynchronous catalog reads are the initial synchronization with the Tauri backend.
  // eslint-disable-next-line react-hooks/set-state-in-effect
  useEffect(() => { void refreshRoots().catch(reason => setError(String(reason))); void invoke<Choice[]>("list_tags").then(setTags).catch(reason => setError(String(reason))); void invoke<Choice[]>("list_collections").then(setCollections).catch(reason => setError(String(reason))); void invoke<string>("get_cover_cache_directory").then(setCache).catch(reason => setError(String(reason))); }, [refreshRoots]);
  useEffect(() => { void load().catch(reason => setError(String(reason))); }, [load]);
  useEffect(() => { const timer = window.setTimeout(() => setQuery(searchInput), 300); return () => window.clearTimeout(timer); }, [searchInput]);
  useEffect(() => { const observer = new IntersectionObserver(entries => { if (entries[0]?.isIntersecting && hasMore) void load(books.length).catch(reason => setError(String(reason))); }, { rootMargin: "600px" }); if (sentinel.current) observer.observe(sentinel.current); return () => observer.disconnect(); }, [books.length, hasMore, load]);
  useEffect(() => {
    const refreshCatalogSoon = () => {
      if (catalogRefreshTimer.current !== null) return;
      catalogRefreshTimer.current = window.setTimeout(() => {
        catalogRefreshTimer.current = null;
        void load();
      }, 1_000);
    };
    const pending = Promise.all([
      listen<Progress>("scan-progress", ({ payload }) => setProgress(payload)),
      listen("scan-catalog-updated", refreshCatalogSoon),
      listen<{ scanId: string }>("scan-completed", ({ payload }) => { setProgress(current => current?.scanId === payload.scanId ? null : current); void refreshRoots(); void load(); }),
      listen<{ scanId: string; message: string }>("scan-failed", ({ payload }) => { setProgress(current => current?.scanId === payload.scanId ? null : current); setError(payload.message); }),
    ]);
    return () => {
      if (catalogRefreshTimer.current !== null) window.clearTimeout(catalogRefreshTimer.current);
      void pending.then(all => all.forEach(stop => stop()));
    };
  }, [load, refreshRoots]);
  useEffect(() => { const pending = listen<IndexProgress>("search-index-progress", ({ payload }) => setIndexProgress(payload)); return () => { void pending.then(stop => stop()); }; }, []);
  const add = async () => { const path = await invoke<string | null>("choose_library_folder"); if (!path) return; try { await invoke("add_library_root", { path }); await refreshRoots(); setActive("roots"); } catch (reason) { setError(String(reason)); } };
  const rescan = async (root: Root) => { const scanId = crypto.randomUUID(); setProgress({ scanId, stage: "discovery", discoveredCount: 0, changedCount: 0, extractedCount: 0, indexedCount: 0, currentPath: root.path }); try { await invoke("rescan_library_root", { rootId: root.id, scanId }); } catch (reason) { setError(String(reason)); setProgress(null); } };
  const remove = async (root: Root) => { if (!window.confirm(language === "ja" ? `「${root.displayName}」をカタログから削除します。EPUB ファイルは変更されません。` : `Remove “${root.displayName}” from the catalog? EPUB files will not be changed.`)) return; await invoke("remove_library_root", { rootId: root.id }); await refreshRoots(); void load(); };
  const rebuildIndex = async () => { const rebuildId = crypto.randomUUID(); setIndexProgress({ rebuildId, completed: 0, total: 0 }); try { await invoke("rebuild_search_index", { rebuildId }); void load(); } catch (reason) { setError(String(reason)); } finally { setIndexProgress(null); } };
  const select = (item: string) => {
    setActive(item);
    if (["all", "recent", "attention"].includes(item)) {
      setNeedsMetadata(item === "attention");
      if (item === "recent") setSort("dateAdded");
      else if (active === "recent") setSort("title");
    }
  };
  const activeGroup = libraryTabs.some(([id]) => id === active) ? "all" : noteTabs.some(([id]) => id === active) ? "passages" : organizationTabs.some(([id]) => id === active) ? "collections" : active === "roots" ? "settings" : active;
  const tabs = activeGroup === "all" ? libraryTabs : activeGroup === "passages" ? noteTabs : activeGroup === "collections" ? organizationTabs : activeGroup === "settings" ? settingsTabs : [];
  const viewTabs = <div className="mb-6 flex flex-wrap gap-2" aria-label={language === "ja" ? "表示切り替え" : "View options"}>{tabs.map(([id, japanese, english]) => <button key={id} aria-pressed={active === id} className={`rounded-lg border px-3 py-2 text-sm ${active === id ? "border-amber-300/30 bg-amber-400/15 text-amber-200" : "border-white/10 text-stone-300 hover:bg-white/5"}`} onClick={() => select(id)}>{language === "ja" ? japanese : english}</button>)}</div>;
  const viewTitle = activeGroup === "collections" ? organizationTabs.find(([id]) => id === active) : libraryTabs.find(([id]) => id === (needsMetadata ? "attention" : active));
  const smartShelves = <SmartShelves filter={{ readingStatus, libraryRootId: rootId, tagId, collectionId, needsMetadata, hideDuplicateTitles, query: searchInput, sort, offset: 0, limit: PAGE_SIZE }} onSelect={(filter: ShelfFilter) => {
      setReadingStatus(filter.readingStatus ?? undefined); setRootId(filter.libraryRootId ?? undefined); setTagId(filter.tagId ?? undefined); setCollectionId(filter.collectionId ?? undefined);
      setNeedsMetadata(filter.needsMetadata); setHideDuplicateTitles(filter.hideDuplicateTitles); setSearchInput(filter.query); setQuery(filter.query); setSort(filter.sort as Sort);
    }} />;
  const filterCount = [rootId, tagId, collectionId, hideDuplicateTitles].filter(Boolean).length;
  const filters = <>
    <div className="mb-3 flex flex-wrap items-center gap-3">
      <div className="min-w-48 flex-1"><input type="search" className="control xl:col-span-2" value={searchInput} onChange={e => setSearchInput(e.target.value)} placeholder={t("search")} aria-label={t("search")} /></div>
      <select aria-label={language === "ja" ? "読書状態で絞り込み" : "Filter by reading status"} className="control w-auto" value={readingStatus ?? ""} onChange={e => setReadingStatus(e.target.value || undefined)}>
        <option value="">{language === "ja" ? "すべての状態" : "All statuses"}</option>
        {readingStatuses.map(([value, english, japanese]) => <option key={value} value={value}>{language === "ja" ? japanese : english}</option>)}
      </select>
      <div className="w-full sm:w-44"><select aria-label={t("sort")} className="control" value={sort} onChange={e => setSort(e.target.value as Sort)}><option value="title">{t("titleSort")}</option><option value="author">{t("authorSort")}</option><option value="series">{t("seriesSort")}</option><option value="dateAdded">{t("addedSort")}</option><option value="modified">{t("modifiedSort")}</option><option value="folder">{t("folderSort")}</option></select></div>
    </div>
    <div className="mb-5 flex flex-wrap items-start gap-3">
      <details className="min-w-0 flex-1 rounded-lg border border-white/10">
        <summary className="cursor-pointer px-3 py-2 text-sm text-stone-300">{language === "ja" ? "絞り込み" : "Filters"}{filterCount > 0 && <span className="ml-2 rounded bg-amber-400/15 px-2 py-0.5 text-amber-200">{filterCount}</span>}</summary>
        <div className="grid gap-3 border-t border-white/10 p-3 sm:grid-cols-2 lg:grid-cols-3"><select aria-label={t("rootFilter")} className="control" value={rootId ?? ""} onChange={e => setRootId(e.target.value ? Number(e.target.value) : undefined)}><option value="">{t("allRoots")}</option>{roots.map(r => <option key={r.id} value={r.id}>{r.displayName}</option>)}</select><select aria-label={t("tagFilter")} className="control" value={tagId ?? ""} onChange={e => setTagId(e.target.value ? Number(e.target.value) : undefined)}><option value="">{t("allTags")}</option>{tags.map(([id, name]) => <option key={id} value={id}>{name}</option>)}</select><select aria-label={t("collectionFilter")} className="control" value={collectionId ?? ""} onChange={e => setCollectionId(e.target.value ? Number(e.target.value) : undefined)}><option value="">{t("allCollections")}</option>{collections.map(([id, name]) => <option key={id} value={id}>{name}</option>)}</select><label className="flex items-center gap-2 rounded-lg border border-white/10 bg-stone-900 px-3 py-2 text-sm"><input type="checkbox" checked={hideDuplicateTitles} onChange={e => setHideDuplicateTitles(e.target.checked)} />{t("hideDuplicateTitles")}</label></div>
        {filterCount > 0 && <button className="m-3 mt-0 text-sm text-amber-300 underline" onClick={() => { setRootId(undefined); setTagId(undefined); setCollectionId(undefined); setHideDuplicateTitles(false); }}>{language === "ja" ? "絞り込みを解除" : "Clear filters"}</button>}
      </details>
      <details className="min-w-0 basis-full rounded-lg border border-white/10">
        <summary className="cursor-pointer px-3 py-2 text-sm text-stone-300">{language === "ja" ? "保存した棚" : "Saved shelves"}</summary>
        <div className="border-t border-white/10 p-3">{smartShelves}</div>
      </details>
    </div>
  </>;
  const browser = <section className="w-full self-start">
    <div className="mb-6 flex flex-wrap items-end justify-between gap-4"><div><h1 className="mt-1 text-3xl font-semibold">{viewTitle?.[language === "ja" ? 1 : 2]}</h1><p className="mt-1 text-sm text-stone-400">{books.length}{hasMore ? "+" : ""} {t("booksShown")}</p></div><div className="flex rounded-lg border border-white/10 p-1"><button aria-pressed={layout === "grid"} className="rounded px-3 py-2 text-sm" onClick={() => setLayout("grid")}>{t("grid")}</button><button aria-pressed={layout === "list"} className="rounded px-3 py-2 text-sm" onClick={() => setLayout("list")}>{t("list")}</button></div></div>
    {activeGroup === "collections" && <div className="mb-4 flex flex-wrap gap-2" aria-label={language === "ja" ? "絞り込み" : "Browse groups"}>
      <button className="rounded-lg border border-white/15 px-3 py-2 text-sm" aria-pressed={active === "tags" ? tagId === undefined : collectionId === undefined} onClick={() => active === "tags" ? setTagId(undefined) : setCollectionId(undefined)}>{active === "tags" ? t("allTags") : t("allCollections")}</button>
      {(active === "tags" ? tags : collections).map(([id, name]) => <button key={id} className="rounded-lg border border-white/15 px-3 py-2 text-sm" aria-pressed={(active === "tags" ? tagId : collectionId) === id} onClick={() => active === "tags" ? setTagId(id) : setCollectionId(id)}>{name}</button>)}
    </div>}
    {filters}
    {selectedBookIds.length > 0 && <div className="mb-4 flex flex-wrap items-center gap-3 rounded-lg border border-amber-300/30 bg-amber-300/10 p-3 text-sm"><span>{selectedBookIds.length} selected</span><input className="control max-w-xs" value={batchTags} onChange={e => setBatchTags(e.target.value)} placeholder="Replace tags, comma separated" /><button className="rounded bg-amber-300 px-3 py-2 font-medium text-stone-950" onClick={() => void invoke("batch_set_book_tags", { request: { bookIds: selectedBookIds, tagNames: batchTags.split(",") } }).then(() => { setSelectedBookIds([]); setBatchTags(""); void load(); }).catch(reason => setError(String(reason)))}>Apply tags</button><button className="text-stone-300 underline" onClick={() => setSelectedBookIds([])}>Clear</button></div>}
    {books.length === 0 ? <div className="rounded-xl border border-dashed border-white/15 p-12 text-center text-stone-400">{roots.length ? t("noMatches") : t("addFolderPrompt")}</div> : <div className={layout === "grid" ? "grid grid-cols-2 gap-4 sm:grid-cols-3 lg:grid-cols-4 2xl:grid-cols-6" : "overflow-hidden rounded-xl border border-white/10"}>
      {books.map(book => { const checked = selectedBookIds.includes(book.id); const unavailable = language === "ja" ? "ソースなし" : "Unavailable"; const toggle = () => setSelectedBookIds(current => checked ? current.filter(id => id !== book.id) : [...current, book.id]); return layout === "grid" ? <article key={book.id} className="catalog-item catalog-item-grid relative min-w-0"><label className="absolute left-2 top-2 z-10 rounded bg-stone-950/80 p-1"><input aria-label={`Select ${book.effectiveTitle}`} type="checkbox" checked={checked} onChange={toggle} /></label>{!book.isAvailable && <span className="absolute right-2 top-2 z-10 rounded bg-red-950/90 px-2 py-1 text-xs text-red-200">{unavailable}</span>}<button className="block w-full text-left" onClick={() => setDetailBookId(book.id)}><div className="relative aspect-[2/3] overflow-hidden rounded-xl bg-stone-900 shadow-lg"><Cover book={book} language={language} showCovers={showCovers} />{book.isFinished && <span className="absolute bottom-2 right-2"><FinishedMark language={language} /></span>}</div><h2 className="mt-3 truncate font-medium" title={book.effectiveTitle}>{book.effectiveTitle}</h2><p className="mt-1 truncate text-sm text-stone-400">{book.effectiveCreator || t("unknownAuthor")}</p><p className="truncate text-xs text-amber-200/80">{[book.effectiveSeries, book.effectiveVolume].filter(Boolean).join(" · ")}</p></button></article> : <article key={book.id} className="catalog-item catalog-item-list flex gap-4 border-b border-white/10 p-3 last:border-0"><label className="pt-1"><input aria-label={`Select ${book.effectiveTitle}`} type="checkbox" checked={checked} onChange={toggle} /></label><button className="flex min-w-0 flex-1 gap-4 text-left" onClick={() => setDetailBookId(book.id)}><div className="h-20 w-14 shrink-0 overflow-hidden rounded bg-stone-800"><Cover book={book} language={language} showCovers={showCovers} /></div><div className="min-w-0"><h2 className="flex items-center gap-2 font-medium"><span className="truncate">{book.effectiveTitle}</span>{book.isFinished && <FinishedMark language={language} />} {!book.isAvailable && <span className="ml-2 text-xs text-red-300">{unavailable}</span>}</h2><p className="mt-1 truncate text-sm text-stone-400">{book.effectiveCreator || t("unknownAuthor")}</p><p className="mt-1 truncate text-xs text-amber-200/80">{[book.effectiveSeries, book.effectiveVolume].filter(Boolean).join(" · ") || book.parentFolderPath}</p></div></button></article>; })}
    </div>}
    <div ref={sentinel} className="h-px" />
  </section>;
  const rootsView = <section className="w-full max-w-4xl"><div className="mb-8 flex flex-wrap items-end justify-between gap-4"><div><p className="text-sm font-medium text-amber-300">{language === "ja" ? "設定" : "Settings"}</p><h1 className="mt-1 text-3xl font-semibold">{language === "ja" ? "ライブラリルート" : "Library Roots"}</h1><p className="mt-2 text-stone-400">{language === "ja" ? "選んだフォルダーは読み取り専用でスキャンします。" : "Selected folders are scanned read-only."}</p></div><button className="primary" onClick={() => void add()}>{t("addFolder")}</button></div>{progress && <div className="mb-4 rounded-lg bg-stone-900 p-4 text-sm text-stone-300"><p>{language === "ja" ? `スキャン中 (${progress.stage === "indexing" ? "索引" : progress.stage === "extraction" ? "抽出" : "検出"}): ${progress.discoveredCount} 件を検出、${progress.indexedCount ?? 0} 件を登録` : `Scanning (${progress.stage ?? "discovery"}): ${progress.discoveredCount} found, ${progress.indexedCount ?? 0} cataloged`}</p><p className="mt-1 truncate text-xs text-stone-500">{progress.currentPath}</p><button className="mt-3 text-amber-300 underline" onClick={() => void invoke("cancel_scan", { scanId: progress.scanId })}>{t("cancel")}</button></div>}<div className="overflow-hidden rounded-xl border border-white/10 bg-stone-900/60">{roots.length === 0 ? <p className="p-8 text-center text-stone-400">{t("noRoots")}</p> : roots.map(root => <div key={root.id} className="flex flex-wrap items-center justify-between gap-4 border-b border-white/10 p-5 last:border-0"><div className="min-w-0"><p className="font-medium">{root.displayName}</p><p className="mt-1 truncate text-sm text-stone-400">{root.path}</p><p className="mt-2 text-xs text-stone-500">{root.bookCount} {language === "ja" ? "冊 · 最終スキャン:" : "books · Last scanned:"} {time(root.lastScannedAt, language)}</p></div><div className="flex gap-3"><button className="text-sm text-amber-300" onClick={() => void rescan(root)}>{t("rescan")}</button><button className="text-sm text-stone-400 hover:text-red-300" onClick={() => void remove(root)}>{t("removeCatalog")}</button></div></div>)}</div></section>;
  const settings = <section className="w-full max-w-4xl"><h1 className="mb-6 text-3xl font-semibold">{language === "ja" ? "設定" : "Settings"}</h1><div className="mb-6 rounded-xl border border-white/10 bg-stone-900/60 p-5"><h2 className="mb-3 font-semibold">{t("language")}</h2><button type="button" role="switch" aria-checked={language === "en"} aria-label={t("language")} onClick={switchLanguage} className="flex w-full max-w-xs items-center justify-between rounded-lg bg-stone-800 px-3 py-2 text-sm"><span>{language === "ja" ? "日本語" : "English"}</span><span className="rounded bg-amber-400 px-2 py-0.5 text-xs font-bold text-stone-950">{language === "ja" ? "EN" : "日本語"}</span></button></div><p className="text-sm font-medium text-amber-300">{language === "ja" ? "設定" : "Settings"}</p><div className="mb-6 rounded-xl border border-white/10 bg-stone-900/60 p-5"><label className="block font-semibold" htmlFor="duplicate-filtering">{language === "ja" ? "重複する本" : "Duplicate books"}</label><select id="duplicate-filtering" className="control mt-3" value={duplicateFiltering} onChange={e => { setDuplicateFiltering(e.target.value); localStorage.setItem("tmw-duplicate-filtering", e.target.value); }}><option value="off">{language === "ja" ? "すべてのコピーを表示" : "Show all copies"}</option><option value="high">{language === "ja" ? "確度の高い重複をまとめる" : "Collapse confident duplicates"}</option><option value="all">{language === "ja" ? "すべての重複候補をまとめる" : "Collapse all candidate duplicates"}</option></select><p className="mt-2 text-sm text-stone-400">{language === "ja" ? "候補グループごとに1冊表示します。本や読書データは保持されます。" : "Shows one copy per candidate group in the library, preferring available books. All candidates includes possible matches. Books and reading data are retained."}</p></div><h2 className="mt-1 text-xl font-semibold">{t("coverCache")}</h2><label className="flex cursor-pointer items-center gap-2  text-sm"><input type="checkbox" className="accent-amber-400" checked={showCovers} onChange={e => { setShowCovers(e.target.checked); localStorage.setItem("tmw-show-covers", String(e.target.checked)); }} />{language === "ja" ? "表紙を表示" : "Show covers"}</label><p className="mt-2 text-stone-400">{t("cacheNote")}</p><div className="mt-6 rounded-xl border border-white/10 bg-stone-900/60 p-5"><p className="text-sm text-stone-400">{t("currentLocation")}</p><p className="mt-2 break-all font-medium">{cache ?? t("loading")}</p><div className="mt-5 flex flex-wrap gap-3"><button className="primary" onClick={async () => { const path = await invoke<string | null>("choose_cover_cache_folder"); if (path) { await invoke("set_cover_cache_directory", { path }); setCache(path); } }}>{t("changeLocation")}</button><button className="rounded-xl border border-amber-300/50 px-5 py-3 font-semibold text-amber-200" onClick={() => void invoke("regenerate_cover_cache").catch(reason => setError(String(reason)))}>{t("regenerateCovers")}</button></div><p className="mt-4 text-xs leading-5 text-stone-500">{t("cacheSafe")}</p></div><div className="mt-6 rounded-xl border border-white/10 bg-stone-900/60 p-5"><h2 className="font-semibold">{language === "ja" ? "検索インデックス" : "Search index"}</h2><p className="mt-2 text-sm text-stone-400">{language === "ja" ? "通常は自動的に増分更新されます。検索結果が不完全な場合のみ手動で再構築してください。" : "Normally updated incrementally. Rebuild manually only if search results appear incomplete."}</p>{indexProgress && <p className="mt-3 text-sm text-amber-200">{indexProgress.total ? `${indexProgress.completed.toLocaleString()} / ${indexProgress.total.toLocaleString()}` : language === "ja" ? "準備中…" : "Preparing…"}</p>}<div className="mt-4 flex gap-3">{indexProgress ? <button className="text-amber-300 underline" onClick={() => void invoke("cancel_search_index_rebuild", { rebuildId: indexProgress.rebuildId })}>{t("cancel")}</button> : <button className="rounded-xl border border-amber-300/50 px-5 py-3 font-semibold text-amber-200" onClick={() => void rebuildIndex()}>{language === "ja" ? "検索インデックスを再構築" : "Rebuild search index"}</button>}</div></div><DictionaryManager /><YomitanNotices /><CompanionSettings /><BackupControls language={language} onError={setError} /></section>;
  return <div className="min-h-screen bg-stone-950 text-stone-100"><div className="mx-auto flex min-h-screen max-w-[1800px] flex-col md:flex-row"><aside className="flex flex-col border-b border-white/10 bg-stone-900/70 px-4 py-4 md:sticky md:top-0 md:h-screen md:w-56 md:shrink-0 md:border-b-0 md:border-r"><div className="mb-5 flex items-center gap-2.5"><div className="grid size-8 place-items-center rounded-lg bg-amber-400 text-base font-black text-stone-950">本</div><div><p className="text-sm font-semibold">TMW Library</p><p className="text-xs text-stone-400">{language === "ja" ? "ローカル EPUB ライブラリ" : "Local EPUB library"}</p></div></div><nav aria-label="Main navigation" className="flex flex-1 flex-wrap gap-1 md:flex-col md:flex-nowrap">{navigation.map(([id, japanese, english]) => <button key={id} aria-current={activeGroup === id ? "page" : undefined} className={`rounded-lg px-3 py-2 text-left text-sm ${id === "settings" ? "md:mt-auto" : ""} ${activeGroup === id ? "bg-amber-400/15 text-amber-200" : "text-stone-300 hover:bg-white/5"}`} onClick={() => select(id)}>{language === "ja" ? japanese : english}</button>)}</nav></aside><main className="min-w-0 flex-1 px-6 py-8 sm:px-8">{tabs.length > 0 && viewTabs}{error && <p role="alert" className="fixed bottom-5 right-5 z-10 max-w-md rounded-lg border border-red-400/30 bg-stone-900 p-3 text-sm text-red-200">{error}</p>}{active === "history" ? <LookupHistory onJump={(bookId, cfi) => setPassageReader({ bookId, cfi })} /> : active === "passages" ? <SavedPassages onJump={(passage, cfi) => setPassageReader({ bookId: passage.bookId, cfi })} /> : active === "home" ? <ContinueReading language={language} onLibrary={() => select("all")} onRoots={() => select("roots")} /> : active === "roots" ? rootsView : active === "settings" ? settings : browser}</main>{passageReader && <EpubReader bookId={passageReader.bookId} initialCfi={passageReader.cfi} onClose={() => setPassageReader(null)} />}{detailBookId && <BookDetailsPanel bookId={detailBookId} onClose={() => setDetailBookId(null)} onSaved={() => void load()} />}</div></div>;
}
