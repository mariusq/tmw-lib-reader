import { fireEvent, render, screen, within, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockImplementation(async command => command === "get_reading_activity" ? { enabled: true, days: [], books: [], finished: 0 } : []) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
import { invoke } from "@tauri-apps/api/core";
import { LibraryShell } from "./LibraryShell";

class TestIntersectionObserver {
  observe() {}
  disconnect() {}
}
vi.stubGlobal("IntersectionObserver", TestIntersectionObserver);
vi.stubGlobal("scrollTo", vi.fn());

describe("LibraryShell", () => {
  it("clears visible status filters and remembers the library layout", async () => {
    localStorage.setItem("tmw-interface-language", "en");
    localStorage.setItem("tmw-library-layout", "list");
    render(<LibraryShell />);
    fireEvent.click(screen.getByRole("button", { name: "Browse full library" }));
    expect(screen.getByRole("button", { name: "List" })).toHaveAttribute("aria-pressed", "true");
    fireEvent.change(screen.getByRole("combobox", { name: "Filter by reading status" }), { target: { value: "reading" } });
    fireEvent.click(screen.getByRole("button", { name: "Remove filter: Reading" }));
    expect(screen.getByRole("combobox", { name: "Filter by reading status" })).toHaveValue("");
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "Story" } });
    fireEvent.click(screen.getAllByRole("button", { name: "Reset search & filters" })[0]);
    expect(screen.getByRole("searchbox")).toHaveValue("");
    fireEvent.click(screen.getByRole("button", { name: "Grid" }));
    expect(localStorage.getItem("tmw-library-layout")).toBe("grid");
    localStorage.removeItem("tmw-interface-language");
    localStorage.removeItem("tmw-library-layout");
  });
  it("opens a consolidated series and persists confidence and toggle preferences", async () => {
    vi.stubGlobal("scrollY", 480);
    localStorage.setItem("tmw-interface-language", "en");
    localStorage.setItem("tmw-series-confidence", "medium");
    localStorage.removeItem("tmw-consolidate-series");
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_reading_activity") return { enabled: true, days: [], books: [], finished: 0 };
      if (command !== "browse_books") return [];
      const options = (args as { seriesOptions?: { consolidate: boolean; selectedKey?: string } }).seriesOptions;
      const base = { effectiveCreator: "Author", isAvailable: true, isFinished: false };
      if (options?.selectedKey) return [{ ...base, id: 1, effectiveTitle: "Story 1", seriesKey: "proposal:test", seriesTitle: "Story", seriesCount: 2 }, { ...base, id: 2, effectiveTitle: "Story 2", seriesKey: "proposal:test", seriesTitle: "Story", seriesCount: 2 }];
      if (options?.consolidate) return [{ ...base, id: 1, effectiveTitle: "Story 1", seriesKey: "proposal:test", seriesTitle: "Story", seriesCount: 2 }];
      return [{ ...base, id: 1, effectiveTitle: "Story 1" }];
    });
    render(<LibraryShell />);
    fireEvent.click(screen.getByRole("button", { name: "Browse full library" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "Consolidate by series" }));
    expect(localStorage.getItem("tmw-consolidate-series")).toBe("true");
    fireEvent.click(await screen.findByRole("button", { name: /Story Author Series/ }));
    expect(await screen.findByRole("heading", { name: "Story 2" })).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("browse_books", expect.objectContaining({ seriesOptions: { consolidate: true, confidence: "medium", selectedKey: "proposal:test" } }));
    const seriesPage = screen.getByRole("heading", { name: "Story 2" }).closest("section");
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    expect(seriesPage).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Story 2" })).not.toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Duplicate books" })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Series confidence" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Library" }));
    fireEvent.click(await screen.findByRole("button", { name: /Story Author Series/ }));
    await screen.findByRole("heading", { name: "Story 2" });
    expect(screen.getByRole("navigation", { name: "Breadcrumb" })).toBeInTheDocument();
    const callsBeforeBack = vi.mocked(invoke).mock.calls.filter(([command]) => command === "browse_books").length;
    fireEvent.click(screen.getByRole("button", { name: "Back to library" }));
    expect(screen.getAllByRole("button", { name: /Story Author Series/ })).toHaveLength(1);
    expect(await screen.findByText("Series · 2 books")).toBeInTheDocument();
    expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "browse_books")).toHaveLength(callsBeforeBack);
    await waitFor(() => expect(window.scrollTo).toHaveBeenCalledWith({ top: 480, behavior: "instant" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "Consolidate by series" }));
    expect(await screen.findByRole("heading", { name: "Story 1" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    fireEvent.change(screen.getByRole("combobox", { name: "Series confidence" }), { target: { value: "all" } });
    expect(localStorage.getItem("tmw-series-confidence")).toBe("all");
    for (const name of ["tmw-interface-language", "tmw-series-confidence", "tmw-consolidate-series"]) localStorage.removeItem(name);
    vi.stubGlobal("scrollY", 0);
    vi.mocked(invoke).mockResolvedValue([]);
  });
  it("uses the Settings confidence level only while duplicate hiding is checked", async () => {
    localStorage.setItem("tmw-duplicate-filtering", "all");
    render(<LibraryShell />);
    fireEvent.click(screen.getByRole("button", { name: "すべての本を見る" }));
    const toggle = screen.getByRole("checkbox", { name: "同名の重複を隠す" });
    fireEvent.click(toggle);
    expect(invoke).toHaveBeenCalledWith("browse_books", expect.objectContaining({ request: expect.objectContaining({ duplicateFiltering: "all" }) }));
    fireEvent.click(toggle);
    expect(invoke).toHaveBeenLastCalledWith("browse_books", expect.objectContaining({ request: expect.objectContaining({ duplicateFiltering: "off" }) }));
    fireEvent.click(screen.getByRole("button", { name: "設定" }));
    expect(screen.getByRole("combobox", { name: "重複する本" })).toHaveValue("all");
    fireEvent.change(screen.getByRole("combobox", { name: "重複する本" }), { target: { value: "high" } });
    expect(localStorage.getItem("tmw-duplicate-filtering")).toBe("high");
    localStorage.removeItem("tmw-duplicate-filtering");
  });
  it("renders the empty library state", () => {
    render(<LibraryShell />);
    expect(screen.getByRole("heading", { name: "続きを読む" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "すべての本を見る" }));
    expect(screen.getByRole("heading", { name: "すべての本" })).toBeInTheDocument();
    expect(screen.getByRole("searchbox", { name: "タイトル・著者・フォルダーを検索" })).toBeInTheDocument();
    expect(screen.getByText("絞り込み", { selector: "summary" }).parentElement).not.toHaveAttribute("open");
    fireEvent.click(screen.getByText("絞り込み", { selector: "summary" }));
    expect(screen.getByRole("checkbox", { name: "同名の重複を隠す" })).toBeInTheDocument();
  });
  it("shows finished checkmarks in grid and list, including collection filters", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_reading_activity") return { enabled: true, days: [], books: [], finished: 0 };
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
    fireEvent.click(screen.getByText("絞り込み", { selector: "summary" }));
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
    expect(screen.queryByRole("checkbox", { name: "要確認のみ" })).not.toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("browse_books", expect.objectContaining({ request: expect.objectContaining({ needsMetadata: true }) }));

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

  it("separates settings categories and preserves cover preferences across navigation", () => {
    localStorage.setItem("tmw-interface-language", "en");
    localStorage.removeItem("tmw-show-covers");
    render(<LibraryShell />);
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    const categories = within(screen.getByRole("navigation", { name: "Settings categories" }));
    expect(categories.getAllByRole("button")).toHaveLength(8);
    expect(screen.getByRole("combobox", { name: "Series confidence" })).toBeVisible();
    expect(screen.queryByRole("checkbox", { name: "Show covers" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Restore backup" })).not.toBeInTheDocument();
    fireEvent.click(categories.getByRole("button", { name: "Covers" }));
    expect(categories.getByRole("button", { name: "Covers" })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("heading", { name: "Covers" })).toHaveFocus();
    fireEvent.click(screen.getByRole("checkbox", { name: "Show covers" }));
    expect(localStorage.getItem("tmw-show-covers")).toBe("false");
    fireEvent.click(categories.getByRole("button", { name: "Backup & restore" }));
    expect(screen.getByRole("button", { name: "Restore backup" })).toBeVisible();
    expect(screen.queryByRole("button", { name: "Regenerate cover cache" })).not.toBeInTheDocument();
    fireEvent.click(categories.getByRole("button", { name: "Covers" }));
    expect(screen.getByRole("checkbox", { name: "Show covers" })).not.toBeChecked();
    localStorage.removeItem("tmw-interface-language");
    localStorage.removeItem("tmw-show-covers");
  });

});

