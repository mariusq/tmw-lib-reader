import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import { readingStatuses } from "../features/reader/readingStatuses";

export function ReadingStatus({ bookId, onChanged, compact = false }: { bookId: number; onChanged?: () => void; compact?: boolean }) {
  const [state, setState] = useState<{ status: string; completedAt: number | null }>();
  const [error, setError] = useState<string>();
  const [saving, setSaving] = useState(false);
  useEffect(() => {
    let current = true;
    void invoke<typeof state>("get_reading_state", { bookId })
      .then((value) => {
        if (current) setState(value);
      })
      .catch((reason) => {
        if (current) setError(String(reason));
      });
    return () => {
      current = false;
    };
  }, [bookId]);
  return (
    <div className={compact ? "reader-status text-sm" : "text-sm"}>
      <label>
        <span className={compact ? "sr-only" : undefined}>Reading status</span>{" "}
        <select
          aria-label="Reading status"
          title="Reading status"
          className={compact ? "reader-control" : "control mt-1"}
          disabled={!state || saving}
          value={state?.status ?? "unset"}
          onChange={async (event) => {
            setSaving(true);
            setError(undefined);
            try {
              await invoke("set_reading_status", { bookId, status: event.target.value });
              setState(await invoke("get_reading_state", { bookId }));
              onChanged?.();
            } catch (reason) {
              setError(String(reason));
            } finally {
              setSaving(false);
            }
          }}
        >
          {readingStatuses.map(([value, label]) => (
            <option key={value} value={value}>
              {label}
            </option>
          ))}
        </select>
      </label>
      {state?.completedAt && (
        <p className="mt-1 text-xs text-stone-400">
          Completed {new Date(state.completedAt * 1000).toLocaleDateString()}
        </p>
      )}
      {error && <p role="alert">{error}</p>}
    </div>
  );
}
