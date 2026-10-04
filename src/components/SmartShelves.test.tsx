import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { SmartShelves, type ShelfFilter } from "./SmartShelves";
import { ReadingStatus } from "./ReadingStatus";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
beforeEach(() => vi.mocked(invoke).mockReset());
const filter: ShelfFilter = {
  readingStatus: "want",
  tagId: 3,
  query: "物語",
  sort: "author",
  needsMetadata: false,
  hideDuplicateTitles: false,
  offset: 0,
  limit: 80,
};

it("opens saved filters and saves edits without changing book data", async () => {
  const shelf = { id: 7, name: "Weekend", version: 1, filter, sortOrder: 2 };
  vi.mocked(invoke).mockImplementation(async (command) =>
    command === "list_smart_shelves" ? [shelf] : 7,
  );
  const onSelect = vi.fn();
  const { rerender } = render(<SmartShelves filter={filter} onSelect={onSelect} />);
  await screen.findByRole("option", { name: "Weekend" });
  fireEvent.change(screen.getByLabelText("Smart shelf"), { target: { value: "7" } });
  expect(onSelect).toHaveBeenCalledWith(filter);
  const changed = { ...filter, readingStatus: "paused" };
  rerender(<SmartShelves filter={changed} onSelect={onSelect} />);
  fireEvent.change(screen.getByLabelText("Shelf name"), { target: { value: "Later" } });
  fireEvent.change(screen.getByLabelText("Shelf position"), { target: { value: "-1" } });
  fireEvent.click(screen.getByRole("button", { name: "Update shelf" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("save_smart_shelf", {
      shelf: { ...shelf, name: "Later", filter: changed, sortOrder: -1 },
    }),
  );
  await waitFor(() => expect(screen.getByRole("button", { name: "Delete shelf" })).toBeEnabled());
  fireEvent.click(screen.getByRole("button", { name: "Delete shelf" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("delete_smart_shelf", { shelfId: 7 }));
  expect(
    vi
      .mocked(invoke)
      .mock.calls.every(([command]) =>
        ["list_smart_shelves", "save_smart_shelf", "delete_smart_shelf"].includes(command),
      ),
  ).toBe(true);
});

it("updates status only through an explicit selection and displays completion date", async () => {
  let status = "paused";
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "set_reading_status") {
      status = (args as { status: string }).status;
      return;
    }
    return { status, completedAt: status === "finished" ? 172800 : null };
  });
  const changed = vi.fn();
  render(<ReadingStatus bookId={9} onChanged={changed} />);
  await waitFor(() => expect(screen.getByLabelText("Reading status")).toHaveValue("paused"));
  expect(invoke).not.toHaveBeenCalledWith("set_reading_status", expect.anything());
  fireEvent.change(screen.getByLabelText("Reading status"), { target: { value: "finished" } });
  await screen.findByText(/^Completed /);
  expect(invoke).toHaveBeenCalledWith("set_reading_status", { bookId: 9, status: "finished" });
  expect(changed).toHaveBeenCalledOnce();
});
