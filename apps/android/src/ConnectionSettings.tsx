import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
type Status = { paired: boolean; url: string; message: string };
export default function ConnectionSettings() {
  const [url, setUrl] = useState(""); const [code, setCode] = useState("");
  const [status, setStatus] = useState<Status | null>(null); const [busy, setBusy] = useState(false); const [error, setError] = useState("");
  useEffect(() => { void invoke<Status>("private_connection", { action: "status" }).then(s => { setStatus(s); setUrl(s.url); }).catch(e => setError(String(e))); }, []);
  async function run(action: string) {
    setBusy(true); setError("");
    try { const s = await invoke<Status>("private_connection", { action, url, code }); setStatus(s); setCode(""); if (action === "forget") setUrl(""); }
    catch (e) { setError(String(e)); } finally { setBusy(false); }
  }
  return <section className="space-y-3 rounded-xl border border-slate-700 p-4">
    <h2 className="font-semibold">Private PC connection</h2>
    <p className="text-sm text-slate-400">Enable the desktop service, create a pairing code, and connect both devices to your private Tailscale network. The PC must stay awake.</p>
    <label className="block">PC HTTPS address<input className="control block w-full" type="url" autoCapitalize="none" autoCorrect="off" placeholder="https://your-pc.your-tailnet.ts.net" value={url} onChange={e => setUrl(e.target.value)} disabled={busy} /></label>
    <label className="block">Pairing code<input className="control block w-full" autoCapitalize="none" autoCorrect="off" autoComplete="off" maxLength={12} value={code} onChange={e => setCode(e.target.value)} disabled={busy} /></label>
    <div className="flex flex-wrap gap-2"><button className="control" disabled={busy || !url || code.length !== 12} onClick={() => void run("pair")}>Pair phone</button>
    <button className="control" disabled={busy || !status?.paired} onClick={() => void run("check")}>Check connection</button>
    <button className="control" disabled={busy || !status?.paired} onClick={() => void run("forget")}>Forget connection</button></div>
    <p aria-live="polite">{busy ? "Connecting…" : status?.message}</p>{error && <p role="alert" className="text-red-300">{error}</p>}
    <p className="text-xs text-slate-400">Catalog and download management follow in Phase 4. Forgetting removes the phone credential; revoke the device in desktop Settings to block access.</p>
  </section>;
}
