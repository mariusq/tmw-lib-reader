import notice from "../../crates/japanese-core/vendor/yomitan/NOTICE.txt?raw";
import license from "../../crates/japanese-core/vendor/yomitan/LICENSE?raw";

export function YomitanNotices() {
  return (
    <section className="mt-6 rounded-xl border border-white/10 bg-stone-900/60 p-5" aria-label="Yomitan license notices">
      <h2 className="font-semibold">Dictionary lookup code notices</h2>
      <p className="mt-2 text-sm text-stone-400">Japanese lookup includes Yomitan language transforms, licensed under GPL version 3 or later.</p>
      <details className="mt-3">
        <summary className="cursor-pointer text-amber-300">Yomitan attribution and full GPL license</summary>
        <pre className="mt-3 max-h-96 overflow-auto whitespace-pre-wrap break-words text-xs">{notice}{"\n"}{license}</pre>
      </details>
    </section>
  );
}
