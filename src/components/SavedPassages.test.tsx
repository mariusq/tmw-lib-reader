import { fireEvent, render, screen, waitFor, cleanup } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { SavedPassages, type SavedPassage } from "./SavedPassages";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const passage: SavedPassage = { id: 1, bookId: 2, bookTitle: "物語", surface: "読んだ", headword: "読む", reading: "よむ", sentence: "本を読んだ。", note: "", locationCfi: "epubcfi(/6/2!/4/2/1:0)", isAvailable: true };
beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async command => command === "list_saved_passages" ? [passage] : command === "get_passage_location" ? passage.locationCfi : undefined);
});
afterEach(cleanup);

it("edits a local note and validates the anchor before jumping", async () => {
  const jump = vi.fn();
  render(<SavedPassages onJump={jump} />);
  const note = await screen.findByLabelText("Note for 読んだ");
  fireEvent.change(note, { target: { value: "好きな文章" } });
  fireEvent.click(screen.getByText("Save changes"));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("edit_passage", { id: 1, sentence: "本を読んだ。", note: "好きな文章" }));
  await waitFor(() => expect(screen.getByText("Jump to passage")).toBeEnabled());
  fireEvent.click(screen.getByText("Jump to passage"));
  await waitFor(() => expect(jump).toHaveBeenCalledWith(passage, passage.locationCfi));
});

it("keeps unavailable excerpts readable and disables jumping", async () => {
  vi.mocked(invoke).mockResolvedValue([{ ...passage, isAvailable: false }]);
  render(<SavedPassages bookId={2} onJump={vi.fn()} />);
  expect(await screen.findByDisplayValue("本を読んだ。")).toBeVisible();
  expect(screen.getByText("Jump to passage")).toBeDisabled();
  expect(invoke).toHaveBeenCalledWith("list_saved_passages", { bookId: 2, offset: 0 });
});

it("shows changed-source errors without opening an unrelated location", async () => {
  const jump = vi.fn();
  vi.mocked(invoke).mockImplementation(async command => {
    if (command === "list_saved_passages") return [passage];
    throw new Error("The source EPUB has changed.");
  });
  render(<SavedPassages onJump={jump} />);
  fireEvent.click(await screen.findByText("Jump to passage"));
  expect(await screen.findByRole("alert")).toHaveTextContent("source EPUB has changed");
  expect(jump).not.toHaveBeenCalled();
});

it("deletes only the bookmark after confirmation", async () => {
  vi.spyOn(window, "confirm").mockReturnValueOnce(true);
  render(<SavedPassages onJump={vi.fn()} />);
  fireEvent.click(await screen.findByText("Delete"));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("delete_passage", { id: 1 }));
});
