import { useRef, useState, type ReactNode } from "react";

const categories = [
  ["general", "一般", "General", "言語、シリーズ、重複表示", "Language, series and duplicate preferences"],
  ["roots", "ライブラリルート", "Library Roots", "フォルダーとスキャン", "Folders and library scanning"],
  ["covers", "表紙", "Covers", "表紙の表示とキャッシュ", "Cover visibility and cache storage"],
  ["dictionaries", "辞書", "Dictionaries", "辞書のインポートと検索設定", "Dictionary imports and lookup preferences"],
  ["connection", "Android 接続", "Android connection", "プライベート接続とペアリング", "Private connection and phone pairing"],
  ["backup", "バックアップと復元", "Backup & restore", "カタログと読書データの保護", "Protect your catalog and reading data"],
  ["maintenance", "メンテナンス", "Maintenance", "検索インデックスの修復", "Repair the search index"],
  ["about", "ライセンス", "Licenses", "辞書コードの帰属とライセンス", "Dictionary code attribution and licenses"],
] as const;
type Category = typeof categories[number][0];

export function SettingsLayout({ language, pages }: { language: "ja" | "en"; pages: Record<Category, ReactNode> }) {
  const [active, setActive] = useState<Category>("general");
  const heading = useRef<HTMLHeadingElement>(null);
  const category = categories.find(([id]) => id === active)!;
  const japanese = language === "ja";
  return <section className="w-full max-w-6xl" aria-label={japanese ? "設定" : "Settings"}>
    <header className="mb-7">
      <h1 className="text-3xl font-semibold">{japanese ? "設定" : "Settings"}</h1>
      <p className="mt-2 text-sm text-stone-400">{japanese ? "項目を選んで設定を変更します。変更は自動保存されます。" : "Choose a category to manage your preferences and library tools."}</p>
    </header>
    <div className="flex flex-col gap-6 xl:flex-row xl:gap-8">
      <nav aria-label={japanese ? "設定カテゴリー" : "Settings categories"} className="flex flex-wrap gap-1 rounded-xl border border-white/10 bg-stone-900/60 p-2 xl:sticky xl:top-8 xl:w-52 xl:shrink-0 xl:self-start xl:flex-col">
        {categories.map(([id, ja, en]) => <button key={id} type="button" aria-current={id === active ? "page" : undefined}
          className={`rounded-lg px-3 py-2.5 text-left text-sm transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-amber-300 ${id === active ? "bg-amber-400/15 font-semibold text-amber-200" : "text-stone-300 hover:bg-white/5 hover:text-stone-100"}`}
          onClick={() => { setActive(id); heading.current?.focus({ preventScroll: true }); heading.current?.scrollIntoView?.({ block: "start", behavior: "instant" }); }}>
          {japanese ? ja : en}
        </button>)}
      </nav>
      <div className="min-w-0 flex-1">
        <h2 ref={heading} tabIndex={-1} className="text-xl font-semibold outline-none">{category[japanese ? 1 : 2]}</h2>
        <p className="mt-2 mb-5 text-sm text-stone-400">{category[japanese ? 3 : 4]}</p>
        {categories.map(([id]) => <div key={id} hidden={id !== active}>{pages[id]}</div>)}
      </div>
    </div>
  </section>;
}
