import { useEffect, useState } from "react";
import { mobileUser, type SyncStatus } from "./userData";

export default function UserSyncStatus() {
  const [status, setStatus] = useState<SyncStatus>();
  const [error, setError] = useState("");
  useEffect(() => {
    let disposed = false;
    const check = () =>
      void mobileUser<SyncStatus>({ action: "userSyncStatus" })
        .then((value) => {
          if (!disposed) setStatus(value);
        })
        .catch((e) => {
          if (!disposed) setError(String(e));
        });
    check();
    const timer = setInterval(check, 2000);
    return () => {
      disposed = true;
      clearInterval(timer);
    };
  }, []);
  return (
    <section className="rounded border border-slate-700 p-3 text-sm" aria-label="User data sync">
      <p aria-live="polite">
        {status?.message ?? "Checking saved-data sync…"}
        {status ? ` · ${status.pending} pending` : ""}
      </p>
      {!!status?.rejected?.length && (
        <details>
          <summary>Updates need attention ({status.rejected.length})</summary>
          <p>Their local copies are retained.</p>
          {status.rejected.map((r) => (
            <article key={r.id} className="border-t border-slate-700 py-2">
              <p>{r.reason}</p>
              {r.operation?.fields.sentence && <p lang="ja">{r.operation.fields.sentence}</p>}
              {r.operation?.fields.note && <p>{r.operation.fields.note}</p>}
              {r.operation?.fields.locationCfi && (
                <p className="break-all text-xs">
                  Retained position: {r.operation.fields.locationCfi}
                </p>
              )}
            </article>
          ))}
        </details>
      )}
      {error && <p role="alert">{error}</p>}
      <button
        className="control"
        disabled={status?.busy}
        onClick={() => {
          setError("");
          void mobileUser<SyncStatus>({ action: "userSync" })
            .then(setStatus)
            .catch((e) => setError(String(e)));
        }}
      >
        Sync saved data now
      </button>
    </section>
  );
}
