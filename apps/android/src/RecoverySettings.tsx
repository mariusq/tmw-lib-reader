import { useState } from "react";
import { mobileUser } from "./userData";

export function recoveryReaderSettings(storage: Storage) {
  const result: Record<string, string> = {};
  for (let i = 0; i < storage.length; i++) {
    const key = storage.key(i);
    if (key && (key.startsWith("tmw-local-cfi-") ||
      ["tmw-reader-font-size", "tmw-reader-theme", "tmw-reader-writing-mode", "tmw-reader-hide-furigana", "tmw-last-download", "tmw-screen"].includes(key))) {
      result[key] = storage.getItem(key) ?? "";
    }
  }
  return result;
}

export default function RecoverySettings() {
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  async function exportData() {
    setBusy(true); setMessage(""); setError("");
    try {
      const result = await mobileUser<{ message: string }>({
        action: "exportRecovery", webState: recoveryReaderSettings(localStorage),
      });
      setMessage(result.message);
    } catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  }
  return <section className="rounded-xl border border-slate-700 p-4 space-y-3">
    <h2 className="font-semibold">Backup and recovery</h2>
    <p>Save a recovery ZIP containing progress, bookmarks, notes, lookup history,
      pending and rejected sync changes, catalog identities and reader settings.
      It works offline and includes phone-only records.</p>
    <p>Book files, the dictionary, covers and pairing credentials are excluded.
      Keep manually imported EPUB copies separately. The ZIP contains private notes
      and reading history and is not encrypted. Choose a trusted local folder.</p>
    <button className="control" disabled={busy} onClick={() => void exportData()}>
      {busy ? "Exporting…" : "Export phone data"}
    </button>
    {message && <p role="status">{message}</p>}
    {error && <p role="alert">{error}</p>}
    <p>Install signed updates over this app. Uninstalling or clearing app data erases
      downloads and unsynced records. Export first and copy the ZIP off the phone.</p>
    <p>This is a readable recovery export; automatic restore is not available.
      Keep it for recovering notes and positions or a future assisted restore.
      Do not send its SQLite-derived records directly to the PC sync API.</p>
  </section>;
}
