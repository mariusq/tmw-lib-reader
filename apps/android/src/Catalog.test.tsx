import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import Catalog from "./Catalog";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("./SavedPassages", () => ({ default: () => null }));
class Observer { observe() {} disconnect() {} }
vi.stubGlobal("IntersectionObserver", Observer);
afterEach(() => { localStorage.clear(); vi.resetAllMocks(); });
it("shows recent downloads independently of browsing and opens their current local copy", async () => {
  localStorage.setItem("tmw-show-covers", "false");
  const books = Array.from({ length: 5 }, (_, i) => ({ ns: "pc", id: `id${i}`, title: `Story ${i}`, creator: "Author", download: { file: `book${i}.epub`, version: "sha256-version", bytes: 10 } }));
  vi.mocked(invoke).mockImplementation(async (command, payload) => {
    if (command === "read_mobile_book") return new ArrayBuffer(10);
    const args = (payload as { args: { action: string; recent?: boolean } }).args;
    if (args.action === "browse") return { items: args.recent ? books : [], ns: "pc", namespaces: ["pc"], next: null };
    if (args.action === "cacheSettings") return { budget: 100000000, usage: 0 };
    return {};
  });
  const onOpen = vi.fn();
  render(<Catalog onOpen={onOpen} />);
  expect(screen.getByRole("heading", { name: "Jump back in" })).toBeTruthy();
  fireEvent.click(await screen.findByRole("button", { name: "Resume: Story 0" }));
  await vi.waitFor(() => expect(onOpen).toHaveBeenCalledWith(expect.objectContaining({ file: "book0.epub", catalog: { ns: "pc", id: "id0", version: "sha256-version" } })));
  expect(screen.getAllByRole("button", { name: /^Resume:/ })).toHaveLength(5);
  expect(invoke).not.toHaveBeenCalledWith("mobile_storage", { args: expect.objectContaining({ action: "cover" }) });
});
