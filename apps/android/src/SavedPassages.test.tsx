import { act, fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import SavedPassages from "./SavedPassages";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const book = { ns: "a".repeat(32), id: "b".repeat(32), version: `sha256-${"c".repeat(64)}` };
const record = {
  entityId: "d".repeat(32),
  contentVersion: book.version,
  deleted: false,
  fields: { surface: "猫", sentence: "猫です。", note: "old note", locationCfi: "epubcfi(/6/2)" },
};

it("saves only edited fields and deletes by stable identity", async () => {
  const requests: Record<string, unknown>[] = [];
  vi.mocked(invoke).mockImplementation(async (_, input) => {
    const args = (input as { args: Record<string, unknown> }).args;
    requests.push(args);
    return args.action === "userState" ? { progress: null, passages: [record], pending: 0 } : {};
  });
  await act(async () => {
    render(<SavedPassages book={book} revision={0} onJump={vi.fn()} />);
  });
  fireEvent.change(screen.getByRole("textbox", { name: "Passage note" }), {
    target: { value: "new note" },
  });
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "Save changes" }));
  });
  expect(requests.find((r) => r.action === "userSave")).toMatchObject({
    ...book,
    kind: "passage",
    entityId: record.entityId,
    fields: { note: "new note" },
    deleted: false,
  });
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "Delete passage" }));
  });
  const saves = requests.filter((r) => r.action === "userSave");
  expect(saves[saves.length - 1]).toMatchObject({
    entityId: record.entityId,
    fields: {},
    deleted: true,
  });
});

it("retains a changed-version excerpt and disables jumping to its anchor", async () => {
  vi.mocked(invoke).mockResolvedValue({
    progress: null,
    passages: [{ ...record, contentVersion: `sha256-${"e".repeat(64)}` }],
    pending: 0,
  });
  const jump = vi.fn();
  await act(async () => {
    render(<SavedPassages book={book} revision={0} onJump={jump} />);
  });
  expect(screen.getByText("猫")).toBeTruthy();
  expect(
    (screen.getByRole("button", { name: "Go to passage" }) as HTMLButtonElement).disabled,
  ).toBe(true);
  expect(jump).not.toHaveBeenCalled();
});

it("pages legacy notes without requiring a downloaded book", async () => {
  vi.mocked(invoke).mockImplementation(async (_, input) => {
    const args = (input as { args: Record<string, unknown> }).args;
    return {
      progress: null,
      pending: 0,
      next: args.offset === 0 ? 50 : null,
      passages: [
        {
          ...record,
          entityId: String(args.offset),
          contentVersion: null,
          fields: { ...record.fields, surface: args.offset === 0 ? "First note" : "Next note" },
        },
      ],
    };
  });
  await act(async () => {
    render(<SavedPassages book={{ ...book, version: null }} revision={0} onJump={vi.fn()} />);
  });
  expect(
    (screen.getByRole("button", { name: "Go to passage" }) as HTMLButtonElement).disabled,
  ).toBe(true);
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "Next notes" }));
  });
  expect(screen.getByText("Next note")).toBeTruthy();
  expect(invoke).toHaveBeenCalledWith("mobile_storage", {
    args: { ...book, version: null, action: "userState", offset: 50 },
  });
});
