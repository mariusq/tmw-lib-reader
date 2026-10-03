import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue([]) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
import { invoke } from "@tauri-apps/api/core";
import { LibraryShell } from "./LibraryShell";

class TestIntersectionObserver {
  observe() {}
  disconnect() {}
}
vi.stubGlobal("IntersectionObserver", TestIntersectionObserver);

describe("LibraryShell", () => {
  it("renders the empty library state", () => {
    render(<LibraryShell />);
    expect(screen.getByRole("heading", { name: "続きを読む" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "すべての本を見る" }));
    expect(screen.getByRole("heading", { name: "すべての本" })).toBeInTheDocument();
    expect(screen.getByRole("searchbox", { name: "タイトル・著者・フォルダーを検索" })).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "同名の重複を隠す" })).toBeInTheDocument();
  });
  it("shows finished checkmarks in grid and list, including collection filters", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "browse_books") return [
        { id: 1, effectiveTitle: "Finished book", effectiveCreator: "Author", isAvailable: true, isFinished: true },
        { id: 2, effectiveTitle: "Unread book", effectiveCreator: "Author", isAvailable: true, isFinished: false },
      ];
      if (command === "list_collections") return [[7, "Favorites"]];
      return [];
    });
    render(<LibraryShell />);
    fireEvent.click(screen.getByRole("button", { name: "すべての本を見る" }));
    expect(await screen.findByRole("img", { name: "読了" })).toBeInTheDocument();
    expect(screen.getAllByRole("img", { name: "読了" })).toHaveLength(1);
    fireEvent.change(screen.getByRole("combobox", { name: "コレクションで絞り込み" }), { target: { value: "7" } });
    fireEvent.click(screen.getByRole("button", { name: "リスト" }));
    expect(screen.getAllByRole("img", { name: "読了" })).toHaveLength(1);
    vi.mocked(invoke).mockResolvedValue([]);
  });

});
