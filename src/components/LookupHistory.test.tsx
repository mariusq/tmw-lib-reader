import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { LookupHistory } from "./LookupHistory";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const row = {
  id: 1,
  surface: "読んだ",
  headword: "読む",
  reading: "よむ",
  bookId: 2,
  bookTitle: "物語",
  locationCfi: "epubcfi(/6/2!/4/2/1:0)",
  sentence: "本を読んだ。",
  lookedUpAt: 1,
  count: 4,
  isAvailable: true,
};
beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (command) =>
    command === "list_lookup_history"
      ? [row]
      : command === "get_lookup_history_location"
        ? row.locationCfi
        : null,
  );
});
afterEach(cleanup);

it("shows counts, validates anchors, and controls history independently", async () => {
  const jump = vi.fn();
  render(<LookupHistory onJump={jump} />);
  expect(await screen.findByText(/Looked up 4 times/)).toBeVisible();
  fireEvent.click(screen.getByText("Jump to passage"));
  await waitFor(() => expect(jump).toHaveBeenCalledWith(2, row.locationCfi));
  fireEvent.click(screen.getByText("Tracking on"));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("set_app_setting", {
      key: "lookup_history_enabled",
      value: "false",
    }),
  );
  expect(await screen.findByText("Tracking off")).toBeVisible();
  const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
  fireEvent.click(screen.getByText("Clear history"));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("clear_lookup_history"));
  expect(
    vi
      .mocked(invoke)
      .mock.calls.some(
        ([command]) => command === "delete_passage" || command === "save_reading_location",
      ),
  ).toBe(false);
  confirm.mockRestore();
});

it("ignores a stale search response and keeps unavailable history readable", async () => {
  let resolveOld!: (rows: (typeof row)[]) => void;
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command !== "list_lookup_history") return null;
    if ((args as { query: string }).query === "")
      return new Promise<(typeof row)[]>((resolve) => {
        resolveOld = resolve;
      });
    return [{ ...row, surface: "新しい", isAvailable: false }];
  });
  render(<LookupHistory onJump={vi.fn()} />);
  await waitFor(() => expect(resolveOld).toBeDefined());
  fireEvent.change(screen.getByLabelText("Search lookup history"), { target: { value: "新しい" } });
  expect(await screen.findByText(/新しい/)).toBeVisible();
  resolveOld([row]);
  await waitFor(() => expect(screen.queryByText(/^読んだ/)).not.toBeInTheDocument());
  expect(screen.getByText("Jump to passage")).toBeDisabled();
  expect(screen.getByText("本を読んだ。")).toBeVisible();
});
