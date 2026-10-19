import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
type Status = { enabled: boolean; address: string; devices: { id: string; name: string }[] };
export function CompanionSettings() {
  const [status, setStatus] = useState<Status | null>(null);
  const [code, setCode] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [autostart, setAutostart] = useState(true);
  const refresh = () => invoke<Status>("companion_status").then(setStatus).catch(e => setError(String(e)));
  useEffect(() => {
    void refresh();
    void invoke<string | null>("get_app_setting", { key: "companion_autostart" }).then(value => setAutostart(value !== "false")).catch(() => undefined);
  }, []);
  async function toggleAutostart(enabled: boolean) {
    setError("");
    try { await invoke("set_app_setting", { key: "companion_autostart", value: String(enabled) }); setAutostart(enabled); }
    catch (e) { setError(String(e)); }
  }
  async function action(command: string, args?: Record<string, unknown>) {
    setBusy(true); setError("");
    try { const result = await invoke<string>(command, args); setCode(command === "companion_pairing_code" ? result : ""); await refresh(); }
    catch (e) { setError(String(e)); } finally { setBusy(false); }
  }
  return <section className="mt-6 rounded-xl border border-white/10 bg-stone-900/60 p-5">
    <h2 className="text-lg font-semibold">Private Android connection</h2>
    <p className="mt-2 text-sm text-stone-400">Listens on this PC only (loopback). Keep this PC awake. Use private Tailscale Serve for HTTPS access from your phone.</p>
    <label className="mt-3 flex cursor-pointer items-center gap-2 text-sm"><input type="checkbox" className="accent-amber-400" checked={autostart} onChange={e => void toggleAutostart(e.target.checked)} />Start automatically when the desktop opens</label>
    <p className="mt-2">{status?.enabled ? `Listening on ${status.address}` : "Service disabled"}</p>
    <div className="mt-3 flex gap-3"><button className="primary" disabled={busy} onClick={() => void action(status?.enabled ? "companion_disable" : "companion_enable")}>{status?.enabled ? "Disable service" : "Enable service"}</button>
    {status?.enabled && <button disabled={busy} onClick={() => void action("companion_pairing_code")}>Create pairing code</button>}
    <button disabled={busy} onClick={() => void refresh()}>Refresh devices</button></div>
    {code && <p role="status" className="mt-3">Pairing code: <strong>{code}</strong> · expires in 2 minutes · one use. Share only with your own phone.</p>}
    {status?.devices.map(d => <div className="mt-3 flex justify-between" key={d.id}><span>{d.name}</span><button disabled={busy} onClick={() => void action("companion_revoke", { deviceId: d.id })}>Revoke</button></div>)}
    {error && <p role="alert">{error}</p>}
  </section>;
}
