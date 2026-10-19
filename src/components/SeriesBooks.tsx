import { convertFileSrc } from "@tauri-apps/api/core";
import { GeneratedCover } from "./GeneratedCover";
import { useState } from "react";

import { groupSeriesCopies, type SeriesBook } from "./seriesCopyGroups";

function Thumbnail({ book, showCovers, grid }: { book: SeriesBook; showCovers: boolean; grid: boolean }) {
  const [broken, setBroken] = useState(false);
  return <div className={grid ? "aspect-[2/3] w-full overflow-hidden rounded-lg bg-stone-800" : "h-24 w-16 shrink-0 overflow-hidden rounded-lg bg-stone-800"}>{showCovers && book.effectiveCoverPath && !broken ? <img className="h-full w-full object-cover" src={convertFileSrc(book.effectiveCoverPath)} alt="" loading="lazy" onError={() => setBroken(true)} /> : <GeneratedCover title={book.effectiveTitle} />}</div>;
}

export function SeriesBooks({ books, groupCopies, showCovers, layout = "list", language, hasMore, onOpen, selectedIds = [], onToggle }: { books: SeriesBook[]; layout?: "grid" | "list"; groupCopies: boolean; showCovers: boolean; language: "ja" | "en"; hasMore: boolean; onOpen: (id: number) => void; selectedIds?: number[]; onToggle?: (id: number) => void }) {
  const groups = groupCopies ? groupSeriesCopies(books) : books.map(book => [book]);
  const grid = layout === "grid";
  const ja = language === "ja";
  return <div className="space-y-3" aria-label={ja ? "シリーズの本" : "Series books"}>
    <p className="text-sm text-stone-400">{ja ? `${groups.length} 件 · ${books.length} コピー${hasMore ? "（読み込み済み）" : ""}` : `${groups.length} titles · ${books.length} copies${hasMore ? " loaded so far" : ""}`}</p>
    <div data-layout={layout} className={grid ? "library-grid items-start" : "space-y-3"}>{groups.map(copies => { const book = copies[0]; return <article key={book.id} className="min-w-0 rounded-xl border border-white/10 bg-stone-900/50 p-4">
      {onToggle && <label className="mb-2 flex items-center gap-2 text-xs text-stone-400"><input type="checkbox" aria-label={`Select ${book.effectiveTitle} #${book.id}`} checked={selectedIds.includes(book.id)} onChange={() => onToggle(book.id)} />{ja ? "選択" : "Select"}</label>}
      <button className={grid ? "block w-full text-left" : "flex w-full items-center gap-4 text-left"} onClick={() => onOpen(book.id)}><Thumbnail book={book} showCovers={showCovers} grid={grid} /><span className={grid ? "mt-3 block min-w-0" : "min-w-0 flex-1"}><h2 className="block truncate font-medium" title={book.effectiveTitle}>{book.effectiveTitle}</h2><span className="mt-1 block text-sm text-stone-400">{book.effectiveCreator || (ja ? "著者不明" : "Unknown author")}{book.effectiveVolume ? ` · ${book.effectiveVolume}` : ""}</span><span className="mt-2 flex gap-3 text-xs">{book.isFinished && <span className="text-emerald-300">✓ {ja ? "読了" : "Finished"}</span>}{!book.isAvailable && <span className="text-red-300">{ja ? "ソースなし" : "Unavailable"}</span>}</span></span></button>
      {copies.length > 1 && <details className="mt-3 border-t border-white/10 pt-3"><summary className="cursor-pointer text-sm text-amber-200">{ja ? `${copies.length} コピーを表示` : `Show ${copies.length} copies`}</summary><p className="mt-2 text-xs text-stone-400">{ja ? "同じタイトル・著者のコピー。進捗とメモは個別に保持されます。" : "Matching title and author. Each copy keeps its own progress and bookmarks."}</p><ul className="mt-2 space-y-2">{copies.map((copy, index) => <li key={copy.id}><button className="flex w-full flex-wrap items-center justify-between gap-2 rounded-lg border border-white/10 px-3 py-2 text-left text-sm hover:bg-white/5" onClick={() => onOpen(copy.id)}><span>{ja ? `コピー ${index + 1}` : `Copy ${index + 1}`} · #{copy.id}</span><span className={copy.isAvailable ? "text-stone-400" : "text-red-300"}>{copy.isFinished ? (ja ? "✓ 読了 · " : "✓ Finished · ") : ""}{copy.isAvailable ? (ja ? "利用可能" : "Available") : (ja ? "ソースなし" : "Unavailable")}</span></button></li>)}</ul></details>}
    </article>; })}</div>
  </div>;
}
