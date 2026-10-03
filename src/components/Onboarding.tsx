import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

const KEY = "onboarding_completed_v1";

export function Onboarding({ children }: { children: React.ReactNode }) {
  const [ready, setReady] = useState<boolean | null>(null);
  useEffect(() => {
    void invoke<string | null>("get_app_setting", { key: KEY })
      .then((value) => setReady(value === "true"))
      .catch(() => setReady(false));
  }, []);
  if (ready === null) return <div className="min-h-screen bg-stone-950" />;
  if (ready) return children;
  const finish = async () => {
    await invoke("set_app_setting", { key: KEY, value: "true" });
    setReady(true);
  };
  return <main className="grid min-h-screen place-items-center bg-stone-950 p-6 text-stone-100">
    <section className="w-full max-w-2xl rounded-2xl border border-white/10 bg-stone-900 p-8 shadow-2xl">
      <div className="grid size-12 place-items-center rounded-xl bg-amber-400 text-xl font-black text-stone-950">本</div>
      <h1 className="mt-6 text-3xl font-semibold">Welcome to TMW Library</h1>
      <p className="mt-3 leading-7 text-stone-300">Your EPUB library stays on this computer. TMW Library reads folders you explicitly select to build a private local catalog.</p>
      <div className="mt-6 grid gap-3 sm:grid-cols-2">
        <div className="rounded-xl bg-stone-800 p-4"><p className="font-medium text-amber-200">Source files stay read-only</p><p className="mt-2 text-sm leading-6 text-stone-400">The app never edits, renames, moves, uploads, or deletes your EPUBs.</p></div>
        <div className="rounded-xl bg-stone-800 p-4"><p className="font-medium text-amber-200">App data is separate</p><p className="mt-2 text-sm leading-6 text-stone-400">Metadata and edits live in the local SQLite catalog. Covers use a separate, regenerable cache.</p></div>
      </div>
      <p className="mt-5 text-sm text-stone-400">Next, add <span className="text-stone-200">F:\tmw collection</span> or another folder from Library Roots.</p>
      <button className="primary mt-7" onClick={() => void finish()}>Continue</button>
    </section>
  </main>;
}
