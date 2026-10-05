import { act, fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import LookupHistory from "./LookupHistory";
import type { HistoryPage } from "./historyData";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const empty = (word?: string, next: number | null = null): HistoryPage => ({
  settings: { enabled: true, retentionLimit: 10000 }, pending: 0, next,
  rows: word ? [{ ns: "a".repeat(32), bookId: "b".repeat(32), entityId: "c".repeat(32), contentVersion: null,
    fields: { surface: word, headword: null, reading: null, sentence: "context", locationCfi: "",
      dictionaryId: "fixture", dictionaryEntryId: null, lookedUpAt: "1" } }] : [],
});
it("pages history natively and ignores a stale response after the search changes", async () => {
  vi.useFakeTimers();
  let resolveOld: (value: HistoryPage) => void = () => {};
  vi.mocked(invoke).mockImplementation(async (_, input) => {
    const args = (input as { args: { query: string; offset: number } }).args;
    if (!args.query) return empty("first", 50);
    if (args.query === "old") return new Promise<HistoryPage>((resolve) => { resolveOld = resolve; });
    return empty("new result");
  });
  try {
    render(<LookupHistory />);
    await act(async () => { await vi.advanceTimersByTimeAsync(250); });
    fireEvent.click(screen.getByText("Next history"));
    await act(async () => { await vi.advanceTimersByTimeAsync(250); });
    expect(invoke).toHaveBeenLastCalledWith("mobile_storage", { args: { action: "historyList", query: "", offset: 50 } });
    fireEvent.change(screen.getByLabelText("Search history"), { target: { value: "old" } });
    await act(async () => { await vi.advanceTimersByTimeAsync(250); });
    fireEvent.change(screen.getByLabelText("Search history"), { target: { value: "new" } });
    await act(async () => { await vi.advanceTimersByTimeAsync(250); });
    await act(async () => { resolveOld(empty("stale result")); });
    expect(screen.getByText(/new result/)).toBeTruthy();
    expect(screen.queryByText(/stale result/)).toBeNull();
  } finally { vi.useRealTimers(); }
});
