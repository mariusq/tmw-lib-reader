import { fireEvent, render, screen, within } from "@testing-library/react";
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

  it("groups navigation and keeps settings controls reachable", async () => {
    localStorage.removeItem("tmw-interface-language");
    render(<LibraryShell />);
    const navigation = within(screen.getByRole("navigation", { name: "Main navigation" }));
    expect(navigation.getAllByRole("button")).toHaveLength(5);
    expect(screen.queryByRole("switch")).not.toBeInTheDocument();

    fireEvent.click(navigation.getByRole("button", { name: "ライブラリ" }));
    fireEvent.click(screen.getByRole("button", { name: "最近追加" }));
    expect(screen.getByRole("heading", { name: "最近追加" })).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("browse_books", expect.objectContaining({ request: expect.objectContaining({ sort: "dateAdded" }) }));
    fireEvent.click(screen.getByRole("button", { name: "要確認のメタデータ" }));
    expect(screen.getByRole("checkbox", { name: "要確認のみ" })).toBeChecked();

    fireEvent.click(navigation.getByRole("button", { name: "読書ノート" }));
    expect(await screen.findByRole("heading", { name: /Saved passages/ })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "調べた言葉" }));
    expect(await screen.findByRole("heading", { name: /Lookup history/ })).toBeInTheDocument();

    fireEvent.click(navigation.getByRole("button", { name: "コレクション" }));
    fireEvent.click(screen.getByRole("button", { name: "タグ" }));
    expect(screen.getByRole("heading", { name: "タグ" })).toBeInTheDocument();

    fireEvent.click(navigation.getByRole("button", { name: "設定" }));
    fireEvent.click(screen.getByRole("button", { name: "ライブラリルート" }));
    expect(screen.getByRole("heading", { name: "ライブラリルート" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "フォルダーを追加" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "一般" }));
    fireEvent.click(screen.getByRole("switch", { name: "言語" }));
    expect(navigation.getByRole("button", { name: "Reading Notes" })).toBeInTheDocument();
    expect(localStorage.getItem("tmw-interface-language")).toBe("en");
    localStorage.removeItem("tmw-interface-language");
  });

});

