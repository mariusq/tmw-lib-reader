import { invoke } from "@tauri-apps/api/core";
import { useState } from "react";

export function BackupControls({ language, onError }: { language: "ja" | "en"; onError: (message: string) => void }) {
  const [message, setMessage] = useState("");
  const exportBackup = async () => {
    try {
      const path = await invoke<string | null>("export_catalog_backup");
      if (path) setMessage(language === "ja" ? `バックアップを保存しました: ${path}` : `Backup saved: ${path}`);
    } catch (error) { onError(String(error)); }
  };
  const restore = async () => {
    const warning = language === "ja"
      ? "現在のカタログを選択したバックアップで置き換えます。復元前に自動安全バックアップが作成されます。EPUB とカバーキャッシュは変更されません。続行しますか？"
      : "This replaces the current catalog with the selected backup. An automatic safety backup is created first. EPUBs and the cover cache are not changed. Continue?";
    if (!window.confirm(warning)) return;
    try {
      const safety = await invoke<string | null>("import_catalog_backup");
      if (safety) {
        setMessage(language === "ja" ? `復元しました。安全バックアップ: ${safety}` : `Catalog restored. Safety backup: ${safety}`);
        window.setTimeout(() => window.location.reload(), 1200);
      }
    } catch (error) { onError(String(error)); }
  };
  return <div className="mt-6 rounded-xl border border-white/10 bg-stone-900/60 p-5">
    <h2 className="text-lg font-semibold">{language === "ja" ? "カタログのバックアップ" : "Catalog backup"}</h2>
    <p className="mt-2 text-sm leading-6 text-stone-400">{language === "ja" ? "メタデータ、修正、タグ、コレクション、読書位置を保存します。EPUB と再生成可能なカバー画像は含まれません。" : "Preserves metadata, corrections, tags, collections, and reading positions. EPUB files and regenerable cover images are not included."}</p>
    <div className="mt-4 flex flex-wrap gap-3"><button className="primary" onClick={() => void exportBackup()}>{language === "ja" ? "バックアップをエクスポート" : "Export backup"}</button><button className="rounded-xl border border-red-300/50 px-5 py-3 font-semibold text-red-200" onClick={() => void restore()}>{language === "ja" ? "バックアップから復元" : "Restore backup"}</button></div>
    {message && <p role="status" className="mt-4 break-all text-sm text-emerald-300">{message}</p>}
  </div>;
}
