import { useEffect, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { EpubReader } from "../features/reader/EpubReader";

type ResumeBook = { id: number; title: string; creator: string; coverPath: string | null; lastReadAt: number; hasLocation: boolean; isAvailable: boolean };

export function ContinueReading({ language, onLibrary, onRoots }: { language: "ja" | "en"; onLibrary: () => void; onRoots: () => void }) {
  const [books, setBooks] = useState<ResumeBook[]>([]);
  const [primary, setPrimary] = useState<ResumeBook | null>(null);
  const [readerId, setReaderId] = useState<number | null>(null);
  const [revision, setRevision] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const ja = language === "ja";
  useEffect(() => {
    let disposed = false;
    Promise.all([
      invoke<ResumeBook[]>("list_resume_books", { availableOnly: false }),
      invoke<ResumeBook[]>("list_resume_books", { availableOnly: true }),
    ]).then(([recent, available]) => {
      if (!disposed) { setBooks(recent); setPrimary(available[0] ?? null); setError(null); }
    }).catch(reason => { if (!disposed) setError(String(reason)); })
      .finally(() => { if (!disposed) setLoading(false); });
    return () => { disposed = true; };
  }, [revision]);
  return <section className="w-full self-start">
    <p className="text-sm font-medium text-amber-300">TMW Library</p>
    <h1 className="mt-1 text-3xl font-semibold">{ja ? "続きを読む" : "Continue Reading"}</h1>
    <p className="mt-3 text-stone-400">{ja ? "最後に読んだ場所から、また物語へ。" : "Return to your story, right where you left off."}</p>
    <div className="my-6 flex flex-wrap gap-3">
      {primary && <button className="primary" onClick={() => setReaderId(primary.id)}>{ja ? "続きを読む" : "Continue reading"}: {primary.title}</button>}
      <button className="rounded-xl border border-white/20 px-5 py-3" onClick={onLibrary}>{ja ? "すべての本を見る" : "Browse full library"}</button>
    </div>
    {error && <p role="alert" className="mb-4 text-red-300">{error}</p>}
    {loading ? <p role="status">{ja ? "読み込み中…" : "Loading…"}</p> : books.length === 0 ?
      <div className="rounded-xl border border-dashed border-white/15 p-8 text-stone-400"><p>{ja ? "ライブラリから本を開くと、ここから再開できます。" : "Open a book from your library and it will appear here next time."}</p><button className="mt-4 text-amber-300 underline" onClick={onRoots}>{ja ? "ライブラリフォルダーを追加" : "Add a library folder"}</button></div> :
      <div className="grid grid-cols-2 gap-5 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-6">
        {books.map(book => <article key={book.id} className="min-w-0">
          <button disabled={!book.isAvailable} className="w-full text-left disabled:opacity-50" onClick={() => setReaderId(book.id)} aria-label={`${ja ? "続きを読む" : "Resume"}: ${book.title}`}>
            <ResumeCover path={book.coverPath} />
            <h2 className="mt-3 truncate font-medium" title={book.title}>{book.title}</h2>
            <p className="truncate text-sm text-stone-400">{book.creator}</p>
            <p className="mt-2 text-xs text-stone-400">{new Intl.DateTimeFormat(ja ? "ja-JP" : "en", { dateStyle: "medium", timeStyle: "short" }).format(book.lastReadAt * 1000)}</p>
            <p className="mt-1 text-xs text-amber-200">{book.hasLocation ? (ja ? "読書位置を保存済み" : "Saved reading position") : (ja ? "読書位置なし · 最初から開く" : "No saved position · opens at the beginning")}</p>
          </button>
          {!book.isAvailable && <div className="mt-2 text-xs text-red-200"><p>{ja ? "ソースが見つかりません。フォルダーを接続して再スキャンしてください。" : "Source unavailable. Reconnect its folder and rescan."}</p><button className="mt-2 underline" onClick={onRoots}>{ja ? "ライブラリルート" : "Library roots"}</button></div>}
        </article>)}
      </div>}
    {readerId !== null && <EpubReader bookId={readerId} onClose={() => { setReaderId(null); setRevision(value => value + 1); }} />}
  </section>;
}

function ResumeCover({ path }: { path: string | null }) {
  const [broken, setBroken] = useState(false);
  return <div className="aspect-[2/3] overflow-hidden rounded-xl bg-stone-900">{path && !broken ? <img src={convertFileSrc(path)} alt="" loading="lazy" decoding="async" className="h-full w-full object-cover" onError={() => setBroken(true)} /> : <div className="grid h-full place-items-center text-3xl text-amber-200/60">本</div>}</div>;
}
