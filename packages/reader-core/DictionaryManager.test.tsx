import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import DictionaryManager from "./DictionaryManager";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const dictionary = { id: 7, title: "Test Japanese", revision: "1", enabled: true, priority: 0, termCount: 4, attribution: null };
beforeEach(() => {
  vi.mocked(invoke).mockImplementation(async command => {
    if (command === "dictionary_manage") return [dictionary];
    if (command === "dictionary_import_status") return { running: false, progress: {} };
    return null;
  });
});
describe("DictionaryManager", () => {
  it("saves a per-source limit and rejects invalid values", async () => {
    render(<DictionaryManager />);
    const input = await screen.findByLabelText("Result limit for Test Japanese");
    fireEvent.change(input, { target: { value: "2" } }); fireEvent.blur(input);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("dictionary_manage", { action: "limit", id: 7, resultLimit: 2 }));
    await waitFor(() => expect(input).not.toBeDisabled());
    fireEvent.change(input, { target: { value: "-1" } }); fireEvent.blur(input);
    expect(input).toHaveValue(0);
  });
  it("persists toggles and priority through management and refreshes", async () => {
    render(<DictionaryManager />);
    fireEvent.click(await screen.findByRole("checkbox", { name: "Enable Test Japanese" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("dictionary_manage", { action: "update", id: 7, enabled: false, priority: 0 }));
    await waitFor(() => expect(screen.getByLabelText("Priority for Test Japanese")).not.toBeDisabled());
    const priority = screen.getByLabelText("Priority for Test Japanese");
    fireEvent.change(priority, { target: { value: "12" } }); fireEvent.blur(priority);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("dictionary_manage", { action: "update", id: 7, enabled: true, priority: 12 }));
  });
  it("selects replacement explicitly and preserves the existing import on error", async () => {
    vi.mocked(invoke).mockImplementation(async command => {
      if (command === "dictionary_manage") return [dictionary];
      if (command === "dictionary_import") throw new Error("Invalid index.json: choose a Yomitan term ZIP");
      return { running: false, progress: {} };
    });
    render(<DictionaryManager />);
    fireEvent.click(await screen.findByRole("button", { name: "Replace Test Japanese from ZIP" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Invalid index.json");
    expect(invoke).toHaveBeenCalledWith("dictionary_import", { replace: 7 });
    expect(screen.getByRole("heading", { name: "Test Japanese" })).toBeInTheDocument();
  });
  it("cancels an active import and confirms only app-managed removal", async () => {
    vi.mocked(invoke).mockImplementation(async command => {
      if (command === "dictionary_manage") return [dictionary];
      if (command === "dictionary_import_status") return { running: true, progress: { phase: "terms", filesDone: 1, filesTotal: 2, terms: 3, tags: 0 } };
      return null;
    });
    const view = render(<DictionaryManager />);
    fireEvent.click(await screen.findByRole("button", { name: "Cancel import" }));
    expect(invoke).toHaveBeenCalledWith("cancel_dictionary_import");
    view.unmount();
    vi.mocked(invoke).mockImplementation(async command => command === "dictionary_manage" ? [dictionary] : { running: false, progress: {} });
    render(<DictionaryManager />);
    fireEvent.click(await screen.findByRole("button", { name: "Remove Test Japanese" }));
    expect(screen.getByRole("alertdialog")).toHaveTextContent("source ZIP and saved passages/history stay untouched");
    fireEvent.click(screen.getByRole("button", { name: "Confirm removal" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("dictionary_manage", { action: "remove", id: 7 }));
  });
});
